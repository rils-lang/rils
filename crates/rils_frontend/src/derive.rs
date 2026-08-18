use std::collections::HashSet;

use crate::{
    ast::{Block, Expr, GenericParameter, ImplMethod, Literal, NamedField, Program, Stmt},
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
    let mut derived_types = HashSet::new();
    let mut explicit_types = HashSet::new();
    for statement in statements.iter() {
        let statement = unwrap_public(statement);
        match statement {
            Stmt::Struct {
                name, attributes, ..
            } if attributes.iter().any(is_default_derive) => {
                default_types.insert(name.clone());
                derived_types.insert(name.clone());
            }
            Stmt::Impl {
                trait_name: Some(trait_name),
                target: Type::Named { name, .. },
                ..
            } if trait_name == "Default" => {
                default_types.insert(name.clone());
                explicit_types.insert(name.clone());
            }
            _ => {}
        }
    }
    if let Some(name) = derived_types.intersection(&explicit_types).next() {
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
                "type `{name}` cannot both derive Default and provide an explicit Default impl"
            ),
            span,
        });
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
        let derived = derive_statement(unwrap_public(&statement), &default_types)?;
        expanded.push(statement);
        if let Some(derived) = derived {
            expanded.push(derived);
        }
    }
    *statements = expanded;
    Ok(())
}

fn derive_statement(
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
    let mut derives_default = false;
    for attribute in attributes {
        if attribute.path != ["derive"] {
            return Err(ParseError {
                message: format!("unsupported attribute `{}`", attribute.path.join("::")),
                span: attribute.span,
            });
        }
        for argument in &attribute.arguments {
            if argument.len() == 1 && argument[0] == "Default" {
                if derives_default {
                    return Err(ParseError {
                        message: "duplicate `Default` derive".into(),
                        span: attribute.span,
                    });
                }
                derives_default = true;
            } else {
                return Err(ParseError {
                    message: format!("unsupported derive `{}`", argument.join("::")),
                    span: attribute.span,
                });
            }
        }
    }
    if !derives_default {
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
                Ok((
                    field.name.clone(),
                    default_expression(&field.type_annotation, field.span)?,
                ))
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

fn is_default_derive(attribute: &crate::ast::Attribute) -> bool {
    attribute.path == ["derive"]
        && attribute
            .arguments
            .iter()
            .any(|argument| argument.len() == 1 && argument[0] == "Default")
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
