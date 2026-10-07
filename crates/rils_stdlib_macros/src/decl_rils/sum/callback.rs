//! Generate owned callback calls from signatures, independently of method names.

use quote::{format_ident, quote};
use syn::{Error, FnArg, ImplItemFn, ItemEnum, ReturnType, Type};

use super::Tokens;
use types::Shape;

mod types;

pub(super) fn supports(item: &ItemEnum, method: &ImplItemFn) -> bool {
    method.sig.receiver().is_some_and(|receiver| receiver.reference.is_none())
        && method.sig.inputs.iter().skip(1).any(|argument| {
            matches!(argument, FnArg::Typed(argument) if matches!(argument.ty.as_ref(), Type::BareFn(_)))
        })
        && call(item, method).is_ok()
}

pub(super) fn call(item: &ItemEnum, method: &ImplItemFn) -> syn::Result<Tokens> {
    let ReturnType::Type(_, output) = &method.sig.output else {
        return Err(Error::new_spanned(
            &method.sig.output,
            "callback method needs a return type",
        ));
    };
    let output = Shape::parse(item, method, output)?;
    if matches!(output, Shape::Shared(_)) {
        return Err(Error::new_spanned(
            &method.sig.output,
            "callback method cannot return a temporary borrow",
        ));
    }
    let owner = item.ident.to_string();
    let input_pattern = Shape::receiver(item).pattern();
    let output_pattern = output.pattern();
    let import = format_ident!("import_{}", owner.to_lowercase());
    let arity = method.sig.inputs.len();
    let mut imports = Vec::new();
    let mut call_arguments = Vec::new();
    for (index, argument) in method.sig.inputs.iter().enumerate().skip(1) {
        let FnArg::Typed(argument) = argument else {
            return Err(Error::new_spanned(argument, "unexpected callback receiver"));
        };
        let Type::BareFn(function) = argument.ty.as_ref() else {
            return Err(Error::new_spanned(
                &argument.ty,
                "callback method parameters must be callable",
            ));
        };
        let binding = format_ident!("function_{index}");
        imports.push(quote!(let #binding = arguments.next().expect("arity checked");));
        let mut inputs = Vec::new();
        let mut patterns = Vec::new();
        let mut values = Vec::new();
        for (position, argument) in function.inputs.iter().enumerate() {
            let shape = Shape::parse(item, method, &argument.ty)?;
            let name = format_ident!("callback_{index}_{position}");
            let ty = shape.rust_type();
            patterns.push(shape.pattern());
            inputs.push(quote!(#name: #ty));
            values.push(shape.encode(quote!(#name)));
        }
        let result = match &function.output {
            ReturnType::Default => Shape::Unit,
            ReturnType::Type(_, ty) => Shape::parse(item, method, ty)?,
        };
        if matches!(result, Shape::Shared(_)) {
            return Err(Error::new_spanned(
                &function.output,
                "callback cannot return a temporary borrow",
            ));
        }
        let result_type = result.rust_type();
        let result_pattern = result.pattern();
        imports.push(quote! {
            super::super::callback::check_signature(
                &crate::Type::function(vec![#(#patterns),*], #result_pattern), &#binding,
            )?;
        });
        let decoded = result.decode(quote!(value));
        call_arguments.push(quote! {{
            let bridge = &bridge;
            let types = &types;
            let callback = &callback;
            move |#(#inputs),*| -> Result<#result_type, crate::runtime_builtins::NativeCallError<E>> {
                let arguments = vec![#(#values),*];
                let value = callback.try_borrow_mut()
                    .map_err(|_| "reentrant native callback")?(&#binding, arguments)
                    .map_err(crate::runtime_builtins::NativeCallError::Callback)?;
                #decoded.map_err(crate::runtime_builtins::NativeCallError::Bridge)
            }
        }});
    }
    let name = format_ident!("__rils_try_{}", method.sig.ident);
    let exported = output.encode(quote!(result));
    Ok(quote! {
        if arguments.len() != #arity {
            return Err(format!("native callback method expects {} arguments, found {}", #arity, arguments.len()).into());
        }
        let mut arguments = arguments.into_iter();
        let receiver = arguments.next().expect("arity checked");
        let mut bridge = super::super::sum_native::SumBridge::new(&receiver, #owner, context)?;
        #(#imports)*
        let types = super::super::callback::Types::new(
            &#input_pattern, bridge.rils_type(), &#output_pattern, expected,
        )?;
        bridge.for_type(types.output(), context)?;
        let native_self = bridge.#import(receiver)?;
        let callback = std::cell::RefCell::new(callback);
        let result = native_self.#name(#(#call_arguments),*)?;
        Ok(#exported)
    })
}
