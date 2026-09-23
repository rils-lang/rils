#![allow(linker_messages)]

mod builtin_files;
mod builtin_ids;
mod catalog_files;
mod decl_rils;
mod numeric_files;
mod stdlib;
mod type_patterns;

use proc_macro::TokenStream;

#[proc_macro]
pub fn builtin_id_declarations(input: TokenStream) -> TokenStream {
    builtin_ids::expand(input)
}

#[proc_macro]
pub fn builtin_file(input: TokenStream) -> TokenStream {
    builtin_files::expand(input)
}

#[proc_macro]
pub fn builtin_catalog_file(input: TokenStream) -> TokenStream {
    catalog_files::expand(input)
}

#[proc_macro]
pub fn builtin_numeric_file(input: TokenStream) -> TokenStream {
    numeric_files::expand(input)
}

#[proc_macro]
pub fn builtin_stdlib(input: TokenStream) -> TokenStream {
    stdlib::expand(input)
}

#[proc_macro]
pub fn type_pattern(input: TokenStream) -> TokenStream {
    type_patterns::expand(input)
}

#[proc_macro_attribute]
pub fn decl_rils(attribute: TokenStream, item: TokenStream) -> TokenStream {
    decl_rils::expand_definition(attribute, item)
}

#[proc_macro]
pub fn decl_rils_trait_source(input: TokenStream) -> TokenStream {
    decl_rils::trait_definition::expand_source(input)
}

#[proc_macro]
pub fn decl_rils_trait_metadata(input: TokenStream) -> TokenStream {
    decl_rils::trait_definition::expand_metadata(input)
}

#[proc_macro]
pub fn decl_rils_metadata(input: TokenStream) -> TokenStream {
    decl_rils::expand_metadata(input)
}

#[proc_macro]
pub fn decl_rils_source(input: TokenStream) -> TokenStream {
    decl_rils::expand_source(input)
}

#[proc_macro]
pub fn decl_rils_trait_impls(input: TokenStream) -> TokenStream {
    decl_rils::expand_trait_impls(input)
}

#[proc_macro]
pub fn decl_rils_native(input: TokenStream) -> TokenStream {
    decl_rils::expand_native(input)
}
