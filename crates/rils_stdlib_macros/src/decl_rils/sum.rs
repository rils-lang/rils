//! Native bridge generation for the declared Option and Result methods.

use quote::quote;
use syn::{Error, FnArg, ImplItemFn, ItemEnum, ReturnType, Type};

use super::{Definition, Tokens};

pub(super) fn supports_direct_bridge(item: &ItemEnum, method: &ImplItemFn) -> bool {
    if uses_sum_adapter(item, method) {
        return true;
    }
    if callback_operation(item, method).is_some() {
        return true;
    }
    if !matches!(item.ident.to_string().as_str(), "Option" | "Result")
        || method.sig.inputs.len() != 1
    {
        return false;
    }
    let Some(receiver) = method.sig.receiver() else {
        return false;
    };
    let ReturnType::Type(_, result) = &method.sig.output else {
        return false;
    };
    if receiver.reference.is_some() {
        return receiver.mutability.is_none()
            && matches!(result.as_ref(), Type::Path(path) if path.path.is_ident("bool"));
    }
    if item.ident != "Result" {
        return false;
    }
    let Type::Path(path) = result.as_ref() else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident != "Option" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return false;
    };
    matches!(arguments.args.first(), Some(syn::GenericArgument::Type(Type::Path(path))) if path.path.is_ident("T") || path.path.is_ident("E"))
}

pub(super) fn uses_sum_adapter(item: &ItemEnum, method: &ImplItemFn) -> bool {
    let Some(receiver) = method.sig.receiver() else {
        return false;
    };
    let owner = item.ident.to_string();
    let name = method.sig.ident.to_string();
    matches!(
        (
            owner.as_str(),
            receiver.reference.is_some(),
            receiver.mutability.is_some(),
            name.as_str(),
        ),
        (
            "Option",
            false,
            _,
            "unwrap" | "unwrap_or" | "expect" | "or" | "xor"
        ) | ("Option", true, true, "take" | "replace")
            | (
                "Result",
                false,
                _,
                "unwrap" | "unwrap_or" | "expect" | "unwrap_err" | "expect_err" | "ok" | "err",
            )
    )
}

fn callback_operation(item: &ItemEnum, method: &ImplItemFn) -> Option<Tokens> {
    if method.sig.receiver()?.reference.is_some() || method.sig.inputs.len() != 2 {
        return None;
    }
    let FnArg::Typed(argument) = method.sig.inputs.last()? else {
        return None;
    };
    if !matches!(argument.ty.as_ref(), Type::BareFn(_)) {
        return None;
    }
    let operation = match (
        item.ident.to_string().as_str(),
        method.sig.ident.to_string().as_str(),
    ) {
        ("Option", "map") => quote!(OptionMap),
        ("Option", "and_then") => quote!(OptionAndThen),
        ("Option", "or_else") => quote!(OptionOrElse),
        ("Option", "filter") => quote!(OptionFilter),
        ("Result", "map") => quote!(ResultMap),
        ("Result", "map_err") => quote!(ResultMapErr),
        ("Result", "and_then") => quote!(ResultAndThen),
        ("Result", "or_else") => quote!(ResultOrElse),
        _ => return None,
    };
    Some(operation)
}

