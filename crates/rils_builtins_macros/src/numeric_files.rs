use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use rils_syntax::ast::{Attribute, ImplMethod, Stmt};
use syn::{Error, Ident, LitStr, Token, parse::Parse, parse_macro_input};

use crate::{builtin_files, builtin_ids};

mod keyword {
    syn::custom_keyword!(complete);
    syn::custom_keyword!(family);
}

struct Input {
    config_path: LitStr,
    source_path: LitStr,
    prefix: LitStr,
    family: Ident,
    visibility: syn::Visibility,
    intrinsics: Ident,
    constants: Ident,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let config_path = input.parse()?;
        input.parse::<Token![;]>()?;
        let source_path = input.parse()?;
        input.parse::<Token![;]>()?;
        input.parse::<keyword::complete>()?;
        let prefix = input.parse()?;
        input.parse::<Token![;]>()?;
        input.parse::<keyword::family>()?;
        let family = input.parse()?;
        input.parse::<Token![;]>()?;
        let visibility = input.parse()?;
        input.parse::<Token![const]>()?;
        let intrinsics = input.parse()?;
        input.parse::<Token![,]>()?;
        let constants = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(Self {
            config_path,
            source_path,
            prefix,
            family,
            visibility,
            intrinsics,
            constants,
        })
    }
}

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    match expand_input(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand_input(input: Input) -> syn::Result<proc_macro2::TokenStream> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
        Error::new(
            input.source_path.span(),
            "CARGO_MANIFEST_DIR is unavailable",
        )
    })?;
    let source_path = PathBuf::from(manifest).join(input.source_path.value());
    let source = fs::read_to_string(&source_path).map_err(|error| {
        Error::new(
            input.source_path.span(),
            format!("failed to read `{}`: {error}", source_path.display()),
        )
    })?;
    let tokens = rils_syntax::lex(&source)
        .map_err(|error| Error::new(input.source_path.span(), error.message))?;
    let program = rils_syntax::parser::parse_builtin_declarations(tokens)
        .map_err(|error| Error::new(input.source_path.span(), error.message))?;
    let (_, configured) = builtin_ids::load(&input.config_path.value())
        .map_err(|error| Error::new(input.config_path.span(), error))?;

    let prefix = input.prefix.value();
    let direct_prefix = format!("{prefix}::");
    let expected = configured
        .keys()
        .filter(|path| {
            path.strip_prefix(&direct_prefix)
                .is_some_and(|name| !name.contains("::"))
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let family_name = input.family.to_string();
    let expected_primitives = expected_primitives(&family_name).ok_or_else(|| {
        Error::new(
            input.family.span(),
            "numeric family must be `Integer` or `Float`",
        )
    })?;
    let mut primitive_impls = BTreeMap::new();
    for statement in &program.statements {
        let Stmt::Impl {
            target, methods, ..
        } = public_inner(statement)
        else {
            continue;
        };
        let Some(primitive) = primitive_name(target) else {
            continue;
        };
        if primitive_impls
            .insert(primitive, methods.as_slice())
            .is_some()
        {
            return Err(Error::new(
                input.source_path.span(),
                format!("numeric primitive `{primitive}` is declared more than once"),
            ));
        }
    }
    let actual_primitives = primitive_impls.keys().copied().collect::<BTreeSet<_>>();
    let expected_primitive_set = expected_primitives.iter().copied().collect::<BTreeSet<_>>();
    if actual_primitives != expected_primitive_set {
        let missing = expected_primitive_set
            .difference(&actual_primitives)
            .copied()
            .collect::<Vec<_>>();
        let unexpected = actual_primitives
            .difference(&expected_primitive_set)
            .copied()
            .collect::<Vec<_>>();
        return Err(Error::new(
            input.source_path.span(),
            format!(
                "{family_name} declarations must cover exactly [{}]; missing [{}], unexpected [{}]",
                expected_primitives.join(", "),
                missing.join(", "),
                unexpected.join(", ")
            ),
        ));
    }
    let canonical_primitive = expected_primitives[0];
    let methods = primitive_impls[canonical_primitive];
    for primitive in &expected_primitives[1..] {
        let other = primitive_impls[primitive];
        if methods.len() != other.len()
            || methods
                .iter()
                .zip(other)
                .any(|(left, right)| !same_method_declaration(left, right))
        {
            return Err(Error::new(
                input.source_path.span(),
                format!(
                    "numeric primitive `{primitive}` does not expose the same declarations as `{canonical_primitive}`"
                ),
            ));
        }
    }

    let mut declared = BTreeSet::new();
    let mut intrinsics = Vec::new();
    let mut constants = Vec::new();
    for method in methods {
        if let Some(attribute) = method
            .attributes
            .iter()
            .find(|attribute| attribute.path.as_slice() != ["constant"])
        {
            return Err(Error::new(
                input.source_path.span(),
                format!(
                    "unsupported numeric member attribute `{}` on `{}`",
                    attribute.path.join("::"),
                    method.name
                ),
            ));
        }
        if has_attribute(&method.attributes, "constant") {
            constants.push(constant_tokens(method, &source, &family_name)?);
            continue;
        }
        let path = format!("{prefix}::{}", method.name);
        let Some((id, _)) = configured.get(&path) else {
            return Err(Error::new(
                input.source_path.span(),
                format!("intrinsic `{}` has no configured ID `{path}`", method.name),
            ));
        };
        declared.insert(path.clone());
        intrinsics.push(intrinsic_tokens(method, *id, &source)?);
    }
    let missing = expected.difference(&declared).cloned().collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(Error::new(
            input.source_path.span(),
            format!("numeric declaration is incomplete: {}", missing.join(", ")),
        ));
    }

    let source_literal = LitStr::new(&source_path.to_string_lossy(), input.source_path.span());
    let visibility = input.visibility;
    let intrinsics_name = input.intrinsics;
    let constants_name = input.constants;
    let constant_type = format_ident!("{family_name}ConstantDeclaration");
    Ok(quote! {
        const _: &str = include_str!(#source_literal);
        #visibility const #intrinsics_name: &[IntrinsicDeclaration] = &[#(#intrinsics),*];
        #visibility const #constants_name: &[#constant_type] = &[#(#constants),*];
    })
}

fn expected_primitives(family: &str) -> Option<Vec<&'static str>> {
    match family {
        "Integer" => Some(
            rils_syntax::IntegerType::ALL
                .iter()
                .map(|kind| kind.name())
                .collect(),
        ),
        "Float" => Some(vec!["f32", "f64"]),
        _ => None,
    }
}

