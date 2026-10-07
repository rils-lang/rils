//! Generate sum bridges from receiver and argument signatures.

use quote::quote;
use syn::{Error, ImplItemFn, ItemEnum, ReturnType, Type};

use super::{Definition, Tokens};

mod callback;
pub(crate) mod fallible;
mod owned;
mod query;

pub(super) fn supports_direct_bridge(item: &ItemEnum, method: &ImplItemFn) -> bool {
    matches!(item.ident.to_string().as_str(), "Option" | "Result")
        && (uses_sum_adapter(item, method) || shared_query(method))
}

pub(super) fn uses_sum_adapter(item: &ItemEnum, method: &ImplItemFn) -> bool {
    owned::supports(item, method) || callback::supports(item, method)
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
    let mut callback_symbols = Vec::new();
    let mut queries = Vec::new();
    let mut owned_calls = Vec::new();
    let mut owned_symbols = Vec::new();
    for method in &definition.methods {
        let name = &method.sig.ident;
        let module = &definition.module;
        let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        if callback::supports(&definition.item, method) {
            let call = callback::call(&definition.item, method)?;
            callback_symbols.push(symbol.clone());
            callbacks.push(quote! { #symbol => Some((|| { #call })()), });
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
        pub fn call_callback_symbol<E>(symbol: &str, arguments: std::vec::Vec<crate::Value>, context: &crate::runtime_builtins::NativeOwnedContext, expected: Option<&crate::Type>, callback: &mut crate::runtime_builtins::NativeCallback<'_, E>) -> Option<Result<crate::Value, crate::runtime_builtins::NativeCallError<E>>> {
            match symbol { #(#callbacks)* _ => None }
        }
        pub fn is_callback_symbol(symbol: &str) -> bool {
            [#(#callback_symbols),*].contains(&symbol)
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
