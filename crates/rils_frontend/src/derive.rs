use std::collections::HashSet;

use crate::{
    ast::{
        Block, EnumVariant, Expr, GenericParameter, ImplMethod, Literal, NamedField, Parameter,
        Program, Stmt,
    },
    default::{DefaultPlan, default_plan},
    parser::ParseError,
    source::Span,
    types::Type,
};

pub(crate) fn expand(program: &mut Program) -> Result<(), ParseError> {
    expand_scope(&mut program.statements)
}

fn expand_scope(statements: &mut Vec<Stmt>) -> Result<(), ParseError> {
    let mut default_types = HashSet::new();
    let mut debug_types = HashSet::new();
    let mut nominal_types = HashSet::new();
    let mut derived_defaults = HashSet::new();
    let mut derived_debug = HashSet::new();
    let mut explicit_defaults = HashSet::new();
    let mut explicit_debug = HashSet::new();
    for statement in statements.iter() {
        let statement = unwrap_public(statement);
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
                    .any(|attribute| has_derive(attribute, "Default"))
                {
                    default_types.insert(name.clone());
                    derived_defaults.insert(name.clone());
                }
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
            } if trait_leaf(trait_name) == "Default" => {
                default_types.insert(name.clone());
                explicit_defaults.insert(name.clone());
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
    for (derived, explicit, trait_name) in [
        (&derived_defaults, &explicit_defaults, "Default"),
        (&derived_debug, &explicit_debug, "Debug"),
    ] {
        if let Some(name) = derived.intersection(explicit).next() {
            let span = statements
                .iter()
                .map(unwrap_public)
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
                    "type `{name}` cannot both derive {trait_name} and provide an explicit {trait_name} impl"
                ),
                span,
            });
        }
    }

    let mut expanded = Vec::with_capacity(statements.len());
    for mut statement in std::mem::take(statements) {
        if let Stmt::Module {
            statements: Some(module_statements),
            ..
        } = unwrap_public_mut(&mut statement)
        {
            expand_scope(module_statements)?;
        }
        let derived = derive_statements(
            unwrap_public(&statement),
            &default_types,
            &debug_types,
            &nominal_types,
        )?;
        expanded.push(statement);
        expanded.extend(derived);
    }
    *statements = expanded;
    Ok(())
}

fn derive_statements(
    statement: &Stmt,
    default_types: &HashSet<String>,
    debug_types: &HashSet<String>,
    nominal_types: &HashSet<String>,
) -> Result<Vec<Stmt>, ParseError> {
    let attributes = match statement {
        Stmt::Struct { attributes, .. } | Stmt::Enum { attributes, .. } => attributes,
        _ => return Ok(Vec::new()),
    };
    validate_attributes(attributes)?;
    if matches!(statement, Stmt::Enum { .. })
        && attributes
            .iter()
            .any(|attribute| has_derive(attribute, "Default"))
    {
        return Err(ParseError {
            message: "Default can currently only be derived for structs".into(),
            span: attributes
                .iter()
                .find(|attribute| has_derive(attribute, "Default"))
                .map(|attribute| attribute.span)
                .unwrap_or_default(),
        });
    }
    let mut derived = Vec::new();
    if let Some(default) = derive_default_statement(statement, default_types)? {
        derived.push(default);
    }
    if let Some(debug) = derive_debug_statement(statement, debug_types, nominal_types)? {
        derived.push(debug);
    }
    Ok(derived)
}

