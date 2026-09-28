//! Collect native registrations declared beside standard-library types.

use std::{
    fs,
    path::{Path, PathBuf},
};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Error, Item, LitStr, parse_macro_input};

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let folder = parse_macro_input!(input as LitStr);
    match collect(&folder) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn collect(folder: &LitStr) -> syn::Result<proc_macro2::TokenStream> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| Error::new_spanned(folder, "CARGO_MANIFEST_DIR is unavailable"))?;
    let root = PathBuf::from(manifest).join(folder.value());
    if !root.is_dir() {
        return Err(Error::new_spanned(
            folder,
            "native registry source folder does not exist",
        ));
    }
    let mut files = Vec::new();
    visit(&root, &root, &mut files).map_err(|error| Error::new_spanned(folder, error))?;
    files.sort();

    let mut layouts = Vec::new();
    let mut elements = Vec::new();
    let mut dependencies = Vec::new();
    for relative in files {
        let absolute = root.join(&relative);
        let source =
            fs::read_to_string(&absolute).map_err(|error| Error::new_spanned(folder, error))?;
        let parsed = syn::parse_file(&source)?;
        let mut segments = relative
            .components()
            .map(|segment| segment.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let file = segments.pop().expect("visited source has a filename");
        let stem = file.strip_suffix(".rs").expect("visited Rust source");
        if stem != "mod" {
            segments.push(stem.to_owned());
        }
        let path = segments.iter().map(|segment| format_ident!("{segment}"));
        let module = if segments.is_empty() {
            quote!(crate::stdlib)
        } else {
            quote!(crate::stdlib::#(#path)::*)
        };
        for item in parsed.items {
            let Item::Const(item) = item else { continue };
            if !matches!(item.vis, syn::Visibility::Public(_)) {
                continue;
            }
            let name = item.ident.to_string();
            let ident = &item.ident;
            if name == "NATIVE_LAYOUT" || name.starts_with("NATIVE_LAYOUT_") {
                layouts.push(quote!(#module::#ident));
            } else if name == "NATIVE_ELEMENT" || name.starts_with("NATIVE_ELEMENT_") {
                elements.push(quote!(#module::#ident));
            }
        }
        let tracked = format!(
            "{}/{}",
            folder.value().replace('\\', "/"),
            relative.to_string_lossy().replace('\\', "/")
        );
        dependencies.push(quote!(
            const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #tracked));
        ));
    }
    Ok(quote! {
        #(#dependencies)*
        static LAYOUTS: &[rils_native::LayoutRegistration] = &[#(#layouts),*];
        static ELEMENTS: &[rils_native::ElementRegistration] = &[#(#elements),*];
        static REGISTRY: rils_native::NativeRegistry = rils_native::NativeRegistry::new(LAYOUTS, ELEMENTS);

        pub fn registry() -> &'static rils_native::NativeRegistry {
            &REGISTRY
        }
    })
}

fn visit(root: &Path, folder: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(folder).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            visit(root, &path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(
                path.strip_prefix(root)
                    .map_err(|error| error.to_string())?
                    .to_owned(),
            );
        }
    }
    Ok(())
}
