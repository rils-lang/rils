//! Infer scoped statements and their explicit or inferred return types.

use super::*;

impl Inferencer<'_> {
    pub(super) fn statements(&mut self, statements: &[Stmt], returns: &mut Vec<Type>) -> Type {
        let mut result = Type::Unit;
        for statement in statements {
            result = self.statement(statement, returns);
            if !matches!(
                statement,
                Stmt::Expr {
                    terminated: false,
                    ..
                }
            ) {
                result = Type::Unit;
            }
        }
        result
    }

    fn statement(&mut self, statement: &Stmt, returns: &mut Vec<Type>) -> Type {
        match statement {
            Stmt::Module {
                name,
                name_span,
                statements,
                ..
            } => {
                self.define_binding(
                    name,
                    *name_span,
                    Binding {
                        constructor: None,
                        ty: Type::Unknown,
                    },
                );
                if let Some(statements) = statements {
                    self.module_path.push(name.clone());
                    self.with_scope_value(|inferencer| inferencer.statements(statements, returns));
                    self.module_path.pop();
                }
                Type::Unit
            }
            Stmt::Use { imports, .. } => {
                for import in imports {
                    if import.kind == crate::ast::UseImportKind::Glob {
                        let prefix = format!("{}::", import.path.join("::"));
                        let mut bindings = self
                            .host_functions
                            .iter()
                            .filter_map(|(path, signature)| {
                                let name = path.strip_prefix(&prefix)?;
                                (!name.contains("::"))
                                    .then(|| (name.to_owned(), signature.as_type()))
                            })
                            .collect::<Vec<_>>();
                        for builtin in rils_builtins::BUILTINS.iter().filter(|builtin| {
                            builtin.path.starts_with(&prefix)
                                && !builtin.path[prefix.len()..].contains("::")
                        }) {
                            let name = builtin.path[prefix.len()..].to_owned();
                            let ty =
                                crate::standard_library::standard_function_signature(builtin.path)
                                    .map_or(Type::Unknown, |signature| signature.as_type());
                            bindings.push((name, ty));
                        }
                        for (name, ty) in bindings {
                            self.scopes.last_mut().expect("scope exists").insert(
                                name.clone(),
                                Binding {
                                    constructor: constructors::standard_constructor(&format!(
                                        "{prefix}{name}"
                                    )),
                                    ty,
                                },
                            );
                        }
                        continue;
                    }
                    let name = import.binding_name().expect("single use import");
                    let name_span = import.alias_span.unwrap_or(import.name_span);
                    let path_name = import.path.join("::");
                    let ty = self
                        .host_functions
                        .get(&path_name)
                        .cloned()
                        .or_else(|| {
                            crate::standard_library::standard_function_signature(&path_name)
                        })
                        .map_or_else(
                            || {
                                if name.chars().next().is_some_and(char::is_uppercase) {
                                    Type::named(&path_name)
                                } else {
                                    Type::Unknown
                                }
                            },
                            |signature| signature.as_type(),
                        );
                    self.define_binding(
                        name,
                        name_span,
                        Binding {
                            constructor: constructors::standard_constructor(&path_name),
                            ty,
                        },
                    );
                }
                Type::Unit
            }
            Stmt::Let {
                name,
                name_span,
                type_annotation,
                initializer,
                ..
            } => {
                let inferred = self.expression(initializer, returns);
                if let Some(expected) = type_annotation {
                    let expected = self.syntax_type(expected);
                    self.unify(&inferred, &expected);
                    self.apply_expected_type(initializer, &expected);
                }
                let ty = type_annotation
                    .as_ref()
                    .map(|ty| self.syntax_type(ty))
                    .unwrap_or(inferred);
                self.define_binding(
                    name,
                    *name_span,
                    Binding {
                        constructor: None,
                        ty: ty.clone(),
                    },
                );
                if type_annotation.is_none() {
                    self.type_hint(*name_span, ty, ": ");
                }
                Type::Unit
            }
            Stmt::Function {
                name,
                name_span,
                generic_parameters,
                parameters,
                return_type,
                body,
                ..
            } => {
                let bound_types = generic_parameters
                    .iter()
                    .filter(|parameter| !parameter.bounds.is_empty())
                    .map(|parameter| {
                        (
                            parameter.name.clone(),
                            Type::BoundVariable {
                                name: parameter.name.clone(),
                                bounds: parameter.bounds.clone(),
                            },
                        )
                    })
                    .collect::<HashMap<_, _>>();
                let parameter_types = parameters
                    .iter()
                    .map(|parameter| {
                        self.optional_syntax_type(parameter.type_annotation.as_ref())
                            .substitute(&bound_types)
                    })
                    .collect::<Vec<_>>();
                let declared_return = return_type.as_ref().map(|ty| self.syntax_type(ty));
                self.scopes.last_mut().expect("scope exists").insert(
                    name.clone(),
                    Binding {
                        constructor: None,
                        ty: Type::function(
                            parameter_types.clone(),
                            declared_return.clone().unwrap_or(Type::Unknown),
                        ),
                    },
                );
                let resolved = self.with_scope_value(|inferencer| {
                    for parameter in parameters {
                        let ty =
                            inferencer.optional_syntax_type(parameter.type_annotation.as_ref());
                        inferencer.define_binding(
                            &parameter.name,
                            parameter.span,
                            Binding {
                                constructor: None,
                                ty,
                            },
                        );
                    }
                    let mut explicit_returns = Vec::new();
                    let tail = inferencer.block_contents(body, &mut explicit_returns);
                    let resolved = declared_return
                        .clone()
                        .unwrap_or_else(|| inferred_return(explicit_returns, tail));
                    inferencer.apply_function_return_type(body, &resolved);
                    resolved
                });
                let signature = Type::function(parameter_types, resolved.clone());
                self.result
                    .binding_types
                    .insert(*name_span, signature.clone());
                if return_type.is_none() && is_known(&resolved) {
                    self.result.hints.push(RawTypeHint {
                        position: body.span.start,
                        span: *name_span,
                        ty: resolved.clone(),
                        prefix: " -> ",
                    });
                }
                if let Some(binding) = self.scopes.last_mut().and_then(|scope| scope.get_mut(name))
                {
                    binding.ty = signature;
                }
                Type::Unit
            }
            Stmt::Struct {
                name, name_span, ..
            }
            | Stmt::Enum {
                name, name_span, ..
            } => {
                self.scopes.last_mut().expect("scope exists").insert(
                    name.clone(),
                    Binding {
                        constructor: None,
                        ty: Type::Named {
                            name: name.clone(),
                            arguments: Vec::new(),
                        },
                    },
                );
                self.result.binding_types.insert(
                    *name_span,
                    Type::Named {
                        name: name.clone(),
                        arguments: Vec::new(),
                    },
                );
                Type::Unit
            }
            Stmt::TypeAlias { .. } => Type::Unit,
            Stmt::Impl {
                target, methods, ..
            } => {
                let target = self.syntax_type(target);
                for method in methods {
                    self.with_scope_value(|inferencer| {
                        inferencer.scopes.last_mut().expect("scope exists").insert(
                            "Self".into(),
                            Binding {
                                constructor: None,
                                ty: target.clone(),
                            },
                        );
                        let parameter_types = method
                            .parameters
                            .iter()
                            .map(|parameter| inferencer.impl_parameter_type(parameter, &target))
                            .collect::<Vec<_>>();
                        for parameter in &method.parameters {
                            let ty = inferencer.impl_parameter_type(parameter, &target);
                            inferencer.define_binding(
                                &parameter.name,
                                parameter.span,
                                Binding {
                                    constructor: None,
                                    ty,
                                },
                            );
                        }
                        let mut method_returns = Vec::new();
                        let tail = inferencer.block_contents(&method.body, &mut method_returns);
                        let resolved = method
                            .return_type
                            .as_ref()
                            .map(|ty| resolve_impl_self(&inferencer.syntax_type(ty), &target))
                            .unwrap_or_else(|| inferred_return(method_returns, tail));
                        inferencer.apply_function_return_type(&method.body, &resolved);
                        inferencer.result.binding_types.insert(
                            method.name_span,
                            Type::function(parameter_types, resolved.clone()),
                        );
                        if method.return_type.is_none() && is_known(&resolved) {
                            inferencer.result.hints.push(RawTypeHint {
                                position: method.body.span.start,
                                span: method.name_span,
                                ty: resolved,
                                prefix: " -> ",
                            });
                        }
                    });
                }
                Type::Unit
            }
            Stmt::Trait { methods, .. } => {
                for method in methods {
                    if let Some(return_type) = &method.return_type {
                        self.result.binding_types.insert(
                            method.name_span,
                            Type::function(
                                method
                                    .parameters
                                    .iter()
                                    .map(|parameter| {
                                        self.optional_syntax_type(
                                            parameter.type_annotation.as_ref(),
                                        )
                                    })
                                    .collect(),
                                self.syntax_type(return_type),
                            ),
                        );
                    }
                }
                Type::Unit
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expression(condition, returns);
                self.block(body, returns);
                Type::Unit
            }
            Stmt::Loop { body, .. } => {
                self.with_scope_value(|inferencer| inferencer.block_contents(body, returns));
                Type::Unknown
            }
            Stmt::For {
                binding,
                binding_span,
                iterable,
                body,
                ..
            } => {
                let iterable_type = self.expression(iterable, returns);
                let item_type = self.iterable_item_type(&iterable_type);
                self.with_scope_value(|inferencer| {
                    inferencer.define_binding(
                        binding,
                        *binding_span,
                        Binding {
                            constructor: None,
                            ty: item_type.clone(),
                        },
                    );
                    inferencer.type_hint(*binding_span, item_type, ": ");
                    inferencer.block_contents(body, returns);
                });
                Type::Unit
            }
            Stmt::Return { value, .. } => {
                let return_type = if let Some(value) = value {
                    self.expression(value, returns)
                } else {
                    Type::Unit
                };
                returns.push(return_type);
                Type::Unit
            }
            Stmt::Break { value, .. } => value
                .as_ref()
                .map(|value| self.expression(value, returns))
                .unwrap_or(Type::Unit),
            Stmt::Continue { .. } => Type::Unit,
            Stmt::Expr { expression, .. } => self.expression(expression, returns),
        }
    }

    pub(super) fn block(&mut self, block: &Block, returns: &mut Vec<Type>) -> Type {
        self.with_scope_value(|inferencer| inferencer.block_contents(block, returns))
    }

    pub(super) fn block_contents(&mut self, block: &Block, returns: &mut Vec<Type>) -> Type {
        self.statements(&block.statements, returns)
    }
}
