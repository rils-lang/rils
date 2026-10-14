//! Bridges keyed storage from the collection field and exported declarations.

use super::*;

mod calls;
pub(super) mod output;
mod queue;
mod signature;

pub(super) fn tokens(definition: &Definition) -> Option<proc_macro2::TokenStream> {
    let Fields::Unnamed(fields) = &definition.item.fields else {
        return None;
    };
    if fields.unnamed.len() != 1 {
        return None;
    }
    let storage = fields.unnamed[0]
        .ty
        .to_token_stream()
        .to_string()
        .replace(' ', "");
    let (ordered, parameters) = match storage.as_str() {
        "std::collections::VecDeque<T>" => (false, 1usize),
        "std::collections::BinaryHeap<T>" => (true, 1usize),
        "std::collections::HashMap<K,V>" => (false, 2usize),
        "std::collections::BTreeMap<K,V>" => (true, 2usize),
        "std::collections::HashSet<T>" => (false, 1usize),
        "std::collections::BTreeSet<T>" => (true, 1usize),
        _ => return None,
    };
    let owner = definition.item.ident.to_string();
    let queued =
        storage == "std::collections::VecDeque<T>" || storage == "std::collections::BinaryHeap<T>";
    let module = &definition.path;
    let mut borrowed = Vec::new();
    let mut owned = Vec::new();
    let mut owned_symbols = Vec::new();
    for method in definition
        .methods
        .iter()
        .chain(exported_trait_methods(definition).map(|(method, _)| method))
    {
        if let Err(error) = signature::validate(method, parameters == 2) {
            return Some(error.into_compile_error());
        }
        let name = method.sig.ident.to_string();
        let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        let arity = method.sig.inputs.len();
        let requires_frame = matches!(
            name.as_str(),
            "insert" | "iter" | "into_iter" | "push" | "push_front" | "push_back"
        );
        let call = if method.sig.receiver().is_none() {
            quote!(Err(
                format!("{} requires concrete type arguments at the call site", #symbol)
            ))
        } else if requires_frame {
            let body = if queued {
                queue::call(method, &definition.item.ident, ordered)
            } else {
                calls::owned(method, parameters == 2, &definition.item.ident)
            };
            owned_symbols.push(symbol.clone());
            owned.push(quote! {
                #symbol => Some(if arguments.len() == #arity {
                    #body
                } else { Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())) }),
            });
            quote!(Err(
                "owned native call requires an owned argument frame".into()
            ))
        } else {
            if queued {
                queue::call(method, &definition.item.ident, ordered)
            } else {
                calls::borrowed(method, parameters == 2, &definition.item.ident)
            }
        };
        borrowed.push(quote! {
            #symbol => Some(if arguments.len() == #arity { #call }
                else { Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())) }),
        });
    }
    Some(quote! {
        const COLLECTION: crate::runtime_builtins::keyed_collection::KeyedCollection =
            crate::runtime_builtins::keyed_collection::KeyedCollection {
                name: #owner, ordered: #ordered, parameters: #parameters,
            };
        pub fn call_symbol(symbol: &str, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
            match symbol { #(#borrowed)* _ => None }
        }
        pub fn is_owned_symbol(symbol: &str) -> bool { [#(#owned_symbols),*].contains(&symbol) }
        pub fn call_owned_symbol(symbol: &str, arguments: Vec<crate::Value>, context: &crate::runtime_builtins::NativeOwnedContext) -> Option<Result<crate::Value, String>> {
            match symbol { #(#owned)* _ => None }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_field_selects_bridge_independently_of_exported_type_name() {
        for (storage, backend, ordered) in [
            (quote!(std::collections::HashSet<T>), "native_set", false),
            (quote!(std::collections::BTreeSet<T>), "native_set", true),
            (quote!(std::collections::HashMap<K, V>), "native_map", false),
            (quote!(std::collections::BTreeMap<K, V>), "native_map", true),
        ] {
            let generics = if backend == "native_map" {
                quote!(K, V)
            } else {
                quote!(T)
            };
            let arguments = if backend == "native_map" {
                quote!(key: K, value: V)
            } else {
                quote!(value: T)
            };
            let result = if backend == "native_map" {
                quote!(Option<V>)
            } else {
                quote!(bool)
            };
            let definition = Definition::parse(
                syn::parse_quote!(core::collections::Renamed),
                syn::parse_quote! {
                    mod renamed {
                        pub struct Renamed<#generics>(#storage);
                        impl<#generics> Renamed<#generics> {
                            #[export_rils]
                            pub fn insert(&mut self, #arguments) -> #result { todo!() }
                        }
                    }
                },
            )
            .unwrap();
            let generated = tokens(&definition).unwrap();
            syn::parse2::<syn::File>(generated.clone()).unwrap();
            let text = generated.to_string();
            assert!(text.contains(if backend == "native_map" {
                "native_collections :: map"
            } else {
                "native_collections :: set"
            }));
            assert!(text.contains("name : \"Renamed\""));
            assert!(text.contains(&format!("ordered : {ordered}")));
            assert!(text.contains("core::collections::Renamed::insert"));
        }
    }
}