fn derive_default_statement(
    statement: &Stmt,
    default_types: &HashSet<String>,
) -> Result<Option<Stmt>, ParseError> {
    let Stmt::Struct {
        attributes,
        name,
        name_span,
        generic_parameters,
        fields,
        span,
    } = statement
    else {
        return Ok(None);
    };
    if !attributes
        .iter()
        .any(|attribute| has_derive(attribute, "Default"))
    {
        return Ok(None);
    }

    let mut impl_generics = generic_parameters.clone();
    for field in fields {
        require_default(
            &field.type_annotation,
            &mut impl_generics,
            default_types,
            field,
            name,
        )?;
    }
    let type_arguments = generic_parameters
        .iter()
        .map(|parameter| Type::Variable(parameter.name.clone()))
        .collect();
    let target = Type::Named {
        name: name.clone(),
        arguments: type_arguments,
    };
    let body_expression = Expr::RecordLiteral {
        path: vec![name.clone()],
        fields: fields
            .iter()
            .map(|field| {
                Ok(crate::ast::RecordField {
                    name: field.name.clone(),
                    name_span: field.span,
                    value: default_expression(&field.type_annotation, field.span)?,
                })
            })
            .collect::<Result<Vec<_>, ParseError>>()?,
        span: *span,
    };
    let method = ImplMethod {
        name: "default".into(),
        name_span: *name_span,
        generic_parameters: Vec::new(),
        parameters: Vec::new(),
        return_type: Some(target.clone()),
        body: Block {
            statements: vec![Stmt::Expr {
                expression: body_expression,
                terminated: false,
            }],
            span: *span,
        },
        span: *span,
    };
    Ok(Some(Stmt::Impl {
        generic_parameters: impl_generics,
        trait_name: Some("Default".into()),
        target,
        associated_types: Vec::new(),
        methods: vec![method],
        span: *span,
    }))
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

fn validate_attributes(attributes: &[crate::ast::Attribute]) -> Result<(), ParseError> {
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
            if !matches!(name.as_str(), "Default" | "Debug") {
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

fn require_default(
    ty: &Type,
    generics: &mut [GenericParameter],
    default_types: &HashSet<String>,
    field: &NamedField,
    owner: &str,
) -> Result<(), ParseError> {
    let supported = match default_plan(ty) {
        Some(DefaultPlan::TraitCall(Type::Named { name, .. })) => default_types.contains(&name),
        Some(DefaultPlan::TraitCall(Type::Variable(name))) => {
            if let Some(parameter) = generics.iter_mut().find(|parameter| parameter.name == name) {
                if !parameter.bounds.iter().any(|bound| bound == "Default") {
                    parameter.bounds.push("Default".into());
                }
                true
            } else {
                false
            }
        }
        Some(DefaultPlan::TraitCall(_)) | None => false,
        Some(_) => true,
    };
    if supported {
        Ok(())
    } else {
        Err(ParseError {
            message: format!(
                "cannot derive Default for `{owner}`: field `{}` of type `{ty}` does not implement Default",
                field.name
            ),
            span: field.span,
        })
    }
}

fn default_expression(ty: &Type, span: Span) -> Result<Expr, ParseError> {
    let plan = default_plan(ty).ok_or_else(|| ParseError {
        message: format!("type `{ty}` does not implement Default"),
        span,
    })?;
    default_expression_from_plan(&plan, span)
}

fn default_expression_from_plan(plan: &DefaultPlan, span: Span) -> Result<Expr, ParseError> {
    let literal = |value| Expr::Literal { value, span };
    Ok(match plan {
        DefaultPlan::Unit => literal(Literal::Unit),
        DefaultPlan::Bool => literal(Literal::Bool(false)),
        DefaultPlan::Integer(crate::types::IntegerType::I8) => literal(Literal::I8(0)),
        DefaultPlan::Integer(crate::types::IntegerType::I16) => literal(Literal::I16(0)),
        DefaultPlan::Integer(crate::types::IntegerType::I32) => literal(Literal::I32(0)),
        DefaultPlan::Integer(crate::types::IntegerType::I64) => literal(Literal::I64(0)),
        DefaultPlan::Integer(crate::types::IntegerType::I128) => literal(Literal::I128(0)),
        DefaultPlan::Integer(crate::types::IntegerType::Isize) => literal(Literal::Isize(0)),
        DefaultPlan::Integer(crate::types::IntegerType::U8) => literal(Literal::U8(0)),
        DefaultPlan::Integer(crate::types::IntegerType::U16) => literal(Literal::U16(0)),
        DefaultPlan::Integer(crate::types::IntegerType::U32) => literal(Literal::U32(0)),
        DefaultPlan::Integer(crate::types::IntegerType::U64) => literal(Literal::U64(0)),
        DefaultPlan::Integer(crate::types::IntegerType::U128) => literal(Literal::U128(0)),
        DefaultPlan::Integer(crate::types::IntegerType::Usize) => literal(Literal::Usize(0)),
        DefaultPlan::Float(crate::types::FloatType::F32) => literal(Literal::F32(0.0)),
        DefaultPlan::Float(crate::types::FloatType::F64) => literal(Literal::F64(0.0)),
        DefaultPlan::Char => literal(Literal::Char('\0')),
        DefaultPlan::String => literal(Literal::String(String::new())),
        DefaultPlan::Tuple(elements) => Expr::Tuple {
            elements: elements
                .iter()
                .map(|element| default_expression_from_plan(element, span))
                .collect::<Result<_, _>>()?,
            span,
        },
        DefaultPlan::Array {
            element, length, ..
        } => Expr::Array {
            elements: (0..*length)
                .map(|_| default_expression_from_plan(element, span))
                .collect::<Result<_, _>>()?,
            repeat: None,
            span,
        },
        DefaultPlan::Option(_) => Expr::Variable {
            name: "None".into(),
            span,
        },
        DefaultPlan::EmptyCollection { name, .. } => Expr::Call {
            callee: Box::new(Expr::Path {
                segments: vec![name.clone(), "new".into()],
                span,
            }),
            arguments: Vec::new(),
            span,
        },
        DefaultPlan::TraitCall(ty) => Expr::Call {
            callee: Box::new(Expr::QualifiedPath {
                target: ty.clone(),
                trait_name: "Default".into(),
                member: "default".into(),
                span,
            }),
            arguments: Vec::new(),
            span,
        },
    })
}

fn unwrap_public(statement: &Stmt) -> &Stmt {
    match statement {
        Stmt::Public { statement, .. } => statement,
        statement => statement,
    }
}

fn unwrap_public_mut(statement: &mut Stmt) -> &mut Stmt {
    match statement {
        Stmt::Public { statement, .. } => statement,
        statement => statement,
    }
}
