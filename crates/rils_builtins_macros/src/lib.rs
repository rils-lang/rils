#![allow(linker_messages)]

mod catalog_files;
mod source_helpers;
mod stdlib;

use proc_macro::TokenStream;

#[proc_macro]
pub fn builtin_catalog_file(input: TokenStream) -> TokenStream {
    catalog_files::expand(input)
}

#[proc_macro]
pub fn builtin_stdlib(input: TokenStream) -> TokenStream {
    stdlib::expand(input)
}
