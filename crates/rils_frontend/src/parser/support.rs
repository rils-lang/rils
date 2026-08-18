use super::*;

impl Parser {
    pub(super) fn if_expression(&mut self, start: Span) -> Result<Expr, ParseError> {
        let condition = self.expression()?;
        let then_branch = self.block("expected `{` after `if` condition")?;
        let else_branch = if self.take(&TokenKind::Else).is_some() {
            if let Some(if_token) = self.take(&TokenKind::If) {
                Some(Box::new(self.if_expression(if_token.span)?))
            } else {
                Some(Box::new(Expr::Block(
                    self.block("expected `{` or `if` after `else`")?,
                )))
            }
        } else {
            None
        };
        let end = else_branch
            .as_ref()
            .map_or(then_branch.span, |branch| branch.span());
        Ok(Expr::If {
            condition: Box::new(condition),
            then_branch,
            else_branch,
            span: start.merge(end),
        })
    }

    pub(super) fn block(&mut self, message: &str) -> Result<Block, ParseError> {
        let left = self.expect(&TokenKind::LeftBrace, message)?;
        self.block_after_left(left.span)
    }

    pub(super) fn block_after_left(&mut self, left: Span) -> Result<Block, ParseError> {
        self.block_depth += 1;
        let result = (|| {
            let mut statements = Vec::new();
            while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
                statements.push(self.statement()?);
            }
            let right = self.expect(&TokenKind::RightBrace, "expected `}` after block")?;
            Ok(Block {
                statements,
                span: left.merge(right.span),
            })
        })();
        self.block_depth -= 1;
        result
    }

    pub(super) fn expect_identifier(
        &mut self,
        message: &str,
    ) -> Result<(String, Span), ParseError> {
        let token = self.advance().clone();
        if let TokenKind::Identifier(name) = token.kind {
            Ok((name, token.span))
        } else {
            Err(ParseError {
                message: message.into(),
                span: token.span,
            })
        }
    }

    pub(super) fn expect_path_segment(
        &mut self,
        message: &str,
    ) -> Result<(String, Span), ParseError> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Identifier(name) => Ok((name, token.span)),
            TokenKind::Crate => Ok(("crate".into(), token.span)),
            TokenKind::Super => Ok(("super".into(), token.span)),
            _ => Err(ParseError {
                message: message.into(),
                span: token.span,
            }),
        }
    }

    pub(super) fn expect(&mut self, kind: &TokenKind, message: &str) -> Result<Token, ParseError> {
        self.take(kind).ok_or_else(|| self.error_here(message))
    }

    pub(super) fn take(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.check(kind) {
            self.current += 1;
            Some(self.previous().clone())
        } else {
            None
        }
    }

    pub(super) fn check(&self, kind: &TokenKind) -> bool {
        discriminant(&self.peek().kind) == discriminant(kind)
    }

    pub(super) fn advance(&mut self) -> &Token {
        if !self.check(&TokenKind::Eof) {
            self.current += 1;
        }
        self.previous()
    }

    pub(super) fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    pub(super) fn previous(&self) -> &Token {
        &self.tokens[self.current.saturating_sub(1)]
    }

    pub(super) fn error_here(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            span: self.peek().span,
        }
    }

    pub(super) fn looks_like_record_literal(&self) -> bool {
        self.check(&TokenKind::LeftBrace)
            && self
                .tokens
                .get(self.current + 1)
                .is_some_and(|token| matches!(token.kind, TokenKind::Identifier(_)))
            && self
                .tokens
                .get(self.current + 2)
                .is_some_and(|token| matches!(token.kind, TokenKind::Colon))
    }

    pub(super) fn generic_parameters(&mut self) -> Result<Vec<GenericParameter>, ParseError> {
        if self.take(&TokenKind::Less).is_none() {
            return Ok(Vec::new());
        }
        let mut parameters = Vec::new();
        loop {
            let (name, span) = self.expect_identifier("expected generic parameter name")?;
            if parameters
                .iter()
                .any(|parameter: &GenericParameter| parameter.name == name)
            {
                return Err(ParseError {
                    message: format!("duplicate generic parameter `{name}`"),
                    span,
                });
            }
            let mut bounds = Vec::new();
            if self.take(&TokenKind::Colon).is_some() {
                loop {
                    let (bound, bound_span) =
                        self.expect_identifier("expected trait name in generic bound")?;
                    self.type_references.push(TypeReference {
                        name: bound.clone(),
                        span: bound_span,
                        definition_span: None,
                        is_builtin: matches!(
                            bound.as_str(),
                            "Copy"
                                | "Clone"
                                | "Default"
                                | "Eq"
                                | "Hash"
                                | "Iterator"
                                | "IntoIterator"
                        ),
                        arguments: Vec::new(),
                    });
                    if bounds.contains(&bound) {
                        return Err(ParseError {
                            message: format!("duplicate trait bound `{bound}`"),
                            span: bound_span,
                        });
                    }
                    bounds.push(bound);
                    if self.take(&TokenKind::Plus).is_none() {
                        break;
                    }
                }
            }
            parameters.push(GenericParameter { name, bounds, span });
            if self.take(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::Greater, "expected `>` after generic parameters")?;
        Ok(parameters)
    }
}
