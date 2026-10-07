//! Native callback bridge generated from an exported free-function signature.

use proc_macro2::TokenStream as Tokens;
use quote::{ToTokens, format_ident, quote};
use syn::{Error, FnArg, ReturnType, Type};

use super::Input;

fn runtime_type(input: &Input, ty: &Type) -> syn::Result<Tokens> {
    match ty {
        Type::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => {
            let name = &path.path.segments[0].ident;
            if input
                .function
                .sig
                .generics
                .type_params()
                .any(|generic| generic.ident == *name)
            {
                Ok(quote!(crate::Value))
            } else if matches!(
                name.to_string().as_str(),
                "bool"
                    | "char"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "f32"
                    | "f64"
            ) {
                Ok(quote!(#name))
            } else {
                Err(Error::new_spanned(
                    ty,
                    "native callback bridge needs a supported value type",
                ))
            }
        }
        Type::Tuple(tuple) if tuple.elems.is_empty() => Ok(quote!(())),
        _ => Err(Error::new_spanned(
            ty,
            "native callback bridge needs a supported value type",
        )),
    }
}

pub(super) fn tokens(input: &Input) -> syn::Result<Tokens> {
    if !input.native_callback {
        return Err(Error::new_spanned(
            &input.function,
            "native callback export needs a fallible callback bound",
        ));
    }
    let adapter_name = if input.shadow_callback {
        format_ident!("__rils_try_{}", input.function.sig.ident)
    } else {
        input.function.sig.ident.clone()
    };
    let path = input.path.to_token_stream().to_string().replace(' ', "");
    let module = input
        .module_segments()
        .into_iter()
        .skip(1)
        .collect::<Vec<_>>();
    let rust_adapter = quote!(rils_stdlib::stdlib::#(#module::)*#adapter_name);
    let parameters = input.function.sig.inputs.iter().collect::<Vec<_>>();
    let mut imports = Vec::new();
    let mut call_arguments = Vec::new();
    let mut callback_count = 0;
    for (index, parameter) in parameters.iter().enumerate() {
        let FnArg::Typed(parameter) = parameter else {
            unreachable!()
        };
        let binding = format_ident!("input_{index}");
        if let Type::BareFn(function) = parameter.ty.as_ref() {
            callback_count += 1;
            let function_value = format_ident!("callback_value_{index}");
            imports.push(quote!(let #function_value = arguments.next().expect("arity checked");));
            let callback_inputs = function
                .inputs
                .iter()
                .enumerate()
                .map(|(position, argument)| {
                    let name = format_ident!("callback_arg_{index}_{position}");
                    let ty = runtime_type(input, &argument.ty)?;
                    Ok((name, ty))
                })
                .collect::<syn::Result<Vec<_>>>()?;
            let names = callback_inputs
                .iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>();
            let types = callback_inputs.iter().map(|(_, ty)| ty).collect::<Vec<_>>();
            let result_type = match &function.output {
                ReturnType::Default => quote!(()),
                ReturnType::Type(_, result) => runtime_type(input, result)?,
            };
            call_arguments.push(quote! {{
                let callback_cell = &callback_cell;
                move |#(#names: #types),*| -> Result<#result_type, crate::runtime_builtins::NativeCallError<E>> {
                    use crate::runtime_builtins::native_value::NativeValue;
                    let values = vec![#(NativeValue::into_value(#names)),*];
                    let mut invoke = callback_cell.try_borrow_mut()
                        .map_err(|_| crate::runtime_builtins::NativeCallError::Bridge("reentrant native callback".into()))?;
                    let result = (*invoke)(&#function_value, values)
                        .map_err(crate::runtime_builtins::NativeCallError::Callback)?;
                    <#result_type as NativeValue>::from_owned_value(result)
                        .map_err(crate::runtime_builtins::NativeCallError::Bridge)
                }
            }});
        } else {
            let ty = runtime_type(input, &parameter.ty)?;
            imports.push(quote! {
                let #binding = <#ty as crate::runtime_builtins::native_value::NativeValue>::from_owned_value(arguments.next().expect("arity checked"))
                    .map_err(crate::runtime_builtins::NativeCallError::Bridge)?;
            });
            call_arguments.push(quote!(#binding));
        }
    }
    if callback_count == 0 {
        return Err(Error::new_spanned(
            &input.function.sig,
            "native callback export needs a function parameter",
        ));
    }
    let result_type = runtime_type(input, &input.result()?)?;
    let arity = parameters.len();
    Ok(quote! {
        pub fn is_callback_symbol(symbol: &str) -> bool { symbol == #path }
        pub fn call_callback_symbol<E>(
            symbol: &str,
            arguments: std::vec::Vec<crate::Value>,
            callback: &mut crate::runtime_builtins::NativeCallback<'_, E>,
        ) -> Option<Result<crate::Value, crate::runtime_builtins::NativeCallError<E>>> {
            if symbol != #path { return None; }
            Some((|| {
                use crate::runtime_builtins::native_value::NativeValue;
                if arguments.len() != #arity {
                    return Err(crate::runtime_builtins::NativeCallError::Bridge(format!(
                        "native function expects {} arguments, found {}", #arity, arguments.len()
                    )));
                }
                let mut arguments = arguments.into_iter();
                #(#imports)*
                let callback_cell = std::cell::RefCell::new(callback);
                let result: #result_type = #rust_adapter(#(#call_arguments),*)?;
                Ok(NativeValue::into_value(result))
            })())
        }
    })
}
