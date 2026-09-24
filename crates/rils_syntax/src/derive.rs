use std::collections::HashSet;

use crate::{
    ast::{Block, EnumVariant, Expr, GenericParameter, ImplMethod, Parameter, Program, Stmt},
    parser::ParseError,
    source::Span,
    types::Type,
};

/// A derive expander supplied by a trusted language package.
#[derive(Clone, Copy)]
pub struct NativeDeriveDefinition {
    pub name: &'static str,
    pub expand: fn(&Stmt) -> Result<Option<crate::quote::QuotedStatement>, ParseError>,
}

pub(crate) fn expand_with(
    program: &mut Program,
    native_derives: &[NativeDeriveDefinition],
) -> Result<(), ParseError> {
    expand_scope(&mut program.statements, native_derives)
}

fn expand_scope(
    statements: &mut Vec<Stmt>,
    native_derives: &[NativeDeriveDefinition],
) -> Result<(), ParseError> {
    let mut debug_types = HashSet::new();
    let mut nominal_types = HashSet::new();
    let mut derived_debug = HashSet::new();
    let mut explicit_debug = HashSet::new();
    for statement in statements.iter() {
        match statement {
            Stmt::Struct {
                name, attributes, ..
            }
            | Stmt::Enum {
                name, attributes, ..
            } => {
                nominal_types.insert(name.clone());
                if attributes
                    .iter()
                    .any(|attribute| has_derive(attribute, "Debug"))
                {
                    debug_types.insert(name.clone());
                    derived_debug.insert(name.clone());
                }
            }
            Stmt::Impl {
                trait_name: Some(trait_name),
                target: Type::Named { name, .. },
                ..
            } if trait_leaf(trait_name) == "Debug" => {
                debug_types.insert(name.clone());
                explicit_debug.insert(name.clone());
            }
            _ => {}
        }
    }
    if let Some(name) = derived_debug.intersection(&explicit_debug).next() {
        let span = statements
            .iter()
            .find_map(|statement| match statement {
                Stmt::Struct {
                    name: candidate,
                    span,
                    ..
                } if candidate == name => Some(*span),
                _ => None,
            })
            .unwrap_or_default();
        return Err(ParseError {
            message: format!(
                "type `{name}` cannot both derive Debug and provide an explicit Debug impl"
            ),
            span,
        });
    }
    for statement in statements.iter() {
        let (name, attributes, span) = match statement {
            Stmt::Struct {
                name,
                attributes,
                span,
                ..
            }
            | Stmt::Enum {
                name,
                attributes,
                span,
                ..
            } => (name, attributes, *span),
            _ => continue,
        };
        for definition in native_derives {
            if attributes
                .iter()
                .any(|attribute| has_derive(attribute, definition.name))
                && statements.iter().any(|candidate| {
                    matches!(candidate, Stmt::Impl {
                        trait_name: Some(trait_name),
                        target: Type::Named { name: target, .. },
                        ..
                    } if target == name && trait_leaf(trait_name) == definition.name)
                })
            {
                return Err(ParseError {
                    message: format!(
                        "type `{name}` cannot both derive {} and provide an explicit {} impl",
                        definition.name, definition.name
                    ),
                    span,
                });
            }
        }
    }

    let mut expanded = Vec::with_capacity(statements.len());
    for mut statement in std::mem::take(statements) {
        if let Stmt::Module {
            statements: Some(module_statements),
            ..
        } = &mut statement
        {
            expand_scope(module_statements, native_derives)?;
        }
        let derived = derive_statements(&statement, &debug_types, &nominal_types, native_derives)?;
        expanded.push(statement);
        expanded.extend(derived);
    }
    *statements = expanded;
    Ok(())
}

fn derive_statements(
    statement: &Stmt,
    debug_types: &HashSet<String>,
    nominal_types: &HashSet<String>,
    native_derives: &[NativeDeriveDefinition],
) -> Result<Vec<Stmt>, ParseError> {
    if let Stmt::Function { attributes, .. } = statement {
        if let Some(attribute) = attributes.first() {
            return Err(ParseError {
                message: "attributes are not supported on ordinary functions".into(),
                span: attribute.span,
            });
        }
        return Ok(Vec::new());
    }
    let attributes = match statement {
        Stmt::Struct { attributes, .. } | Stmt::Enum { attributes, .. } => attributes,
        _ => return Ok(Vec::new()),
    };
    validate_attributes(attributes, native_derives)?;
    let mut derived = Vec::new();
    if let Some(debug) = derive_debug_statement(statement, debug_types, nominal_types)? {
        derived.push(debug);
    }
    for definition in native_derives {
        if attributes
            .iter()
            .any(|attribute| has_derive(attribute, definition.name))
        {
            if let Some(quoted) = (definition.expand)(statement)? {
                let origin = match statement {
                    Stmt::Struct { span, .. } | Stmt::Enum { span, .. } => *span,
                    _ => unreachable!("derive attributes occur on types"),
                };
                derived.push(quoted.parse(origin)?);
            }
        }
    }
    Ok(derived)
}

fn has_derive(attribute: &crate::ast::Attribute, name: &str) -> bool {
    attribute.path == ["derive"]
        && attribute
            .arguments
            .iter()
            .any(|argument| argument.len() == 1 && argument[0] == name)
}

