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
        Type::Path(path) if path.qself.is_some() => associated_tokens(path),
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
        Type::Array(array) => {
            let element = tokens(&array.elem)?;
            match &array.len {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Int(length),
                    ..
                }) => {
                    let length = length.base10_parse::<usize>()?;
                    Ok(quote!(TypePattern::Array { element: &#element, length: #length }))
                }
                syn::Expr::Path(length) if length.path.get_ident().is_some() => {
                    let length = length.path.get_ident().unwrap().to_string();
                    Ok(quote!(TypePattern::ArrayParameter { element: &#element, length: #length }))
                }
                _ => Err(Error::new_spanned(
                    &array.len,
                    "exported array length must be a usize literal or const parameter",
                )),
            }
        }
        Type::Slice(slice) => {
            let element = tokens(&slice.elem)?;
            Ok(quote!(TypePattern::Slice(&#element)))
        }
        Type::Infer(_) => Err(Error::new_spanned(
            ty,
            "exported signatures require an explicit type or declared generic parameter",
        )),
        _ => Err(Error::new_spanned(
            ty,
            "unsupported type in built-in type pattern",
        )),
    }
}

fn associated_tokens(path: &syn::TypePath) -> syn::Result<proc_macro2::TokenStream> {
    let qself = path.qself.as_ref().expect("associated path");
    let base = tokens(&qself.ty)?;
    let segments = path.path.segments.iter().collect::<Vec<_>>();
    let member = segments
        .last()
        .ok_or_else(|| Error::new_spanned(path, "missing associated member"))?;
    let name = LitStr::new(&member.ident.to_string(), member.ident.span());
    let arguments = type_arguments(&member.arguments)?;
    let trait_name = if qself.position == 0 {
        quote!(None)
    } else {
        let name = segments[..qself.position]
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        quote!(Some(#name))
    };
    Ok(quote!(TypePattern::Associated {
        base: &#base,
        trait_name: #trait_name,
        name: #name,
        arguments: &[#(#arguments),*],
    }))
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
        "OwnedIterator".to_owned()
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

/// Preserve declared bounds rather than widening native generic parameters.
pub(crate) fn with_generics(
    ty: &Type,
    generics: &syn::Generics,
) -> syn::Result<proc_macro2::TokenStream> {
    if let Type::Path(path) = ty
        && let Some(name) = path.path.get_ident()
        && let Some(parameter) = generics
            .type_params()
            .find(|parameter| parameter.ident == *name)
    {
        let name = name.to_string();
        let bounds = parameter
            .bounds
            .iter()
            .map(|bound| match bound {
                syn::TypeParamBound::Trait(bound) => {
                    let mut path = bound.path.clone();
                    if path.segments.len() == 3
                        && path.segments[0].ident == "std"
                        && path.segments[1].ident == "fmt"
                    {
                        path.segments[0].ident = syn::parse_quote!(core);
                    }
                    path_tokens(&path)
                }
                _ => Err(Error::new_spanned(
                    bound,
                    "unsupported exported generic bound",
                )),
            })
            .collect::<syn::Result<Vec<_>>>()?;
        if bounds.is_empty() {
            return Ok(quote!(TypePattern::Generic(#name)));
        }
        return Ok(quote!(TypePattern::BoundGeneric { name: #name, bounds: &[#(#bounds),*] }));
    }
    if let Type::Reference(reference) = ty {
        let inner = with_generics(&reference.elem, generics)?;
        let mutable = reference.mutability.is_some();
        return Ok(quote!(TypePattern::Reference { mutable: #mutable, inner: &#inner }));
    }
    tokens(ty)
}
