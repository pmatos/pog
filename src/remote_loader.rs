use std::process::Command;
use std::sync::RwLock;

use crate::cache::{LineCache, CHUNK_SIZE};
use crate::error::{PogError, Result};
use crate::file_source::FileSource;

const MAX_RETRIES: usize = 3;
const RETRY_DELAY_MS: u64 = 500;
const MAX_CACHED_CHUNKS: usize = 20;

pub struct RemoteFile {
    host: String,
    path: String,
    display_name: String,
    line_count: usize,
    cache: RwLock<LineCache>,
}

impl RemoteFile {
    pub fn open(host: &str, path: &str) -> Result<Self> {
        let display_name = format!("{}:{}", host, path);

        let line_count = Self::fetch_line_count_static(host, path)?;

        Ok(Self {
            host: host.to_string(),
            path: path.to_string(),
            display_name,
            line_count,
            cache: RwLock::new(LineCache::new(MAX_CACHED_CHUNKS)),
        })
    }

    fn fetch_line_count_static(host: &str, path: &str) -> Result<usize> {
        Self::with_retry(|| {
            let output = Command::new("ssh")
                .arg(host)
                .arg(format!("wc -l < '{}'", path))
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("No such file") {
                    return Err(PogError::FileNotFound {
                        path: format!("{}:{}", host, path),
                    });
                }
                if stderr.contains("Permission denied") {
                    return Err(PogError::PermissionDenied {
                        path: format!("{}:{}", host, path),
                    });
                }
                return Err(PogError::Ssh {
                    host: host.to_string(),
                    message: stderr.to_string(),
                });
            }

            let stdout = String::from_utf8(output.stdout)?;
            let count: usize = stdout
                .trim()
                .parse()
                .map_err(|_| PogError::Ssh {
                    host: host.to_string(),
                    message: format!("Invalid line count: {}", stdout.trim()),
                })?;

            Ok(count)
        })
    }

    fn fetch_chunk(&self, chunk_start: usize) -> Result<Vec<String>> {
        let start_line = chunk_start + 1; // 1-based indexing
        let count = CHUNK_SIZE.min(self.line_count.saturating_sub(chunk_start));

        Self::with_retry(|| {
            // Use tail -n +N | head -n M for faster access
            // tail -n +N outputs from line N onwards (1-based)
            // head -n M takes first M lines from that
            let cmd = format!(
                "tail -n +{} '{}' | head -n {}",
                start_line,
                self.path,
                count
            );

            let output = Command::new("ssh")
                .arg(&self.host)
                .arg(&cmd)
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(PogError::Ssh {
                    host: self.host.clone(),
                    message: stderr.to_string(),
                });
            }

            let stdout = String::from_utf8(output.stdout)?;
            let lines: Vec<String> = stdout.lines().map(|l| l.to_string()).collect();
            Ok(lines)
        })
    }

    fn with_retry<T, F>(mut operation: F) -> Result<T>
    where
        F: FnMut() -> Result<T>,
    {
        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            match operation() {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < MAX_RETRIES - 1 {
                        std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    fn ensure_chunk_loaded(&self, chunk_start: usize) -> Result<()> {
        {
            let cache = self.cache.read().unwrap();
            if cache.contains_line(chunk_start) {
                return Ok(());
            }
        }

        let lines = self.fetch_chunk(chunk_start)?;

        {
            let mut cache = self.cache.write().unwrap();
            cache.insert_chunk(chunk_start, lines);
        }

        Ok(())
    }
}

impl FileSource for RemoteFile {
    fn line_count(&self) -> usize {
        self.line_count
    }

    fn file_size(&self) -> Result<u64> {
        Self::with_retry(|| {
            let output = Command::new("ssh")
                .arg(&self.host)
                .arg(format!("stat -c%s '{}'", self.path))
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(PogError::Ssh {
                    host: self.host.clone(),
                    message: stderr.to_string(),
                });
            }

            let stdout = String::from_utf8(output.stdout)?;
            let size: u64 = stdout
                .trim()
                .parse()
                .map_err(|_| PogError::Ssh {
                    host: self.host.clone(),
                    message: format!("Invalid file size: {}", stdout.trim()),
                })?;

            Ok(size)
        })
    }

    fn get_line(&self, line_num: usize) -> Result<Option<String>> {
        if line_num >= self.line_count {
            return Ok(None);
        }

        let chunk_start = LineCache::chunk_start_for_line(line_num);
        self.ensure_chunk_loaded(chunk_start)?;

        let mut cache = self.cache.write().unwrap();
        Ok(cache.get_line(line_num).cloned())
    }

    fn get_lines(&self, start_line: usize, count: usize) -> Result<Vec<(usize, String)>> {
        let end_line = (start_line + count).min(self.line_count);
        let actual_count = end_line.saturating_sub(start_line);

        if actual_count == 0 {
            return Ok(Vec::new());
        }

        let first_chunk = LineCache::chunk_start_for_line(start_line);
        let last_chunk = LineCache::chunk_start_for_line(end_line.saturating_sub(1));

        let mut chunk_start = first_chunk;
        while chunk_start <= last_chunk {
            self.ensure_chunk_loaded(chunk_start)?;
            chunk_start += CHUNK_SIZE;
        }

        let mut result = Vec::with_capacity(actual_count);
        let mut cache = self.cache.write().unwrap();

        for line_num in start_line..end_line {
            if let Some(line) = cache.get_line(line_num) {
                result.push((line_num, line.clone()));
            }
        }

        Ok(result)
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }
}
