//! Discover generated language-package sources from Rust declarations.

use std::{collections::BTreeSet, fs, path::PathBuf};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Error, Item, LitStr, Path, parse_macro_input};

fn snake_case(name: &str) -> String {
    let chars = name.chars().collect::<Vec<_>>();
    let mut result = String::new();
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch.is_uppercase()
            && index > 0
            && (chars[index - 1].is_lowercase()
                || chars[index - 1].is_ascii_digit()
                || chars.get(index + 1).is_some_and(|next| next.is_lowercase()))
        {
            result.push('_');
        }
        result.extend(ch.to_lowercase());
    }
    if let Some(rest) = result.strip_prefix("b_tree_") {
        format!("btree_{rest}")
    } else {
        result
    }
}

fn marked_path(attribute: &Attribute, module: &Path, name: &str) -> syn::Result<String> {
    if !matches!(attribute.meta, syn::Meta::Path(_)) {
        return Err(Error::new_spanned(
            attribute,
            "Rils export marker does not accept arguments",
        ));
    }
    let mut parts = module
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect::<Vec<_>>();
    parts.push(snake_case(name));
    Ok(format!("{}.rils", parts.join("/")))
}

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
    let mut paths = Vec::new();
    collect_paths(&folder, &mut paths).map_err(|error| {
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
            let Some((_, contents)) = &module.content else {
                return Err(Error::new_spanned(
                    module,
                    "standard-library declaration module must be inline",
                ));
            };
            let marked = contents
                .iter()
                .filter_map(|item| match item {
                    Item::Trait(item) => item
                        .attrs
                        .iter()
                        .find(|attr| attr.path().is_ident("rils_trait"))
                        .map(|attr| (&item.ident, 1u8, attr)),
                    Item::Enum(item) => item
                        .attrs
                        .iter()
                        .find(|attr| attr.path().is_ident("rils_enum"))
                        .map(|attr| (&item.ident, 0u8, attr)),
                    Item::Struct(item) => item
                        .attrs
                        .iter()
                        .find(|attr| attr.path().is_ident("rils_struct"))
                        .map(|attr| (&item.ident, 0u8, attr)),
                    Item::Fn(item) => item
                        .attrs
                        .iter()
                        .find(|attr| attr.path().is_ident("rils_fn"))
                        .map(|attr| (&item.sig.ident, 2u8, attr)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let selected = if contents.iter().any(|item| {
                matches!(item, Item::Macro(item) if item.mac.path.is_ident("primitive_integer_family") || item.mac.path.is_ident("primitive_float_family"))
            }) {
                if !marked.is_empty() {
                    return Err(Error::new_spanned(
                        module,
                        "primitive family cannot share a Rils declaration module",
                    ));
                }
                vec![(
                    relative,
                    segments.last().cloned().unwrap_or_default(),
                    0u8,
                )]
            } else {
                if marked.is_empty() {
                    return Err(Error::new_spanned(
                        module,
                        "expected a #[rils_struct], #[rils_enum], #[rils_trait], or #[rils_fn] declaration",
                    ));
                }
                marked
                    .into_iter()
                    .map(|(name, kind, attr)| {
                        Ok((
                            marked_path(attr, &declaration_path, &name.to_string())?,
                            if kind == 2 {
                                format!("{}_{}", segments.join("_"), name)
                            } else {
                                name.to_string().to_lowercase()
                            },
                            kind,
                        ))
                    })
                    .collect::<syn::Result<Vec<_>>>()?
            };
            for (relative, name, kind) in selected {
                if !seen.insert(relative.clone()) {
                    return Err(Error::new_spanned(
                        module,
                        format!("duplicate standard-library source `{relative}`"),
                    ));
                }
                let relative = LitStr::new(&relative, directory.span());
                let callback = format_ident!("{name}_definition");
                let source = if kind == 1 {
                    quote!(rils_stdlib::#callback!(decl_rils_trait_source))
                } else if kind == 2 {
                    quote!(rils_stdlib::#callback!(decl_rils_function_source))
                } else {
                    quote!(rils_stdlib::#callback!(decl_rils_source))
                };
                exports.push(quote!((#relative, #source)));
            }
        }
        let absolute = LitStr::new(&path.to_string_lossy(), directory.span());
        dependencies.push(quote!(
            const _: &str = include_str!(#absolute);
        ));
    }
    Ok(quote!({ #(#dependencies)* &[#(#exports),*] }))
}

fn collect_paths(folder: &std::path::Path, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_paths(&path, output)?;
        } else {
            output.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_module_exports_each_marked_declaration_at_its_own_path() {
        let directory = LitStr::new(
            "tests/fixtures/mixed_stdlib",
            proc_macro2::Span::call_site(),
        );
        let tokens = collect(&directory).unwrap().to_string();
        assert!(tokens.contains("core/collections/buffer.rils"));
        assert!(tokens.contains("core/collections/state.rils"));
        assert!(tokens.contains("core/collections/marker.rils"));
        assert!(tokens.contains("buffer_definition"));
        assert!(tokens.contains("state_definition"));
        assert!(tokens.contains("marker_definition"));
        assert!(!tokens.contains("helper_definition"));
    }

    #[test]
    fn single_declaration_without_marker_is_not_exported() {
        let directory = LitStr::new(
            "tests/fixtures/unmarked_stdlib",
            proc_macro2::Span::call_site(),
        );
        let error = collect(&directory).unwrap_err();
        assert!(error.to_string().contains("expected a #[rils_struct]"));
    }
}
