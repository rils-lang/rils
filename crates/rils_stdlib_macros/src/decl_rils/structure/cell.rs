//! Native cell bridges generated from exported declarations.
use super::*;

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
    let backend = match storage.as_str() {
        "std::cell::Cell<T>" => quote!(cell),
        "std::cell::RefCell<T>" => quote!(ref_cell),
        _ => return None,
    };
    let mut borrowed = Vec::new();
    let mut owned = Vec::new();
    let mut symbols = Vec::new();
    let module = &definition.path;
    for method in &definition.methods {
        let name = &method.sig.ident;
        let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        let arity = method.sig.inputs.len();
        let expected = if name == "set" || name == "replace" {
            2
        } else {
            1
        };
        let valid_receiver = if name == "new" {
            method.sig.receiver().is_none()
        } else {
            method.sig.receiver().is_some_and(|receiver| {
                receiver.reference.is_some() && receiver.mutability.is_none()
            })
        };
        if arity != expected || !valid_receiver {
            return Some(
                Error::new_spanned(
                    &method.sig,
                    "cell method signature does not match its native storage protocol",
                )
                .into_compile_error(),
            );
        }
        let body = match name.to_string().as_str() {
            "new" => quote! {
                let value = arguments.pop().expect("arity checked");
                let ty = crate::Type::of_value(&value).ok_or("cell requires a concrete item type")?;
                let mut codec = crate::value::record_codec::NativeRecordCodec::with_definitions(&context.structs, &context.enums).with_owned_conversion();
                let value = codec.into_native(value, context.layout(&ty)?)?;
                let result = operations::new(value)?;
                crate::value::native_instance::from_native(result, std::rc::Rc::new(codec))
            },
            "borrow" | "borrow_mut" => {
                let mutable = name == "borrow_mut";
                quote! {
                    let reference = crate::value::ReferenceValue::new_dynamic_cell(&arguments[0], #mutable)?;
                    Ok(crate::Value::from_reference(std::rc::Rc::new(reference)))
                }
            }
            "get" | "set" | "replace" => {
                let input = if arity == 2 {
                    quote! {
                        let layout = object.with(operations::item_layout)??;
                        let value = codec.into_native(arguments.pop().expect("arity checked"), layout)?;
                        let result = object.with(|view| operations::#name(view, value))??;
                    }
                } else {
                    quote!(let result = object.with(operations::#name)??;)
                };
                let output = if matches!(method.sig.output, ReturnType::Default) {
                    quote! { let () = result; Ok(crate::Value::from(crate::value::native_unit())) }
                } else {
                    quote!(codec.from_native(result))
                };
                quote! {
                    let object = crate::value::native_receiver::NativeReceiver::from_value(&arguments[0])?.ok_or("expected native cell receiver")?;
                    let mut codec = object.codec().with_owned_conversion();
                    #input
                    #output
                }
            }
            _ => {
                return Some(
                    Error::new_spanned(&method.sig, "unsupported native cell operation")
                        .into_compile_error(),
                );
            }
        };
        symbols.push(symbol.clone());
        borrowed.push(quote! { #symbol => Some(Err(if arguments.len() == #arity { "cell methods require an owned argument frame".into() }
            else { format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len()) })), });
        owned.push(quote! { #symbol => Some(if arguments.len() == #arity { (|| {
            use rils_stdlib::stdlib::cell::native_storage::#backend as operations;
            #body
        })() } else { Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())) }), });
    }
    Some(quote! {
        pub fn is_owned_symbol(symbol: &str) -> bool { [#(#symbols),*].contains(&symbol) }
        pub fn call_symbol(symbol: &str, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
            match symbol { #(#borrowed)* _ => None }
        }
        pub fn call_owned_symbol(symbol: &str, mut arguments: Vec<crate::Value>, context: &crate::runtime_builtins::NativeOwnedContext) -> Option<Result<crate::Value, String>> {
            match symbol { #(#owned)* _ => None }
        }
    })
}
