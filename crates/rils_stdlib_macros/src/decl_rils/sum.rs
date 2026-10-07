//! Generate sum bridges from receiver and argument signatures.

use quote::quote;
use syn::{Error, FnArg, ImplItemFn, ItemEnum, ReturnType, Type};

use super::{Definition, Tokens};

pub(crate) mod fallible;
mod owned;
mod query;

pub(super) fn supports_direct_bridge(item: &ItemEnum, method: &ImplItemFn) -> bool {
    matches!(item.ident.to_string().as_str(), "Option" | "Result")
        && (uses_sum_adapter(item, method)
            || callback_operation(item, method).is_some()
            || shared_query(method))
}

pub(super) fn uses_sum_adapter(item: &ItemEnum, method: &ImplItemFn) -> bool {
    owned::supports(item, method)
}

fn shared_query(method: &ImplItemFn) -> bool {
    method.sig.inputs.len() == 1
        && method
            .sig
            .receiver()
            .is_some_and(|receiver| receiver.reference.is_some() && receiver.mutability.is_none())
        && matches!(&method.sig.output, ReturnType::Type(_, ty)
            if matches!(ty.as_ref(), Type::Path(path) if path.path.is_ident("bool")))
}

fn callback_operation(item: &ItemEnum, method: &ImplItemFn) -> Option<Tokens> {
    if method.sig.receiver()?.reference.is_some() || method.sig.inputs.len() != 2 {
        return None;
    }
    let FnArg::Typed(argument) = method.sig.inputs.last()? else {
        return None;
    };
    if !matches!(argument.ty.as_ref(), Type::BareFn(_)) {
        return None;
    }
    Some(
        match (
            item.ident.to_string().as_str(),
            method.sig.ident.to_string().as_str(),
        ) {
            ("Option", "map") => quote!(OptionMap),
            ("Option", "and_then") => quote!(OptionAndThen),
            ("Option", "or_else") => quote!(OptionOrElse),
            ("Option", "filter") => quote!(OptionFilter),
            ("Result", "map") => quote!(ResultMap),
            ("Result", "map_err") => quote!(ResultMapErr),
            ("Result", "and_then") => quote!(ResultAndThen),
            ("Result", "or_else") => quote!(ResultOrElse),
            _ => return None,
        },
    )
}

pub(super) fn native_tokens(definition: &Definition) -> syn::Result<Tokens> {
    if !matches!(
        definition.item.ident.to_string().as_str(),
        "Option" | "Result"
    ) {
        return Err(Error::new_spanned(
            &definition.item.ident,
            "native bridge supports Option and Result",
        ));
    }
    let mut callbacks = Vec::new();
    let mut queries = Vec::new();
    let mut owned_calls = Vec::new();
    let mut owned_symbols = Vec::new();
    for method in &definition.methods {
        let name = &method.sig.ident;
        let module = &definition.module;
        let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        if let Some(operation) = callback_operation(&definition.item, method) {
            callbacks.push(quote! {
                #symbol => Some(super::super::callback::call(
                    super::super::callback::Operation::#operation, arguments, callback,
                )),
            });
            queries.push(
                quote! { #symbol => Some(Err("native callback context is unavailable".into())), },
            );
        } else if uses_sum_adapter(&definition.item, method) {
            let call = owned::call(&definition.item, method)?;
            let arity = method.sig.inputs.len();
            owned_symbols.push(symbol.clone());
            owned_calls.push(quote! { #symbol => Some((|| { #call })()), });
            queries.push(quote! {
                #symbol => Some(Err(if arguments.len() == #arity {
                    "owned native call requires an owned argument frame".into()
                } else {
                    format!("native method expects {} arguments, found {}", #arity, arguments.len())
                })),
            });
        } else if shared_query(method) {
            let call = query::call(&definition.item, method);
            queries.push(quote! { #symbol => Some((|| { #call })()), });
        } else {
            return Err(Error::new_spanned(
                &method.sig,
                "exported method signature has no native conversion",
            ));
        }
    }
    Ok(quote! {
        pub fn call_callback_symbol<E>(symbol: &str, arguments: &[crate::Value], callback: &mut crate::runtime_builtins::NativeCallback<'_, E>) -> Option<Result<crate::Value, crate::runtime_builtins::NativeCallError<E>>> {
            match symbol { #(#callbacks)* _ => None }
        }
        pub fn call_symbol(symbol: &str, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
            match symbol { #(#queries)* _ => None }
        }
        pub fn call_owned_symbol(symbol: &str, arguments: std::vec::Vec<crate::Value>, context: &crate::runtime_builtins::NativeOwnedContext) -> Option<Result<crate::Value, String>> {
            match symbol { #(#owned_calls)* _ => None }
        }
        pub fn is_owned_symbol(symbol: &str) -> bool {
            matches!(symbol, #(#owned_symbols)|*)
        }
    })
}
