//! Shared-handle calls generated from the declared storage and method signatures.
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
        "std::rc::Rc<T>" => quote!(strong),
        "std::rc::Weak<T>" => quote!(weak),
        _ => return None,
    };
    let mut borrowed = Vec::new();
    let mut owned = Vec::new();
    let mut owned_symbols = Vec::new();
    let module = &definition.path;
    for method in &definition.methods {
        let name = &method.sig.ident;
        let symbol = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        let arity = method.sig.inputs.len();
        let body = if method.sig.receiver().is_none() {
            if arity != 1 || name != "new" {
                return Some(
                    Error::new_spanned(&method.sig, "unsupported shared-handle constructor")
                        .into_compile_error(),
                );
            }
            owned_symbols.push(symbol.clone());
            owned.push(quote! {
                #symbol => Some(if arguments.len() == #arity { (|| {
                    let value = arguments.into_iter().next().expect("arity checked");
                    let ty = crate::Type::of_value(&value).ok_or("shared handle requires a concrete item type")?;
                    let mut codec = crate::value::record_codec::NativeRecordCodec::with_definitions(&context.structs, &context.enums).with_owned_conversion();
                    let value = codec.into_native(value, context.layout(&ty)?)?;
                    let result = rils_stdlib::stdlib::rc::native_storage::#backend::#name(value)?;
                    crate::value::native_instance::from_native(result, std::rc::Rc::new(codec))
                })() } else { Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())) }),
            });
            quote!(Err(
                "shared-handle constructor requires an owned argument frame".into()
            ))
        } else {
            if arity != 1
                || !method
                    .sig
                    .receiver()
                    .is_some_and(|r| r.reference.is_some() && r.mutability.is_none())
            {
                return Some(
                    Error::new_spanned(
                        &method.sig,
                        "shared-handle query requires &self and no arguments",
                    )
                    .into_compile_error(),
                );
            }
            let scalar = matches!(&method.sig.output, ReturnType::Type(_, ty) if matches!(ty.as_ref(), Type::Path(path) if path.path.is_ident("usize")));
            let output = if scalar {
                quote!(Ok(crate::Value::from(crate::value::native_usize(result))))
            } else {
                quote!(crate::value::native_instance::from_native(
                    result,
                    std::rc::Rc::new(object.codec())
                ))
            };
            quote! { (|| {
                let object = crate::value::native_receiver::NativeReceiver::from_value(&arguments[0])?
                    .ok_or("expected native shared-handle receiver")?;
                let result = object.with(rils_stdlib::stdlib::rc::native_storage::#backend::#name)??;
                #output
            })() }
        };
        borrowed.push(quote! {
            #symbol => Some(if arguments.len() == #arity { #body }
                else { Err(format!("native method `{}` expects {} arguments, found {}", #symbol, #arity, arguments.len())) }),
        });
    }
    let owned_functions = if owned_symbols.is_empty() {
        quote!()
    } else {
        quote! {
            pub fn is_owned_symbol(symbol: &str) -> bool { [#(#owned_symbols),*].contains(&symbol) }
            pub fn call_owned_symbol(symbol: &str, arguments: Vec<crate::Value>, context: &crate::runtime_builtins::NativeOwnedContext) -> Option<Result<crate::Value, String>> {
                match symbol { #(#owned)* _ => None }
            }
        }
    };
    Some(quote! {
        pub fn call_symbol(symbol: &str, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
            match symbol { #(#borrowed)* _ => None }
        }
        #owned_functions
    })
}
