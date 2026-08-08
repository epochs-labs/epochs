//! EpochQL recursive-descent parser (grammar v1.0).
//!
//! Hand-written over the [`crate::lexer`] — no `nom`/`pest`. Public entry
//! points: [`parse`] (single statement) and [`parse_script`] (multiple).

use std::collections::BTreeMap;

use crate::ast::*;
use crate::error::{ParseError, Result};
use crate::lexer::{Lexer, Token, TokenKind};

/// Parse a single EpochQL statement (optional trailing `;`).
pub fn parse(source: &str) -> Result<Statement> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut parser = Parser::new(source, tokens);
    let stmt = parser.parse_statement()?;
    parser.expect_eof_or_semicolon()?;
    Ok(stmt)
}

/// Parse one or more statements separated by `;`.
pub fn parse_script(source: &str) -> Result<Vec<Statement>> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut parser = Parser::new(source, tokens);
    let mut stmts = Vec::new();

    while !parser.is_eof() {
        stmts.push(parser.parse_statement()?);
        if parser.check(&TokenKind::Semicolon) {
            parser.bump();
        }
        // Allow trailing whitespace / eof
        if parser.is_eof() {
            break;
        }
        // If next token starts a new statement, continue; otherwise error
        if !parser.can_start_statement() {
            return Err(parser.error("expected statement or end of input"));
        }
    }

    Ok(stmts)
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    pos: usize,
}

impl<'a> Parser<'a> {
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

    fn looks_like_ident(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Ident(_)
                | TokenKind::Use
                | TokenKind::Branch
                | TokenKind::Commit
                | TokenKind::Tag
                | TokenKind::Head
                | TokenKind::Match
                | TokenKind::Traverse
                | TokenKind::MergeBase
                | TokenKind::ShortestPath
                | TokenKind::Ancestors
                | TokenKind::Where
                | TokenKind::Select
                | TokenKind::As
                | TokenKind::Create
                | TokenKind::Delete
                | TokenKind::From
                | TokenKind::Checkout
                | TokenKind::Merge
                | TokenKind::Into
                | TokenKind::Strategy
                | TokenKind::FastForward
                | TokenKind::ThreeWay
                | TokenKind::Squash
                | TokenKind::Diff
                | TokenKind::And
                | TokenKind::Or
                | TokenKind::Message
                | TokenKind::Parents
                | TokenKind::Path
                | TokenKind::Parent
                | TokenKind::Child
                | TokenKind::Ancestor
                | TokenKind::Descendant
                | TokenKind::Null
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Collection
                | TokenKind::Index
                | TokenKind::Key
                | TokenKind::Drop
                | TokenKind::On
                | TokenKind::TypeKw
                | TokenKind::TyString
                | TokenKind::TyInt
                | TokenKind::TyBool
                | TokenKind::TyBytes
        )
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

    fn expect_eof_or_semicolon(&mut self) -> Result<()> {
        if self.check(&TokenKind::Semicolon) {
            self.bump();
        }
        if !self.is_eof() {
            return Err(self.error("unexpected tokens after statement"));
        }
        Ok(())
    }

