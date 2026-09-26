use super::*;

impl Parser<'_> {
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
            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
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
        let (token, next) = self.stream.cursor_at(self.position).take(kind)?;
        self.position = next.position();
        Some(token.clone())
    }

    pub(super) fn check(&self, kind: &TokenKind) -> bool {
        self.stream.cursor_at(self.position).check(kind)
    }

    pub(super) fn advance(&mut self) -> &Token {
        let cursor = self.stream.cursor_at(self.position);
        if let Some((token, next)) = cursor.advance() {
            self.position = next.position();
            token
        } else {
            &self.fallback_token
        }
    }

    pub(super) fn peek(&self) -> &Token {
        self.stream
            .cursor_at(self.position)
            .peek()
            .or_else(|| self.stream.cursor_at(self.position).previous())
            .unwrap_or(&self.fallback_token)
    }

    pub(super) fn previous(&self) -> &Token {
        self.stream
            .cursor_at(self.position)
            .previous()
            .unwrap_or(&self.fallback_token)
    }

    pub(super) fn error_here(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            span: self.peek().span,
        }
    }

    pub(super) fn looks_like_record_literal(&self) -> bool {
        if !self.check(&TokenKind::LeftBrace) {
            return false;
        }
        let next = self.stream.cursor_at(self.position + 1).peek();
        if next.is_some_and(|token| matches!(token.kind, TokenKind::RightBrace)) {
            return self.allow_empty_record_literal;
        }
        next.is_some_and(|token| matches!(token.kind, TokenKind::Identifier(_)))
            && self
                .stream
                .cursor_at(self.position + 2)
                .peek()
                .is_some_and(|token| matches!(token.kind, TokenKind::Colon))
    }

    pub(super) fn parenthesized_empty_record_ahead(&self) -> bool {
        matches!(
            self.stream
                .cursor_at(self.position)
                .peek()
                .map(|token| &token.kind),
            Some(TokenKind::Identifier(_))
        ) && matches!(
            self.stream
                .cursor_at(self.position + 1)
                .peek()
                .map(|token| &token.kind),
            Some(TokenKind::LeftBrace)
        ) && matches!(
            self.stream
                .cursor_at(self.position + 2)
                .peek()
                .map(|token| &token.kind),
            Some(TokenKind::RightBrace)
        ) && matches!(
            self.stream
                .cursor_at(self.position + 3)
                .peek()
                .map(|token| &token.kind),
            Some(TokenKind::RightParen)
        )
    }

    pub(super) fn generic_parameters(&mut self) -> Result<Vec<GenericParameter>, ParseError> {
        if self.take(&TokenKind::Less).is_none() {
            return Ok(Vec::new());
        }
        let mut parameters = Vec::new();
        self.generic_scopes.push(Vec::new());
        loop {
            let is_const = self.take(&TokenKind::Const).is_some();
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
            if is_const {
                self.expect(
                    &TokenKind::Colon,
                    "expected `: usize` after const parameter",
                )?;
                let ty = self.type_annotation()?;
                if ty != Type::USIZE {
                    return Err(ParseError {
                        message: "const parameters currently require usize".into(),
                        span,
                    });
                }
            } else if self.take(&TokenKind::Colon).is_some() {
                loop {
                    let bound = self.type_annotation()?;
                    let Type::Named { .. } = &bound else {
                        return Err(self.error_here("expected trait name in generic bound"));
                    };
                    if bounds.contains(&bound) {
                        return Err(ParseError {
                            message: format!("duplicate trait bound `{bound}`"),
                            span,
                        });
                    }
                    bounds.push(bound);
                    if self.take(&TokenKind::Plus).is_none() {
                        break;
                    }
                }
            }
            parameters.push(GenericParameter {
                is_const,
                name,
                bounds,
                span,
            });
            *self
                .generic_scopes
                .last_mut()
                .expect("generic scope exists") = parameters.clone();
            if self.take(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::Greater, "expected `>` after generic parameters")?;
        self.generic_scopes.pop();
        Ok(parameters)
    }
}
