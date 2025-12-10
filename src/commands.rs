use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum PogCommand {
    Goto { line: usize },
    Lines,
    Top,
    Size,
    Mark { line: usize, color: String },
    Unmark { line: usize },
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
                return Err("usage: mark <line_number> <color>".to_string());
            }
            let line: usize = parts[1]
                .parse()
                .map_err(|_| format!("invalid line number: {}", parts[1]))?;
            if line == 0 {
                return Err("line number must be >= 1".to_string());
            }
            let color = parts[2..].join(" ");
            Ok(PogCommand::Mark { line, color })
        }
        "unmark" => {
            if parts.len() != 2 {
                return Err("usage: unmark <line_number>".to_string());
            }
            let line: usize = parts[1]
                .parse()
                .map_err(|_| format!("invalid line number: {}", parts[1]))?;
            if line == 0 {
                return Err("line number must be >= 1".to_string());
            }
            Ok(PogCommand::Unmark { line })
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
        assert_eq!(
            parse_command("mark 10 red"),
            Ok(PogCommand::Mark { line: 10, color: "red".to_string() })
        );
        assert_eq!(
            parse_command("MARK 5 #FF0000"),
            Ok(PogCommand::Mark { line: 5, color: "#FF0000".to_string() })
        );
        assert_eq!(
            parse_command("mark 1 light blue"),
            Ok(PogCommand::Mark { line: 1, color: "light blue".to_string() })
        );
        assert!(parse_command("mark").is_err());
        assert!(parse_command("mark 10").is_err());
        assert!(parse_command("mark abc red").is_err());
        assert!(parse_command("mark 0 red").is_err());
    }

    #[test]
    fn test_parse_unmark() {
        assert_eq!(parse_command("unmark 10"), Ok(PogCommand::Unmark { line: 10 }));
        assert_eq!(parse_command("UNMARK 1"), Ok(PogCommand::Unmark { line: 1 }));
        assert!(parse_command("unmark").is_err());
        assert!(parse_command("unmark abc").is_err());
        assert!(parse_command("unmark 0").is_err());
        assert!(parse_command("unmark 10 extra").is_err());
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
}
