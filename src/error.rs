use std::fmt;
use std::io;

#[derive(Debug)]
pub enum PogError {
    Io(io::Error),
    Ssh { host: String, message: String },
    Utf8(std::string::FromUtf8Error),
    #[allow(dead_code)]
    ConnectionFailed { host: String },
    FileNotFound { path: String },
    PermissionDenied { path: String },
}

impl std::error::Error for PogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PogError::Io(e) => Some(e),
            PogError::Utf8(e) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for PogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PogError::Io(e) => write!(f, "I/O error: {}", e),
            PogError::Ssh { host, message } => {
                write!(f, "SSH error connecting to {}: {}", host, message)
            }
            PogError::Utf8(e) => write!(f, "UTF-8 error: {}", e),
            PogError::ConnectionFailed { host } => {
                write!(f, "Failed to connect to {}", host)
            }
            PogError::FileNotFound { path } => write!(f, "File not found: {}", path),
            PogError::PermissionDenied { path } => write!(f, "Permission denied: {}", path),
        }
    }
}

impl From<io::Error> for PogError {
    fn from(err: io::Error) -> Self {
        PogError::Io(err)
    }
}

impl From<std::string::FromUtf8Error> for PogError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        PogError::Utf8(err)
    }
}

pub type Result<T> = std::result::Result<T, PogError>;
