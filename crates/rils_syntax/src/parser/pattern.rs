use super::*;

impl Parser {
    pub(super) fn pattern(&mut self) -> Result<Pattern, ParseError> {
        let token = self.advance().clone();
        if let Some(value) = scalar_literal(&token.kind) {
            return Ok(Pattern::Literal {
                value,
                span: token.span,
            });
        }
        match token.kind {
            TokenKind::Identifier(name) if name == "_" => {
                Ok(Pattern::Wildcard { span: token.span })
            }
            TokenKind::Identifier(name) if name == "Some" => {
                self.expect(&TokenKind::LeftParen, "expected `(` after `Some`")?;
                let inner = self.pattern()?;
                let right =
                    self.expect(&TokenKind::RightParen, "expected `)` after Some pattern")?;
                Ok(Pattern::Some {
                    inner: Box::new(inner),
                    span: token.span.merge(right.span),
                })
            }
            TokenKind::Identifier(name) if name == "None" => Ok(Pattern::None { span: token.span }),
            TokenKind::Identifier(name) if matches!(name.as_str(), "Ok" | "Err") => {
                self.expect(&TokenKind::LeftParen, "expected `(` after Result variant")?;
                let inner = self.pattern()?;
                let right =
                    self.expect(&TokenKind::RightParen, "expected `)` after Result pattern")?;
                let span = token.span.merge(right.span);
                if name == "Ok" {
                    Ok(Pattern::Ok {
                        inner: Box::new(inner),
                        span,
                    })
                } else {
                    Ok(Pattern::Err {
                        inner: Box::new(inner),
                        span,
                    })
                }
            }
            TokenKind::Identifier(name) => self.named_pattern(name, token.span),
            TokenKind::Crate => self.named_pattern("crate".into(), token.span),
            TokenKind::Super => self.named_pattern("super".into(), token.span),
            TokenKind::String(value) => Ok(Pattern::Literal {
                value: Literal::String(value),
                span: token.span,
            }),
            TokenKind::True => Ok(Pattern::Literal {
                value: Literal::Bool(true),
                span: token.span,
            }),
            TokenKind::False => Ok(Pattern::Literal {
                value: Literal::Bool(false),
                span: token.span,
            }),
            TokenKind::LeftParen => {
                if let Some(right) = self.take(&TokenKind::RightParen) {
                    Ok(Pattern::Literal {
                        value: Literal::Unit,
                        span: token.span.merge(right.span),
                    })
                } else {
                    let pattern = self.pattern()?;
                    self.expect(&TokenKind::RightParen, "expected `)` after pattern")?;
                    Ok(pattern)
                }
            }
            TokenKind::Minus => {
                let number = self.advance().clone();
                match negated_scalar_literal(&number.kind) {
                    Some(value) => Ok(Pattern::Literal {
                        value,
                        span: token.span.merge(number.span),
                    }),
                    None => Err(ParseError {
                        message: "expected number after `-` in pattern".into(),
                        span: number.span,
                    }),
                }
            }
            _ => Err(ParseError {
                message: format!("expected pattern, found {}", token.kind.name()),
                span: token.span,
            }),
        }
    }

    pub(super) fn named_pattern(
        &mut self,
        name: String,
        start: Span,
    ) -> Result<Pattern, ParseError> {
        let mut path = vec![name];
        let mut span = start;
        while self.take(&TokenKind::ColonColon).is_some() {
            let (segment, segment_span) =
                self.expect_path_segment("expected variant name after `::`")?;
            path.push(segment);
            span = span.merge(segment_span);
        }

        if self.take(&TokenKind::LeftParen).is_some() {
            let mut fields = Vec::new();
            if !self.check(&TokenKind::RightParen) {
                loop {
                    fields.push(self.pattern()?);
                    if self.take(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
            }
            let right = self.expect(&TokenKind::RightParen, "expected `)` after tuple pattern")?;
            return Ok(Pattern::TupleVariant {
                path,
                fields,
                span: start.merge(right.span),
            });
        }

        if self.take(&TokenKind::LeftBrace).is_some() {
            let mut fields = Vec::new();
            while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
                let (field_name, field_span) =
                    self.expect_identifier("expected field name in record pattern")?;
                if fields
                    .iter()
                    .any(|(existing, _): &(String, Pattern)| existing == &field_name)
                {
                    return Err(ParseError {
                        message: format!("duplicate field `{field_name}` in pattern"),
                        span: field_span,
                    });
                }
                let pattern = if self.take(&TokenKind::Colon).is_some() {
                    self.pattern()?
                } else {
                    Pattern::Binding {
                        name: field_name.clone(),
                        span: field_span,
                    }
                };
                fields.push((field_name, pattern));
                if self.take(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            let right = self.expect(&TokenKind::RightBrace, "expected `}` after record pattern")?;
            return Ok(Pattern::Record {
                path,
                fields,
                span: start.merge(right.span),
            });
        }

        if path.len() == 1 {
            Ok(Pattern::Binding {
                name: path.pop().expect("one path segment"),
                span,
            })
        } else {
            Ok(Pattern::Path { path, span })
        }
    }
}
