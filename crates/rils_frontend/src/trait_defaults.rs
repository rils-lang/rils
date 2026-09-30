//! Expands trait method bodies into implementations before shared analysis.

use std::{collections::HashMap, sync::OnceLock};

use crate::{
    ast::{
        AssociatedType, Block, GenericParameter, ImplMethod, Program, Stmt, TraitMethod, Visibility,
    },
    types::Type,
};

/// Adds callable copies of the exported Iterator defaults for the bytecode
/// backend's concrete owned iterator representation.
pub fn append_bytecode_iterator_defaults(
    program: &mut Program,
    owner_source: rils_syntax::SourceId,
) {
    let Some(methods) =
        builtin_methods_for_impl("Iterator", owner_source, &mut program.generated_sources)
    else {
        return;
    };
    let item = Type::Variable("T".into());
    let target = Type::Named {
        name: "OwnedIterator".into(),
        arguments: vec![item.clone()],
    };
    let associated_types = vec![AssociatedType {
        name: "Item".into(),
        name_span: rils_syntax::Span::default(),
        generic_parameters: Vec::new(),
        value: Some(item),
        span: rils_syntax::Span::default(),
    }];
    for method in methods.into_iter().filter(|method| method.body.is_some()) {
        let Some(mut body) = method.body else {
            continue;
        };
        resolve_block_types(&mut body, &target, "Iterator", &associated_types);
        let mut generics = vec![GenericParameter {
            is_const: false,
            name: "T".into(),
            bounds: Vec::new(),
            span: method.span,
        }];
        generics.extend(method.generic_parameters);
        let parameters = method
            .parameters
            .into_iter()
            .map(|mut parameter| {
                parameter.type_annotation = parameter
                    .type_annotation
                    .map(|ty| resolve_associated_type(ty, &target, "Iterator", &associated_types));
                if parameter.name == "self" && parameter.type_annotation.is_none() {
                    parameter.type_annotation = Some(target.clone());
                }
                parameter
            })
            .collect();
        program.statements.push(Stmt::Function {
            visibility: Visibility::Private,
            attributes: Vec::new(),
            name: format!("@iterator_{}", method.name),
            name_span: method.name_span,
            generic_parameters: generics,
            parameters,
            return_type: method
                .return_type
                .map(|ty| resolve_associated_type(ty, &target, "Iterator", &associated_types)),
            body,
            span: method.span,
        });
    }
}

pub(crate) fn expand(program: &mut Program, source_tokens: &[rils_syntax::token::Token]) {
    let mut defaults = builtin_defaults().clone();
    let mut local_trait_spans = HashMap::new();
    collect_local_defaults(&program.statements, &mut defaults, &mut local_trait_spans);
    expand_statements(
        &mut program.statements,
        &defaults,
        &local_trait_spans,
        source_tokens,
        &mut program.generated_sources,
    );
}

fn builtin_defaults() -> &'static HashMap<String, Vec<TraitMethod>> {
    static DEFAULTS: OnceLock<HashMap<String, Vec<TraitMethod>>> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        let mut defaults = HashMap::new();
        for declaration in rils_builtins::BUILTINS {
            if declaration.kind != rils_builtins::BuiltinKind::Trait {
                continue;
            }
            let Some(source) = declaration.source else {
                continue;
            };
            let tokens = rils_syntax::lex(source).expect("generated trait source must lex");
            let program = rils_syntax::parser::parse_builtin_declarations(tokens)
                .expect("generated trait source must parse");
            for statement in program.statements {
                if let Stmt::Trait { name, methods, .. } = statement {
                    defaults.insert(name, methods);
                }
            }
        }
        defaults
    })
}

fn collect_local_defaults(
    statements: &[Stmt],
    defaults: &mut HashMap<String, Vec<TraitMethod>>,
    local_trait_spans: &mut HashMap<String, rils_syntax::Span>,
) {
    for statement in statements {
        match statement {
            Stmt::Trait {
                name,
                methods,
                span,
                ..
            } => {
                defaults.insert(name.clone(), methods.clone());
                local_trait_spans.insert(name.clone(), *span);
            }
            Stmt::Module {
                statements: Some(children),
                ..
            } => collect_local_defaults(children, defaults, local_trait_spans),
            _ => {}
        }
    }
}

