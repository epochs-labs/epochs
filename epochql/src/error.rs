//! EpochQL error types.

use std::fmt;

/// Result type for EpochQL parsing.
pub type Result<T> = std::result::Result<T, ParseError>;

/// A parse error with optional source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable message.
    pub message: String,
    /// 0-based byte offset into the source, if known.
    pub offset: Option<usize>,
    /// 1-based line number, if known.
    pub line: Option<usize>,
    /// 1-based column number, if known.
    pub column: Option<usize>,
}

impl ParseError {
    /// Create an error without location.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: None,
            line: None,
            column: None,
        }
    }

    /// Create an error at a specific byte offset (line/column derived later).
    pub fn at(offset: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: Some(offset),
            line: None,
            column: None,
        }
    }

    /// Attach line/column derived from source.
    pub fn with_location(mut self, source: &str) -> Self {
        if let Some(offset) = self.offset {
            let (line, column) = offset_to_line_col(source, offset);
            self.line = Some(line);
            self.column = Some(column);
        }
        self
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(col)) => write!(f, "parse error at {line}:{col}: {}", self.message),
            _ => write!(f, "parse error: {}", self.message),
        }
    }
}

impl std::error::Error for ParseError {}

fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