fn trait_leaf(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

fn validate_attributes(
    attributes: &[crate::ast::Attribute],
    native_derives: &[NativeDeriveDefinition],
) -> Result<(), ParseError> {
    let mut seen = HashSet::new();
    for attribute in attributes {
        if attribute.path != ["derive"] {
            return Err(ParseError {
                message: format!("unsupported attribute `{}`", attribute.path.join("::")),
                span: attribute.span,
            });
        }
        for argument in &attribute.arguments {
            let name = argument.join("::");
            if name != "Debug"
                && !native_derives
                    .iter()
                    .any(|definition| definition.name == name)
            {
                return Err(ParseError {
                    message: format!("unsupported derive `{name}`"),
                    span: attribute.span,
                });
            }
            if !seen.insert(name.clone()) {
                return Err(ParseError {
                    message: format!("duplicate `{name}` derive"),
                    span: attribute.span,
                });
            }
        }
    }
    Ok(())
}

fn derive_debug_statement(
    statement: &Stmt,
    debug_types: &HashSet<String>,
    nominal_types: &HashSet<String>,
) -> Result<Option<Stmt>, ParseError> {
    let (attributes, name, name_span, generic_parameters, field_types, span) = match statement {
        Stmt::Struct {
            attributes,
            name,
            name_span,
            generic_parameters,
            fields,
            span,
            ..
        } => (
            attributes,
            name,
            name_span,
            generic_parameters,
            fields
                .iter()
                .map(|field| (&field.type_annotation, field.span))
                .collect::<Vec<_>>(),
            *span,
        ),
        Stmt::Enum {
            attributes,
            name,
            name_span,
            generic_parameters,
            variants,
            span,
            ..
        } => {
            let mut types = Vec::new();
            for variant in variants {
                match variant {
                    EnumVariant::Unit { .. } => {}
                    EnumVariant::Tuple { fields, span, .. } => {
                        types.extend(fields.iter().map(|ty| (ty, *span)))
                    }
                    EnumVariant::Record { fields, .. } => types.extend(
                        fields
                            .iter()
                            .map(|field| (&field.type_annotation, field.span)),
                    ),
                }
            }
            (
                attributes,
                name,
                name_span,
                generic_parameters,
                types,
                *span,
            )
        }
        _ => return Ok(None),
    };
    if !attributes
        .iter()
        .any(|attribute| has_derive(attribute, "Debug"))
    {
        return Ok(None);
    }
    let mut impl_generics = generic_parameters.clone();
    for (ty, field_span) in field_types {
        require_debug(
            ty,
            &mut impl_generics,
            debug_types,
            nominal_types,
            name,
            field_span,
        )?;
    }
    let target = Type::Named {
        name: name.clone(),
        arguments: generic_parameters
            .iter()
            .map(|parameter| Type::Variable(parameter.name.clone()))
            .collect(),
    };
    let result_type = Type::Result(Box::new(Type::Unit), Box::new(Type::named("FormatError")));
    let method = ImplMethod {
        attributes: Vec::new(),
        name: "fmt".into(),
        name_span: *name_span,
        generic_parameters: Vec::new(),
        parameters: vec![
            Parameter {
                name: "self".into(),
                mutable: false,
                type_annotation: Some(Type::Reference {
                    mutable: false,
                    inner: Box::new(target.clone()),
                }),
                span,
            },
            Parameter {
                name: "formatter".into(),
                mutable: true,
                type_annotation: Some(Type::Reference {
                    mutable: true,
                    inner: Box::new(Type::named("Formatter")),
                }),
                span,
            },
        ],
        return_type: Some(result_type),
        body: Block {
            statements: vec![Stmt::Expr {
                expression: Expr::Call {
                    callee: Box::new(Expr::Member {
                        object: Box::new(Expr::Variable {
                            name: "formatter".into(),
                            span,
                        }),
                        name: "write_derived_debug".into(),
                        span,
                    }),
                    arguments: vec![Expr::Variable {
                        name: "self".into(),
                        span,
                    }],
                    span,
                },
                terminated: false,
            }],
            span,
        },
        span,
    };
    Ok(Some(Stmt::Impl {
        generic_parameters: impl_generics,
        trait_name: Some("Debug".into()),
        target,
        associated_types: Vec::new(),
        methods: vec![method],
        span,
    }))
}

fn require_debug(
    ty: &Type,
    generics: &mut [GenericParameter],
    debug_types: &HashSet<String>,
    nominal_types: &HashSet<String>,
    owner: &str,
    span: Span,
) -> Result<(), ParseError> {
    let supported = match ty {
        Type::Function { .. } => false,
        Type::Variable(name) => generics
            .iter_mut()
            .find(|parameter| parameter.name == *name)
            .is_some_and(|parameter| {
                if !parameter.bounds.iter().any(|bound| bound == "Debug") {
                    parameter.bounds.push("Debug".into());
                }
                true
            }),
        Type::Tuple(elements) => elements
            .iter()
            .all(|ty| require_debug(ty, generics, debug_types, nominal_types, owner, span).is_ok()),
        Type::Array { element, .. }
        | Type::Option(element)
        | Type::Reference { inner: element, .. } => {
            require_debug(element, generics, debug_types, nominal_types, owner, span).is_ok()
        }
        Type::Result(ok, error) => {
            require_debug(ok, generics, debug_types, nominal_types, owner, span).is_ok()
                && require_debug(error, generics, debug_types, nominal_types, owner, span).is_ok()
        }
        Type::Named { name, arguments } => {
            (!nominal_types.contains(name) || debug_types.contains(name))
                && arguments.iter().all(|ty| {
                    require_debug(ty, generics, debug_types, nominal_types, owner, span).is_ok()
                })
        }
        _ => true,
    };
    if supported {
        Ok(())
    } else {
        Err(ParseError {
            message: format!(
                "cannot derive Debug for `{owner}`: field type `{ty}` does not implement Debug"
            ),
            span,
        })
    }
}