fn expand_statements(
    statements: &mut [Stmt],
    defaults: &HashMap<String, Vec<TraitMethod>>,
    local_trait_spans: &HashMap<String, rils_syntax::Span>,
    source_tokens: &[rils_syntax::token::Token],
    generated_sources: &mut Vec<rils_syntax::SourceFile>,
) {
    for statement in statements {
        match statement {
            Stmt::Impl {
                trait_name: Some(trait_name),
                target,
                associated_types,
                methods,
                span,
                ..
            } => {
                let name = trait_name.rsplit("::").next().unwrap_or(trait_name);
                if let Some(provided) = defaults.get(name) {
                    let unique_methods = local_trait_spans
                        .get(name)
                        .and_then(|trait_span| {
                            local_methods_for_impl(
                                name,
                                *trait_span,
                                span.source,
                                source_tokens,
                                generated_sources,
                            )
                        })
                        .or_else(|| builtin_methods_for_impl(name, span.source, generated_sources));
                    for method in unique_methods.as_ref().unwrap_or(provided) {
                        if let Some(body) = &method.body
                            && !methods
                                .iter()
                                .any(|implementation| implementation.name == method.name)
                        {
                            let mut body = body.clone();
                            resolve_block_types(&mut body, target, trait_name, associated_types);
                            methods.push(ImplMethod {
                                attributes: Vec::new(),
                                name: method.name.clone(),
                                name_span: method.name_span,
                                generic_parameters: method.generic_parameters.clone(),
                                parameters: method
                                    .parameters
                                    .iter()
                                    .cloned()
                                    .map(|mut parameter| {
                                        parameter.type_annotation =
                                            parameter.type_annotation.map(|ty| {
                                                resolve_associated_type(
                                                    ty,
                                                    target,
                                                    trait_name,
                                                    associated_types,
                                                )
                                            });
                                        parameter
                                    })
                                    .collect(),
                                return_type: method.return_type.clone().map(|ty| {
                                    resolve_associated_type(
                                        ty,
                                        target,
                                        trait_name,
                                        associated_types,
                                    )
                                }),
                                body,
                                span: method.span,
                            });
                        }
                    }
                }
            }
            Stmt::Module {
                statements: Some(children),
                ..
            } => expand_statements(
                children,
                defaults,
                local_trait_spans,
                source_tokens,
                generated_sources,
            ),
            _ => {}
        }
    }
}

fn builtin_methods_for_impl(
    name: &str,
    owner_source: rils_syntax::SourceId,
    generated_sources: &mut Vec<rils_syntax::SourceFile>,
) -> Option<Vec<TraitMethod>> {
    let source = rils_builtins::BUILTINS
        .iter()
        .find(|declaration| {
            declaration.kind == rils_builtins::BuiltinKind::Trait
                && declaration.path.rsplit("::").next() == Some(name)
        })?
        .source?;
    let source_id = generated_source_id("stdlib", name, owner_source, generated_sources);
    let tokens = rils_syntax::lex_with_source_id(source, source_id)
        .expect("generated trait source must lex");
    rils_syntax::parser::parse_builtin_declarations(tokens)
        .expect("generated trait source must parse")
        .statements
        .into_iter()
        .find_map(|statement| match statement {
            Stmt::Trait { methods, .. } => Some(methods),
            _ => None,
        })
}

fn local_methods_for_impl(
    name: &str,
    trait_span: rils_syntax::Span,
    owner_source: rils_syntax::SourceId,
    source_tokens: &[rils_syntax::token::Token],
    generated_sources: &mut Vec<rils_syntax::SourceFile>,
) -> Option<Vec<TraitMethod>> {
    let source_id = generated_source_id("trait", name, owner_source, generated_sources);
    let tokens = source_tokens
        .iter()
        .filter(|token| {
            token.span.source == trait_span.source
                && token.span.start >= trait_span.start
                && token.span.end <= trait_span.end
        })
        .cloned()
        .map(|mut token| {
            token.span.source = source_id;
            token
        })
        .collect();
    rils_syntax::parse(tokens)
        .expect("source trait was already parsed")
        .statements
        .into_iter()
        .find_map(|statement| match statement {
            Stmt::Trait { methods, .. } => Some(methods),
            _ => None,
        })
}

