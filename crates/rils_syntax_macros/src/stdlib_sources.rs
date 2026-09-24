//! Discover generated language-package sources from Rust declarations.

use std::{collections::BTreeSet, fs, path::PathBuf};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Error, Item, LitStr, Path, parse_macro_input};

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
    let mut paths = fs::read_dir(&folder)
        .map_err(|error| {
            Error::new(
                directory.span(),
                format!("cannot read source directory: {error}"),
            )
        })?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            Error::new(
                directory.span(),
                format!("cannot list source directory: {error}"),
            )
        })?;
    paths.sort();

    let mut exports = Vec::new();
    let mut dependencies = Vec::new();
    let mut seen = BTreeSet::new();
    for path in paths {
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
        for item in &file.items {
            let Item::Mod(module) = item else { continue };
            let Some(attribute) = module
                .attrs
                .iter()
                .find(|attribute| attribute.path().is_ident("decl_rils"))
            else {
                continue;
            };
            let declaration_path = attribute.parse_args::<Path>()?;
            let segments = declaration_path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>();
            let relative = format!("{}.rils", segments.join("/"));
            if !seen.insert(relative.clone()) {
                return Err(Error::new_spanned(
                    module,
                    format!("duplicate standard-library source `{relative}`"),
                ));
            }
            let Some((_, contents)) = &module.content else {
                return Err(Error::new_spanned(
                    module,
                    "standard-library declaration module must be inline",
                ));
            };
            let marked = contents
                .iter()
                .filter_map(|item| match item {
                    Item::Trait(item)
                        if item
                            .attrs
                            .iter()
                            .any(|attr| attr.path().is_ident("rils_trait")) =>
                    {
                        Some((item.ident.to_string().to_lowercase(), true))
                    }
                    Item::Enum(item)
                        if item
                            .attrs
                            .iter()
                            .any(|attr| attr.path().is_ident("rils_enum")) =>
                    {
                        Some((item.ident.to_string().to_lowercase(), false))
                    }
                    Item::Struct(item)
                        if item
                            .attrs
                            .iter()
                            .any(|attr| attr.path().is_ident("rils_struct")) =>
                    {
                        Some((item.ident.to_string().to_lowercase(), false))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let declarations = contents
                .iter()
                .filter_map(|item| match item {
                    Item::Trait(item) => Some((item.ident.to_string().to_lowercase(), true)),
                    Item::Enum(item) => Some((item.ident.to_string().to_lowercase(), false)),
                    Item::Struct(item) => Some((item.ident.to_string().to_lowercase(), false)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let selected = if !marked.is_empty() {
                marked
            } else {
                match declarations.as_slice() {
                    [(name, is_trait)] => vec![(name.clone(), *is_trait)],
                    [] if contents.iter().any(|item| matches!(item, Item::Macro(_))) => {
                        vec![(segments.last().cloned().unwrap_or_default(), false)]
                    }
                    _ => {
                        return Err(Error::new_spanned(
                            module,
                            "expected one type or trait declaration",
                        ));
                    }
                }
            };
            let relative = LitStr::new(&relative, directory.span());
            let sources = selected.iter().map(|(name, is_trait)| {
                let callback = format_ident!("{name}_definition");
                if *is_trait {
                    quote!(rils_stdlib::#callback!(decl_rils_trait_source))
                } else {
                    quote!(rils_stdlib::#callback!(decl_rils_source))
                }
            });
            exports.push(quote!((#relative, concat!(#(#sources),*))));
        }
        let absolute = LitStr::new(&path.to_string_lossy(), directory.span());
        dependencies.push(quote!(
            const _: &str = include_str!(#absolute);
        ));
    }
    Ok(quote!({ #(#dependencies)* &[#(#exports),*] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_module_exports_each_marked_declaration_in_one_source() {
        let directory = LitStr::new(
            "tests/fixtures/mixed_stdlib",
            proc_macro2::Span::call_site(),
        );
        let tokens = collect(&directory).unwrap().to_string();
        assert!(tokens.contains("core/collections.rils"));
        assert!(tokens.contains("buffer_definition"));
        assert!(tokens.contains("state_definition"));
        assert!(tokens.contains("marker_definition"));
        assert!(!tokens.contains("helper_definition"));
    }
}
