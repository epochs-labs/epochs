//! DDL parser for `.eql` migration files.

use crate::error::{ParseError, Result};
use crate::lexer::{Lexer, Token, TokenKind};

/// Field / index value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    /// UTF-8 string.
    String,
    /// Signed integer.
    Int,
    /// Boolean.
    Bool,
    /// Raw bytes (stored as-is).
    Bytes,
}

impl FieldType {
    /// Parse from keyword text.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "STRING" => Some(Self::String),
            "INT" => Some(Self::Int),
            "BOOL" => Some(Self::Bool),
            "BYTES" => Some(Self::Bytes),
            _ => None,
        }
    }
}

/// One DDL statement from a migration file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DdlStatement {
    /// `CREATE COLLECTION name KEY path TYPE [, ...]`.
    CreateCollection {
        /// Collection name.
        name: String,
        /// Primary key fields.
        keys: Vec<(String, FieldType)>,
    },
    /// `CREATE INDEX ON collection (path)` or `PATH "a.b" TYPE …`.
    CreateIndex {
        /// Collection name.
        collection: String,
        /// Field path.
        path: String,
        /// Value type.
        field_type: FieldType,
    },
    /// `DROP INDEX ON collection …`.
    DropIndex {
        /// Collection name.
        collection: String,
        /// Field path.
        path: String,
    },
}

/// Parse a full `.eql` migration file into DDL statements.
pub fn parse_migration(source: &str) -> Result<Vec<DdlStatement>> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut parser = DdlParser::new(source, tokens);
    let mut stmts = Vec::new();
    while !parser.is_eof() {
        if parser.check(&TokenKind::Semicolon) {
            parser.bump();
            continue;
        }
        stmts.push(parser.parse_statement()?);
        if parser.check(&TokenKind::Semicolon) {
            parser.bump();
        }
    }
    Ok(stmts)
}

struct DdlParser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    pos: usize,
}

impl<'a> DdlParser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn is_eof(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    fn bump(&mut self) -> Token {
        let tok = self.current().clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token> {
        if self.check(&kind) {
            Ok(self.bump())
        } else {
            Err(self.error(format!(
                "expected {kind:?}, found {:?}",
                self.current().kind
            )))
        }
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError::at(self.current().offset, message).with_location(self.source)
    }

    fn parse_statement(&mut self) -> Result<DdlStatement> {
        match &self.current().kind {
            TokenKind::Create => {
                self.bump();
                match &self.current().kind {
                    TokenKind::Collection => self.parse_create_collection(),
                    TokenKind::Index => self.parse_create_index(),
                    _ => Err(self.error("expected COLLECTION or INDEX after CREATE")),
                }
            }
            TokenKind::Drop => {
                self.bump();
                self.expect(TokenKind::Index)?;
                self.parse_drop_index()
            }
            _ => Err(self.error("expected CREATE or DROP")),
        }
    }

    fn parse_create_collection(&mut self) -> Result<DdlStatement> {
        self.expect(TokenKind::Collection)?;
        let name = self.expect_name()?;
        self.expect(TokenKind::Key)?;
        let mut keys = Vec::new();
        loop {
            let path = self.expect_path()?;
            let ty = self.expect_type()?;
            keys.push((path, ty));
            if self.check(&TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(DdlStatement::CreateCollection { name, keys })
    }

    fn parse_create_index(&mut self) -> Result<DdlStatement> {
        self.expect(TokenKind::Index)?;
        self.expect(TokenKind::On)?;
        let collection = self.expect_name()?;

        if self.check(&TokenKind::LParen) {
            self.bump();
            let path = self.expect_path()?;
            self.expect(TokenKind::RParen)?;
            // Default type STRING for shorthand CREATE INDEX ON c (path)
            Ok(DdlStatement::CreateIndex {
                collection,
                path,
                field_type: FieldType::String,
            })
        } else if self.check(&TokenKind::Path) {
            self.bump();
            let path = match &self.current().kind {
                TokenKind::String(s) => {
                    let s = s.clone();
                    self.bump();
                    s
                }
                _ => self.expect_path()?,
            };
            self.expect(TokenKind::TypeKw)?;
            let field_type = self.expect_type()?;
            Ok(DdlStatement::CreateIndex {
                collection,
                path,
                field_type,
            })
        } else {
            Err(self.error("expected (path) or PATH \"…\" TYPE …"))
        }
    }

    fn parse_drop_index(&mut self) -> Result<DdlStatement> {
        self.expect(TokenKind::On)?;
        let collection = self.expect_name()?;
        if self.check(&TokenKind::LParen) {
            self.bump();
            let path = self.expect_path()?;
            self.expect(TokenKind::RParen)?;
            Ok(DdlStatement::DropIndex { collection, path })
        } else if self.check(&TokenKind::Path) {
            self.bump();
            let path = match &self.current().kind {
                TokenKind::String(s) => {
                    let s = s.clone();
                    self.bump();
                    s
                }
                _ => self.expect_path()?,
            };
            Ok(DdlStatement::DropIndex { collection, path })
        } else {
            Err(self.error("expected (path) or PATH after DROP INDEX ON"))
        }
    }

    fn expect_name(&mut self) -> Result<String> {
        match &self.current().kind {
            TokenKind::Ident(s) => {
                let s = s.clone();
                self.bump();
                Ok(s)
            }
            TokenKind::String(s) => {
                let s = s.clone();
                self.bump();
                Ok(s)
            }
            _ if self.looks_like_name() => {
                let start = self.current().offset;
                let end = self.current().end;
                self.bump();
                Ok(self.source[start..end].to_string())
            }
            _ => Err(self.error("expected identifier")),
        }
    }

    fn looks_like_name(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Ident(_)
                | TokenKind::Collection
                | TokenKind::Index
                | TokenKind::Key
                | TokenKind::Path
        )
    }

    fn expect_path(&mut self) -> Result<String> {
        let mut parts = Vec::new();
        parts.push(self.expect_name()?);
        while self.check(&TokenKind::Dot) {
            self.bump();
            parts.push(self.expect_name()?);
        }
        Ok(parts.join("."))
    }

    fn expect_type(&mut self) -> Result<FieldType> {
        let ty = match &self.current().kind {
            TokenKind::TyString => FieldType::String,
            TokenKind::TyInt => FieldType::Int,
            TokenKind::TyBool => FieldType::Bool,
            TokenKind::TyBytes => FieldType::Bytes,
            TokenKind::Ident(s) => {
                FieldType::parse(s).ok_or_else(|| self.error(format!("unknown type '{s}'")))?
            }
            _ => return Err(self.error("expected STRING, INT, BOOL, or BYTES")),
        };
        self.bump();
        Ok(ty)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parses_collection_and_indexes() {
        let stmts = parse_migration(
            r#"
            CREATE COLLECTION items KEY id STRING;
            CREATE INDEX ON items (id);
            CREATE INDEX ON items PATH "meta.prefs.theme" TYPE STRING;
            DROP INDEX ON items PATH "meta.prefs.theme";
            "#,
        )
        .unwrap();
        assert_eq!(stmts.len(), 4);
        assert!(matches!(stmts[0], DdlStatement::CreateCollection { .. }));
        assert!(matches!(
            &stmts[1],
            DdlStatement::CreateIndex { path, .. } if path == "id"
        ));
        assert!(matches!(
            &stmts[2],
            DdlStatement::CreateIndex { path, .. } if path == "meta.prefs.theme"
        ));
        assert!(matches!(stmts[3], DdlStatement::DropIndex { .. }));
    }
}
