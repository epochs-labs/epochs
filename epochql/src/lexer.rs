//! EpochQL lexer — hand-written tokenizer (no external lexer crates).

use crate::error::{ParseError, Result};

/// Lexical token kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords (case-insensitive in source; stored as uppercase variants)
    /// `USE`
    Use,
    /// `BRANCH`
    Branch,
    /// `COMMIT`
    Commit,
    /// `TAG`
    Tag,
    /// `HEAD`
    Head,
    /// `MATCH`
    Match,
    /// `TRAVERSE`
    Traverse,
    /// `MERGE_BASE`
    MergeBase,
    /// `SHORTEST_PATH`
    ShortestPath,
    /// `ANCESTORS`
    Ancestors,
    /// `WHERE`
    Where,
    /// `SELECT`
    Select,
    /// `AS`
    As,
    /// `CREATE`
    Create,
    /// `DELETE`
    Delete,
    /// `FROM`
    From,
    /// `CHECKOUT`
    Checkout,
    /// `MERGE`
    Merge,
    /// `INTO`
    Into,
    /// `STRATEGY`
    Strategy,
    /// `FAST_FORWARD`
    FastForward,
    /// `THREE_WAY`
    ThreeWay,
    /// `SQUASH`
    Squash,
    /// `DIFF`
    Diff,
    /// `AND`
    And,
    /// `OR`
    Or,
    /// `MESSAGE`
    Message,
    /// `PARENTS`
    Parents,
    /// `PATH`
    Path,
    /// `PARENT` (edge type)
    Parent,
    /// `CHILD` (edge type)
    Child,
    /// `ANCESTOR` (edge type)
    Ancestor,
    /// `DESCENDANT` (edge type)
    Descendant,
    /// `NULL`
    Null,
    /// `TRUE`
    True,
    /// `FALSE`
    False,
    /// `COLLECTION`
    Collection,
    /// `INDEX`
    Index,
    /// `KEY`
    Key,
    /// `DROP`
    Drop,
    /// `ON`
    On,
    /// `TYPE` (DDL)
    TypeKw,
    /// `STRING` type
    TyString,
    /// `INT` type
    TyInt,
    /// `BOOL` type
    TyBool,
    /// `BYTES` type
    TyBytes,

    // Identifiers & literals
    /// Unquoted identifier.
    Ident(String),
    /// Double-quoted string literal.
    String(String),
    /// Integer literal.
    Int(i64),
    /// Floating-point literal.
    Float(f64),

    // Symbols
    /// `;`
    Semicolon,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `:`
    Colon,
    /// `=`
    Eq,
    /// `!=`
    Neq,
    /// `*`
    Star,
    /// `..`
    DotDot,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `->`
    ArrowRight,
    /// `<-`
    ArrowLeft,
    /// `-`
    Minus,

    /// End of input.
    Eof,
}

/// A token with its byte span in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Token kind.
    pub kind: TokenKind,
    /// Byte offset of the first character.
    pub offset: usize,
    /// Byte offset just past the last character.
    pub end: usize,
}

/// Tokenizer over an EpochQL source string.
pub struct Lexer<'a> {
    source: &'a str,
    chars: Vec<(usize, char)>,
    pos: usize,
}

