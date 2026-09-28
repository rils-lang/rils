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
    if matches!(&definition.item.fields, syn::Fields::Unnamed(fields)
        if fields.unnamed.len() == 1
            && fields.unnamed[0].ty.to_token_stream().to_string().replace(' ', "") == "std::boxed::Box<T>")
    {
        return boxed_tokens(definition);
    }
    let module = &definition.path;
    let arms = definition
        .methods
        .iter()
        .chain(exported_trait_methods(definition).map(|(method, _)| method))
        .map(|method| {
            if !matches!(super::super::method_binding::MethodBinding::parse(method)?, super::super::method_binding::MethodBinding::Native) {
                return Ok(quote!());
            }
            let name = &method.sig.ident;
            let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
            let arity = method.sig.inputs.len();
            let call = match definition.item.ident.to_string().as_str() {
                "Vec" if shared_scalar_query(method) => {
                    quote!(super::super::vector::with_shared(arguments, |receiver| receiver.#name()))
                }
                // These receiver adapters also enforce per-operation reference and move rules.
                // Rust resolves the adapter and checks its existence; no method-name list lives here.
                "Vec" if method.sig.receiver().is_some_and(|receiver| receiver.reference.is_some() && receiver.mutability.is_none()) => quote!(super::super::vector::#name(arguments)),
                "Vec" => {
                    let inputs = (1..arity).map(|index| quote::format_ident!("input_{index}")).collect::<Vec<_>>();
                    quote!(super::super::vector::#name(arguments, |receiver, #(#inputs),*| receiver.#name(#(#inputs),*)))
                },
                "Range" => quote!(super::super::range::#name(arguments)),
                "Iter" => quote!(super::super::indexed_iter::#name(arguments)),
                _ => return Err(Error::new_spanned(
                    &method.sig,
                    "exported native receiver has no value adapter; implement its conversion or explicitly bind an existing #[rils_legacy_id(...)] or #[rils_import(...)]",
                )),
            };
            Ok(quote! {
                #symbol => Some(if arguments.len() == #arity {
                    #call
                } else {
                    Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len()))
                }),
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
            if !matches!(super::super::method_binding::MethodBinding::parse(method)?, super::super::method_binding::MethodBinding::Native) {
                return Ok(quote!());
            }
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

fn shared_scalar_query(method: &ImplItemFn) -> bool {
    method.sig.inputs.len() == 1
        && method
            .sig
            .receiver()
            .is_some_and(|receiver| receiver.reference.is_some() && receiver.mutability.is_none())
        && matches!(&method.sig.output, ReturnType::Type(_, ty)
            if matches!(ty.as_ref(), Type::Path(path)
                if path.path.is_ident("bool") || path.path.is_ident("usize")))
}
