use super::*;

impl Parser {
    pub(super) fn expression(&mut self) -> Result<Expr, ParseError> {
        self.assignment()
    }

    pub(super) fn assignment(&mut self) -> Result<Expr, ParseError> {
        let expression = self.range()?;
        if self.take(&TokenKind::Equal).is_some() {
            let value = self.assignment()?;
            let span = expression.span().merge(value.span());
            if matches!(
                &expression,
                Expr::Variable { .. }
                    | Expr::Member { .. }
                    | Expr::Index { .. }
                    | Expr::Unary {
                        operator: UnaryOp::Dereference,
                        ..
                    }
            ) {
                return Ok(Expr::Assign {
                    target: Box::new(expression),
                    value: Box::new(value),
                    span,
                });
            }
            return Err(ParseError {
                message: "invalid assignment target".into(),
                span: expression.span(),
            });
        }
        Ok(expression)
    }

    pub(super) fn range(&mut self) -> Result<Expr, ParseError> {
        let start = self.logical_or()?;
        if self.take(&TokenKind::DotDot).is_none() {
            return Ok(start);
        }
        let end = self.logical_or()?;
        let span = start.span().merge(end.span());
        Ok(Expr::Range {
            start: Box::new(start),
            end: Box::new(end),
            span,
        })
    }

    pub(super) fn logical_or(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.logical_and()?;
        while self.take(&TokenKind::OrOr).is_some() {
            let right = self.logical_and()?;
            let span = expression.span().merge(right.span());
            expression = Expr::Logical {
                left: Box::new(expression),
                operator: LogicalOp::Or,
                right: Box::new(right),
                span,
            };
        }
        Ok(expression)
    }

    pub(super) fn logical_and(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.equality()?;
        while self.take(&TokenKind::AndAnd).is_some() {
            let right = self.equality()?;
            let span = expression.span().merge(right.span());
            expression = Expr::Logical {
                left: Box::new(expression),
                operator: LogicalOp::And,
                right: Box::new(right),
                span,
            };
        }
        Ok(expression)
    }

    pub(super) fn equality(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.comparison()?;
        loop {
            let operator = if self.take(&TokenKind::EqualEqual).is_some() {
                BinaryOp::Equal
            } else if self.take(&TokenKind::BangEqual).is_some() {
                BinaryOp::NotEqual
            } else {
                break;
            };
            expression = self.binary(expression, operator, Self::comparison)?;
        }
        Ok(expression)
    }

    pub(super) fn comparison(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.term()?;
        loop {
            let operator = if self.take(&TokenKind::Greater).is_some() {
                BinaryOp::Greater
            } else if self.take(&TokenKind::GreaterEqual).is_some() {
                BinaryOp::GreaterEqual
            } else if self.take(&TokenKind::Less).is_some() {
                BinaryOp::Less
            } else if self.take(&TokenKind::LessEqual).is_some() {
                BinaryOp::LessEqual
            } else {
                break;
            };
            expression = self.binary(expression, operator, Self::term)?;
        }
        Ok(expression)
    }

    pub(super) fn term(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.factor()?;
        loop {
            let operator = if self.take(&TokenKind::Plus).is_some() {
                BinaryOp::Add
            } else if self.take(&TokenKind::Minus).is_some() {
                BinaryOp::Subtract
            } else {
                break;
            };
            expression = self.binary(expression, operator, Self::factor)?;
        }
        Ok(expression)
    }

    pub(super) fn factor(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.unary()?;
        loop {
            let operator = if self.take(&TokenKind::Star).is_some() {
                BinaryOp::Multiply
            } else if self.take(&TokenKind::Slash).is_some() {
                BinaryOp::Divide
            } else if self.take(&TokenKind::Percent).is_some() {
                BinaryOp::Remainder
            } else {
                break;
            };
            expression = self.binary(expression, operator, Self::unary)?;
        }
        Ok(expression)
    }