impl<'a> Lexer<'a> {
    /// Create a lexer over `source`.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.char_indices().collect(),
            pos: 0,
        }
    }

    /// Tokenize the entire source into a vector ending with [`TokenKind::Eof`].
    pub fn tokenize(mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn peek(&self) -> Option<(usize, char)> {
        self.chars.get(self.pos).copied()
    }

    fn peek_ahead(&self, n: usize) -> Option<(usize, char)> {
        self.chars.get(self.pos + n).copied()
    }

    fn bump(&mut self) -> Option<(usize, char)> {
        let item = self.chars.get(self.pos).copied();
        if item.is_some() {
            self.pos += 1;
        }
        item
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while let Some((_, ch)) = self.peek() {
                if ch.is_whitespace() {
                    self.bump();
                } else {
                    break;
                }
            }

            // Line comments: -- ...
            if self.peek().map(|(_, c)| c) == Some('-')
                && self.peek_ahead(1).map(|(_, c)| c) == Some('-')
            {
                self.bump();
                self.bump();
                while let Some((_, ch)) = self.peek() {
                    self.bump();
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }

            break;
        }
    }

    fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace_and_comments();

        let Some((offset, ch)) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                offset: self.source.len(),
                end: self.source.len(),
            });
        };

        let token = match ch {
            ';' => {
                self.bump();
                TokenKind::Semicolon
            }
            ',' => {
                self.bump();
                TokenKind::Comma
            }
            ':' => {
                self.bump();
                TokenKind::Colon
            }
            '*' => {
                self.bump();
                TokenKind::Star
            }
            '(' => {
                self.bump();
                TokenKind::LParen
            }
            ')' => {
                self.bump();
                TokenKind::RParen
            }
            '{' => {
                self.bump();
                TokenKind::LBrace
            }
            '}' => {
                self.bump();
                TokenKind::RBrace
            }
            '[' => {
                self.bump();
                TokenKind::LBracket
            }
            ']' => {
                self.bump();
                TokenKind::RBracket
            }
            '=' => {
                self.bump();
                TokenKind::Eq
            }
            '!' => {
                self.bump();
                if self.peek().map(|(_, c)| c) == Some('=') {
                    self.bump();
                    TokenKind::Neq
                } else {
                    return Err(
                        ParseError::at(offset, "expected '=' after '!'").with_location(self.source)
                    );
                }
            }
            '.' => {
                self.bump();
                if self.peek().map(|(_, c)| c) == Some('.') {
                    self.bump();
                    TokenKind::DotDot
                } else {
                    TokenKind::Dot
                }
            }
            '-' => {
                self.bump();
                if self.peek().map(|(_, c)| c) == Some('>') {
                    self.bump();
                    TokenKind::ArrowRight
                } else {
                    TokenKind::Minus
                }
            }
            '<' => {
                self.bump();
                if self.peek().map(|(_, c)| c) == Some('-') {
                    self.bump();
                    TokenKind::ArrowLeft
                } else {
                    return Err(
                        ParseError::at(offset, "expected '-' after '<'").with_location(self.source)
                    );
                }
            }
            '"' => return self.lex_string(offset),
            c if c.is_ascii_digit() => return self.lex_number(offset),
            c if c.is_ascii_alphabetic() || c == '_' => return self.lex_ident_or_keyword(offset),
            _ => {
                return Err(
                    ParseError::at(offset, format!("unexpected character '{ch}'"))
                        .with_location(self.source),
                );
            }
        };

        Ok(Token {
            kind: token,
            offset,
            end: self.current_byte_offset(),
        })
    }

    fn current_byte_offset(&self) -> usize {
        self.chars
            .get(self.pos)
            .map(|(i, _)| *i)
            .unwrap_or(self.source.len())
    }

    fn lex_string(&mut self, offset: usize) -> Result<Token> {
        self.bump(); // opening "
        let mut value = String::new();
        loop {
            match self.bump() {
                None => {
                    return Err(ParseError::at(offset, "unterminated string literal")
                        .with_location(self.source));
                }
                Some((_, '"')) => break,
                Some((_, '\\')) => match self.bump() {
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 't')) => value.push('\t'),
                    Some((_, 'r')) => value.push('\r'),
                    Some((_, '"')) => value.push('"'),
                    Some((_, '\\')) => value.push('\\'),
                    Some((_, c)) => value.push(c),
                    None => {
                        return Err(ParseError::at(offset, "unterminated escape in string")
                            .with_location(self.source));
                    }
                },
                Some((_, c)) => value.push(c),
            }
        }
        Ok(Token {
            kind: TokenKind::String(value),
            offset,
            end: self.current_byte_offset(),
        })
    }

    fn lex_number(&mut self, offset: usize) -> Result<Token> {
        let start = offset;
        while self.peek().is_some_and(|(_, c)| c.is_ascii_digit()) {
            self.bump();
        }

        let is_float = self.peek().map(|(_, c)| c) == Some('.')
            && self.peek_ahead(1).is_some_and(|(_, c)| c.is_ascii_digit());

        if is_float {
            self.bump(); // .
            while self.peek().is_some_and(|(_, c)| c.is_ascii_digit()) {
                self.bump();
            }
            let end = self.current_byte_offset();
            let text = &self.source[start..end];
            let value: f64 = text.parse().map_err(|_| {
                ParseError::at(offset, format!("invalid float literal '{text}'"))
                    .with_location(self.source)
            })?;
            Ok(Token {
                kind: TokenKind::Float(value),
                offset,
                end,
            })
        } else {
            let end = self.current_byte_offset();
            let text = &self.source[start..end];
            let value: i64 = text.parse().map_err(|_| {
                ParseError::at(offset, format!("invalid integer literal '{text}'"))
                    .with_location(self.source)
            })?;
            Ok(Token {
                kind: TokenKind::Int(value),
                offset,
                end,
            })
        }
    }

    fn lex_ident_or_keyword(&mut self, offset: usize) -> Result<Token> {
        let start = offset;
        while self
            .peek()
            .is_some_and(|(_, c)| c.is_ascii_alphanumeric() || c == '_')
        {
            self.bump();
        }
        let end = self.current_byte_offset();
        let text = &self.source[start..end];
        let kind = keyword_or_ident(text);
        Ok(Token { kind, offset, end })
    }
}

