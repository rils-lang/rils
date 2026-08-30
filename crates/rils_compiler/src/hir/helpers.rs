use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn block_statements(
        &mut self,
        block: &Block,
    ) -> Result<Vec<HirStatement>, CompileError> {
        let first_local = self.mutable.len();
        self.scopes.push(HashMap::new());
        let mut statements = self.statements(&block.statements)?;
        self.scopes.pop();
        for local in (first_local..self.mutable.len()).rev() {
            if self.captured.contains(&local) {
                continue;
            }
            statements.push(HirStatement::DropLocal {
                local,
                span: block.span,
            });
        }
        Ok(statements)
    }

    pub(super) fn pattern(&mut self, pattern: &Pattern) -> Result<HirPattern, CompileError> {
        Ok(match pattern {
            Pattern::Wildcard { .. } => HirPattern::Wildcard,
            Pattern::Binding { name, .. } => {
                let local = self.mutable.len();
                self.mutable.push(false);
                self.scopes.last_mut().unwrap().insert(name.clone(), local);
                HirPattern::Binding(local)
            }
            Pattern::Literal { value, .. } => HirPattern::Literal(lower_literal(value)),
            Pattern::Some { inner, .. } => HirPattern::Some(Box::new(self.pattern(inner)?)),
            Pattern::None { .. } => HirPattern::None,
            Pattern::Ok { inner, .. } => HirPattern::Ok(Box::new(self.pattern(inner)?)),
            Pattern::Err { inner, .. } => HirPattern::Err(Box::new(self.pattern(inner)?)),
            Pattern::TupleVariant { path, fields, span } => HirPattern::TupleVariant {
                path: self.canonical_enum_variant_path(path, *span)?,
                fields: fields
                    .iter()
                    .map(|pattern| self.pattern(pattern))
                    .collect::<Result<_, _>>()?,
            },
            Pattern::Record { path, fields, span } => HirPattern::Record {
                path: self.canonical_record_pattern_path(path, *span)?,
                fields: fields
                    .iter()
                    .map(|(name, pattern)| Ok((name.clone(), self.pattern(pattern)?)))
                    .collect::<Result<_, CompileError>>()?,
            },
            Pattern::Path { path, span } => {
                HirPattern::Path(self.canonical_enum_variant_path(path, *span)?)
            }
        })
    }

    pub(super) fn place(&mut self, expression: &Expr) -> Result<HirPlace, CompileError> {
        match expression {
            Expr::Variable { name, span } => Ok(HirPlace {
                local: self.lookup(name).ok_or_else(|| {
                    CompileError::unsupported(format!("unknown local `{name}`"), *span)
                })?,
                projections: Vec::new(),
            }),
            Expr::Member { object, name, span } => {
                let mut place = self.place(object)?;
                place.projections.push(match name.parse::<usize>() {
                    Ok(index) => HirProjection::Index(Box::new(HirExpression::Literal {
                        value: HirLiteral::Usize(index),
                        span: *span,
                    })),
                    Err(_) => HirProjection::Field(name.clone()),
                });
                Ok(place)
            }
            Expr::Index { object, index, .. } => {
                let mut place = self.place(object)?;
                place
                    .projections
                    .push(HirProjection::Index(Box::new(self.expression(index)?)));
                Ok(place)
            }
            Expr::Unary {
                operator: UnaryOp::Dereference,
                operand,
                ..
            } => self.place(operand),
            _ => Err(CompileError::unsupported(
                "place must be rooted in a local value",
                expression.span(),
            )),
        }
    }

    pub(super) fn method_receiver(
        &mut self,
        expression: &Expr,
        receiver: ReceiverMode,
    ) -> Result<HirExpression, CompileError> {
        match receiver {
            ReceiverMode::Owned => self.expression(expression),
            ReceiverMode::Reference { mutable }
                if matches!(
                    self.expression_type(expression),
                    Some(Type::Reference { .. })
                ) =>
            {
                Ok(HirExpression::Reborrow {
                    reference: Box::new(self.expression(expression)?),
                    mutable,
                    span: expression.span(),
                })
            }
            ReceiverMode::Reference { mutable } => match expression {
                Expr::Variable { name, span } => Ok(HirExpression::BorrowLocal {
                    local: self.lookup(name).ok_or_else(|| {
                        CompileError::unsupported(format!("unknown local `{name}`"), *span)
                    })?,
                    mutable,
                    span: *span,
                }),
                Expr::Member { .. } | Expr::Index { .. } => Ok(HirExpression::BorrowPlace {
                    place: self.place(expression)?,
                    mutable,
                    span: expression.span(),
                }),
                _ => Ok(HirExpression::BorrowTemporary {
                    value: Box::new(self.expression(expression)?),
                    mutable,
                    span: expression.span(),
                }),
            },
        }
    }

    pub(super) fn lookup(&self, name: &str) -> Option<LocalId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    pub(super) fn type_id(&self, name: &str, span: Span) -> Result<TypeId, CompileError> {
        self.symbol_id(self.types, name).ok_or_else(|| {
            CompileError::unsupported(format!("unknown bytecode type `{name}`"), span)
        })
    }

    pub(super) fn expression_id(
        &self,
        expression: &Expr,
    ) -> Result<rils_frontend::ExprId, CompileError> {
        self.expression_ids.get(expression).ok_or_else(|| {
            CompileError::new("expression has no semantic identity", expression.span())
        })
    }

    pub(super) fn expression_type(&self, expression: &Expr) -> Option<Type> {
        let id = self.expression_id(expression).ok()?;
        self.typeck_results.expression_type(id).cloned()
    }

    pub(super) fn resolved_definition(&self, id: rils_frontend::ExprId) -> Option<MethodInfo> {
        let rils_frontend::semantic::ResolvedCall::Definition(definition) =
            self.typeck_results.resolved_call(id)?
        else {
            return None;
        };
        self.resolved_definitions.get(definition).copied()
    }

    pub(super) fn resolved_value(&self, id: rils_frontend::ExprId) -> Option<MethodInfo> {
        let definition = self.typeck_results.resolved_value(id)?;
        self.resolved_definitions.get(&definition).copied()
    }

    pub(super) fn resolved_builtin(
        &self,
        expression: rils_frontend::ExprId,
    ) -> Option<(
        rils_builtins::BuiltinId,
        rils_frontend::semantic::BuiltinCallKind,
        Option<rils_builtins::ReceiverMode>,
    )> {
        let rils_frontend::semantic::ResolvedCall::Builtin { id, kind, receiver } =
            self.typeck_results.resolved_call(expression)?
        else {
            return None;
        };
        Some((*id, *kind, *receiver))
    }

    pub(super) fn resolved_import(
        &self,
        id: rils_frontend::ExprId,
    ) -> Option<(&str, &FunctionSignature, &str)> {
        let rils_frontend::semantic::ResolvedCall::Import {
            name,
            signature,
            capability,
        } = self.typeck_results.resolved_call(id)?
        else {
            return None;
        };
        Some((name, signature, capability))
    }

    pub(super) fn resolved_host(&self, id: rils_frontend::ExprId) -> Option<&str> {
        let rils_frontend::semantic::ResolvedCall::Host { path } =
            self.typeck_results.resolved_call(id)?
        else {
            return None;
        };
        Some(path)
    }

    pub(super) fn host_function(
        &self,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<Option<HostFunctionDeclaration>, CompileError> {
        self.host_function_candidates(name)
            .map(|functions| self.select_host_overload(functions, arguments, None, span))
            .transpose()
    }

    pub(super) fn host_function_candidates(
        &self,
        name: &str,
    ) -> Option<&[HostFunctionDeclaration]> {
        let name = self.anchored_name(name);
        if !self.namespace.is_empty() {
            let relative = format!("{}::{name}", self.namespace);
            if let Some(functions) = self.host_functions.get(&relative) {
                return Some(functions);
            }
        }
        self.host_functions.get(&name).map(Vec::as_slice)
    }

    pub(super) fn host_method(
        &self,
        object: &Expr,
        name: &str,
        call_arguments: &[Expr],
        span: Span,
    ) -> Result<Option<HostFunctionDeclaration>, CompileError> {
        let object_type = self.expression_type_for_overload(object)?;
        let Type::Named {
            name: receiver_type,
            arguments: type_arguments,
        } = &object_type
        else {
            return Ok(None);
        };
        if !type_arguments.is_empty() {
            return Ok(None);
        }
        self.host_methods
            .get(&format!("{receiver_type}::{name}"))
            .map(|functions| {
                self.select_host_overload(functions, call_arguments, Some(object), span)
            })
            .transpose()
    }

    pub(super) fn select_host_overload(
        &self,
        candidates: &[HostFunctionDeclaration],
        arguments: &[Expr],
        receiver: Option<&Expr>,
        span: Span,
    ) -> Result<HostFunctionDeclaration, CompileError> {
        let actual_types = receiver
            .into_iter()
            .chain(arguments)
            .map(|argument| self.expression_type_for_overload(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let mut matches = candidates
            .iter()
            .filter_map(|candidate| {
                let expected = candidate.signature.parameters.as_ref()?;
                (expected.len() == actual_types.len())
                    .then(|| overload_score(self.host_contract, expected, &actual_types))?
                    .map(|score| (score, candidate))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|(score, candidate)| (*score, candidate.function_id));
        let name = candidates
            .first()
            .map(|candidate| candidate.name.as_str())
            .unwrap_or("<unknown>");
        let Some((best_score, best)) = matches.first().copied() else {
            return Err(CompileError::unsupported(
                format!(
                    "no host overload of `{name}` accepts ({})\navailable candidates:\n{}",
                    actual_types
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    format_host_candidates(candidates)
                ),
                span,
            ));
        };
        if matches
            .get(1)
            .is_some_and(|(score, _)| *score == best_score)
        {
            return Err(CompileError::unsupported(
                format!(
                    "ambiguous host call `{name}` for arguments ({})\ncandidates:\n{}\nadd explicit type annotations or casts to select one overload",
                    actual_types
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    format_host_candidates(
                        &matches
                            .iter()
                            .take_while(|(score, _)| *score == best_score)
                            .map(|(_, candidate)| (*candidate).clone())
                            .collect::<Vec<_>>()
                    )
                ),
                span,
            ));
        }
        Ok(best.clone())
    }

    pub(super) fn expression_type_for_overload(
        &self,
        expression: &Expr,
    ) -> Result<Type, CompileError> {
        if let Expr::Call {
            callee,
            arguments,
            span,
        } = expression
        {
            let direct_name = match callee.as_ref() {
                Expr::Path { segments, .. } => Some(self.resolve_self_path(segments).join("::")),
                Expr::Variable { name, .. } if self.lookup(name).is_none() => Some(name.clone()),
                _ => None,
            };
            if let Some(name) = direct_name
                && let Some(candidates) = self.host_function_candidates(&name)
            {
                return self
                    .select_host_overload(candidates, arguments, None, *span)
                    .map(|function| function.signature.return_type);
            }
            if let Expr::Member { object, name, .. } = callee.as_ref() {
                let receiver_type = self.expression_type_for_overload(object)?;
                if let Type::Named {
                    name: receiver_type,
                    arguments: type_arguments,
                } = receiver_type
                    && type_arguments.is_empty()
                    && let Some(candidates) =
                        self.host_methods.get(&format!("{receiver_type}::{name}"))
                {
                    return self
                        .select_host_overload(candidates, arguments, Some(object), *span)
                        .map(|function| function.signature.return_type);
                }
            }
        }
        Ok(self.expression_type(expression).unwrap_or(Type::Unknown))
    }

    pub(super) fn symbol_id<T: Copy>(&self, symbols: &HashMap<String, T>, name: &str) -> Option<T> {
        let anchored = self.anchored_name(name);
        if anchored != name {
            return symbols.get(&anchored).copied();
        }
        if !self.namespace.is_empty() {
            let relative = format!("{}::{name}", self.namespace);
            if let Some(id) = symbols.get(&relative).copied() {
                return Some(id);
            }
        }
        symbols.get(name).copied()
    }

    pub(super) fn anchored_name(&self, name: &str) -> String {
        let path = name.split("::").map(str::to_owned).collect::<Vec<_>>();
        let prefix = self
            .namespace
            .split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        resolve_anchored_path(&prefix, &path).unwrap_or_else(|| name.to_owned())
    }

    pub(super) fn resolve_self_path(&self, path: &[String]) -> Vec<String> {
        if path.first().is_some_and(|segment| segment == "Self")
            && let Some(self_type) = &self.self_type
        {
            return self_type
                .split("::")
                .map(str::to_owned)
                .chain(path.iter().skip(1).cloned())
                .collect();
        }
        path.to_vec()
    }

    pub(super) fn enum_variant_path(
        &self,
        path: &[String],
        span: Span,
    ) -> Result<(TypeId, String), CompileError> {
        if path.len() < 2 {
            return Err(CompileError::unsupported(
                "expected an enum variant path",
                span,
            ));
        }
        Ok((
            self.type_id(&path[..path.len() - 1].join("::"), span)?,
            path.last().unwrap().clone(),
        ))
    }

    pub(super) fn canonical_record_pattern_path(
        &self,
        path: &[String],
        span: Span,
    ) -> Result<Vec<String>, CompileError> {
        let name = path.join("::");
        if let Some(type_id) = self.symbol_id(self.types, &name) {
            return Ok(self.canonical_type_path(type_id));
        }
        self.canonical_enum_variant_path(path, span)
    }

    pub(super) fn canonical_enum_variant_path(
        &self,
        path: &[String],
        span: Span,
    ) -> Result<Vec<String>, CompileError> {
        let Some((variant, type_path)) = path.split_last() else {
            return Err(CompileError::unsupported(
                "expected an enum variant path",
                span,
            ));
        };
        let type_id = self.type_id(&type_path.join("::"), span)?;
        let mut canonical = self.canonical_type_path(type_id);
        canonical.push(variant.clone());
        Ok(canonical)
    }

    pub(super) fn canonical_type_path(&self, type_id: TypeId) -> Vec<String> {
        let name = match &self.type_definitions[type_id] {
            HirTypeDefinition::Struct { name, .. } | HirTypeDefinition::Enum { name, .. } => name,
        };
        name.split("::").map(str::to_owned).collect()
    }
}
