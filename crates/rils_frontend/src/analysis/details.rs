use super::*;

pub(super) fn generic_parameters_detail(parameters: &[GenericParameter]) -> String {
    if parameters.is_empty() {
        return String::new();
    }
    let parameters = parameters
        .iter()
        .map(|parameter| {
            if parameter.bounds.is_empty() {
                parameter.name.clone()
            } else {
                format!("{}: {}", parameter.name, parameter.bounds.join(" + "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{parameters}>")
}

pub(super) fn parameter_detail(parameter: &Parameter) -> String {
    if parameter.name == "self"
        && let Some(Type::Reference { mutable, .. }) = &parameter.type_annotation
    {
        return if *mutable {
            "&mut self".into()
        } else {
            "&self".into()
        };
    }
    let name = if parameter.mutable {
        format!("mut {}", parameter.name)
    } else {
        parameter.name.clone()
    };
    parameter
        .type_annotation
        .as_ref()
        .map(|ty| format!("{name}: {ty}"))
        .unwrap_or(name)
}

pub(super) fn function_detail(
    name: &str,
    generic_parameters: &[GenericParameter],
    parameters: &[Parameter],
    return_type: Option<&Type>,
) -> String {
    let generic_parameters = generic_parameters_detail(generic_parameters);
    let parameters = parameters
        .iter()
        .map(parameter_detail)
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = return_type
        .map(|ty| format!(" -> {ty}"))
        .unwrap_or_default();
    format!("fn {name}{generic_parameters}({parameters}){return_type}")
}

pub(super) const MAX_HOVER_MEMBERS: usize = 8;

pub(super) fn hover_member_lines<T>(
    members: &[T],
    member_name: &str,
    render: impl Fn(&T) -> String,
) -> String {
    let mut lines = members
        .iter()
        .take(MAX_HOVER_MEMBERS)
        .map(|member| format!("    {},", render(member)))
        .collect::<Vec<_>>();
    let omitted = members.len().saturating_sub(MAX_HOVER_MEMBERS);
    if omitted > 0 {
        lines.push(format!("    // ... {omitted} more {member_name}"));
    }
    lines.join("\n")
}

pub(super) fn struct_detail(
    name: &str,
    generic_parameters: &[GenericParameter],
    fields: &[NamedField],
) -> String {
    let generic_parameters = generic_parameters_detail(generic_parameters);
    if fields.is_empty() {
        return format!("struct {name}{generic_parameters}");
    }
    let fields = hover_member_lines(fields, "fields", |field| {
        format!("{}: {}", field.name, field.type_annotation)
    });
    format!("struct {name}{generic_parameters} {{\n{fields}\n}}")
}

pub(super) fn enum_variant_detail(variant: &EnumVariant) -> String {
    match variant {
        EnumVariant::Unit { name, .. } => name.clone(),
        EnumVariant::Tuple { name, fields, .. } => format!(
            "{name}({})",
            fields
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        EnumVariant::Record { name, fields, .. } => format!(
            "{name} {{ {} }}",
            fields
                .iter()
                .map(|field| format!("{}: {}", field.name, field.type_annotation))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

pub(super) fn enum_variant_name_and_span(variant: &EnumVariant) -> (&str, Span) {
    match variant {
        EnumVariant::Unit { name, span }
        | EnumVariant::Tuple { name, span, .. }
        | EnumVariant::Record { name, span, .. } => {
            let name_span = if span.start == span.end {
                *span
            } else {
                Span::in_source(span.source, span.start, span.start + name.len())
            };
            (name, name_span)
        }
    }
}

pub(super) fn enum_variant_declaration(enum_name: &str, variant: &EnumVariant) -> String {
    format!("{enum_name}::{}", enum_variant_detail(variant))
}

pub(super) fn enum_detail(
    name: &str,
    generic_parameters: &[GenericParameter],
    variants: &[EnumVariant],
) -> String {
    let generic_parameters = generic_parameters_detail(generic_parameters);
    if variants.is_empty() {
        return format!("enum {name}{generic_parameters}");
    }
    let variants = hover_member_lines(variants, "variants", enum_variant_detail);
    format!("enum {name}{generic_parameters} {{\n{variants}\n}}")
}

pub(super) fn associated_type_detail(associated: &AssociatedType) -> String {
    let generic_parameters = generic_parameters_detail(&associated.generic_parameters);
    let value = associated
        .value
        .as_ref()
        .map(|value| format!(" = {value}"))
        .unwrap_or_default();
    format!("type {}{generic_parameters}{value}", associated.name)
}

pub(super) fn trait_method_detail(method: &TraitMethod) -> String {
    function_detail(
        &method.name,
        &method.generic_parameters,
        &method.parameters,
        method.return_type.as_ref(),
    )
}

pub(super) fn impl_method_detail(method: &ImplMethod) -> String {
    function_detail(
        &method.name,
        &method.generic_parameters,
        &method.parameters,
        method.return_type.as_ref(),
    )
}

pub(super) fn trait_detail(
    name: &str,
    bounds: &[String],
    associated_types: &[AssociatedType],
    methods: &[TraitMethod],
) -> String {
    let bounds = if bounds.is_empty() {
        String::new()
    } else {
        format!(": {}", bounds.join(" + "))
    };
    let members = associated_types
        .iter()
        .map(|associated| format!("    {};", associated_type_detail(associated)))
        .chain(
            methods
                .iter()
                .map(|method| format!("    {};", trait_method_detail(method))),
        )
        .collect::<Vec<_>>();
    if members.is_empty() {
        format!("trait {name}{bounds}")
    } else {
        format!("trait {name}{bounds} {{\n{}\n}}", members.join("\n"))
    }
}

pub(super) fn member_name_span(span: Span, name: &str) -> Span {
    Span::new(span.end.saturating_sub(name.len()), span.end)
}

pub(super) fn source_path_segment_span(path: &[String], index: usize, span: Span) -> Span {
    let canonical_length =
        path.iter().map(String::len).sum::<usize>() + path.len().saturating_sub(1) * 2;
    if canonical_length == span.end.saturating_sub(span.start) {
        let start = span.start
            + path[..index]
                .iter()
                .map(|segment| segment.len() + 2)
                .sum::<usize>();
        return Span::in_source(span.source, start, start + path[index].len());
    }

    // Host resolution expands an imported type's first source token into its
    // canonical module segments. Recover the original token from the suffix,
    // whose spelling and length are unchanged.
    let suffix_length = path[index + 1..]
        .iter()
        .map(|segment| segment.len() + 2)
        .sum::<usize>();
    Span::in_source(
        span.source,
        span.start,
        span.end.saturating_sub(suffix_length),
    )
}

pub(super) fn collect_self_type_references(program: &Program) -> HashMap<Span, String> {
    fn visit(
        statements: &[Stmt],
        references: &[crate::ast::TypeReference],
        output: &mut HashMap<Span, String>,
    ) {
        for statement in statements {
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => visit(statements, references, output),
                Stmt::Impl {
                    target: Type::Named { name, .. },
                    span,
                    ..
                } => {
                    for reference in references.iter().filter(|reference| {
                        reference.name == "Self"
                            && reference.span.source == span.source
                            && span.start <= reference.span.start
                            && reference.span.end <= span.end
                    }) {
                        output.insert(reference.span, name.clone());
                    }
                }
                _ => {}
            }
        }
    }

    let mut output = HashMap::new();
    visit(&program.statements, &program.type_references, &mut output);
    output
}

pub(super) fn hash_key_type_supported(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Bool
            | Type::Char
            | Type::String
            | Type::Integer(_)
            | Type::IntegerVariable(_)
            | Type::Variable(_)
            | Type::Unknown
    )
}