    pub(super) fn binary(
        &mut self,
        left: Expr,
        operator: BinaryOp,
        parse_right: fn(&mut Self) -> Result<Expr, ParseError>,
    ) -> Result<Expr, ParseError> {
        let right = parse_right(self)?;
        let span = left.span().merge(right.span());
        Ok(Expr::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right),
            span,
        })
    }

    pub(super) fn unary(&mut self) -> Result<Expr, ParseError> {
        if let Some(token) = self.take(&TokenKind::Ampersand) {
            let mutable = self.take(&TokenKind::Mut).is_some();
            let target = self.unary()?;
            let span = token.span.merge(target.span());
            return Ok(Expr::Borrow {
                mutable,
                target: Box::new(target),
                span,
            });
        }
        if let Some(token) = self.take(&TokenKind::Star) {
            let operand = self.unary()?;
            let span = token.span.merge(operand.span());
            return Ok(Expr::Unary {
                operator: UnaryOp::Dereference,
                operand: Box::new(operand),
                span,
            });
        }
        if let Some(token) = self.take(&TokenKind::Bang) {
            let operand = self.unary()?;
            let span = token.span.merge(operand.span());
            return Ok(Expr::Unary {
                operator: UnaryOp::Not,
                operand: Box::new(operand),
                span,
            });
        }
        if let Some(token) = self.take(&TokenKind::Minus) {
            let operand = self.unary()?;
            let span = token.span.merge(operand.span());
            return Ok(Expr::Unary {
                operator: UnaryOp::Negate,
                operand: Box::new(operand),
                span,
            });
        }
        self.call()
    }

    pub(super) fn call(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.primary()?;
        loop {
            if self.take(&TokenKind::LeftParen).is_some() {
                let mut arguments = Vec::new();
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        arguments.push(self.expression()?);
                        if self.take(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let close = self.expect(&TokenKind::RightParen, "expected `)` after arguments")?;
                let span = expression.span().merge(close.span);
                expression = Expr::Call {
                    callee: Box::new(expression),
                    arguments,
                    span,
                };
            } else if self.take(&TokenKind::Dot).is_some() {
                let token = self.advance().clone();
                let (name, member_span) = match token.kind {
                    TokenKind::Identifier(name) => (name, token.span),
                    TokenKind::Integer(index) if index >= 0 => (index.to_string(), token.span),
                    TokenKind::Usize(index) => (index.to_string(), token.span),
                    _ => {
                        return Err(ParseError {
                            message: "expected member name or tuple index after `.`".into(),
                            span: token.span,
                        });
                    }
                };
                let span = expression.span().merge(member_span);
                expression = Expr::Member {
                    object: Box::new(expression),
                    name,
                    span,
                };
            } else if self.take(&TokenKind::LeftBracket).is_some() {
                let index = self.expression()?;
                let close = self.expect(&TokenKind::RightBracket, "expected `]` after index")?;
                let span = expression.span().merge(close.span);
                expression = Expr::Index {
                    object: Box::new(expression),
                    index: Box::new(index),
                    span,
                };
            } else if let Some(question) = self.take(&TokenKind::Question) {
                let span = expression.span().merge(question.span);
                expression = Expr::Try {
                    operand: Box::new(expression),
                    span,
                };
            } else if self.looks_like_record_literal() {
                let Some(path) = expression_path(&expression) else {
                    break;
                };
                self.take(&TokenKind::LeftBrace);
                let mut fields = Vec::new();
                loop {
                    let (name, name_span) =
                        self.expect_identifier("expected field name in constructor")?;
                    if fields
                        .iter()
                        .any(|(existing, _): &(String, Expr)| existing == &name)
                    {
                        return Err(ParseError {
                            message: format!("duplicate field `{name}`"),
                            span: name_span,
                        });
                    }
                    self.expect(&TokenKind::Colon, "expected `:` after field name")?;
                    fields.push((name, self.expression()?));
                    if self.take(&TokenKind::Comma).is_none() {
                        break;
                    }
                    if self.check(&TokenKind::RightBrace) {
                        break;
                    }
                }
                let right = self.expect(
                    &TokenKind::RightBrace,
                    "expected `}` after constructor fields",
                )?;
                expression = Expr::RecordLiteral {
                    path,
                    fields,
                    span: expression.span().merge(right.span),
                };
            } else {
                break;
            }
        }
        Ok(expression)
    }

    pub(super) fn primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.advance().clone();
        if let Some(value) = scalar_literal(&token.kind) {
            return Ok(Expr::Literal {
                value,
                span: token.span,
            });
        }
        let expression = match token.kind {
            TokenKind::String(value) => Expr::Literal {
                value: Literal::String(value),
                span: token.span,
            },
            TokenKind::True => Expr::Literal {
                value: Literal::Bool(true),
                span: token.span,
            },
            TokenKind::False => Expr::Literal {
                value: Literal::Bool(false),
                span: token.span,
            },
            TokenKind::Nil => {
                return Err(ParseError {
                    message: "`nil` has been removed; use `None` with an `Option<T>` type".into(),
                    span: token.span,
                });
            }
            TokenKind::Identifier(name) => {
                let mut segments = vec![name];
                let mut span = token.span;
                while self.take(&TokenKind::ColonColon).is_some() {
                    let (segment, segment_span) =
                        self.expect_identifier("expected name after `::`")?;
                    segments.push(segment);
                    span = span.merge(segment_span);
                }
                if segments.len() == 1 {
                    Expr::Variable {
                        name: segments.pop().expect("one segment"),
                        span,
                    }
                } else {
                    Expr::Path { segments, span }
                }
            }
            TokenKind::Less => {
                let target = self.type_annotation()?;
                self.expect(&TokenKind::As, "expected `as` in qualified path")?;
                let trait_type = self.type_annotation()?;
                let Type::Named {
                    name: trait_name,
                    arguments,
                } = trait_type
                else {
                    return Err(self.error_here("expected trait name after `as`"));
                };
                if !arguments.is_empty() {
                    return Err(self.error_here("generic traits are not supported yet"));
                }
                let greater = self.expect(&TokenKind::Greater, "expected `>` after trait name")?;
                self.expect(&TokenKind::ColonColon, "expected `::` after qualified path")?;
                let (member, member_span) =
                    self.expect_identifier("expected member name after `::`")?;
                Expr::QualifiedPath {
                    target,
                    trait_name,
                    member,
                    span: token.span.merge(greater.span).merge(member_span),
                }
            }
            TokenKind::LeftParen => {
                if let Some(right) = self.take(&TokenKind::RightParen) {
                    return Ok(Expr::Literal {
                        value: Literal::Unit,
                        span: token.span.merge(right.span),
                    });
                }
                let first = self.expression()?;
                if self.take(&TokenKind::Comma).is_none() {
                    self.expect(&TokenKind::RightParen, "expected `)` after expression")?;
                    first
                } else {
                    let mut elements = vec![first];
                    while !self.check(&TokenKind::RightParen) {
                        elements.push(self.expression()?);
                        if self.take(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    let right = self.expect(&TokenKind::RightParen, "expected `)` after tuple")?;
                    Expr::Tuple {
                        elements,
                        span: token.span.merge(right.span),
                    }
                }
            }
            TokenKind::LeftBracket => {
                if let Some(right) = self.take(&TokenKind::RightBracket) {
                    Expr::Array {
                        elements: Vec::new(),
                        repeat: None,
                        span: token.span.merge(right.span),
                    }
                } else {
                    let first = self.expression()?;
                    let (elements, repeat) = if self.take(&TokenKind::Semicolon).is_some() {
                        (vec![first], Some(Box::new(self.expression()?)))
                    } else {
                        let mut elements = vec![first];
                        while self.take(&TokenKind::Comma).is_some()
                            && !self.check(&TokenKind::RightBracket)
                        {
                            elements.push(self.expression()?);
                        }
                        (elements, None)
                    };
                    let right =
                        self.expect(&TokenKind::RightBracket, "expected `]` after array")?;
                    Expr::Array {
                        elements,
                        repeat,
                        span: token.span.merge(right.span),
                    }
                }
            }
            TokenKind::LeftBrace => Expr::Block(self.block_after_left(token.span)?),
            TokenKind::If => self.if_expression(token.span)?,
            TokenKind::Match => self.match_expression(token.span)?,
            _ => {
                return Err(ParseError {
                    message: format!("expected expression, found {}", token.kind.name()),
                    span: token.span,
                });
            }
        };
        Ok(expression)
    }

    pub(super) fn match_expression(&mut self, start: Span) -> Result<Expr, ParseError> {
        let value = self.expression()?;
        self.expect(&TokenKind::LeftBrace, "expected `{` after match expression")?;
        let mut arms = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let pattern = self.pattern()?;
            self.expect(&TokenKind::FatArrow, "expected `=>` after match pattern")?;
            let expression = self.expression()?;
            let is_block_like = matches!(
                expression,
                Expr::If { .. } | Expr::Match { .. } | Expr::Block(_)
            );
            if self.take(&TokenKind::Comma).is_none()
                && !self.check(&TokenKind::RightBrace)
                && !is_block_like
            {
                return Err(self.error_here("expected `,` after match arm"));
            }
            arms.push(MatchArm {
                pattern,
                expression,
            });
        }

        if arms.is_empty() {
            return Err(self.error_here("match expression requires at least one arm"));
        }
        let right = self.expect(&TokenKind::RightBrace, "expected `}` after match arms")?;
        Ok(Expr::Match {
            value: Box::new(value),
            arms,
            span: start.merge(right.span),
        })
    }
}
