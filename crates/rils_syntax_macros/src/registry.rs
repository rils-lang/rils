//! Collect derive handlers from Rust declaration modules.

use std::{fs, path::PathBuf};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Error, Item, LitStr, parse_macro_input};

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let directory = parse_macro_input!(input as LitStr);
    match collect(&directory) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn collect(directory: &LitStr) -> syn::Result<proc_macro2::TokenStream> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|error| {
        Error::new(
            directory.span(),
            format!("missing CARGO_MANIFEST_DIR: {error}"),
        )
    })?);
    let folder = root.join(directory.value());
    let mut files = fs::read_dir(&folder)
        .map_err(|error| {
            Error::new(
                directory.span(),
                format!("cannot read derive directory: {error}"),
            )
        })?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            Error::new(
                directory.span(),
                format!("cannot read derive directory: {error}"),
            )
        })?;
    files.sort();
    let mut handlers = Vec::new();
    let mut dependencies = Vec::new();
    for path in files {
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|error| {
            Error::new(
                directory.span(),
                format!("cannot read {}: {error}", path.display()),
            )
        })?;
        let file = syn::parse_file(&source).map_err(|error| {
            Error::new(
                directory.span(),
                format!("cannot parse {}: {error}", path.display()),
            )
        })?;
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| {
                Error::new(directory.span(), "derive source has an invalid module name")
            })?;
        let module = format_ident!("{stem}");
        let absolute = LitStr::new(&path.to_string_lossy(), directory.span());
        let mut found = false;
        for item in &file.items {
            let Item::Mod(declaration) = item else {
                continue;
            };
            if !declaration
                .attrs
                .iter()
                .any(|attribute| attribute.path().is_ident("decl_rils"))
            {
                continue;
            }
            let has_derive = declaration.content.as_ref().is_some_and(|(_, items)| items.iter().any(|item| {
                matches!(item, Item::Fn(function) if function.attrs.iter().any(|attribute| attribute.path().is_ident("rils_derive")))
            }));
            if has_derive {
                found = true;
                let declaration_name = &declaration.ident;
                handlers.push(quote!(crate::stdlib::#module::#declaration_name::DERIVE));
            }
        }
        if found {
            dependencies.push(quote!(
                const _: &str = include_str!(#absolute);
            ));
        }
    }
    Ok(quote!({ #(#dependencies)* &[#(#handlers),*] }))
}
