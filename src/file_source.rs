use crate::error::Result;

pub trait FileSource: Send + Sync {
    /// Returns total number of lines in the file
    fn line_count(&self) -> usize;

    /// Get a single line by 0-based line number
    #[allow(dead_code)]
    fn get_line(&self, line_num: usize) -> Result<Option<String>>;

    /// Get multiple lines efficiently (batch operation)
    fn get_lines(&self, start_line: usize, count: usize) -> Result<Vec<(usize, String)>>;

    /// Display name for window title
    fn display_name(&self) -> &str;
}
