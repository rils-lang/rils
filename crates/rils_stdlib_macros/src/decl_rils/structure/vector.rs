//! Native collection calls generated from storage, receiver, argument and result shapes.
use super::*;

mod call;
mod construction;
mod iteration;
mod schema;
mod signature;

pub(super) fn tokens(definition: &Definition) -> Option<proc_macro2::TokenStream> {
    let backend = match storage::parameter(&definition.item) {
        Ok(backend) => backend,
        Err(error) => return Some(error.into_compile_error()),
    };
    let (_, receiver) = backend?;
    Some(generate(definition, &receiver).unwrap_or_else(Error::into_compile_error))
}

fn generate(definition: &Definition, receiver: &Path) -> syn::Result<proc_macro2::TokenStream> {
    let parameters = storage::language_parameters(&definition.item)?;
    let element = storage::element_type(&definition.item)?;
    let schema = schema::tokens(&element, &parameters)?;
    let count = parameters.len();
    let owner = definition.item.ident.to_string();
    let module = &definition.path;
    let generics = parameters
        .iter()
        .map(|_| quote!(rils_value::DynamicValue))
        .collect::<Vec<_>>();
    let mut borrowed = Vec::new();
    let mut owned = Vec::new();
    let mut symbols = Vec::new();
    let mut constructors = Vec::new();
    let mut constructor_symbols = Vec::new();
    for (method, implementation) in definition
        .methods
        .iter()
        .map(|method| (method, None))
        .chain(
            exported_trait_methods(definition)
                .map(|(method, implementation)| (method, Some(implementation))),
        )
    {
        let name = &method.sig.ident;
        let method_name = name.to_string();
        let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        let arity = method.sig.inputs.len();
        let constructor = construction::input(method, &parameters);
        if let Some(input) = constructor {
            let body = construction::call(input, method, receiver, &owner, &generics);
            constructor_symbols.push(symbol.clone());
            constructors.push(quote!(#symbol => Some(#body),));
            borrowed.push(quote!(#symbol => Some(Err("native constructor requires an owned argument frame and a result type".into())),));
            continue;
        }
        let (body, takes_ownership) = if iteration::owned(method, implementation, &parameters) {
            (
                quote! {
                    super::super::sequence_access::with_owned_sequence(arguments, &SCHEMA, context, |storage| {
                        #receiver::<#(#generics,)* _>::with_storage(storage).#name()
                    })
                },
                true,
            )
        } else if iteration::supports(method, &parameters) {
            (
                quote! {
                    super::super::sequence_access::with_borrowed_sequence(arguments, &SCHEMA, #method_name, |storage| {
                        #receiver::<#(#generics,)* _>::with_storage(storage).#name()
                    })
                },
                false,
            )
        } else {
            let signature = signature::Signature::parse(method, &parameters)?;
            (
                call::tokens(method, receiver, &signature, &parameters),
                signature.owns_arguments(),
            )
        };
        let arm = quote! { #symbol => Some(if arguments.len() == #arity { #body }
        else { Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())) }), };
        if takes_ownership {
            symbols.push(symbol.clone());
            owned.push(arm);
            borrowed.push(quote!(#symbol => Some(Err("owned native call requires an owned argument frame".into())),));
        } else {
            borrowed.push(arm);
        }
    }
    let owned_dispatch = (!symbols.is_empty() || !constructor_symbols.is_empty()).then(|| quote! {
        pub fn is_owned_symbol(symbol: &str) -> bool { [#(#symbols),*].contains(&symbol) }
        pub fn call_owned_symbol(symbol: &str, arguments: Vec<crate::Value>, context: &crate::runtime_builtins::NativeOwnedContext) -> Option<Result<crate::Value, String>> {
            match symbol { #(#owned)* _ => None }
        }
    });
    let constructor_dispatch = (!constructor_symbols.is_empty()).then(|| quote! {
        pub fn is_constructor_symbol(symbol: &str) -> bool { [#(#constructor_symbols),*].contains(&symbol) }
        pub fn call_constructor_symbol(symbol: &str, arguments: Vec<crate::Value>, context: &crate::runtime_builtins::NativeOwnedContext, expected: Option<&crate::Type>) -> Option<Result<crate::Value, String>> {
            match symbol { #(#constructors)* _ => None }
        }
    });
    Ok(quote! {
        const SCHEMA: rils_stdlib::storage::EntrySchema = rils_stdlib::storage::EntrySchema { owner: #owner, parameters: #count, entry: #schema };
        pub fn call_symbol(symbol: &str, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
            match symbol { #(#borrowed)* _ => None }
        }
        #owned_dispatch
        #constructor_dispatch
    })
}
