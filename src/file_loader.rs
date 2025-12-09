use memmap2::Mmap;
use std::fs::File;
use std::io;
use std::path::Path;

pub struct MappedFile {
    mmap: Mmap,
    line_offsets: Vec<usize>,
}

impl MappedFile {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        let mut loader = Self {
            mmap,
            line_offsets: vec![0],
        };

        loader.build_line_index();
        Ok(loader)
    }

    fn build_line_index(&mut self) {
        let data = &self.mmap[..];

        for (i, &byte) in data.iter().enumerate() {
            if byte == b'\n' {
                let next_line_start = i + 1;
                if next_line_start < data.len() {
                    self.line_offsets.push(next_line_start);
                }
            }
        }
    }

    pub fn line_count(&self) -> usize {
        self.line_offsets.len()
    }

    pub fn get_line(&self, line_num: usize) -> Option<&str> {
        if line_num >= self.line_offsets.len() {
            return None;
        }

        let start = self.line_offsets[line_num];
        let end = if line_num + 1 < self.line_offsets.len() {
            self.line_offsets[line_num + 1]
        } else {
            self.mmap.len()
        };

        let line_bytes = &self.mmap[start..end];
        let line_bytes = if line_bytes.ends_with(b"\n") {
            &line_bytes[..line_bytes.len() - 1]
        } else {
            line_bytes
        };
        let line_bytes = if line_bytes.ends_with(b"\r") {
            &line_bytes[..line_bytes.len() - 1]
        } else {
            line_bytes
        };

        std::str::from_utf8(line_bytes).ok()
    }

    pub fn get_lines(&self, start_line: usize, count: usize) -> Vec<(usize, &str)> {
        let mut lines = Vec::with_capacity(count);
        for i in start_line..(start_line + count).min(self.line_count()) {
            if let Some(line) = self.get_line(i) {
                lines.push((i, line));
            }
        }
        lines
    }

}
