use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum PogCommand {
    Goto { line: usize },
    Lines,
    Top,
    Size,
    Mark {
        line: usize,
        region: Option<(usize, usize)>,  // (start_col, end_col) 1-based from user
        color: String,
    },
    Unmark {
        line: usize,
        region: Option<(usize, usize)>,  // Optional: specific region to unmark
    },
    Search { pattern: String },
    SearchNext,
    SearchPrev,
    SearchClear,
}

#[derive(Debug, Clone)]
pub enum CommandResponse {
    Ok(Option<String>),
    Error(String),
}

impl fmt::Display for CommandResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandResponse::Ok(None) => write!(f, "OK"),
            CommandResponse::Ok(Some(msg)) => write!(f, "OK {}", msg),
            CommandResponse::Error(msg) => write!(f, "ERROR {}", msg),
        }
    }
}

pub fn parse_command(input: &str) -> Result<PogCommand, String> {
    let input = input.trim();
    let parts: Vec<&str> = input.split_whitespace().collect();

    if parts.is_empty() {
        return Err("empty command".to_string());
    }

    match parts[0].to_lowercase().as_str() {
        "goto" => {
            if parts.len() != 2 {
                return Err("usage: goto <line_number>".to_string());
            }
            let line: usize = parts[1]
                .parse()
                .map_err(|_| format!("invalid line number: {}", parts[1]))?;
            if line == 0 {
                return Err("line number must be >= 1".to_string());
            }
            Ok(PogCommand::Goto { line })
        }
        "lines" => {
            if parts.len() != 1 {
                return Err("usage: lines".to_string());
            }
            Ok(PogCommand::Lines)
        }
        "top" => {
            if parts.len() != 1 {
                return Err("usage: top".to_string());
            }
            Ok(PogCommand::Top)
        }
        "size" => {
            if parts.len() != 1 {
                return Err("usage: size".to_string());
            }
            Ok(PogCommand::Size)
        }
        "mark" => {
            if parts.len() < 3 {
                return Err("usage: mark <line_number> [<start>-<end>] <color>".to_string());
            }
            let line: usize = parts[1]
                .parse()
                .map_err(|_| format!("invalid line number: {}", parts[1]))?;
            if line == 0 {
                return Err("line number must be >= 1".to_string());
            }

            // Check if parts[2] looks like a range (contains '-' and numeric on both sides)
            if let Some((start_str, end_str)) = parts[2].split_once('-') {
                if let (Ok(start), Ok(end)) = (start_str.parse::<usize>(), end_str.parse::<usize>()) {
                    // It's a region mark
                    if parts.len() < 4 {
                        return Err("usage: mark <line_number> <start>-<end> <color>".to_string());
                    }
                    if start == 0 || end == 0 {
                        return Err("column numbers must be >= 1".to_string());
                    }
                    if start >= end {
                        return Err("start column must be less than end column".to_string());
                    }
                    let color = parts[3..].join(" ");
                    return Ok(PogCommand::Mark {
                        line,
                        region: Some((start, end)),
                        color,
                    });
                }
            }
            // Fall through: it's a full-line mark
            let color = parts[2..].join(" ");
            Ok(PogCommand::Mark { line, region: None, color })
        }
        "unmark" => {
            if parts.len() < 2 {
                return Err("usage: unmark <line_number> [<start>-<end>]".to_string());
            }
            let line: usize = parts[1]
                .parse()
                .map_err(|_| format!("invalid line number: {}", parts[1]))?;
            if line == 0 {
                return Err("line number must be >= 1".to_string());
            }

            let region = if parts.len() >= 3 {
                if let Some((start_str, end_str)) = parts[2].split_once('-') {
                    if let (Ok(start), Ok(end)) = (start_str.parse::<usize>(), end_str.parse::<usize>()) {
                        if start == 0 || end == 0 {
                            return Err("column numbers must be >= 1".to_string());
                        }
                        Some((start, end))
                    } else {
                        return Err(format!("invalid range: {}", parts[2]));
                    }
                } else {
                    return Err(format!("invalid range format: {}", parts[2]));
                }
            } else {
                None
            };

            Ok(PogCommand::Unmark { line, region })
        }
        "search" => {
            if parts.len() < 2 {
                return Err("usage: search <regex_pattern>".to_string());
            }
            let pattern = parts[1..].join(" ");
            if pattern.is_empty() {
                return Err("search pattern cannot be empty".to_string());
            }
            Ok(PogCommand::Search { pattern })
        }
        "search-next" => {
            if parts.len() != 1 {
                return Err("usage: search-next".to_string());
            }
            Ok(PogCommand::SearchNext)
        }
        "search-prev" => {
            if parts.len() != 1 {
                return Err("usage: search-prev".to_string());
            }
            Ok(PogCommand::SearchPrev)
        }
        "search-clear" => {
            if parts.len() != 1 {
                return Err("usage: search-clear".to_string());
            }
            Ok(PogCommand::SearchClear)
        }
        cmd => Err(format!("unknown command: {}", cmd)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_goto() {
        assert_eq!(
            parse_command("goto 100"),
            Ok(PogCommand::Goto { line: 100 })
        );
        assert_eq!(
            parse_command("GOTO 1"),
            Ok(PogCommand::Goto { line: 1 })
        );
        assert_eq!(
            parse_command("  goto   42  "),
            Ok(PogCommand::Goto { line: 42 })
        );
    }

    #[test]
    fn test_parse_lines() {
        assert_eq!(parse_command("lines"), Ok(PogCommand::Lines));
        assert_eq!(parse_command("LINES"), Ok(PogCommand::Lines));
        assert_eq!(parse_command("  lines  "), Ok(PogCommand::Lines));
        assert!(parse_command("lines extra").is_err());
    }

    #[test]
    fn test_parse_top() {
        assert_eq!(parse_command("top"), Ok(PogCommand::Top));
        assert_eq!(parse_command("TOP"), Ok(PogCommand::Top));
        assert!(parse_command("top extra").is_err());
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_command("size"), Ok(PogCommand::Size));
        assert_eq!(parse_command("SIZE"), Ok(PogCommand::Size));
        assert!(parse_command("size extra").is_err());
    }

    #[test]
    fn test_parse_mark() {
        // Full-line marks
        assert_eq!(
            parse_command("mark 10 red"),
            Ok(PogCommand::Mark { line: 10, region: None, color: "red".to_string() })
        );
        assert_eq!(
            parse_command("MARK 5 #FF0000"),
            Ok(PogCommand::Mark { line: 5, region: None, color: "#FF0000".to_string() })
        );
        assert_eq!(
            parse_command("mark 1 light blue"),
            Ok(PogCommand::Mark { line: 1, region: None, color: "light blue".to_string() })
        );
        assert!(parse_command("mark").is_err());
        assert!(parse_command("mark 10").is_err());
        assert!(parse_command("mark abc red").is_err());
        assert!(parse_command("mark 0 red").is_err());
    }

    #[test]
    fn test_parse_mark_region() {
        // Region marks
        assert_eq!(
            parse_command("mark 10 5-20 red"),
            Ok(PogCommand::Mark { line: 10, region: Some((5, 20)), color: "red".to_string() })
        );
        assert_eq!(
            parse_command("mark 100 1-50 #FF0000"),
            Ok(PogCommand::Mark { line: 100, region: Some((1, 50)), color: "#FF0000".to_string() })
        );
        assert_eq!(
            parse_command("mark 1 10-20 light blue"),
            Ok(PogCommand::Mark { line: 1, region: Some((10, 20)), color: "light blue".to_string() })
        );
        // Error cases
        assert!(parse_command("mark 10 0-5 red").is_err());   // column 0 invalid
        assert!(parse_command("mark 10 5-0 red").is_err());   // column 0 invalid
        assert!(parse_command("mark 10 5-5 red").is_err());   // start >= end
        assert!(parse_command("mark 10 10-5 red").is_err());  // start > end
        assert!(parse_command("mark 10 5-20").is_err());      // missing color
    }

    #[test]
    fn test_parse_unmark() {
        // Full-line unmark
        assert_eq!(parse_command("unmark 10"), Ok(PogCommand::Unmark { line: 10, region: None }));
        assert_eq!(parse_command("UNMARK 1"), Ok(PogCommand::Unmark { line: 1, region: None }));
        assert!(parse_command("unmark").is_err());
        assert!(parse_command("unmark abc").is_err());
        assert!(parse_command("unmark 0").is_err());
    }

    #[test]
    fn test_parse_unmark_region() {
        // Region unmark
        assert_eq!(
            parse_command("unmark 10 5-20"),
            Ok(PogCommand::Unmark { line: 10, region: Some((5, 20)) })
        );
        assert_eq!(
            parse_command("unmark 100 1-50"),
            Ok(PogCommand::Unmark { line: 100, region: Some((1, 50)) })
        );
        // Error cases
        assert!(parse_command("unmark 10 0-5").is_err());    // column 0 invalid
        assert!(parse_command("unmark 10 abc").is_err());   // invalid range format
        assert!(parse_command("unmark 10 5").is_err());     // not a range
    }

    #[test]
    fn test_parse_errors() {
        assert!(parse_command("").is_err());
        assert!(parse_command("goto").is_err());
        assert!(parse_command("goto abc").is_err());
        assert!(parse_command("goto 0").is_err());
        assert!(parse_command("unknown 123").is_err());
    }

    #[test]
    fn test_response_format() {
        assert_eq!(format!("{}", CommandResponse::Ok(None)), "OK");
        assert_eq!(
            format!("{}", CommandResponse::Ok(Some("done".to_string()))),
            "OK done"
        );
        assert_eq!(
            format!("{}", CommandResponse::Error("failed".to_string())),
            "ERROR failed"
        );
    }

    #[test]
    fn test_parse_search() {
        assert_eq!(
            parse_command("search error"),
            Ok(PogCommand::Search { pattern: "error".to_string() })
        );
        assert_eq!(
            parse_command("SEARCH Error"),
            Ok(PogCommand::Search { pattern: "Error".to_string() })
        );
        assert_eq!(
            parse_command("search error.*warning"),
            Ok(PogCommand::Search { pattern: "error.*warning".to_string() })
        );
        assert_eq!(
            parse_command("search multiple words"),
            Ok(PogCommand::Search { pattern: "multiple words".to_string() })
        );
        assert!(parse_command("search").is_err());
    }

    #[test]
    fn test_parse_search_next() {
        assert_eq!(parse_command("search-next"), Ok(PogCommand::SearchNext));
        assert_eq!(parse_command("SEARCH-NEXT"), Ok(PogCommand::SearchNext));
        assert!(parse_command("search-next extra").is_err());
    }

    #[test]
    fn test_parse_search_prev() {
        assert_eq!(parse_command("search-prev"), Ok(PogCommand::SearchPrev));
        assert_eq!(parse_command("SEARCH-PREV"), Ok(PogCommand::SearchPrev));
        assert!(parse_command("search-prev extra").is_err());
    }

    #[test]
    fn test_parse_search_clear() {
        assert_eq!(parse_command("search-clear"), Ok(PogCommand::SearchClear));
        assert_eq!(parse_command("SEARCH-CLEAR"), Ok(PogCommand::SearchClear));
        assert!(parse_command("search-clear extra").is_err());
    }
}
