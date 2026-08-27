use std::{collections::BTreeSet, fs, path::PathBuf};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use rils_syntax::{
    Span, Type,
    ast::{AssociatedType, Attribute, EnumVariant, ImplMethod, Parameter, Stmt, TraitMethod},
};
use syn::{Error, Ident, LitStr, Token, parse::Parse, parse_macro_input};

use crate::builtin_ids;

mod keyword {
    syn::custom_keyword!(backend);
    syn::custom_keyword!(complete);
    syn::custom_keyword!(kind);
    syn::custom_keyword!(path);
    syn::custom_keyword!(partial);
}

struct Input {
    config_path: LitStr,
    source_path: LitStr,
    prefix: LitStr,
    require_complete: bool,
    kind: Option<Ident>,
    declaration_path: Option<LitStr>,
    backend: Ident,
    visibility: syn::Visibility,
    name: Ident,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let config_path = input.parse()?;
        input.parse::<Token![;]>()?;
        let source_path = input.parse()?;
        input.parse::<Token![;]>()?;
        let require_complete = if input.peek(keyword::complete) {
            input.parse::<keyword::complete>()?;
            true
        } else if input.peek(keyword::partial) {
            input.parse::<keyword::partial>()?;
            false
        } else {
            return Err(input.error("expected `complete` or `partial`"));
        };
        let prefix = input.parse()?;
        input.parse::<Token![;]>()?;
        let kind = if input.peek(keyword::kind) {
            input.parse::<keyword::kind>()?;
            let kind = input.parse()?;
            input.parse::<Token![;]>()?;
            Some(kind)
        } else {
            None
        };
        let declaration_path = if input.peek(keyword::path) {
            input.parse::<keyword::path>()?;
            let path = input.parse()?;
            input.parse::<Token![;]>()?;
            Some(path)
        } else {
            None
        };
        input.parse::<keyword::backend>()?;
        let backend = input.parse()?;
        input.parse::<Token![;]>()?;
        let visibility = input.parse()?;
        input.parse::<Token![const]>()?;
        let name = input.parse()?;
        input.parse::<Token![;]>()?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after built-in file declaration"));
        }
        Ok(Self {
            config_path,
            source_path,
            prefix,
            require_complete,
            kind,
            declaration_path,
            backend,
            visibility,
            name,
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
    let tokens = rils_syntax::lex(&source).map_err(|error| {
        Error::new(
            input.source_path.span(),
            format!(
                "failed to lex `{}`: {}",
                source_path.display(),
                error.message
            ),
        )
    })?;
    let program = rils_syntax::parser::parse_builtin_declarations(tokens).map_err(|error| {
        Error::new(
            input.source_path.span(),
            format!(
                "failed to parse `{}`: {}",
                source_path.display(),
                error.message
            ),
        )
    })?;

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
    if input.require_complete && expected.is_empty() {
        return Err(Error::new(
            input.prefix.span(),
            format!("built-in group `{prefix}` does not exist or contains no IDs"),
        ));
    }

    let mut members = Vec::new();
    let mut declared = BTreeSet::new();
    let mut declaration = None;
    let mut primitive_declaration: Option<(String, Vec<String>, String, proc_macro2::TokenStream)> =
        None;
    for statement in &program.statements {
        match public_inner(statement) {
            Stmt::Enum {
                name,
                generic_parameters,
                variants,
                ..
            } => {
                if declaration.is_some() {
                    return Err(Error::new(
                        input.source_path.span(),
                        "a built-in file must contain exactly one type declaration",
                    ));
                }
                declaration = Some((
                    name.clone(),
                    generic_parameters
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .collect::<Vec<_>>(),
                    documentation(source.as_str(), statement_span(statement)),
                    quote!(BuiltinKind::Enum),
                ));
                members.extend(
                    variants
                        .iter()
                        .map(|variant| variant_tokens(variant, &source))
                        .collect::<syn::Result<Vec<_>>>()?,
                );
            }
            Stmt::Struct {
                name,
                generic_parameters,
                fields,
                ..
            } => {
                if declaration.is_some() {
                    return Err(Error::new(
                        input.source_path.span(),
                        "a built-in file must contain exactly one type declaration",
                    ));
                }
                declaration = Some((
                    name.clone(),
                    generic_parameters
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .collect::<Vec<_>>(),
                    documentation(source.as_str(), statement_span(statement)),
                    quote!(BuiltinKind::Struct),
                ));
                members.extend(
                    fields
                        .iter()
                        .map(|field| field_tokens(field, &source))
                        .collect::<syn::Result<Vec<_>>>()?,
                );
            }
            Stmt::Trait {
                name,
                associated_types,
                methods: trait_methods,
                ..
            } => {
                if declaration.is_some() {
                    return Err(Error::new(
                        input.source_path.span(),
                        "a built-in file must contain exactly one type declaration",
                    ));
                }
                declaration = Some((
                    name.clone(),
                    Vec::new(),
                    documentation(source.as_str(), statement_span(statement)),
                    quote!(BuiltinKind::Trait),
                ));
                members.extend(
                    associated_types
                        .iter()
                        .map(|associated| associated_type_tokens(associated, &source))
                        .collect::<syn::Result<Vec<_>>>()?,
                );
                for method in trait_methods {
                    let default_path = format!("{prefix}::{}", method.name);
                    let path =
                        member_builtin_path(&method.name, &method.attributes, &default_path)?;
                    validate_member_path(
                        path.as_deref(),
                        &configured,
                        &mut declared,
                        &input,
                        &method.name,
                    )?;
                    members.push(trait_method_tokens(method, path.as_deref(), &source)?);
                }
            }
            Stmt::Impl {
                generic_parameters,
                target,
                methods,
                ..
            } => {
                if let Some(path) = primitive_path(target) {
                    let candidate = (
                        path.to_owned(),
                        generic_parameters
                            .iter()
                            .map(|parameter| parameter.name.clone())
                            .collect::<Vec<_>>(),
                        documentation(source.as_str(), statement_span(statement)),
                        quote!(BuiltinKind::Primitive),
                    );
                    if let Some(existing) = &primitive_declaration {
                        if existing.0 != candidate.0 {
                            return Err(Error::new(
                                input.source_path.span(),
                                "a built-in file cannot describe multiple primitive types",
                            ));
                        }
                    }
                    primitive_declaration = Some(candidate);
                }
                for method in methods {
                    let default_path = format!("{prefix}::{}", method.name);
                    let path =
                        member_builtin_path(&method.name, &method.attributes, &default_path)?;
                    if let Some(path) = &path
                        && !configured.contains_key(path)
                    {
                        return Err(Error::new(
                            input.source_path.span(),
                            format!(
                                "method `{}` has no built-in ID `{path}` in `{}`",
                                method.name,
                                input.config_path.value()
                            ),
                        ));
                    }
                    if path
                        .as_ref()
                        .is_some_and(|path| !declared.insert(path.clone()))
                    {
                        return Err(Error::new(
                            input.source_path.span(),
                            format!(
                                "built-in ID `{}` is declared more than once",
                                path.as_deref().unwrap_or_default()
                            ),
                        ));
                    }
                    members.push(method_tokens(method, path.as_deref(), &source)?);
                }
            }
            _ => {}
        }
    }
    let missing = expected.difference(&declared).cloned().collect::<Vec<_>>();
    if input.require_complete && !missing.is_empty() {
        return Err(Error::new(
            input.source_path.span(),
            format!(
                "built-in file `{}` is incomplete; missing declarations: {}",
                input.source_path.value(),
                missing.join(", ")
            ),
        ));
    }
    let declaration = declaration.or(primitive_declaration);
    let Some((path, type_parameters, type_documentation, inferred_kind)) = declaration else {
        return Err(Error::new(
            input.source_path.span(),
            "a built-in file requires one struct, enum, or primitive impl declaration",
        ));
    };

    let source_literal = LitStr::new(&source_path.to_string_lossy(), input.source_path.span());
    let visibility = input.visibility;
    let name = input.name;
    let members_name = format_ident!("{name}_MEMBERS");
    let path = input
        .declaration_path
        .unwrap_or_else(|| LitStr::new(&path, input.source_path.span()));
    let type_parameters = type_parameters
        .iter()
        .map(|parameter| LitStr::new(parameter, input.source_path.span()))
        .collect::<Vec<_>>();
    let type_documentation = LitStr::new(&type_documentation, input.source_path.span());
    let backend = input.backend;
    let kind = input
        .kind
        .map(|kind| quote!(BuiltinKind::#kind))
        .unwrap_or(inferred_kind);
    Ok(quote! {
        const _: &str = include_str!(#source_literal);
        const #members_name: &[BuiltinMember] = &[#(#members),*];
        #visibility const #name: BuiltinDeclaration = BuiltinDeclaration {
            path: #path,
            kind: #kind,
            type_parameters: &[#(#type_parameters),*],
            members: #members_name,
            signature: None,
            backend: BuiltinBackend::#backend,
            documentation: #type_documentation,
        };
    })
}

fn public_inner(statement: &Stmt) -> &Stmt {
    match statement {
        Stmt::Public { statement, .. } => public_inner(statement),
        other => other,
    }
}

fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Public { span, .. } => *span,
        Stmt::Enum { span, .. }
        | Stmt::Struct { span, .. }
        | Stmt::Impl { span, .. }
        | Stmt::Trait { span, .. } => *span,
        _ => Span::default(),
    }
}

fn primitive_path(target: &Type) -> Option<&'static str> {
    match target {
        Type::String => Some("string"),
        Type::Integer(kind) => Some(kind.name()),
        Type::Float(kind) => Some(kind.name()),
        _ => None,
    }
}

fn member_builtin_path(
    name: &str,
    attributes: &[Attribute],
    default_path: &str,
) -> syn::Result<Option<String>> {
    if let Some(attribute) = attributes.iter().find(|attribute| {
        attribute.path.len() != 1
            || !matches!(
                attribute.path[0].as_str(),
                "metadata" | "runtime" | "import" | "provided"
            )
    }) {
        return Err(Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "unsupported built-in member attribute `{}` on `{}`",
                attribute.path.join("::"),
                name
            ),
        ));
    }
    let metadata = find_attribute(attributes, "metadata");
    let runtime = find_attribute(attributes, "runtime");
    let import = find_attribute(attributes, "import");
    if usize::from(metadata.is_some())
        + usize::from(runtime.is_some())
        + usize::from(import.is_some())
        > 1
    {
        return Err(Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "built-in member `{}` cannot combine metadata, runtime, and import backends",
                name
            ),
        ));
    }
    if let Some(attribute) = metadata {
        if !attribute.arguments.is_empty() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!("`metadata` on `{name}` does not accept arguments"),
            ));
        }
        return Ok(None);
    }
    if let Some(attribute) = runtime {
        let [path] = attribute.arguments.as_slice() else {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!("`runtime` on `{name}` requires exactly one path"),
            ));
        };
        return Ok(Some(path.join("::")));
    }
    if import.is_some() {
        return Ok(None);
    }
    Ok(Some(default_path.to_owned()))
}

