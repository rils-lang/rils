//! Shared queries inspect a discriminant and call the declared method body.

use quote::quote;
use syn::{ImplItemFn, ItemEnum};

use super::Tokens;

pub(super) fn call(item: &ItemEnum, method: &ImplItemFn) -> Tokens {
    let name = &method.sig.ident;
    let owner = item.ident.to_string();
    let conversion = if item.ident == "Option" {
        quote! {
            let native_self = match branch {
                crate::value::borrowed_sum::Branch::Some => rils_stdlib::stdlib::option::Option::Some(()),
                crate::value::borrowed_sum::Branch::None => rils_stdlib::stdlib::option::Option::None,
                _ => return Err("native query expects Option".into()),
            };
        }
    } else {
        quote! {
            let native_self = match branch {
                crate::value::borrowed_sum::Branch::Ok => rils_stdlib::stdlib::result::Result::<(), ()>::Ok(()),
                crate::value::borrowed_sum::Branch::Err => rils_stdlib::stdlib::result::Result::<(), ()>::Err(()),
                _ => return Err("native query expects Result".into()),
            };
        }
    };
    quote! {
        let [receiver] = arguments else {
            return Err(format!("native method expects one receiver, found {} arguments", arguments.len()));
        };
        let branch = super::super::sum_native::branch(receiver).map_err(|message| {
            format!("native query expects {}: {}", #owner, message)
        })?;
        #conversion
        Ok(crate::Value::Bool(native_self.#name()))
    }
}
