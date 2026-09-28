//! Derive the Rils callable signature from one fallible Rust implementation.

use std::collections::HashMap;

use syn::{
    Error, FnArg, GenericArgument, ImplItemFn, ItemFn, PathArguments, ReturnType, Type,
    TypeParamBound, WherePredicate, parse_quote,
};

mod shadow;

pub(crate) use shadow::{function as shadow_function, method as shadow_method};

fn result_parts(ty: &Type) -> Option<(&Type, &Type)> {
    let Type::Path(path) = ty else { return None };
    let parts = path
        .path
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect::<Vec<_>>();
    if parts.as_slice() != ["std", "result", "Result"]
        && parts.as_slice() != ["core", "result", "Result"]
    {
        return None;
    }
    let segment = path.path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    if arguments.args.len() != 2 {
        return None;
    }
    let mut arguments = arguments.args.iter();
    let GenericArgument::Type(output) = arguments.next()? else {
        return None;
    };
    let GenericArgument::Type(error) = arguments.next()? else {
        return None;
    };
    Some((output, error))
}

pub(crate) fn expose_method(method: &ImplItemFn) -> syn::Result<ImplItemFn> {
    let mut exported = method.clone();
    let function = ItemFn {
        attrs: method.attrs.clone(),
        vis: method.vis.clone(),
        sig: method.sig.clone(),
        block: Box::new(method.block.clone()),
    };
    let (function, _, _) = expose(function)?;
    exported.sig = function.sig;
    Ok(exported)
}

fn simple_name(ty: &Type) -> Option<&syn::Ident> {
    let Type::Path(path) = ty else { return None };
    (path.qself.is_none() && path.path.segments.len() == 1).then(|| &path.path.segments[0].ident)
}

pub(super) fn expose(mut function: ItemFn) -> syn::Result<(ItemFn, bool, bool)> {
    let Some(where_clause) = &function.sig.generics.where_clause else {
        return Ok((function, false, false));
    };
    let mut callbacks = HashMap::<String, Type>::new();
    let mut error_name = None::<String>;
    let mut plain = None::<bool>;
    for predicate in &where_clause.predicates {
        let WherePredicate::Type(predicate) = predicate else {
            continue;
        };
        let Some(name) = simple_name(&predicate.bounded_ty) else {
            continue;
        };
        for bound in &predicate.bounds {
            let TypeParamBound::Trait(bound) = bound else {
                continue;
            };
            let Some(segment) = bound.path.segments.last() else {
                continue;
            };
            if !matches!(
                segment.ident.to_string().as_str(),
                "Fn" | "FnMut" | "FnOnce"
            ) {
                continue;
            }
            let PathArguments::Parenthesized(arguments) = &segment.arguments else {
                return Err(Error::new_spanned(
                    segment,
                    "callback bound needs function arguments",
                ));
            };
            let ReturnType::Type(_, output) = &arguments.output else {
                return Err(Error::new_spanned(
                    &arguments.output,
                    "callback must return Result<T, E>",
                ));
            };
            let is_plain = result_parts(output).is_none();
            if plain.is_some_and(|current| current != is_plain) {
                return Err(Error::new_spanned(
                    output,
                    "callbacks must use the same error convention",
                ));
            }
            plain = Some(is_plain);
            let result = if let Some((result, error)) = result_parts(output) {
                let error = simple_name(error).ok_or_else(|| {
                    Error::new_spanned(error, "callback error must be one generic type")
                })?;
                if error_name
                    .as_ref()
                    .is_some_and(|current| current != &error.to_string())
                {
                    return Err(Error::new_spanned(
                        error,
                        "callbacks must share one error type",
                    ));
                }
                error_name = Some(error.to_string());
                result
            } else {
                output.as_ref()
            };
            let inputs = &arguments.inputs;
            if callbacks
                .insert(name.to_string(), parse_quote!(fn(#inputs) -> #result))
                .is_some()
            {
                return Err(Error::new_spanned(name, "duplicate callback bound"));
            }
        }
    }
    if callbacks.is_empty() {
        return Ok((function, false, false));
    }
    for predicate in &where_clause.predicates {
        let WherePredicate::Type(predicate) = predicate else {
            return Err(Error::new_spanned(
                predicate,
                "callback exports only support callback bounds",
            ));
        };
        if simple_name(&predicate.bounded_ty)
            .is_none_or(|name| !callbacks.contains_key(&name.to_string()))
            || predicate.bounds.len() != 1
        {
            return Err(Error::new_spanned(
                predicate,
                "callback exports only support one callable bound per callback",
            ));
        }
    }
    if function
        .sig
        .generics
        .type_params()
        .any(|parameter| !parameter.bounds.is_empty() || parameter.default.is_some())
    {
        return Err(Error::new_spanned(
            &function.sig.generics,
            "callback export generics cannot add Rust-only bounds",
        ));
    }
    let is_plain = plain.expect("callback bound has a mode");
    let result = if is_plain {
        None
    } else {
        let ReturnType::Type(_, output) = &function.sig.output else {
            return Err(Error::new_spanned(
                &function.sig.output,
                "callback export must return std::result::Result<T, E>",
            ));
        };
        let (result, error) = result_parts(output).ok_or_else(|| {
            Error::new_spanned(
                output,
                "callback export must return std::result::Result<T, E>",
            )
        })?;
        if simple_name(error).is_none_or(|name| Some(name.to_string()) != error_name) {
            return Err(Error::new_spanned(
                error,
                "callback error type must match the function error type",
            ));
        }
        Some(result.clone())
    };
    let mut used = 0;
    for argument in &mut function.sig.inputs {
        let FnArg::Typed(argument) = argument else {
            continue;
        };
        if let Some(name) = simple_name(&argument.ty)
            && let Some(callback) = callbacks.get(&name.to_string())
        {
            *argument.ty = callback.clone();
            used += 1;
        }
    }
    if used != callbacks.len() {
        return Err(Error::new_spanned(
            &function.sig,
            "each callback generic must appear as one function parameter",
        ));
    }
    function.sig.generics.params = function.sig.generics.params.into_iter().filter(|parameter| {
        !matches!(parameter, syn::GenericParam::Type(ty) if callbacks.contains_key(&ty.ident.to_string()) || error_name.as_ref().is_some_and(|error| ty.ident == error))
    }).collect();
    function.sig.generics.where_clause = None;
    if let Some(result) = result {
        function.sig.output = parse_quote!(-> #result);
    }
    function.sig.inputs.pop_punct();
    Ok((function, true, is_plain))
}
