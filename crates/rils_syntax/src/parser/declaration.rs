use super::*;

impl Parser {
    pub(super) fn module_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let (name, name_span) = self.expect_identifier("expected module name after `mod`")?;
        let (statements, end) = if let Some(end) = self.take(&TokenKind::Semicolon) {
            (None, end.span)
        } else {
            self.expect(
                &TokenKind::LeftBrace,
                "expected `{` or `;` after module name",
            )?;
            let mut statements = Vec::new();
            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                statements.push(self.statement()?);
            }
            let end = self.expect(&TokenKind::RightBrace, "expected `}` after module")?;
            (Some(statements), end.span)
        };
        Ok(Stmt::Module {
            visibility: Visibility::Private,
            name,
            name_span,
            statements,
            span: start.merge(end),
        })
    }

    pub(super) fn use_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let mut imports = Vec::new();
        self.use_tree(Vec::new(), Vec::new(), &mut imports)?;
        let end = self.expect(&TokenKind::Semicolon, "expected `;` after use item")?;
        Ok(Stmt::Use {
            visibility: Visibility::Private,
            imports,
            span: start.merge(end.span),
        })
    }

    fn use_tree(
        &mut self,
        mut prefix: Vec<String>,
        mut prefix_spans: Vec<Span>,
        imports: &mut Vec<UseImport>,
    ) -> Result<(), ParseError> {
        if let Some(star) = self.take(&TokenKind::Star) {
            if prefix.is_empty() {
                return Err(ParseError {
                    message: "glob import requires a module path".into(),
                    span: star.span,
                });
            }
            imports.push(UseImport {
                path: prefix,
                path_spans: prefix_spans,
                alias: None,
                alias_span: None,
                name_span: star.span,
                kind: UseImportKind::Glob,
                span: star.span,
            });
            return Ok(());
        }
        if self.take(&TokenKind::LeftBrace).is_some() {
            if self.check(&TokenKind::RightBrace) {
                return Err(ParseError {
                    message: "use group must contain at least one item".into(),
                    span: self.peek().span,
                });
            }
            loop {
                self.use_tree(prefix.clone(), prefix_spans.clone(), imports)?;
                if self.take(&TokenKind::Comma).is_some() {
                    if self.check(&TokenKind::RightBrace) {
                        break;
                    }
                } else {
                    break;
                }
            }
            self.expect(&TokenKind::RightBrace, "expected `}` after use group")?;
            return Ok(());
        }

        let (segment, segment_span) = self.expect_path_segment("expected path after `use`")?;
        if segment == "self" && !prefix.is_empty() && !self.check(&TokenKind::ColonColon) {
            let (alias, alias_span) = self.use_alias()?;
            let span = segment_span.merge(alias_span.unwrap_or(segment_span));
            imports.push(UseImport {
                path: prefix,
                path_spans: prefix_spans,
                alias,
                alias_span,
                name_span: segment_span,
                kind: UseImportKind::Single,
                span,
            });
            return Ok(());
        }
        prefix.push(segment);
        prefix_spans.push(segment_span);
        if self.take(&TokenKind::ColonColon).is_some() {
            return self.use_tree(prefix, prefix_spans, imports);
        }
        let (alias, alias_span) = self.use_alias()?;
        let span = prefix_spans[0].merge(alias_span.unwrap_or(segment_span));
        imports.push(UseImport {
            path: prefix,
            path_spans: prefix_spans,
            alias,
            alias_span,
            name_span: segment_span,
            kind: UseImportKind::Single,
            span,
        });
        Ok(())
    }

    fn use_alias(&mut self) -> Result<(Option<String>, Option<Span>), ParseError> {
        if self.take(&TokenKind::As).is_some() {
            let (alias, span) = self.expect_identifier("expected alias after `as`")?;
            Ok((Some(alias), Some(span)))
        } else {
            Ok((None, None))
        }
    }

    pub(super) fn let_statement(&mut self) -> Result<Stmt, ParseError> {
        let start = self.previous().span;
        let mutable = self.take(&TokenKind::Mut).is_some();
        let (name, name_span) = self.expect_identifier("expected variable name after `let`")?;
        let type_annotation = if self.take(&TokenKind::Colon).is_some() {
            let ty = self.type_annotation()?;
            reject_nested_reference(&ty, name_span)?;
            Some(ty)
        } else {
            None
        };
        self.expect(&TokenKind::Equal, "expected `=` after variable name")?;
        let initializer = self.expression()?;
        let end = self
            .expect(
                &TokenKind::Semicolon,
                "expected `;` after variable declaration",
            )?
            .span;
        Ok(Stmt::Let {
            name,
            name_span,
            mutable,
            type_annotation,
            initializer,
            span: start.merge(end),
        })
    }

    pub(super) fn function_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let method = self.function_declaration(start, false)?;
        Ok(Stmt::Function {
            visibility: Visibility::Private,
            attributes: Vec::new(),
            name: method.name,
            name_span: method.name_span,
            generic_parameters: method.generic_parameters,
            parameters: method.parameters,
            return_type: method.return_type,
            body: method.body,
            span: method.span,
        })
    }

    pub(super) fn function_declaration(
        &mut self,
        start: Span,
        allow_receiver: bool,
    ) -> Result<ImplMethod, ParseError> {
        let (name, name_span) = self.expect_identifier("expected function name after `fn`")?;
        let generic_parameters = self.generic_parameters()?;
        self.generic_scopes.push(generic_parameters.clone());
        self.expect(&TokenKind::LeftParen, "expected `(` after function name")?;
        let mut parameters = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                let parameter =
                    self.parameter("expected parameter name in function declaration")?;
                if parameter.name == "self" {
                    if !allow_receiver {
                        return Err(ParseError {
                            message: "`self` parameters are only allowed in impl methods".into(),
                            span: parameter.span,
                        });
                    }
                    if !parameters.is_empty() {
                        return Err(ParseError {
                            message: "`self` must be the first method parameter".into(),
                            span: parameter.span,
                        });
                    }
                }
                if parameters
                    .iter()
                    .any(|existing: &Parameter| existing.name == parameter.name)
                {
                    return Err(ParseError {
                        message: format!("duplicate parameter `{}`", parameter.name),
                        span: parameter.span,
                    });
                }
                parameters.push(parameter);
                if self.take(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RightParen, "expected `)` after parameters")?;
        let return_type = if self.take(&TokenKind::Arrow).is_some() {
            let ty = self.type_annotation()?;
            reject_return_reference(&ty, name_span)?;
            Some(ty)
        } else {
            None
        };
        let outer_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let body = self.block("expected `{` before function body");
        self.loop_depth = outer_loop_depth;
        let body = body?;
        self.generic_scopes.pop();
        let span = start.merge(body.span);
        Ok(ImplMethod {
            attributes: Vec::new(),
            name,
            name_span,
            generic_parameters,
            parameters,
            return_type,
            body,
            span,
        })
    }

    pub(super) fn struct_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let (name, name_span) = self.expect_identifier("expected struct name")?;
        let generic_parameters = self.generic_parameters()?;
        self.generic_scopes.push(generic_parameters.clone());
        let (fields, end) = if let Some(semicolon) = self.take(&TokenKind::Semicolon) {
            (Vec::new(), semicolon.span)
        } else {
            self.named_fields("expected `{` or `;` after struct name")?
        };
        self.generic_scopes.pop();
        Ok(Stmt::Struct {
            visibility: Visibility::Private,
            attributes: Vec::new(),
            name,
            name_span,
            generic_parameters,
            fields,
            span: start.merge(end),
        })
    }

    pub(super) fn enum_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let (name, name_span) = self.expect_identifier("expected enum name")?;
        let generic_parameters = self.generic_parameters()?;
        self.generic_scopes.push(generic_parameters.clone());
        self.expect(&TokenKind::LeftBrace, "expected `{` after enum name")?;
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let (variant_name, variant_start) =
                self.expect_identifier("expected enum variant name")?;
            if variants
                .iter()
                .any(|variant| enum_variant_name(variant) == variant_name)
            {
                return Err(ParseError {
                    message: format!("duplicate enum variant `{variant_name}`"),
                    span: variant_start,
                });
            }
            let variant = if self.take(&TokenKind::LeftParen).is_some() {
                let mut fields = Vec::new();
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        let ty = self.type_annotation()?;
                        reject_owned_reference(&ty, variant_start)?;
                        fields.push(ty);
                        if self.take(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let right =
                    self.expect(&TokenKind::RightParen, "expected `)` after variant fields")?;
                EnumVariant::Tuple {
                    name: variant_name,
                    fields,
                    span: variant_start.merge(right.span),
                }
            } else if self.check(&TokenKind::LeftBrace) {
                let (fields, end) = self.named_fields("expected `{` before variant fields")?;
                if fields.is_empty() {
                    return Err(ParseError {
                        message: "empty record variants should be written as unit variants".into(),
                        span: variant_start.merge(end),
                    });
                }
                EnumVariant::Record {
                    name: variant_name,
                    fields,
                    span: variant_start.merge(end),
                }
            } else {
                EnumVariant::Unit {
                    name: variant_name,
                    span: variant_start,
                }
            };
            variants.push(variant);
            if self.take(&TokenKind::Comma).is_none() && !self.check(&TokenKind::RightBrace) {
                return Err(self.error_here("expected `,` after enum variant"));
            }
        }
        if variants.is_empty() {
            return Err(self.error_here("enum requires at least one variant"));
        }
        let right = self.expect(&TokenKind::RightBrace, "expected `}` after enum variants")?;
        self.generic_scopes.pop();
        Ok(Stmt::Enum {
            visibility: Visibility::Private,
            attributes: Vec::new(),
            name,
            name_span,
            generic_parameters,
            variants,
            span: start.merge(right.span),
        })
    }

    pub(super) fn impl_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let generic_parameters = self.generic_parameters()?;
        self.generic_scopes.push(generic_parameters.clone());
        let first_type = self.type_annotation()?;
        let (trait_name, target) = if self.take(&TokenKind::For).is_some() {
            let Type::Named { name, arguments } = first_type else {
                return Err(ParseError {
                    message: "expected trait name before `for`".into(),
                    span: start,
                });
            };
            if !arguments.is_empty() {
                return Err(ParseError {
                    message: "generic traits are not supported yet".into(),
                    span: start,
                });
            }
            (Some(name), self.type_annotation()?)
        } else {
            (None, first_type)
        };
        if !matches!(
            target,
            Type::Named { .. }
                | Type::Option(_)
                | Type::Result(_, _)
                | Type::String
                | Type::Integer(_)
                | Type::Float(_)
        ) {
            return Err(ParseError {
                message: "impl target must be a nominal or built-in generic type".into(),
                span: start,
            });
        }
        self.expect(&TokenKind::LeftBrace, "expected `{` after impl target")?;
        let mut methods = Vec::new();
        let mut associated_types = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let attributes = self.attributes()?;
            if let Some(keyword) = self.take(&TokenKind::Type) {
                if !attributes.is_empty() {
                    return Err(ParseError {
                        message: "attributes on associated types are not supported yet".into(),
                        span: attributes[0].span,
                    });
                }
                if trait_name.is_none() {
                    return Err(ParseError {
                        message: "associated types are only allowed in trait impls".into(),
                        span: keyword.span,
                    });
                }
                let associated = self.associated_type_declaration(keyword.span, true)?;
                if associated_types
                    .iter()
                    .any(|existing: &AssociatedType| existing.name == associated.name)
                {
                    return Err(ParseError {
                        message: format!("duplicate associated type `{}`", associated.name),
                        span: associated.span,
                    });
                }
                associated_types.push(associated);
                continue;
            }
            let function = self.expect(
                &TokenKind::Fn,
                "only `fn` and `type` declarations are allowed in impl",
            )?;
            let mut method = self.function_declaration(function.span, true)?;
            method.attributes = attributes;
            if methods
                .iter()
                .any(|existing: &ImplMethod| existing.name == method.name)
            {
                return Err(ParseError {
                    message: format!("duplicate method `{}`", method.name),
                    span: method.span,
                });
            }
            methods.push(method);
        }
        let right = self.expect(&TokenKind::RightBrace, "expected `}` after impl block")?;
        self.generic_scopes.pop();
        Ok(Stmt::Impl {
            generic_parameters,
            trait_name,
            target,
            associated_types,
            methods,
            span: start.merge(right.span),
        })
    }

    pub(super) fn trait_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let (name, name_span) = self.expect_identifier("expected trait name")?;
        let mut bounds = Vec::new();
        if self.take(&TokenKind::Colon).is_some() {
            loop {
                let (bound, bound_span) =
                    self.expect_identifier("expected supertrait name after `:`")?;
                if bounds.contains(&bound) {
                    return Err(ParseError {
                        message: format!("duplicate supertrait `{bound}`"),
                        span: bound_span,
                    });
                }
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
                            | "BitFlags"
                            | "Iterator"
                            | "IntoIterator"
                    ),
                    arguments: Vec::new(),
                });
                bounds.push(bound);
                if self.take(&TokenKind::Plus).is_none() {
                    break;
                }
            }
        }
        self.expect(&TokenKind::LeftBrace, "expected `{` after trait name")?;
        let mut methods = Vec::new();
        let mut associated_types = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let attributes = self.attributes()?;
            if let Some(keyword) = self.take(&TokenKind::Type) {
                if !attributes.is_empty() {
                    return Err(ParseError {
                        message: "attributes on associated types are not supported yet".into(),
                        span: attributes[0].span,
                    });
                }
                let associated = self.associated_type_declaration(keyword.span, false)?;
                if associated_types
                    .iter()
                    .any(|existing: &AssociatedType| existing.name == associated.name)
                {
                    return Err(ParseError {
                        message: format!("duplicate associated type `{}`", associated.name),
                        span: associated.span,
                    });
                }
                associated_types.push(associated);
                continue;
            }
            let function = self.expect(
                &TokenKind::Fn,
                "only method signatures and associated types are allowed in trait",
            )?;
            let mut method = self.trait_method_declaration(function.span)?;
            method.attributes = attributes;
            if methods
                .iter()
                .any(|existing: &TraitMethod| existing.name == method.name)
            {
                return Err(ParseError {
                    message: format!("duplicate trait method `{}`", method.name),
                    span: method.span,
                });
            }
            methods.push(method);
        }
        let right = self.expect(&TokenKind::RightBrace, "expected `}` after trait")?;
        Ok(Stmt::Trait {
            visibility: Visibility::Private,
            name,
            name_span,
            bounds,
            associated_types,
            methods,
            span: start.merge(right.span),
        })
    }

    pub(super) fn type_alias_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let (name, name_span) = self.expect_identifier("expected type alias name")?;
        let generic_parameters = self.generic_parameters()?;
        self.generic_scopes.push(generic_parameters.clone());
        self.expect(&TokenKind::Equal, "expected `=` in type alias")?;
        let target = self.type_annotation()?;
        let end = self.expect(&TokenKind::Semicolon, "type alias must end with `;`")?;
        self.generic_scopes.pop();
        Ok(Stmt::TypeAlias {
            visibility: Visibility::Private,
            name,
            name_span,
            generic_parameters,
            target,
            span: start.merge(end.span),
        })
    }

    fn associated_type_declaration(
        &mut self,
        start: Span,
        require_value: bool,
    ) -> Result<AssociatedType, ParseError> {
        let (name, name_span) = self.expect_identifier("expected associated type name")?;
        let generic_parameters = self.generic_parameters()?;
        self.generic_scopes.push(generic_parameters.clone());
        let value = if self.take(&TokenKind::Equal).is_some() {
            Some(self.type_annotation()?)
        } else if require_value {
            return Err(ParseError {
                message: "associated type in impl requires `= Type`".into(),
                span: name_span,
            });
        } else {
            None
        };
        let end = self.expect(
            &TokenKind::Semicolon,
            "associated type declaration must end with `;`",
        )?;
        self.generic_scopes.pop();
        Ok(AssociatedType {
            name,
            name_span,
            generic_parameters,
            value,
            span: start.merge(end.span),
        })
    }

    pub(super) fn trait_method_declaration(
        &mut self,
        start: Span,
    ) -> Result<TraitMethod, ParseError> {
        let (name, name_span) = self.expect_identifier("expected trait method name")?;
        let generic_parameters = self.generic_parameters()?;
        self.generic_scopes.push(generic_parameters.clone());
        self.expect(&TokenKind::LeftParen, "expected `(` after method name")?;
        let mut parameters = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                let parameter = self.parameter("expected trait method parameter")?;
                if parameter.name == "self" && !parameters.is_empty() {
                    return Err(ParseError {
                        message: "`self` must be the first trait method parameter".into(),
                        span: parameter.span,
                    });
                }
                if parameters
                    .iter()
                    .any(|existing: &Parameter| existing.name == parameter.name)
                {
                    return Err(ParseError {
                        message: format!("duplicate parameter `{}`", parameter.name),
                        span: parameter.span,
                    });
                }
                parameters.push(parameter);
                if self.take(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RightParen, "expected `)` after parameters")?;
        let return_type = if self.take(&TokenKind::Arrow).is_some() {
            let ty = self.type_annotation()?;
            reject_return_reference(&ty, name_span)?;
            Some(ty)
        } else {
            None
        };
        let end = self.expect(
            &TokenKind::Semicolon,
            "trait methods without defaults must end with `;`",
        )?;
        self.generic_scopes.pop();
        Ok(TraitMethod {
            attributes: Vec::new(),
            name,
            name_span,
            generic_parameters,
            parameters,
            return_type,
            span: start.merge(end.span),
        })
    }

    pub(super) fn named_fields(
        &mut self,
        message: &str,
    ) -> Result<(Vec<NamedField>, Span), ParseError> {
        self.expect(&TokenKind::LeftBrace, message)?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let (name, span) = self.expect_identifier("expected field name")?;
            if fields.iter().any(|field: &NamedField| field.name == name) {
                return Err(ParseError {
                    message: format!("duplicate field `{name}`"),
                    span,
                });
            }
            self.expect(&TokenKind::Colon, "expected `:` after field name")?;
            let type_annotation = self.type_annotation()?;
            reject_owned_reference(&type_annotation, span)?;
            fields.push(NamedField {
                name,
                type_annotation,
                span,
            });
            if self.take(&TokenKind::Comma).is_none() && !self.check(&TokenKind::RightBrace) {
                return Err(self.error_here("expected `,` after field"));
            }
        }
        let right = self.expect(&TokenKind::RightBrace, "expected `}` after fields")?;
        Ok((fields, right.span))
    }

    fn parameter(&mut self, message: &str) -> Result<Parameter, ParseError> {
        if let Some(reference) = self.take(&TokenKind::Ampersand) {
            let reference_mutable = self.take(&TokenKind::Mut).is_some();
            let (name, span) = self.expect_identifier(message)?;
            if name != "self" {
                return Err(ParseError {
                    message: "reference receiver shorthand is only valid for `self`; use `name: &T` for other parameters".into(),
                    span: reference.span.merge(span),
                });
            }
            if self.check(&TokenKind::Colon) {
                return Err(ParseError {
                    message: "`&self` and `&mut self` receiver forms do not use a type annotation"
                        .into(),
                    span,
                });
            }
            return Ok(Parameter {
                name,
                mutable: false,
                type_annotation: Some(Type::Reference {
                    mutable: reference_mutable,
                    inner: Box::new(Type::named("Self")),
                }),
                span,
            });
        }

        let mutable = self.take(&TokenKind::Mut).is_some();
        let (name, span) = self.expect_identifier(message)?;
        let type_annotation = if self.take(&TokenKind::Colon).is_some() {
            let ty = self.type_annotation()?;
            if !self.allow_nested_parameter_references {
                reject_nested_reference(&ty, span)?;
            }
            Some(ty)
        } else {
            None
        };
        Ok(Parameter {
            name,
            mutable,
            type_annotation,
            span,
        })
    }

    pub(super) fn while_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let condition = self.expression()?;
        self.loop_depth += 1;
        let body = self.block("expected `{` after `while` condition");
        self.loop_depth -= 1;
        let body = body?;
        let span = start.merge(body.span);
        Ok(Stmt::While {
            condition,
            body,
            span,
        })
    }

    pub(super) fn loop_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        self.loop_depth += 1;
        let body = self.block("expected `{` after `loop`");
        self.loop_depth -= 1;
        let body = body?;
        Ok(Stmt::Loop {
            span: start.merge(body.span),
            body,
        })
    }

    pub(super) fn for_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let (binding, binding_span) =
            self.expect_identifier("expected loop binding after `for`")?;
        self.expect(&TokenKind::In, "expected `in` after loop binding")?;
        let iterable = self.expression()?;
        self.loop_depth += 1;
        let body = self.block("expected `{` after for-loop iterator");
        self.loop_depth -= 1;
        let body = body?;
        let span = start.merge(body.span);
        Ok(Stmt::For {
            binding,
            binding_span,
            iterable,
            body,
            span,
        })
    }

    pub(super) fn return_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let value = if self.check(&TokenKind::Semicolon) || self.check(&TokenKind::RightBrace) {
            None
        } else {
            Some(self.expression()?)
        };
        let end = if let Some(token) = self.take(&TokenKind::Semicolon) {
            token.span
        } else if self.check(&TokenKind::RightBrace) || self.is_at_end() {
            value.as_ref().map_or(start, Expr::span)
        } else {
            return Err(self.error_here("expected `;` after return value"));
        };
        Ok(Stmt::Return {
            value,
            span: start.merge(end),
        })
    }

    pub(super) fn break_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        if self.loop_depth == 0 {
            return Err(ParseError {
                message: "`break` can only be used inside a loop".into(),
                span: start,
            });
        }
        let value = if self.check(&TokenKind::Semicolon) || self.check(&TokenKind::RightBrace) {
            None
        } else {
            Some(self.expression()?)
        };
        let end = if let Some(token) = self.take(&TokenKind::Semicolon) {
            token.span
        } else if self.check(&TokenKind::RightBrace) || self.is_at_end() {
            value.as_ref().map_or(start, Expr::span)
        } else {
            return Err(self.error_here("expected `;` after break value"));
        };
        Ok(Stmt::Break {
            value,
            span: start.merge(end),
        })
    }

    pub(super) fn continue_statement(&mut self, start: Span) -> Result<Stmt, ParseError> {
        if self.loop_depth == 0 {
            return Err(ParseError {
                message: "`continue` can only be used inside a loop".into(),
                span: start,
            });
        }
        let end = self
            .expect(&TokenKind::Semicolon, "expected `;` after `continue`")?
            .span;
        Ok(Stmt::Continue {
            span: start.merge(end),
        })
    }

    pub(super) fn expression_statement(&mut self) -> Result<Stmt, ParseError> {
        let expression = self.expression()?;
        let mut terminated = self.take(&TokenKind::Semicolon).is_some();
        let at_boundary = self.check(&TokenKind::RightBrace) || self.is_at_end();
        let is_block_like = matches!(
            expression,
            Expr::If { .. } | Expr::Match { .. } | Expr::Block(_)
        );
        if !terminated && !at_boundary && !is_block_like {
            return Err(self.error_here("expected `;` after expression"));
        }
        if !terminated && !at_boundary && is_block_like {
            terminated = true;
        }
        Ok(Stmt::Expr {
            expression,
            terminated,
        })
    }
}

fn reject_nested_reference(ty: &Type, span: Span) -> Result<(), ParseError> {
    if ty.contains_reference() && !matches!(ty, Type::Reference { .. }) {
        return Err(ParseError {
            message: "references cannot be stored inside owned types".into(),
            span,
        });
    }
    Ok(())
}

fn reject_owned_reference(ty: &Type, span: Span) -> Result<(), ParseError> {
    if ty.contains_reference() {
        return Err(ParseError {
            message: "structs and enums cannot contain reference fields".into(),
            span,
        });
    }
    Ok(())
}

fn reject_return_reference(ty: &Type, span: Span) -> Result<(), ParseError> {
    if ty.contains_reference() {
        return Err(ParseError {
            message: "functions cannot return references".into(),
            span,
        });
    }
    Ok(())
}