pub(super) fn native_tokens(definition: &Definition) -> syn::Result<Tokens> {
    if definition.item.ident != "Option" && definition.item.ident != "Result" {
        return Err(Error::new_spanned(
            &definition.item.ident,
            "native bridge supports Option and Result",
        ));
    }
    let module = &definition.module;
    let callback_implementations = definition
        .methods
        .iter()
        .filter_map(|method| {
            let operation = callback_operation(&definition.item, method)?;
            let name = &method.sig.ident;
            let id_path = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
            Some(quote! {
                #id_path => Some(super::super::callback::call(
                    super::super::callback::Operation::#operation,
                    arguments,
                    callback,
                ))
            })
        })
        .collect::<Vec<_>>();
    let implementations = definition.methods.iter().map(|method| {
        let name = &method.sig.ident;
        let owner = definition.item.ident.to_string();
        let id_path = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        if callback_operation(&definition.item, method).is_some() {
            return Ok(quote! { #id_path => Some(Err("native callback context is unavailable".to_owned())) });
        }
        if !supports_direct_bridge(&definition.item, method) {
            return Err(Error::new_spanned(&method.sig, "exported method signature has no native conversion"));
        }
        if uses_sum_adapter(&definition.item, method)
            && !(definition.item.ident == "Result" && matches!(name.to_string().as_str(), "ok" | "err"))
        {
            let arity = method.sig.inputs.len();
            return Ok(quote! {
                #id_path => Some(if arguments.len() == #arity {
                    super::super::option_result::call(#owner, stringify!(#name), arguments)
                } else {
                    Err(format!("native method expects {} arguments, found {}", #arity, arguments.len()))
                })
            });
        }
        if method.sig.inputs.len() != 1 {
            return Err(Error::new_spanned(&method.sig, "native bridge supports methods without arguments"));
        }
        let receiver = method.sig.receiver().ok_or_else(|| Error::new_spanned(&method.sig, "native method needs a receiver"))?;
        let is_shared = receiver.reference.is_some() && receiver.mutability.is_none();
        let is_owned = receiver.reference.is_none();
        let is_bool = matches!(method.sig.output, ReturnType::Type(_, ref ty) if matches!(ty.as_ref(), Type::Path(path) if path.path.is_ident("bool")));
        let native_query = if is_bool && is_shared {
            let conversion = if definition.item.ident == "Option" {
                quote! {
                    let native_self = match branch {
                        crate::value::borrowed_sum::Branch::Some => rils_stdlib::stdlib::option::Option::Some(()),
                        crate::value::borrowed_sum::Branch::None => rils_stdlib::stdlib::option::Option::None,
                        _ => return Err(format!("`{}` expects Option, found {}", stringify!(#name), receiver.type_name())),
                    };
                }
            } else {
                quote! {
                    let native_self = match branch {
                        crate::value::borrowed_sum::Branch::Ok => rils_stdlib::stdlib::result::Result::<(), ()>::Ok(()),
                        crate::value::borrowed_sum::Branch::Err => rils_stdlib::stdlib::result::Result::<(), ()>::Err(()),
                        _ => return Err(format!("`{}` expects Result, found {}", stringify!(#name), receiver.type_name())),
                    };
                }
            };
            quote! {
                if let Some(branch) = crate::value::sum::branch(receiver)? {
                    #conversion
                    let result: bool = native_self.#name();
                    return Ok(crate::Value::Bool(result));
                }
            }
        } else {
            quote!()
        };
        let result_code = if is_bool && is_shared {
            quote! {
                let result: bool = native_self.#name();
                Ok(crate::Value::Bool(result))
            }
        } else if definition.item.ident == "Result" && is_owned {
            let ReturnType::Type(_, ty) = &method.sig.output else {
                return Err(Error::new_spanned(&method.sig.output, "expected Option<T> or Option<E> result"));
            };
            let Type::Path(result_type) = ty.as_ref() else {
                return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result"));
            };
            let Some(segment) = result_type.path.segments.last() else {
                return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result"));
            };
            if segment.ident != "Option" {
                return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result"));
            }
            let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result"));
            };
            let element_type = match arguments.args.first() {
                Some(syn::GenericArgument::Type(Type::Path(path))) if path.path.is_ident("T") => quote!(ok_type),
                Some(syn::GenericArgument::Type(Type::Path(path))) if path.path.is_ident("E") => quote!(error_type),
                _ => return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result")),
            };
            quote! {
                let result: rils_stdlib::stdlib::option::Option<_> = native_self.#name();
                let value = match result {
                    rils_stdlib::stdlib::option::Option::Some(value) => Some(value),
                    rils_stdlib::stdlib::option::Option::None => None,
                };
                Ok(crate::Value::Option { value, element_type: #element_type })
            }
        } else {
            return Err(Error::new_spanned(&method.sig, "native bridge supports shared bool methods and owned Result to Option methods"));
        };
        let conversion = if definition.item.ident == "Option" {
            quote! {
                let native_self = match receiver {
                    crate::Value::Option { value: Some(value), .. } => rils_stdlib::stdlib::option::Option::Some(value),
                    crate::Value::Option { value: None, .. } => rils_stdlib::stdlib::option::Option::None,
                    value => return Err(format!("`{}` expects Option, found {}", stringify!(#name), value.type_name())),
                };
            }
        } else {
            quote! {
                let (native_self, ok_type, error_type) = match receiver {
                    crate::Value::Result { value: Ok(value), ok_type, error_type } =>
                        (rils_stdlib::stdlib::result::Result::Ok(value), ok_type, error_type),
                    crate::Value::Result { value: Err(value), ok_type, error_type } =>
                        (rils_stdlib::stdlib::result::Result::Err(value), ok_type, error_type),
                    value => return Err(format!("`{}` expects Result, found {}", stringify!(#name), value.type_name())),
                };
                let _ = (&ok_type, &error_type);
            }
        };
        Ok(quote! {
            #id_path => Some((|| -> Result<crate::Value, String> {
                if arguments.len() != 1 {
                    return Err(format!("native method expects one receiver, found {} arguments", arguments.len()));
                }
                let receiver = arguments.first().ok_or_else(|| "missing native receiver".to_owned())?;
                #native_query
                let receiver = match receiver {
                    crate::Value::Reference(reference) => reference.read()?,
                    value => value.clone(),
                };
                let receiver = receiver.materialize_native_sum()
                    .transpose()?
                    .unwrap_or(receiver);
                #conversion
                #result_code
            })())
        })
    }).collect::<syn::Result<Vec<_>>>()?;
    Ok(quote! {
        pub fn call_callback_symbol<E>(
            symbol: &str,
            arguments: &[crate::Value],
            callback: &mut crate::runtime_builtins::NativeCallback<'_, E>,
        ) -> Option<Result<crate::Value, crate::runtime_builtins::NativeCallError<E>>> {
            match symbol { #(#callback_implementations,)* _ => None }
        }

        pub fn call_symbol(
            symbol: &str,
            arguments: &[crate::Value],
        ) -> Option<Result<crate::Value, String>> {
            match symbol { #(#implementations,)* _ => None }
        }
    })
}
