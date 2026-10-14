//! Vector bridges derived from exported signatures; arrays share read-only queries.
use super::*;

pub(super) fn tokens(definition: &Definition) -> Option<proc_macro2::TokenStream> {
    let Fields::Unnamed(fields) = &definition.item.fields else {
        return None;
    };
    if fields.unnamed.len() != 1
        || fields.unnamed[0]
            .ty
            .to_token_stream()
            .to_string()
            .replace(' ', "")
            != "std::vec::Vec<T>"
    {
        return None;
    }
    let owner = definition.item.ident.to_string();
    let module = &definition.path;
    let mut borrowed = Vec::new();
    let mut owned = Vec::new();
    let mut symbols = Vec::new();
    for method in definition
        .methods
        .iter()
        .chain(exported_trait_methods(definition).map(|(method, _)| method))
    {
        let name = method.sig.ident.to_string();
        let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        let arity = method.sig.inputs.len();
        if scalar_query(method) {
            let method_name = &method.sig.ident;
            borrowed.push(quote! { #symbol => Some(if arguments.len() == #arity {
                super::super::sequence_query::with_shared(#name, arguments, |receiver| receiver.#method_name())
            } else { Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())) }), });
            continue;
        }
        let expected = match name.as_str() {
            "new" => 0,
            "from" | "len" | "is_empty" | "pop" | "clear" | "iter" | "into_iter" => 1,
            "insert" => 3,
            _ => 2,
        };
        if arity != expected {
            return Some(
                Error::new_spanned(
                    &method.sig,
                    "vector method signature does not match its native storage protocol",
                )
                .into_compile_error(),
            );
        }
        let requires_frame = matches!(
            name.as_str(),
            "from" | "push" | "insert" | "extend" | "iter" | "into_iter"
        );
        let body = call(method, &definition.item.ident);
        let borrowed_body = if requires_frame {
            symbols.push(symbol.clone());
            owned.push(quote! { #symbol => Some(if arguments.len() == #arity { #body }
                else { Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())) }), });
            quote!(Err(
                "owned native call requires an owned argument frame".into()
            ))
        } else {
            body
        };
        borrowed.push(quote! { #symbol => Some(if arguments.len() == #arity { #borrowed_body }
            else { Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())) }), });
    }
    if symbols.is_empty() && definition.methods.iter().all(scalar_query) {
        return Some(quote! {
            pub fn call_symbol(symbol: &str, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> { match symbol { #(#borrowed)* _ => None } }
        });
    }
    Some(quote! {
        const COLLECTION: crate::runtime_builtins::keyed_collection::KeyedCollection = crate::runtime_builtins::keyed_collection::KeyedCollection { name: #owner, ordered: false, parameters: 1 };
        pub fn call_symbol(symbol: &str, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> { match symbol { #(#borrowed)* _ => None } }
        pub fn is_owned_symbol(symbol: &str) -> bool { [#(#symbols),*].contains(&symbol) }
        pub fn call_owned_symbol(symbol: &str, mut arguments: Vec<crate::Value>, context: &crate::runtime_builtins::NativeOwnedContext) -> Option<Result<crate::Value, String>> { match symbol { #(#owned)* _ => None } }
    })
}

fn scalar_query(method: &ImplItemFn) -> bool {
    method.sig.inputs.len() == 1
        && method
            .sig
            .receiver()
            .is_some_and(|r| r.reference.is_some() && r.mutability.is_none())
        && matches!(&method.sig.output, ReturnType::Type(_, ty) if matches!(ty.as_ref(), Type::Path(path) if path.path.is_ident("usize") || path.path.is_ident("bool")))
}

fn call(method: &ImplItemFn, owner: &syn::Ident) -> proc_macro2::TokenStream {
    let name = method.sig.ident.to_string();
    if name == "new" {
        return quote!(Err(
            "Vec::new requires concrete type arguments at the call site".into()
        ));
    }
    if name == "from" {
        return quote!(crate::runtime_builtins::collection_constructor::from_array(
            arguments, context
        ));
    }
    if name == "into_iter" {
        return quote! { (|| {
            let value = arguments.into_iter().next().expect("arity checked");
            if let crate::Value::Dynamic(object) = value {
                if matches!(object.descriptor().layout().rils_type(), crate::Type::Array { .. }) {
                    return crate::value::native_array::into_iterator(object, context);
                }
                return COLLECTION.into_iter(crate::Value::Dynamic(object), context);
            }
            Err("Vec::into_iter requires native sequence storage".into())
        })() };
    }
    let mutable = method
        .sig
        .receiver()
        .is_some_and(|receiver| receiver.mutability.is_some());
    let arrays = matches!(name.as_str(), "len" | "is_empty" | "contains" | "iter");
    let output = super::keyed::output::tokens(method, owner);
    let operation = match name.as_str() {
        "len" | "is_empty" => {
            let result = if name == "len" {
                quote!(length)
            } else {
                quote!(length == 0)
            };
            quote! {
                let length = object.with(|view| match view.layout()?.rils_type() {
                    crate::Type::Array { length, .. } => Ok(*length), _ => view.sequence_len(),
                })??;
                let result = #result; #output
            }
        }
        "iter" => quote!(crate::runtime_builtins::indexed_iter::borrow(&arguments)),
        "contains" => quote! {
            let result = object.with(|view| rils_stdlib::native_collections::vector::contains(view, |item| crate::value::equality::view_query_equal(item, &arguments[1])))??;
            #output
        },
        "push" | "insert" => {
            let (index, mutation) = if name == "insert" {
                (
                    quote!(let index = arguments[1].get_cloned::<usize>()?;),
                    quote!(view.insert_sequence_item(index, item)),
                )
            } else {
                (quote!(), quote!(view.push_sequence_item(item)))
            };
            quote! {
                #index
                let item = arguments.pop().expect("arity checked");
                let mut codec = crate::value::record_codec::NativeRecordCodec::with_definitions(&context.structs, &context.enums).with_owned_conversion();
                let item = codec.into_native(item, COLLECTION.item_layout(&object, None)?)?;
                let result = object.with_mut(|mut view| #mutation)??; #output
            }
        }
        "pop" => {
            quote! { let result = object.with_mut(|view| rils_stdlib::native_collections::queue::pop(view, true))??; #output }
        }
        "clear" => {
            quote! { let result = object.with_mut(|mut view| view.clear_sequence())??; #output }
        }
        "truncate" | "remove" | "swap_remove" => {
            let operation = match name.as_str() {
                "truncate" => quote!(truncate_sequence),
                "remove" => quote!(take_sequence_item),
                _ => quote!(swap_remove_sequence_item),
            };
            quote! { let index = arguments[1].get_cloned::<usize>()?; let result = object.with_mut(|mut view| view.#operation(index))??; #output }
        }
        "extend" => quote! {
            let source = arguments.pop().expect("arity checked");
            let source_receiver = COLLECTION.receiver(&source)?;
            if object.same_storage(&source_receiver) { return Err("Vec cannot extend itself".into()); }
            drop(source_receiver);
            let crate::Value::Dynamic(source) = source else { return Err("Vec::extend requires an owned Vec".into()); };
            let source = source.into_value().map_err(|error| error.1)?;
            let result = object.with_mut(|view| rils_stdlib::native_collections::vector::extend(view, source))??;
            #output
        },
        _ => {
            return Error::new_spanned(&method.sig, "unsupported generated vector operation")
                .into_compile_error();
        }
    };
    quote! { (|| {
        let object = if #arrays {
            let object = crate::value::native_receiver::NativeReceiver::from_value(&arguments[0])?.ok_or("expected native vector or array")?;
            if !COLLECTION.matches(object.descriptor().layout().rils_type()) && !matches!(object.descriptor().layout().rils_type(), crate::Type::Array { .. }) {
                return Err("expected Vec or array receiver".into());
            }
            object
        } else { COLLECTION.checked_receiver(&arguments[0], #mutable)? };
        #operation
    })() }
}
