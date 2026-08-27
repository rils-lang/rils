use proc_macro::TokenStream;
use quote::quote;
use syn::{Error, GenericArgument, LitStr, PathArguments, ReturnType, Type, parse_macro_input};

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let ty = parse_macro_input!(input as Type);
    match tokens(&ty) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

pub(crate) fn tokens(ty: &Type) -> syn::Result<proc_macro2::TokenStream> {
    match ty {
        Type::Path(path) if path.qself.is_none() => path_tokens(&path.path),
        Type::Reference(reference) => {
            let inner = tokens(&reference.elem)?;
            let mutable = reference.mutability.is_some();
            Ok(quote!(TypePattern::Reference { mutable: #mutable, inner: &#inner }))
        }
        Type::Tuple(tuple) if tuple.elems.is_empty() => Ok(quote!(TypePattern::Unit)),
        Type::Tuple(tuple) => {
            let elements = tuple
                .elems
                .iter()
                .map(tokens)
                .collect::<syn::Result<Vec<_>>>()?;
            Ok(quote!(TypePattern::Tuple(&[#(#elements),*])))
        }
        Type::BareFn(function) => function_tokens(function),
        Type::Paren(parenthesized) => tokens(&parenthesized.elem),
        Type::Group(grouped) => tokens(&grouped.elem),
        Type::Infer(_) => Ok(quote!(TypePattern::Unknown)),
        _ => Err(Error::new_spanned(
            ty,
            "unsupported type in built-in type pattern",
        )),
    }
}

fn function_tokens(function: &syn::TypeBareFn) -> syn::Result<proc_macro2::TokenStream> {
    if function.lifetimes.is_some()
        || function.unsafety.is_some()
        || function.abi.is_some()
        || function.variadic.is_some()
    {
        return Err(Error::new_spanned(
            function,
            "built-in function types must be safe, non-variadic Rust functions",
        ));
    }
    let parameters = function
        .inputs
        .iter()
        .map(|parameter| tokens(&parameter.ty))
        .collect::<syn::Result<Vec<_>>>()?;
    let result = match &function.output {
        ReturnType::Default => quote!(TypePattern::Unit),
        ReturnType::Type(_, result) => tokens(result)?,
    };
    Ok(quote!(TypePattern::Function {
        parameters: &[#(#parameters),*],
        result: &#result,
    }))
}

fn path_tokens(path: &syn::Path) -> syn::Result<proc_macro2::TokenStream> {
    let Some(last) = path.segments.last() else {
        return Err(Error::new_spanned(path, "empty type path"));
    };
    if let Some(segment) = path
        .segments
        .iter()
        .take(path.segments.len().saturating_sub(1))
        .find(|segment| !matches!(segment.arguments, PathArguments::None))
    {
        return Err(Error::new_spanned(
            segment,
            "only the final path segment may have type arguments",
        ));
    }

    let name = last.ident.to_string();
    let arguments = type_arguments(&last.arguments)?;
    let is_single_segment = path.segments.len() == 1;
    if is_single_segment {
        match (name.as_str(), arguments.as_slice()) {
            ("Self", []) => return Ok(quote!(TypePattern::SelfType)),
            ("integer", []) => return Ok(quote!(TypePattern::AnyInteger)),
            ("bool", []) => return Ok(quote!(TypePattern::Bool)),
            ("char", []) => return Ok(quote!(TypePattern::Char)),
            ("string" | "String", []) => return Ok(quote!(TypePattern::String)),
            ("f32", []) => return Ok(quote!(TypePattern::F32)),
            ("f64", []) => return Ok(quote!(TypePattern::F64)),
            ("u32", []) => return Ok(quote!(TypePattern::U32)),
            ("u8", []) => return Ok(quote!(TypePattern::U8)),
            ("usize", []) => return Ok(quote!(TypePattern::Usize)),
            ("Option", [inner]) => return Ok(quote!(TypePattern::Option(&#inner))),
            ("Result", [ok, error]) => {
                return Ok(quote!(TypePattern::Result {
                    ok: &#ok,
                    error: &#error,
                }));
            }
            _ => {}
        }
        if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() && arguments.is_empty() {
            let generic = LitStr::new(&name, last.ident.span());
            return Ok(quote!(TypePattern::Generic(#generic)));
        }
    }

    let path_name = if is_single_segment && name == "Iterator" {
        "SequenceIterator".to_owned()
    } else {
        path.segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    };
    let path_name = LitStr::new(&path_name, last.ident.span());
    Ok(quote!(TypePattern::Named {
        path: #path_name,
        arguments: &[#(#arguments),*],
    }))
}

fn type_arguments(arguments: &PathArguments) -> syn::Result<Vec<proc_macro2::TokenStream>> {
    match arguments {
        PathArguments::None => Ok(Vec::new()),
        PathArguments::AngleBracketed(arguments) => arguments
            .args
            .iter()
            .map(|argument| match argument {
                GenericArgument::Type(ty) => tokens(ty),
                _ => Err(Error::new_spanned(
                    argument,
                    "only type arguments are supported in built-in type patterns",
                )),
            })
            .collect(),
        other => Err(Error::new_spanned(
            other,
            "parenthesized type arguments are not supported here",
        )),
    }
}
