use regex::Regex;

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub line_num: usize,   // 0-based
    pub start_col: usize,  // 0-based
    pub end_col: usize,    // exclusive
}

pub struct SearchState {
    pub pattern: Option<Regex>,
    pub pattern_str: String,
    pub viewport_matches: Vec<SearchMatch>,
    pub current_match_index: Option<usize>,
    pub last_searched_range: Option<(usize, usize)>,
    pub is_active: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            pattern: None,
            pattern_str: String::new(),
            viewport_matches: Vec::new(),
            current_match_index: None,
            last_searched_range: None,
            is_active: false,
        }
    }

    pub fn clear(&mut self) {
        self.pattern = None;
        self.pattern_str.clear();
        self.viewport_matches.clear();
        self.current_match_index = None;
        self.last_searched_range = None;
        self.is_active = false;
    }

    pub fn set_pattern(&mut self, pattern_str: &str) -> Result<(), String> {
        match Regex::new(pattern_str) {
            Ok(regex) => {
                self.pattern = Some(regex);
                self.pattern_str = pattern_str.to_string();
                self.viewport_matches.clear();
                self.current_match_index = None;
                self.last_searched_range = None;
                self.is_active = true;
                Ok(())
            }
            Err(e) => Err(format!("invalid regex: {}", e)),
        }
    }

    pub fn update_matches(&mut self, matches: Vec<SearchMatch>, searched_range: (usize, usize)) {
        self.viewport_matches = matches;
        self.last_searched_range = Some(searched_range);
        if !self.viewport_matches.is_empty() && self.current_match_index.is_none() {
            self.current_match_index = Some(0);
        }
    }

    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.current_match_index
            .and_then(|i| self.viewport_matches.get(i))
    }

    #[allow(dead_code)]
    pub fn next_match_in_viewport(&mut self) -> Option<&SearchMatch> {
        if self.viewport_matches.is_empty() {
            return None;
        }
        let new_index = match self.current_match_index {
            Some(i) => (i + 1) % self.viewport_matches.len(),
            None => 0,
        };
        self.current_match_index = Some(new_index);
        self.viewport_matches.get(new_index)
    }

    #[allow(dead_code)]
    pub fn prev_match_in_viewport(&mut self) -> Option<&SearchMatch> {
        if self.viewport_matches.is_empty() {
            return None;
        }
        let new_index = match self.current_match_index {
            Some(i) => {
                if i == 0 {
                    self.viewport_matches.len() - 1
                } else {
                    i - 1
                }
            }
            None => self.viewport_matches.len() - 1,
        };
        self.current_match_index = Some(new_index);
        self.viewport_matches.get(new_index)
    }

    pub fn needs_research(&self, viewport_start: usize, viewport_size: usize, buffer: usize) -> bool {
        if !self.is_active || self.pattern.is_none() {
            return false;
        }
        match self.last_searched_range {
            Some((searched_start, searched_end)) => {
                let half_buffer = buffer / 2;
                viewport_start < searched_start.saturating_add(half_buffer)
                    || viewport_start + viewport_size > searched_end.saturating_sub(half_buffer)
            }
            None => true,
        }
    }
}

pub fn search_lines(
    pattern: &Regex,
    lines: &[(usize, String)],
) -> Vec<SearchMatch> {
    let mut matches = Vec::new();
    for (line_num, text) in lines {
        for mat in pattern.find_iter(text) {
            matches.push(SearchMatch {
                line_num: *line_num,
                start_col: mat.start(),
                end_col: mat.end(),
            });
        }
    }
    matches
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SearchDirection {
    Forward,
    Backward,
}