fn keyword_or_ident(text: &str) -> TokenKind {
    match text.to_ascii_uppercase().as_str() {
        "USE" => TokenKind::Use,
        "BRANCH" => TokenKind::Branch,
        "COMMIT" => TokenKind::Commit,
        "TAG" => TokenKind::Tag,
        "HEAD" => TokenKind::Head,
        "MATCH" => TokenKind::Match,
        "TRAVERSE" => TokenKind::Traverse,
        "MERGE_BASE" => TokenKind::MergeBase,
        "SHORTEST_PATH" => TokenKind::ShortestPath,
        "ANCESTORS" => TokenKind::Ancestors,
        "WHERE" => TokenKind::Where,
        "SELECT" => TokenKind::Select,
        "AS" => TokenKind::As,
        "CREATE" => TokenKind::Create,
        "DELETE" => TokenKind::Delete,
        "FROM" => TokenKind::From,
        "CHECKOUT" => TokenKind::Checkout,
        "MERGE" => TokenKind::Merge,
        "INTO" => TokenKind::Into,
        "STRATEGY" => TokenKind::Strategy,
        "FAST_FORWARD" => TokenKind::FastForward,
        "THREE_WAY" => TokenKind::ThreeWay,
        "SQUASH" => TokenKind::Squash,
        "DIFF" => TokenKind::Diff,
        "AND" => TokenKind::And,
        "OR" => TokenKind::Or,
        "MESSAGE" => TokenKind::Message,
        "PARENTS" => TokenKind::Parents,
        "PATH" => TokenKind::Path,
        "PARENT" => TokenKind::Parent,
        "CHILD" => TokenKind::Child,
        "ANCESTOR" => TokenKind::Ancestor,
        "DESCENDANT" => TokenKind::Descendant,
        "NULL" => TokenKind::Null,
        "TRUE" => TokenKind::True,
        "FALSE" => TokenKind::False,
        "COLLECTION" => TokenKind::Collection,
        "INDEX" => TokenKind::Index,
        "KEY" => TokenKind::Key,
        "DROP" => TokenKind::Drop,
        "ON" => TokenKind::On,
        "TYPE" => TokenKind::TypeKw,
        "STRING" => TokenKind::TyString,
        "INT" => TokenKind::TyInt,
        "BOOL" => TokenKind::TyBool,
        "BYTES" => TokenKind::TyBytes,
        _ => TokenKind::Ident(text.to_string()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn lexes_keywords_and_arrows() {
        let tokens = Lexer::new("USE BRANCH main MATCH (a)->(b);")
            .tokenize()
            .unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Use));
        assert!(matches!(tokens[1].kind, TokenKind::Branch));
        assert!(matches!(tokens[3].kind, TokenKind::Match));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::ArrowRight));
    }

    #[test]
    fn lexes_string_and_skips_comment() {
        let tokens = Lexer::new("-- comment\nCOMMIT { x: \"hi\" };")
            .tokenize()
            .unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Commit));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::String(ref s) if s == "hi")));
    }
}