fn member_runtime_import(
    name: &str,
    attributes: &[Attribute],
) -> syn::Result<proc_macro2::TokenStream> {
    let Some(attribute) = find_attribute(attributes, "import") else {
        return Ok(quote!(None));
    };
    let [path] = attribute.arguments.as_slice() else {
        return Err(Error::new(
            proc_macro2::Span::call_site(),
            format!("`import` on `{name}` requires exactly one path"),
        ));
    };
    let path = LitStr::new(&path.join("::"), proc_macro2::Span::call_site());
    Ok(quote!(Some(#path)))
}

fn validate_member_path(
    path: Option<&str>,
    configured: &builtin_ids::Members,
    declared: &mut BTreeSet<String>,
    input: &Input,
    member_name: &str,
) -> syn::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    if !configured.contains_key(path) {
        return Err(Error::new(
            input.source_path.span(),
            format!(
                "member `{member_name}` has no built-in ID `{path}` in `{}`",
                input.config_path.value()
            ),
        ));
    }
    if !declared.insert(path.to_owned()) {
        return Err(Error::new(
            input.source_path.span(),
            format!("built-in ID `{path}` is declared more than once"),
        ));
    }
    Ok(())
}

fn find_attribute<'a>(attributes: &'a [Attribute], name: &str) -> Option<&'a Attribute> {
    attributes
        .iter()
        .find(|attribute| attribute.path.as_slice() == [name])
}

