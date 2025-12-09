use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum PogCommand {
    Goto { line: usize },
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