    fn can_start_statement(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Use
                | TokenKind::Match
                | TokenKind::Traverse
                | TokenKind::Where
                | TokenKind::Select
                | TokenKind::Commit
                | TokenKind::Create
                | TokenKind::Delete
                | TokenKind::Checkout
                | TokenKind::Merge
                | TokenKind::Diff
        )
    }

    fn parse_statement(&mut self) -> Result<Statement> {
        match &self.current().kind {
            TokenKind::Commit
            | TokenKind::Create
            | TokenKind::Delete
            | TokenKind::Checkout
            | TokenKind::Merge
            | TokenKind::Diff => Ok(Statement::Version(self.parse_version_statement()?)),
            _ => Ok(Statement::Query(self.parse_query_statement()?)),
        }
    }

    // -------------------------------------------------------------------------
    // Version statements
    // -------------------------------------------------------------------------

    fn parse_version_statement(&mut self) -> Result<VersionStatement> {
        match &self.current().kind {
            TokenKind::Commit => Ok(VersionStatement::Commit(self.parse_commit_stmt()?)),
            TokenKind::Create | TokenKind::Delete => {
                Ok(VersionStatement::Branch(self.parse_branch_stmt()?))
            }
            TokenKind::Checkout => {
                self.bump();
                Ok(VersionStatement::Checkout(self.parse_target_ref()?))
            }
            TokenKind::Merge => Ok(VersionStatement::Merge(self.parse_merge_stmt()?)),
            TokenKind::Diff => Ok(VersionStatement::Diff(self.parse_diff_stmt()?)),
            _ => Err(self.error("expected version statement")),
        }
    }

    fn parse_commit_stmt(&mut self) -> Result<CommitStmt> {
        self.expect(TokenKind::Commit)?;
        let payload = self.parse_map_literal()?;

        let mut message = None;
        let mut parents = None;

        loop {
            match &self.current().kind {
                TokenKind::Message => {
                    self.bump();
                    message = Some(self.expect_string()?);
                }
                TokenKind::Parents => {
                    self.bump();
                    self.expect(TokenKind::LBracket)?;
                    let mut list = Vec::new();
                    if !self.check(&TokenKind::RBracket) {
                        loop {
                            list.push(self.parse_target_ref()?);
                            if self.check(&TokenKind::Comma) {
                                self.bump();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(TokenKind::RBracket)?;
                    parents = Some(list);
                }
                _ => break,
            }
        }

        Ok(CommitStmt {
            payload,
            message,
            parents,
        })
    }

    fn parse_branch_stmt(&mut self) -> Result<BranchStmt> {
        match &self.current().kind {
            TokenKind::Create => {
                self.bump();
                self.expect(TokenKind::Branch)?;
                let name = self.expect_name()?;
                let from = if self.check(&TokenKind::From) {
                    self.bump();
                    Some(self.parse_target_ref()?)
                } else {
                    None
                };
                Ok(BranchStmt::Create { name, from })
            }
            TokenKind::Delete => {
                self.bump();
                self.expect(TokenKind::Branch)?;
                let name = self.expect_name()?;
                Ok(BranchStmt::Delete { name })
            }
            _ => Err(self.error("expected CREATE or DELETE BRANCH")),
        }
    }

    fn parse_merge_stmt(&mut self) -> Result<MergeStmt> {
        self.expect(TokenKind::Merge)?;
        let source = self.parse_target_ref()?;
        self.expect(TokenKind::Into)?;
        let into = self.parse_target_ref()?;
        let strategy = if self.check(&TokenKind::Strategy) {
            self.bump();
            match &self.current().kind {
                TokenKind::FastForward => {
                    self.bump();
                    MergeStrategy::FastForward
                }
                TokenKind::ThreeWay => {
                    self.bump();
                    MergeStrategy::ThreeWay
                }
                TokenKind::Squash => {
                    self.bump();
                    MergeStrategy::Squash
                }
                _ => {
                    return Err(
                        self.error("expected FAST_FORWARD, THREE_WAY, or SQUASH merge strategy")
                    );
                }
            }
        } else {
            MergeStrategy::FastForward
        };
        Ok(MergeStmt {
            source,
            into,
            strategy,
        })
    }

    fn parse_diff_stmt(&mut self) -> Result<DiffStmt> {
        self.expect(TokenKind::Diff)?;
        let left = self.parse_target_ref()?;
        self.expect(TokenKind::And)?;
        let right = self.parse_target_ref()?;
        let path = if self.check(&TokenKind::Path) {
            self.bump();
            Some(self.expect_string()?)
        } else {
            None
        };
        Ok(DiffStmt { left, right, path })
    }

    // -------------------------------------------------------------------------
    // Query statements
    // -------------------------------------------------------------------------

    fn parse_query_statement(&mut self) -> Result<QueryStatement> {
        let mut stmt = QueryStatement::default();

        if self.check(&TokenKind::Use) {
            self.bump();
            stmt.context = Some(ContextClause {
                target: self.parse_target_ref()?,
            });
        }

        if self.check(&TokenKind::Match) {
            stmt.match_clause = Some(self.parse_match_clause()?);
        }

        if self.check(&TokenKind::Traverse) {
            stmt.traversal = Some(self.parse_traversal_clause()?);
        }

        if self.check(&TokenKind::Where) {
            self.bump();
            stmt.where_clause = Some(self.parse_expression()?);
        }

        if self.check(&TokenKind::Select) {
            stmt.select = Some(self.parse_select_clause()?);
        }

        // A query must have at least one clause
        if stmt.context.is_none()
            && stmt.match_clause.is_none()
            && stmt.traversal.is_none()
            && stmt.where_clause.is_none()
            && stmt.select.is_none()
        {
            return Err(
                self.error("expected query clause (USE, MATCH, TRAVERSE, WHERE, or SELECT)")
            );
        }

        Ok(stmt)
    }

    fn parse_match_clause(&mut self) -> Result<MatchClause> {
        self.expect(TokenKind::Match)?;
        let mut patterns = Vec::new();
        patterns.push(self.parse_pattern()?);
        while self.check(&TokenKind::Comma) {
            self.bump();
            patterns.push(self.parse_pattern()?);
        }
        Ok(MatchClause { patterns })
    }

    fn parse_pattern(&mut self) -> Result<Pattern> {
        let mut elements = Vec::new();
        elements.push(PatternElement::Node(self.parse_node_pattern()?));

        while self.is_edge_start() {
            elements.push(PatternElement::Edge(self.parse_edge_pattern()?));
            elements.push(PatternElement::Node(self.parse_node_pattern()?));
        }

        Ok(Pattern { elements })
    }

    fn is_edge_start(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::ArrowRight | TokenKind::ArrowLeft | TokenKind::Minus
        )
    }

    fn parse_node_pattern(&mut self) -> Result<NodePattern> {
        self.expect(TokenKind::LParen)?;

        let mut variable = None;
        let mut label = None;
        let mut properties = BTreeMap::new();

        // Optional variable (may be a keyword used as a name, e.g. `child`)
        if self.looks_like_ident() {
            variable = Some(self.expect_ident()?);
        }

        // Optional :Label
        if self.check(&TokenKind::Colon) {
            self.bump();
            label = Some(self.expect_ident_or_keyword_as_ident()?);
        }

        // Optional { props }
        if self.check(&TokenKind::LBrace) {
            properties = self.parse_map_literal()?;
        }

        self.expect(TokenKind::RParen)?;

        Ok(NodePattern {
            variable,
            label,
            properties,
        })
    }

    fn parse_edge_pattern(&mut self) -> Result<EdgePattern> {
        // Forms:
        //   ->
        //   <-
        //   -[ TYPE? MULTI? ]->
        //   <-[ TYPE? MULTI? ]-

        match &self.current().kind {
            TokenKind::ArrowRight => {
                self.bump();
                Ok(EdgePattern {
                    direction: EdgeDirection::Outgoing,
                    edge_type: EdgeType::Child,
                    multiplier: None,
                })
            }
            TokenKind::ArrowLeft => {
                self.bump();
                // Could be bare `<-` or start of `<-[ ... ]-`
                if self.check(&TokenKind::LBracket) {
                    self.parse_bracket_edge(EdgeDirection::Incoming)
                } else {
                    Ok(EdgePattern {
                        direction: EdgeDirection::Incoming,
                        edge_type: EdgeType::Parent,
                        multiplier: None,
                    })
                }
            }
            TokenKind::Minus => {
                self.bump();
                self.expect(TokenKind::LBracket)?;
                let (edge_type, multiplier) = self.parse_edge_body()?;
                self.expect(TokenKind::RBracket)?;
                self.expect(TokenKind::ArrowRight)?;
                Ok(EdgePattern {
                    direction: EdgeDirection::Outgoing,
                    edge_type: edge_type.unwrap_or(EdgeType::Child),
                    multiplier,
                })
            }
            _ => Err(self.error("expected edge pattern (->, <-, -[...]->, or <-[...]-)")),
        }
    }

    fn parse_bracket_edge(&mut self, direction: EdgeDirection) -> Result<EdgePattern> {
        self.expect(TokenKind::LBracket)?;
        let (edge_type, multiplier) = self.parse_edge_body()?;
        self.expect(TokenKind::RBracket)?;

        match direction {
            EdgeDirection::Incoming => {
                // after `<-[ ... ]` expect `-`
                self.expect(TokenKind::Minus)?;
                Ok(EdgePattern {
                    direction,
                    edge_type: edge_type.unwrap_or(EdgeType::Parent),
                    multiplier,
                })
            }
            EdgeDirection::Outgoing => {
                self.expect(TokenKind::ArrowRight)?;
                Ok(EdgePattern {
                    direction,
                    edge_type: edge_type.unwrap_or(EdgeType::Child),
                    multiplier,
                })
            }
        }
    }

    fn parse_edge_body(&mut self) -> Result<(Option<EdgeType>, Option<HopMultiplier>)> {
        let mut edge_type = None;

        // Optional :TYPE or bare TYPE keyword
        if self.check(&TokenKind::Colon) {
            self.bump();
            edge_type = Some(self.parse_edge_type()?);
        } else if matches!(
            self.current().kind,
            TokenKind::Parent | TokenKind::Child | TokenKind::Ancestor | TokenKind::Descendant
        ) {
            edge_type = Some(self.parse_edge_type()?);
        }

        let multiplier = if self.check(&TokenKind::Star) {
            Some(self.parse_hop_multiplier()?)
        } else {
            None
        };

        Ok((edge_type, multiplier))
    }

    fn parse_edge_type(&mut self) -> Result<EdgeType> {
        match &self.current().kind {
            TokenKind::Parent => {
                self.bump();
                Ok(EdgeType::Parent)
            }
            TokenKind::Child => {
                self.bump();
                Ok(EdgeType::Child)
            }
            TokenKind::Ancestor => {
                self.bump();
                Ok(EdgeType::Ancestor)
            }
            TokenKind::Descendant => {
                self.bump();
                Ok(EdgeType::Descendant)
            }
            _ => Err(self.error("expected PARENT, CHILD, ANCESTOR, or DESCENDANT")),
        }
    }

    fn parse_hop_multiplier(&mut self) -> Result<HopMultiplier> {
        self.expect(TokenKind::Star)?;

        // * | *N | *N..M | *..M | *N..
        if matches!(self.current().kind, TokenKind::Int(_)) {
            let n = self.expect_u32()?;
            if self.check(&TokenKind::DotDot) {
                self.bump();
                if matches!(self.current().kind, TokenKind::Int(_)) {
                    let m = self.expect_u32()?;
                    Ok(HopMultiplier::Range {
                        min: Some(n),
                        max: Some(m),
                    })
                } else {
                    Ok(HopMultiplier::Range {
                        min: Some(n),
                        max: None,
                    })
                }
            } else {
                Ok(HopMultiplier::Exact(n))
            }
        } else if self.check(&TokenKind::DotDot) {
            self.bump();
            if matches!(self.current().kind, TokenKind::Int(_)) {
                let m = self.expect_u32()?;
                Ok(HopMultiplier::Range {
                    min: None,
                    max: Some(m),
                })
            } else {
                Ok(HopMultiplier::Any)
            }
        } else {
            Ok(HopMultiplier::Any)
        }
    }

    fn parse_traversal_clause(&mut self) -> Result<TraversalClause> {
        self.expect(TokenKind::Traverse)?;
        match &self.current().kind {
            TokenKind::MergeBase => {
                self.bump();
                self.expect(TokenKind::LParen)?;
                let a = self.parse_target_ref()?;
                self.expect(TokenKind::Comma)?;
                let b = self.parse_target_ref()?;
                self.expect(TokenKind::RParen)?;
                Ok(TraversalClause::MergeBase(a, b))
            }
            TokenKind::ShortestPath => {
                self.bump();
                self.expect(TokenKind::LParen)?;
                let a = self.parse_target_ref()?;
                self.expect(TokenKind::Comma)?;
                let b = self.parse_target_ref()?;
                self.expect(TokenKind::RParen)?;
                Ok(TraversalClause::ShortestPath(a, b))
            }
            TokenKind::Ancestors => {
                self.bump();
                self.expect(TokenKind::LParen)?;
                let a = self.parse_target_ref()?;
                self.expect(TokenKind::RParen)?;
                Ok(TraversalClause::Ancestors(a))
            }
            _ => Err(self.error("expected MERGE_BASE, SHORTEST_PATH, or ANCESTORS after TRAVERSE")),
        }
    }

    fn parse_select_clause(&mut self) -> Result<SelectClause> {
        self.expect(TokenKind::Select)?;
        let mut items = Vec::new();
        items.push(self.parse_projection_item()?);
        while self.check(&TokenKind::Comma) {
            self.bump();
            items.push(self.parse_projection_item()?);
        }
        Ok(SelectClause { items })
    }

    fn parse_projection_item(&mut self) -> Result<ProjectionItem> {
        let expression = self.parse_expression()?;
        let alias = if self.check(&TokenKind::As) {
            self.bump();
            Some(self.expect_ident()?)
        } else {
            None
        };
        Ok(ProjectionItem { expression, alias })
    }

    // -------------------------------------------------------------------------
    // Target refs & expressions
    // -------------------------------------------------------------------------

    fn parse_target_ref(&mut self) -> Result<TargetRef> {
        match &self.current().kind {
            TokenKind::Head => {
                self.bump();
                Ok(TargetRef::Head)
            }
            TokenKind::Branch => {
                self.bump();
                // BRANCH ident | BRANCH "name"
                let name = match &self.current().kind {
                    TokenKind::Ident(_) => self.expect_ident()?,
                    TokenKind::String(_) => self.expect_string()?,
                    _ => {
                        return Err(self.error("expected branch name after BRANCH"));
                    }
                };
                Ok(TargetRef::Branch(name))
            }
            TokenKind::Commit => {
                self.bump();
                let hex = self.expect_string()?;
                validate_hex_hash(&hex).map_err(|m| self.error(m))?;
                Ok(TargetRef::Commit(hex))
            }
            TokenKind::Tag => {
                self.bump();
                let name = match &self.current().kind {
                    TokenKind::Ident(_) => self.expect_ident()?,
                    TokenKind::String(_) => self.expect_string()?,
                    _ => return Err(self.error("expected tag name after TAG")),
                };
                Ok(TargetRef::Tag(name))
            }
            _ => Err(self.error("expected HEAD, BRANCH, COMMIT, or TAG")),
        }
    }

    fn parse_expression(&mut self) -> Result<Expression> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expression> {
        let mut left = self.parse_and()?;
        while self.check(&TokenKind::Or) {
            self.bump();
            let right = self.parse_and()?;
            left = Expression::Binary {
                left: Box::new(left),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expression> {
        let mut left = self.parse_equality()?;
        while self.check(&TokenKind::And) {
            self.bump();
            let right = self.parse_equality()?;
            left = Expression::Binary {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expression> {
        let mut left = self.parse_postfix()?;
        loop {
            let op = match &self.current().kind {
                TokenKind::Eq => BinaryOp::Eq,
                TokenKind::Neq => BinaryOp::Neq,
                _ => break,
            };
            self.bump();
            let right = self.parse_postfix()?;
            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_postfix(&mut self) -> Result<Expression> {
        let mut expr = self.parse_primary()?;
        while self.check(&TokenKind::Dot) {
            self.bump();
            let property = self.expect_ident_or_keyword_as_ident()?;
            expr = Expression::PropertyAccess {
                object: Box::new(expr),
                property,
            };
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expression> {
        match &self.current().kind {
            TokenKind::Null => {
                self.bump();
                Ok(Expression::Null)
            }
            TokenKind::True => {
                self.bump();
                Ok(Expression::Bool(true))
            }
            TokenKind::False => {
                self.bump();
                Ok(Expression::Bool(false))
            }
            TokenKind::Int(n) => {
                let n = *n;
                self.bump();
                Ok(Expression::Int(n))
            }
            TokenKind::Float(n) => {
                let n = *n;
                self.bump();
                Ok(Expression::Float(n))
            }
            TokenKind::String(s) => {
                let s = s.clone();
                self.bump();
                Ok(Expression::String(s))
            }
            TokenKind::LBrace => Ok(Expression::Map(self.parse_map_literal()?)),
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_expression()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Minus => {
                self.bump();
                match &self.current().kind {
                    TokenKind::Int(n) => {
                        let n = *n;
                        self.bump();
                        Ok(Expression::Int(-n))
                    }
                    TokenKind::Float(n) => {
                        let n = *n;
                        self.bump();
                        Ok(Expression::Float(-n))
                    }
                    _ => Err(self.error("expected number after '-'")),
                }
            }
            // Keyword-as-identifier (e.g. variable `child`)
            _ if self.looks_like_ident() => {
                let name = self.expect_ident()?;
                Ok(Expression::Variable(name))
            }
            _ => Err(self.error("expected expression")),
        }
    }

    fn parse_map_literal(&mut self) -> Result<BTreeMap<String, Expression>> {
        self.expect(TokenKind::LBrace)?;
        let mut map = BTreeMap::new();

        if !self.check(&TokenKind::RBrace) {
            loop {
                let key = self.expect_ident_or_keyword_as_ident()?;
                self.expect(TokenKind::Colon)?;
                let value = self.parse_expression()?;
                map.insert(key, value);
                if self.check(&TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }

        self.expect(TokenKind::RBrace)?;
        Ok(map)
    }

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    fn expect_ident(&mut self) -> Result<String> {
        self.expect_ident_or_keyword_as_ident()
    }

    /// Accept an identifier, or a keyword used as a name (variables, labels, branch names).
    /// Preserves the original source spelling/case.
    fn expect_ident_or_keyword_as_ident(&mut self) -> Result<String> {
        if !self.looks_like_ident() {
            return Err(self.error("expected identifier"));
        }
        let start = self.current().offset;
        let end = self.current().end;
        self.bump();
        Ok(self.source[start..end].to_string())
    }

    /// Branch / tag name: identifier or string literal.
    fn expect_name(&mut self) -> Result<String> {
        match &self.current().kind {
            TokenKind::String(_) => self.expect_string(),
            _ => self.expect_ident(),
        }
    }

    fn expect_string(&mut self) -> Result<String> {
        match &self.current().kind {
            TokenKind::String(s) => {
                let s = s.clone();
                self.bump();
                Ok(s)
            }
            _ => Err(self.error("expected string literal")),
        }
    }

    fn expect_u32(&mut self) -> Result<u32> {
        match &self.current().kind {
            TokenKind::Int(n) if *n >= 0 && *n <= i64::from(u32::MAX) => {
                let n = *n as u32;
                self.bump();
                Ok(n)
            }
            _ => Err(self.error("expected non-negative integer")),
        }
    }
}

fn validate_hex_hash(s: &str) -> std::result::Result<(), String> {
    if s.is_empty() {
        return Err("commit hash must not be empty".into());
    }
    if s.len() > 64 {
        return Err(format!("commit hash too long ({} chars, max 64)", s.len()));
    }
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("commit hash must be hexadecimal, got '{s}'"));
    }
    Ok(())
}