fn generated_source_id(
    origin: &str,
    name: &str,
    owner_source: rils_syntax::SourceId,
    generated_sources: &mut Vec<rils_syntax::SourceFile>,
) -> rils_syntax::SourceId {
    let serial = u32::try_from(generated_sources.len() + 1).expect("too many generated sources");
    assert!(serial <= u16::MAX.into(), "too many generated sources");
    let source_id = rils_syntax::SourceId(0x8000_0000 | ((owner_source.0 & 0x7fff) << 16) | serial);
    generated_sources.push(rils_syntax::SourceFile {
        id: source_id,
        name: format!(
            "<{origin}:{name}:source:{}:default:{serial}>",
            owner_source.0
        ),
    });
    source_id
}

fn resolve_block_types(
    block: &mut Block,
    target: &Type,
    trait_name: &str,
    associated_types: &[AssociatedType],
) {
    for statement in &mut block.statements {
        match statement {
            Stmt::Let {
                type_annotation, ..
            } => {
                if let Some(ty) = type_annotation.take() {
                    *type_annotation = Some(resolve_associated_type(
                        ty,
                        target,
                        trait_name,
                        associated_types,
                    ));
                }
            }
            Stmt::Loop { body, .. } | Stmt::While { body, .. } | Stmt::For { body, .. } => {
                resolve_block_types(body, target, trait_name, associated_types);
            }
            _ => {}
        }
    }
}

pub(crate) fn resolve_associated_type(
    ty: Type,
    target: &Type,
    trait_name: &str,
    associated_types: &[AssociatedType],
) -> Type {
    match ty {
        Type::Associated {
            base,
            trait_name: projection_trait,
            name,
            arguments,
        } if matches!(base.as_ref(), Type::Named { name, arguments } if name == "Self" && arguments.is_empty())
            && projection_trait.as_deref().is_none_or(|projection| {
                projection.rsplit("::").next() == trait_name.rsplit("::").next()
            })
            && arguments.is_empty() =>
        {
            associated_types
                .iter()
                .find(|associated| associated.name == name)
                .and_then(|associated| associated.value.clone())
                .unwrap_or(Type::Associated {
                    base,
                    trait_name: projection_trait,
                    name,
                    arguments,
                })
        }
        Type::Named { name, arguments } if name == "Self" && arguments.is_empty() => target.clone(),
        Type::Option(inner) => Type::Option(Box::new(resolve_associated_type(
            *inner,
            target,
            trait_name,
            associated_types,
        ))),
        Type::Result(ok, error) => Type::Result(
            Box::new(resolve_associated_type(
                *ok,
                target,
                trait_name,
                associated_types,
            )),
            Box::new(resolve_associated_type(
                *error,
                target,
                trait_name,
                associated_types,
            )),
        ),
        Type::Reference { mutable, inner } => Type::Reference {
            mutable,
            inner: Box::new(resolve_associated_type(
                *inner,
                target,
                trait_name,
                associated_types,
            )),
        },
        Type::Tuple(elements) => Type::Tuple(
            elements
                .into_iter()
                .map(|element| {
                    resolve_associated_type(element, target, trait_name, associated_types)
                })
                .collect(),
        ),
        Type::Array { element, length } => Type::Array {
            element: Box::new(resolve_associated_type(
                *element,
                target,
                trait_name,
                associated_types,
            )),
            length,
        },
        Type::Slice(element) => Type::Slice(Box::new(resolve_associated_type(
            *element,
            target,
            trait_name,
            associated_types,
        ))),
        Type::Function {
            parameters,
            return_type,
        } => Type::Function {
            parameters: parameters.map(|parameters| {
                parameters
                    .into_iter()
                    .map(|parameter| {
                        resolve_associated_type(parameter, target, trait_name, associated_types)
                    })
                    .collect()
            }),
            return_type: Box::new(resolve_associated_type(
                *return_type,
                target,
                trait_name,
                associated_types,
            )),
        },
        Type::Named { name, arguments } => Type::Named {
            name: if name == "Iterator" {
                "OwnedIterator".into()
            } else {
                name
            },
            arguments: arguments
                .into_iter()
                .map(|argument| {
                    resolve_associated_type(argument, target, trait_name, associated_types)
                })
                .collect(),
        },
        Type::Associated {
            base,
            trait_name: projection_trait,
            name,
            arguments,
        } => Type::Associated {
            base: Box::new(resolve_associated_type(
                *base,
                target,
                trait_name,
                associated_types,
            )),
            trait_name: projection_trait,
            name,
            arguments: arguments
                .into_iter()
                .map(|argument| {
                    resolve_associated_type(argument, target, trait_name, associated_types)
                })
                .collect(),
        },
        other => other,
    }
}
