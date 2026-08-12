use super::*;

impl Parser {
    pub(super) fn type_annotation(&mut self) -> Result<Type, ParseError> {
        if self.take(&TokenKind::Less).is_some() {
            let base = self.type_annotation()?;
            self.expect(&TokenKind::As, "expected `as` in qualified associated type")?;
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
            self.expect(&TokenKind::Greater, "expected `>` after qualified trait")?;
            self.expect(
                &TokenKind::ColonColon,
                "expected `::` after qualified trait",
            )?;
            let (name, _) = self.expect_identifier("expected associated type after `::`")?;
            let mut arguments = Vec::new();
            if self.take(&TokenKind::Less).is_some() {
                loop {
                    arguments.push(self.type_annotation()?);
                    if self.take(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.expect(&TokenKind::Greater, "expected `>` after type arguments")?;
            }
            return Ok(Type::Associated {
                base: Box::new(base),
                trait_name: Some(trait_name),
                name,
                arguments,
            });
        }
        if self.take(&TokenKind::Ampersand).is_some() {
            let mutable = self.take(&TokenKind::Mut).is_some();
            return Ok(Type::Reference {
                mutable,
                inner: Box::new(self.type_annotation()?),
            });
        }

        if self.take(&TokenKind::LeftParen).is_some() {
            if self.take(&TokenKind::RightParen).is_some() {
                return Ok(Type::Unit);
            }
            let mut elements = vec![self.type_annotation()?];
            if self.take(&TokenKind::Comma).is_none() {
                return Err(self.error_here("tuple types require a trailing comma"));
            }
            while !self.check(&TokenKind::RightParen) {
                elements.push(self.type_annotation()?);
                if self.take(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(&TokenKind::RightParen, "expected `)` after tuple type")?;
            return Ok(Type::Tuple(elements));
        }

        if self.take(&TokenKind::LeftBracket).is_some() {
            let element = self.type_annotation()?;
            self.expect(&TokenKind::Semicolon, "expected `;` before array length")?;
            let token = self.advance().clone();
            let length = match token.kind {
                TokenKind::Integer(length) => usize::try_from(length).ok(),
                TokenKind::Usize(length) => Some(length),
                _ => None,
            }
            .ok_or_else(|| ParseError {
                message: "array length must be a non-negative usize literal".into(),
                span: token.span,
            })?;
            self.expect(&TokenKind::RightBracket, "expected `]` after array type")?;
            return Ok(Type::Array {
                element: Box::new(element),
                length,
            });
        }

        if self.take(&TokenKind::Fn).is_some() {
            self.expect(
                &TokenKind::LeftParen,
                "expected `(` after `fn` in function type",
            )?;
            let mut parameters = Vec::new();
            if !self.check(&TokenKind::RightParen) {
                loop {
                    parameters.push(self.type_annotation()?);
                    if self.take(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
            }
            self.expect(
                &TokenKind::RightParen,
                "expected `)` after function parameter types",
            )?;
            self.expect(
                &TokenKind::Arrow,
                "expected `->` after function parameter types",
            )?;
            let return_type = self.type_annotation()?;
            return Ok(Type::function(parameters, return_type));
        }

        let (name, name_span) = self.expect_path_segment("expected type name")?;
        let generic_definition = self
            .generic_scopes
            .iter()
            .rev()
            .flatten()
            .find(|parameter| parameter.name == name)
            .map(|parameter| parameter.span);
        let is_builtin = matches!(
            name.as_str(),
            "bool"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "f32"
                | "f64"
                | "char"
                | "string"
                | "function"
                | "Option"
                | "Result"
                | "Self"
                | "Copy"
                | "Clone"
                | "Iterator"
                | "IntoIterator"
                | "Range"
                | "Vec"
                | "core"
                | "std"
                | "prelude"
        );
        let mut reference_index = self.type_references.len();
        self.type_references.push(TypeReference {
            name: name.clone(),
            span: name_span,
            definition_span: generic_definition,
            is_builtin,
            arguments: Vec::new(),
        });
        let parse_arguments = |parser: &mut Self| -> Result<Vec<Type>, ParseError> {
            let mut arguments = Vec::new();
            if parser.take(&TokenKind::Less).is_some() {
                loop {
                    arguments.push(parser.type_annotation()?);
                    if parser.take(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                parser.expect(&TokenKind::Greater, "expected `>` after type arguments")?;
            }
            Ok(arguments)
        };

        match name.as_str() {
            "bool" => Ok(Type::Bool),
            "int" => Err(ParseError {
                message: "built-in type `int` was removed; use an explicit integer type such as `i32` or `i64`".into(),
                span: name_span,
            }),
            "float" => Err(ParseError {
                message: "built-in type `float` was removed; use `f32` or `f64`".into(),
                span: name_span,
            }),
            name if crate::types::IntegerType::from_name(name).is_some() => Ok(Type::Integer(
                crate::types::IntegerType::from_name(name).expect("integer name was checked"),
            )),
            name if crate::types::FloatType::from_name(name).is_some() => Ok(Type::Float(
                crate::types::FloatType::from_name(name).expect("float name was checked"),
            )),
            "char" => Ok(Type::Char),
            "string" => Ok(Type::String),
            "function" => Ok(Type::opaque_function()),
            "Option" => {
                self.expect(&TokenKind::Less, "expected `<` after `Option`")?;
                let inner = self.type_annotation()?;
                self.expect(
                    &TokenKind::Greater,
                    "expected `>` after Option element type",
                )?;
                self.type_references[reference_index].arguments = vec![inner.clone()];
                Ok(Type::Option(Box::new(inner)))
            }
            "Result" => {
                self.expect(&TokenKind::Less, "expected `<` after `Result`")?;
                let ok = self.type_annotation()?;
                self.expect(&TokenKind::Comma, "expected `,` in Result type")?;
                let error = self.type_annotation()?;
                self.expect(&TokenKind::Greater, "expected `>` after Result types")?;
                self.type_references[reference_index].arguments = vec![ok.clone(), error.clone()];
                Ok(Type::Result(Box::new(ok), Box::new(error)))
            }
            _ => {
                if generic_definition.is_some() {
                    let base = Type::Variable(name);
                    if self.take(&TokenKind::ColonColon).is_some() {
                        let (associated, _) =
                            self.expect_identifier("expected associated type after `::`")?;
                        let arguments = parse_arguments(self)?;
                        return Ok(Type::Associated {
                            base: Box::new(base),
                            trait_name: None,
                            name: associated,
                            arguments,
                        });
                    }
                    return Ok(base);
                }
                let mut path = vec![name];
                while self.take(&TokenKind::ColonColon).is_some() {
                    let (segment, span) =
                        self.expect_path_segment("expected type name after `::`")?;
                    let builtin_path = matches!(
                        path.first().map(String::as_str),
                        Some("core" | "std" | "prelude")
                    );
                    self.type_references.push(TypeReference {
                        name: segment.clone(),
                        span,
                        definition_span: None,
                        is_builtin: builtin_path,
                        arguments: Vec::new(),
                    });
                    reference_index = self.type_references.len() - 1;
                    path.push(segment);
                }
                let arguments = parse_arguments(self)?;
                self.type_references[reference_index].arguments = arguments.clone();
                let base = Type::Named {
                    name: path.join("::"),
                    arguments,
                };
                Ok(base)
            }
        }
    }
}