fn primitive_name(target: &rils_syntax::Type) -> Option<&'static str> {
    match target {
        rils_syntax::Type::Integer(kind) => Some(kind.name()),
        rils_syntax::Type::Float(kind) => Some(kind.name()),
        _ => None,
    }
}

fn same_method_declaration(left: &ImplMethod, right: &ImplMethod) -> bool {
    left.name == right.name
        && left.return_type == right.return_type
        && left.generic_parameters.len() == right.generic_parameters.len()
        && left
            .generic_parameters
            .iter()
            .zip(&right.generic_parameters)
            .all(|(left, right)| left.name == right.name && left.bounds == right.bounds)
        && left.parameters.len() == right.parameters.len()
        && left
            .parameters
            .iter()
            .zip(&right.parameters)
            .all(|(left, right)| {
                left.name == right.name
                    && left.mutable == right.mutable
                    && left.type_annotation == right.type_annotation
            })
        && left.attributes.len() == right.attributes.len()
        && left
            .attributes
            .iter()
            .zip(&right.attributes)
            .all(|(left, right)| left.path == right.path && left.arguments == right.arguments)
}

fn public_inner(statement: &Stmt) -> &Stmt {
    match statement {
        Stmt::Public { statement, .. } => public_inner(statement),
        other => other,
    }
}

fn has_attribute(attributes: &[Attribute], name: &str) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.path.as_slice() == [name])
}

fn intrinsic_tokens(
    method: &ImplMethod,
    id: u32,
    source: &str,
) -> syn::Result<proc_macro2::TokenStream> {
    let receiver = method
        .parameters
        .first()
        .is_some_and(|parameter| parameter.name == "self");
    let parameters = method.parameters[usize::from(receiver)..]
        .iter()
        .map(|parameter| {
            parameter
                .type_annotation
                .as_ref()
                .ok_or_else(|| {
                    Error::new(
                        proc_macro2::Span::call_site(),
                        format!("parameter `{}` requires a type", parameter.name),
                    )
                })
                .and_then(builtin_files::type_tokens)
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let result = method
        .return_type
        .as_ref()
        .map(builtin_files::type_tokens)
        .transpose()?
        .unwrap_or_else(|| quote!(TypePattern::Unit));
    let kind = if receiver {
        quote!(IntrinsicKind::Method)
    } else {
        quote!(IntrinsicKind::AssociatedFunction)
    };
    let name = LitStr::new(&method.name, proc_macro2::Span::call_site());
    let documentation = LitStr::new(
        &builtin_files::documentation(source, method.span),
        proc_macro2::Span::call_site(),
    );
    Ok(quote! {
        IntrinsicDeclaration {
            id: BuiltinId::from_raw(#id),
            name: #name,
            kind: #kind,
            signature: BuiltinSignature {
                parameters: &[#(#parameters),*],
                result: #result,
                variadic: false,
            },
            documentation: #documentation,
        }
    })
}

fn constant_tokens(
    method: &ImplMethod,
    source: &str,
    family: &str,
) -> syn::Result<proc_macro2::TokenStream> {
    if !method.parameters.is_empty() {
        return Err(Error::new(
            proc_macro2::Span::call_site(),
            format!("numeric constant `{}` cannot have parameters", method.name),
        ));
    }
    let variant = format_ident!("{}", pascal_case(&method.name));
    let id_type = format_ident!("{family}ConstantId");
    let name = LitStr::new(&method.name, proc_macro2::Span::call_site());
    let documentation_span = method
        .attributes
        .first()
        .map_or(method.span, |attribute| attribute.span.merge(method.span));
    let documentation = LitStr::new(
        &builtin_files::documentation(source, documentation_span),
        proc_macro2::Span::call_site(),
    );
    if family == "Integer" {
        let value_type = method
            .return_type
            .as_ref()
            .ok_or_else(|| Error::new(proc_macro2::Span::call_site(), "constant requires a type"))
            .and_then(builtin_files::type_tokens)?;
        Ok(quote! {
            IntegerConstantDeclaration {
                id: #id_type::#variant,
                name: #name,
                value_type: #value_type,
                documentation: #documentation,
            }
        })
    } else {
        Ok(quote! {
            FloatConstantDeclaration {
                id: #id_type::#variant,
                name: #name,
                documentation: #documentation,
            }
        })
    }
}

fn pascal_case(name: &str) -> String {
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| {
                    first.to_uppercase().collect::<String>()
                        + &characters.as_str().to_ascii_lowercase()
                })
                .unwrap_or_default()
        })
        .collect()
}
