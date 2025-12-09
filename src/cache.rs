use std::collections::HashMap;

pub const CHUNK_SIZE: usize = 500;

pub struct CachedChunk {
    pub lines: Vec<String>,
}

pub struct LineCache {
    chunks: HashMap<usize, CachedChunk>,
    max_chunks: usize,
    access_order: Vec<usize>,
}

impl LineCache {
    pub fn new(max_chunks: usize) -> Self {
        Self {
            chunks: HashMap::new(),
            max_chunks,
            access_order: Vec::new(),
        }
    }

    /// Get chunk start line for a given line number
    pub fn chunk_start_for_line(line_num: usize) -> usize {
        (line_num / CHUNK_SIZE) * CHUNK_SIZE
    }

    /// Check if a line is cached
    pub fn contains_line(&self, line_num: usize) -> bool {
        let chunk_start = Self::chunk_start_for_line(line_num);
        if let Some(chunk) = self.chunks.get(&chunk_start) {
            let offset = line_num - chunk_start;
            offset < chunk.lines.len()
        } else {
            false
        }
    }

    /// Get a line from cache if available
    pub fn get_line(&mut self, line_num: usize) -> Option<&String> {
        let chunk_start = Self::chunk_start_for_line(line_num);

        if self.chunks.contains_key(&chunk_start) {
            self.update_access_order(chunk_start);
            let chunk = self.chunks.get(&chunk_start).unwrap();
            let offset = line_num - chunk_start;
            chunk.lines.get(offset)
        } else {
            None
        }
    }

    /// Insert a chunk into the cache
    pub fn insert_chunk(&mut self, start_line: usize, lines: Vec<String>) {
        if self.chunks.len() >= self.max_chunks && !self.chunks.contains_key(&start_line) {
            self.evict_oldest();
        }

        self.chunks.insert(start_line, CachedChunk { lines });
        self.update_access_order(start_line);
    }

    fn update_access_order(&mut self, chunk_start: usize) {
        self.access_order.retain(|&x| x != chunk_start);
        self.access_order.push(chunk_start);
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest) = self.access_order.first().cloned() {
            self.chunks.remove(&oldest);
            self.access_order.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_start_calculation() {
        assert_eq!(LineCache::chunk_start_for_line(0), 0);
        assert_eq!(LineCache::chunk_start_for_line(499), 0);
        assert_eq!(LineCache::chunk_start_for_line(500), 500);
        assert_eq!(LineCache::chunk_start_for_line(501), 500);
        assert_eq!(LineCache::chunk_start_for_line(1000), 1000);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = LineCache::new(5);
        let lines: Vec<String> = (0..500).map(|i| format!("line {}", i)).collect();
        cache.insert_chunk(0, lines);

        assert!(cache.contains_line(0));
        assert!(cache.contains_line(499));
        assert!(!cache.contains_line(500));

        assert_eq!(cache.get_line(0), Some(&"line 0".to_string()));
        assert_eq!(cache.get_line(499), Some(&"line 499".to_string()));
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = LineCache::new(2);

        cache.insert_chunk(0, vec!["a".to_string()]);
        cache.insert_chunk(500, vec!["b".to_string()]);
        cache.insert_chunk(1000, vec!["c".to_string()]);

        assert!(!cache.contains_line(0));
        assert!(cache.contains_line(500));
        assert!(cache.contains_line(1000));
    }
}
