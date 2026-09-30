//! Explicit runtime imports; exported methods otherwise use native calls.

use quote::ToTokens;
use syn::{Error, ImplItemFn, Path};

pub(super) enum MethodBinding {
    Native,
    Import(String),
}

impl MethodBinding {
    pub(super) fn parse(method: &ImplItemFn) -> syn::Result<Self> {
        let mut binding = Self::Native;
        for attribute in &method.attrs {
            let import = attribute.path().is_ident("rils_import");
            if !import {
                continue;
            }
            if !matches!(binding, Self::Native) {
                return Err(Error::new_spanned(
                    attribute,
                    "one runtime import is allowed per exported method",
                ));
            }
            let path = attribute
                .parse_args::<Path>()?
                .to_token_stream()
                .to_string()
                .replace(' ', "");
            binding = Self::Import(path);
        }
        Ok(binding)
    }
}