fn variant_tokens(variant: &EnumVariant, source: &str) -> syn::Result<proc_macro2::TokenStream> {
    let (name, value_type, span) = match variant {
        EnumVariant::Unit { name, span } => (name, quote!(TypePattern::Unit), *span),
        EnumVariant::Tuple { name, fields, span } if fields.len() == 1 => {
            (name, type_tokens(&fields[0])?, *span)
        }
        EnumVariant::Tuple { span, .. } => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "built-in tuple variants require exactly one field at {}",
                    span.start
                ),
            ));
        }
        EnumVariant::Record { name, span, .. } => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "record variant `{name}` is not supported in built-in files at {}",
                    span.start
                ),
            ));
        }
    };
    let name = LitStr::new(name, proc_macro2::Span::call_site());
    let documentation = LitStr::new(&documentation(source, span), proc_macro2::Span::call_site());
    Ok(quote! {
        BuiltinMember {
            name: #name,
            kind: BuiltinMemberKind::Variant,
            signature: None,
            value_type: Some(#value_type),
            receiver: None,
            builtin_id: None,
            runtime_import: None,
            required: false,
            type_parameters: &[],
            documentation: #documentation,
        }
    })
}

fn field_tokens(
    field: &rils_syntax::ast::NamedField,
    source: &str,
) -> syn::Result<proc_macro2::TokenStream> {
    let name = LitStr::new(&field.name, proc_macro2::Span::call_site());
    let value_type = type_tokens(&field.type_annotation)?;
    let documentation = LitStr::new(
        &documentation(source, field.span),
        proc_macro2::Span::call_site(),
    );
    Ok(quote! {
        BuiltinMember {
            name: #name,
            kind: BuiltinMemberKind::Field,
            signature: None,
            value_type: Some(#value_type),
            receiver: None,
            builtin_id: None,
            runtime_import: None,
            required: false,
            type_parameters: &[],
            documentation: #documentation,
        }
    })
}

fn method_tokens(
    method: &ImplMethod,
    path: Option<&str>,
    source: &str,
) -> syn::Result<proc_macro2::TokenStream> {
    function_member_tokens(
        &method.name,
        &method.generic_parameters,
        &method.parameters,
        method.return_type.as_ref(),
        &method.attributes,
        method.span,
        path,
        source,
    )
}

fn trait_method_tokens(
    method: &TraitMethod,
    path: Option<&str>,
    source: &str,
) -> syn::Result<proc_macro2::TokenStream> {
    function_member_tokens(
        &method.name,
        &method.generic_parameters,
        &method.parameters,
        method.return_type.as_ref(),
        &method.attributes,
        method.span,
        path,
        source,
    )
}

#[allow(clippy::too_many_arguments)]
fn function_member_tokens(
    method_name: &str,
    generic_parameters: &[rils_syntax::ast::GenericParameter],
    method_parameters: &[Parameter],
    return_type: Option<&Type>,
    attributes: &[Attribute],
    span: Span,
    path: Option<&str>,
    source: &str,
) -> syn::Result<proc_macro2::TokenStream> {
    let receiver = method_parameters.first().and_then(receiver_tokens);
    let parameter_start = usize::from(receiver.is_some());
    let parameters = method_parameters[parameter_start..]
        .iter()
        .map(parameter_tokens)
        .collect::<syn::Result<Vec<_>>>()?;
    let result = return_type
        .map(type_tokens)
        .transpose()?
        .unwrap_or_else(|| quote!(TypePattern::Unit));
    let type_parameters = generic_parameters
        .iter()
        .map(|parameter| LitStr::new(&parameter.name, proc_macro2::Span::call_site()))
        .collect::<Vec<_>>();
    let name = LitStr::new(method_name, proc_macro2::Span::call_site());
    let builtin_id = path
        .map(|path| {
            let path = LitStr::new(path, proc_macro2::Span::call_site());
            quote!(Some(builtin_id!(#path)))
        })
        .unwrap_or_else(|| quote!(None));
    let runtime_import = member_runtime_import(method_name, attributes)?;
    let required = !attributes
        .iter()
        .any(|attribute| attribute.path.as_slice() == ["provided"]);
    let (kind, receiver) = match receiver {
        Some(receiver) => (quote!(BuiltinMemberKind::Method), quote!(Some(#receiver))),
        None => (quote!(BuiltinMemberKind::AssociatedFunction), quote!(None)),
    };
    let documentation_start = attributes
        .first()
        .map_or(span, |attribute| attribute.span.merge(span));
    let documentation = LitStr::new(
        &documentation(source, documentation_start),
        proc_macro2::Span::call_site(),
    );
    Ok(quote! {
        BuiltinMember {
            name: #name,
            kind: #kind,
            signature: Some(BuiltinSignature {
                parameters: &[#(#parameters),*],
                result: #result,
                variadic: false,
            }),
            value_type: None,
            receiver: #receiver,
            builtin_id: #builtin_id,
            runtime_import: #runtime_import,
            required: #required,
            type_parameters: &[#(#type_parameters),*],
            documentation: #documentation,
        }
    })
}

fn associated_type_tokens(
    associated: &AssociatedType,
    source: &str,
) -> syn::Result<proc_macro2::TokenStream> {
    if !associated.generic_parameters.is_empty() || associated.value.is_some() {
        return Err(Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "built-in associated type `{}` cannot have generics or a default",
                associated.name
            ),
        ));
    }
    let name = LitStr::new(&associated.name, proc_macro2::Span::call_site());
    let documentation = LitStr::new(
        &documentation(source, associated.span),
        proc_macro2::Span::call_site(),
    );
    Ok(quote! {
        BuiltinMember {
            name: #name,
            kind: BuiltinMemberKind::AssociatedType,
            signature: None,
            value_type: Some(TypePattern::Unknown),
            receiver: None,
            builtin_id: None,
            runtime_import: None,
            required: false,
            type_parameters: &[],
            documentation: #documentation,
        }
    })
}

fn receiver_tokens(parameter: &Parameter) -> Option<proc_macro2::TokenStream> {
    if parameter.name != "self" {
        return None;
    }
    Some(match &parameter.type_annotation {
        Some(Type::Reference { mutable: true, .. }) => quote!(ReceiverMode::Mutable),
        Some(Type::Reference { mutable: false, .. }) => quote!(ReceiverMode::Shared),
        None => quote!(ReceiverMode::Owned),
        Some(_) => return None,
    })
}

fn parameter_tokens(parameter: &Parameter) -> syn::Result<proc_macro2::TokenStream> {
    parameter
        .type_annotation
        .as_ref()
        .ok_or_else(|| {
            Error::new(
                proc_macro2::Span::call_site(),
                format!("parameter `{}` requires a type annotation", parameter.name),
            )
        })
        .and_then(type_tokens)
}

pub(crate) fn type_tokens(ty: &Type) -> syn::Result<proc_macro2::TokenStream> {
    Ok(match ty {
        Type::Unit => quote!(TypePattern::Unit),
        Type::Bool => quote!(TypePattern::Bool),
        Type::Char => quote!(TypePattern::Char),
        Type::String => quote!(TypePattern::String),
        Type::Float(rils_syntax::FloatType::F32) => quote!(TypePattern::F32),
        Type::Float(rils_syntax::FloatType::F64) => quote!(TypePattern::F64),
        Type::Integer(rils_syntax::IntegerType::U32) => quote!(TypePattern::U32),
        Type::Integer(rils_syntax::IntegerType::U8) => quote!(TypePattern::U8),
        Type::Integer(rils_syntax::IntegerType::Usize) => quote!(TypePattern::Usize),
        Type::Variable(name) => {
            let name = LitStr::new(name, proc_macro2::Span::call_site());
            quote!(TypePattern::Generic(#name))
        }
        Type::Named { name, arguments } if name == "Self" && arguments.is_empty() => {
            quote!(TypePattern::SelfType)
        }
        Type::Named { name, arguments } if name == "integer" && arguments.is_empty() => {
            quote!(TypePattern::AnyInteger)
        }
        Type::Named { name, arguments }
            if name.len() == 1
                && name.as_bytes()[0].is_ascii_uppercase()
                && arguments.is_empty() =>
        {
            let name = LitStr::new(name, proc_macro2::Span::call_site());
            quote!(TypePattern::Generic(#name))
        }
        Type::Named { name, arguments } => {
            let path = if name == "Iterator" {
                "SequenceIterator"
            } else {
                name
            };
            let path = LitStr::new(path, proc_macro2::Span::call_site());
            let arguments = arguments
                .iter()
                .map(type_tokens)
                .collect::<syn::Result<Vec<_>>>()?;
            quote!(TypePattern::Named { path: #path, arguments: &[#(#arguments),*] })
        }
        Type::Option(inner) => {
            let inner = type_tokens(inner)?;
            quote!(TypePattern::Option(&#inner))
        }
        Type::Result(ok, error) => {
            let ok = type_tokens(ok)?;
            let error = type_tokens(error)?;
            quote!(TypePattern::Result { ok: &#ok, error: &#error })
        }
        Type::Tuple(elements) => {
            let elements = elements
                .iter()
                .map(type_tokens)
                .collect::<syn::Result<Vec<_>>>()?;
            quote!(TypePattern::Tuple(&[#(#elements),*]))
        }
        Type::Function {
            parameters: Some(parameters),
            return_type,
        } => {
            let parameters = parameters
                .iter()
                .map(type_tokens)
                .collect::<syn::Result<Vec<_>>>()?;
            let result = type_tokens(return_type)?;
            quote!(TypePattern::Function { parameters: &[#(#parameters),*], result: &#result })
        }
        Type::Reference { mutable, inner } => {
            let inner = type_tokens(inner)?;
            quote!(TypePattern::Reference { mutable: #mutable, inner: &#inner })
        }
        Type::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => {
            let base = type_tokens(base)?;
            let trait_name = trait_name
                .as_ref()
                .map(|name| {
                    let name = LitStr::new(name, proc_macro2::Span::call_site());
                    quote!(Some(#name))
                })
                .unwrap_or_else(|| quote!(None));
            let name = LitStr::new(name, proc_macro2::Span::call_site());
            let arguments = arguments
                .iter()
                .map(type_tokens)
                .collect::<syn::Result<Vec<_>>>()?;
            quote!(TypePattern::Associated {
                base: &#base,
                trait_name: #trait_name,
                name: #name,
                arguments: &[#(#arguments),*],
            })
        }
        Type::Unknown => quote!(TypePattern::Unknown),
        unsupported => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!("unsupported built-in type `{unsupported:?}`"),
            ));
        }
    })
}

pub(crate) fn documentation(source: &str, span: Span) -> String {
    let mut lines = source[..span.start.min(source.len())]
        .trim_end()
        .lines()
        .rev()
        .map(str::trim_start)
        .take_while(|line| line.starts_with("///"))
        .map(|line| line.trim_start_matches("///").trim().to_owned())
        .collect::<Vec<_>>();
    lines.reverse();
    lines.join("\n")
}
