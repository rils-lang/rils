//! Owned argument and return conversions selected by the Rust signature.

use quote::{format_ident, quote};
use syn::{Error, FnArg, ImplItemFn, ItemEnum, ReturnType, Type};

use super::Tokens;

enum Conversion {
    Item(usize),
    Receiver,
    Option(usize),
    String,
    Bool,
}

fn conversion(item: &ItemEnum, ty: &Type) -> Option<Conversion> {
    let Type::Path(path) = ty else { return None };
    if path.path.is_ident("Self") {
        return Some(Conversion::Receiver);
    }
    if path.path.is_ident("String") {
        return Some(Conversion::String);
    }
    if path.path.is_ident("bool") {
        return Some(Conversion::Bool);
    }
    for (index, generic) in item.generics.type_params().enumerate() {
        if path.path.is_ident(&generic.ident) {
            return Some(Conversion::Item(index));
        }
    }
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    if arguments.args.len() != 1 {
        return None;
    }
    let syn::GenericArgument::Type(ty) = arguments.args.first()? else {
        return None;
    };
    match conversion(item, ty)? {
        Conversion::Item(index) => Some(Conversion::Option(index)),
        _ => None,
    }
}

pub(super) fn supports(item: &ItemEnum, method: &ImplItemFn) -> bool {
    if !matches!(item.ident.to_string().as_str(), "Option" | "Result") {
        return false;
    }
    let Some(receiver) = method.sig.receiver() else {
        return false;
    };
    if receiver.reference.is_some() && receiver.mutability.is_none() {
        return false;
    }
    let ReturnType::Type(_, output) = &method.sig.output else {
        return false;
    };
    let Some(output) = conversion(item, output) else {
        return false;
    };
    if matches!(output, Conversion::String)
        || (item.ident == "Result" && matches!(output, Conversion::Receiver))
    {
        return false;
    }
    if receiver.reference.is_some()
        && (item.ident != "Option" || !matches!(output, Conversion::Receiver))
    {
        return false;
    }
    method.sig.inputs.iter().skip(1).all(|argument| {
        matches!(argument, FnArg::Typed(argument) if matches!(conversion(item, &argument.ty),
            Some(Conversion::Item(_) | Conversion::Receiver | Conversion::String | Conversion::Bool)))
    })
}

pub(super) fn call(item: &ItemEnum, method: &ImplItemFn) -> syn::Result<Tokens> {
    let arity = method.sig.inputs.len();
    let owner = item.ident.to_string();
    let option = item.ident == "Option";
    let import = if option {
        format_ident!("import_option")
    } else {
        format_ident!("import_result")
    };
    let mut bindings = Vec::new();
    let mut inputs = Vec::new();
    for (index, input) in method.sig.inputs.iter().enumerate().skip(1) {
        let FnArg::Typed(input) = input else {
            return Err(Error::new_spanned(input, "unexpected receiver"));
        };
        let binding = format_ident!("argument_{index}");
        let argument_name = match input.pat.as_ref() {
            syn::Pat::Ident(pattern) => pattern.ident.to_string(),
            _ => format!("argument {index}"),
        };
        let convert = match conversion(item, &input.ty) {
            Some(Conversion::Item(index)) => {
                quote!(bridge.item_argument(value, #index, #argument_name)?)
            }
            Some(Conversion::Receiver) => quote!(bridge.#import(value)?),
            Some(Conversion::String) => quote!(super::super::sum_native::string_argument(value)?),
            Some(Conversion::Bool) => {
                quote!(value.as_bool().ok_or("native argument expects bool")?)
            }
            _ => return Err(Error::new_spanned(&input.ty, "unsupported sum argument")),
        };
        bindings.push(quote! {
            let value = arguments.next().expect("arity checked");
            let #binding = #convert;
        });
        inputs.push(binding);
    }
    let fallible = super::fallible::method(method).is_some();
    let name = if fallible {
        format_ident!("__rils_try_{}", method.sig.ident)
    } else {
        method.sig.ident.clone()
    };
    let invoke = quote!(native_self.#name(#(#inputs),*));
    let invoke = if fallible { quote!(#invoke?) } else { invoke };
    let mutable = method
        .sig
        .receiver()
        .expect("checked receiver")
        .reference
        .is_some();
    let check_receiver = if mutable {
        quote! {
            if !matches!(&receiver, crate::Value::Reference(_)) {
                return Err("native method requires a mutable binding".into());
            }
        }
    } else {
        quote!()
    };
    let body = if mutable {
        quote! { bridge.with_mut_option(receiver, |native_self| Ok(#invoke)) }
    } else {
        let ReturnType::Type(_, output) = &method.sig.output else {
            unreachable!()
        };
        let output = match conversion(item, output).expect("checked output") {
            Conversion::Item(index) => quote!(bridge.item_argument(result, #index, "return value")),
            Conversion::Receiver => quote!(bridge.export_option(result, None)),
            Conversion::Option(index) => quote!(bridge.export_option(result, Some(#index))),
            Conversion::Bool => quote!(Ok(crate::Value::Bool(result))),
            _ => return Err(Error::new_spanned(output, "unsupported sum return type")),
        };
        quote! {
            let native_self = bridge.#import(receiver)?;
            let result = #invoke;
            #output
        }
    };
    Ok(quote! {
        if arguments.len() != #arity {
            return Err(format!("native method expects {} arguments, found {}", #arity, arguments.len()));
        }
        let mut arguments = arguments.into_iter();
        let receiver = arguments.next().expect("arity checked");
        #check_receiver
        let mut bridge = super::super::sum_native::SumBridge::new(&receiver, #owner, context)?;
        #(#bindings)*
        #body
    })
}
