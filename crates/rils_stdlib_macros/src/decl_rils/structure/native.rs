//! Native call generation for exported struct methods.

use super::*;

pub(super) fn expand(path: Path, module: ItemMod) -> TokenStream {
    let result = Definition::parse(path, module).and_then(|definition| tokens(&definition));
    match result {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn tokens(definition: &Definition) -> syn::Result<proc_macro2::TokenStream> {
    if let Some(tokens) = super::vector::tokens(definition) {
        return Ok(tokens);
    }
    if matches!(&definition.item.fields, syn::Fields::Unnamed(fields)
        if fields.unnamed.len() == 1
            && fields.unnamed[0].ty.to_token_stream().to_string().replace(' ', "") == "std::boxed::Box<T>")
    {
        return boxed_tokens(definition);
    }
    if let Some(tokens) = super::keyed::tokens(definition) {
        return Ok(tokens);
    }
    if let Some(tokens) = super::shared::tokens(definition) {
        return Ok(tokens);
    }
    if let Some(tokens) = super::cell::tokens(definition) {
        return Ok(tokens);
    }
    let module = &definition.path;
    let arms = definition
        .methods
        .iter()
        .chain(exported_trait_methods(definition).map(|(method, _)| method))
        .map(|method| {
            let name = &method.sig.ident;
            let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
            let arity = method.sig.inputs.len();
            let call = match definition.item.ident.to_string().as_str() {
                "Range" => quote!(Some(super::super::range::#name(arguments))),
                "Iter" => quote!(Some(super::super::indexed_iter::#name(arguments))),
                _ => return Err(Error::new_spanned(
                    &method.sig,
                    "exported native receiver has no value adapter; implement its conversion",
                )),
            };
            Ok(quote! {
                #symbol => if arguments.len() == #arity {
                    #call
                } else {
                    Some(Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())))
                },
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote! {
        pub fn call_symbol(symbol: &str, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
            match symbol { #(#arms)* _ => { let _ = arguments; None } }
        }
    })
}

fn boxed_tokens(definition: &Definition) -> syn::Result<proc_macro2::TokenStream> {
    let module = &definition.path;
    let mut borrowed_arms = Vec::new();
    let mut owned_symbols = Vec::new();
    let arms = definition
        .methods
        .iter()
        .map(|method| {
            let name = &method.sig.ident;
            let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
            owned_symbols.push(symbol.clone());
            let arity = method.sig.inputs.len();
            borrowed_arms.push(quote! {
                #symbol => Some(Err(if arguments.len() == #arity {
                    "owned native call requires an owned argument frame".into()
                } else {
                    format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())
                })),
            });
            Ok(quote! {
                #symbol => Some(if arguments.len() == #arity {
                    super::super::boxed::#name(arguments, context)
                } else {
                    Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len()))
                }),
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote! {
        pub fn call_symbol(symbol: &str, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
            match symbol { #(#borrowed_arms)* _ => None }
        }

        pub fn call_owned_symbol(symbol: &str, arguments: std::vec::Vec<crate::Value>, context: &crate::runtime_builtins::NativeOwnedContext) -> Option<Result<crate::Value, String>> {
            match symbol { #(#arms)* _ => None }
        }

        pub fn is_owned_symbol(symbol: &str) -> bool {
            matches!(symbol, #(#owned_symbols)|*)
        }
    })
}
