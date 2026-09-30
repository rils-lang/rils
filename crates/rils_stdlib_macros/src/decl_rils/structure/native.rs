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
            let name = &method.sig.ident;
            let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
            let arity = method.sig.inputs.len();
            if method.sig.receiver().is_none()
                && name == "new"
                && arity == 0
                && matches!(definition.item.ident.to_string().as_str(), "Vec" | "HashSet" | "HashMap")
            {
                let owner = definition.item.ident.to_string();
                return Ok(quote! {
                    #symbol => Some(if arguments.is_empty() {
                        super::super::collection_constructor::empty(#owner)
                    } else {
                        Err(format!("native method `{}` expects no arguments, found {}", #symbol, arguments.len()))
                    }),
                });
            }
            if definition.item.ident == "Vec" && method.sig.receiver().is_none() && name == "from" {
                return Ok(quote! {
                    #symbol => Some(Err("Vec::from requires an owned argument frame".into())),
                });
            }
            if definition.item.ident != "Vec" && shared_sequence_query(method) {
                let owner = definition.item.ident.to_string();
                return Ok(quote! {
                    #symbol => super::super::sequence_receiver::query(#owner, stringify!(#name), arguments),
                });
            }
            if definition.item.ident != "Vec" && mutable_sequence_clear(method) {
                let owner = definition.item.ident.to_string();
                return Ok(quote! {
                    #symbol => super::super::sequence_receiver::clear(#owner, arguments),
                });
            }
            let call = match definition.item.ident.to_string().as_str() {
                "HashSet" => quote!(super::super::native_set::call_symbol(symbol, arguments)?),
                "HashMap" => quote!(super::super::native_map::call_symbol(symbol, arguments)?),
                "VecDeque" => quote!(super::super::vec_deque::call_symbol(symbol, arguments)?),
                "BinaryHeap" => quote!(super::super::binary_heap::call_symbol(symbol, arguments)?),
                "BTreeSet" => quote!(
                    super::super::native_set::call_symbol(symbol, arguments)
                        .or_else(|| super::super::btree_set::call_symbol(symbol, arguments))?
                ),
                "BTreeMap" => quote!(
                    super::super::native_map::call_symbol(symbol, arguments)
                        .or_else(|| super::super::btree_map::call_symbol(symbol, arguments))?
                ),
                "Rc" | "Weak" => quote!(super::super::rc_native::call_symbol(symbol, arguments)?),
                "Cell" | "RefCell" => quote!(super::super::cell_native::call_symbol(symbol, arguments)?),
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
                    "exported native receiver has no value adapter; implement its conversion",
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

fn shared_sequence_query(method: &ImplItemFn) -> bool {
    shared_scalar_query(method)
        && matches!(method.sig.ident.to_string().as_str(), "len" | "is_empty")
}

fn mutable_sequence_clear(method: &ImplItemFn) -> bool {
    method.sig.ident == "clear"
        && method.sig.inputs.len() == 1
        && method
            .sig
            .receiver()
            .is_some_and(|receiver| receiver.reference.is_some() && receiver.mutability.is_some())
        && matches!(method.sig.output, ReturnType::Default)
}
