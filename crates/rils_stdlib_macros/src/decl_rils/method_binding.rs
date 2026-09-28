//! Explicit compatibility bindings; exported methods otherwise use native calls.

use quote::ToTokens;
use syn::{Error, ImplItemFn, Path};

pub(super) enum MethodBinding {
    Native,
    Legacy(String),
    Import(String),
}

impl MethodBinding {
    pub(super) fn parse(method: &ImplItemFn) -> syn::Result<Self> {
        let mut binding = Self::Native;
        for attribute in &method.attrs {
            let legacy = attribute.path().is_ident("rils_legacy_id");
            let import = attribute.path().is_ident("rils_import");
            if !legacy && !import {
                continue;
            }
            if !matches!(binding, Self::Native) {
                return Err(Error::new_spanned(
                    attribute,
                    "one explicit legacy ID or runtime import is allowed per exported method",
                ));
            }
            let path = attribute
                .parse_args::<Path>()?
                .to_token_stream()
                .to_string()
                .replace(' ', "");
            binding = if legacy {
                Self::Legacy(path)
            } else {
                Self::Import(path)
            };
        }
        Ok(binding)
    }
}
