#![allow(linker_messages)]

mod decl_rils;
mod type_patterns;

use proc_macro::TokenStream;

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
pub fn decl_rils_function_source(input: TokenStream) -> TokenStream {
    decl_rils::function_definition::expand_source(input)
}

#[proc_macro]
pub fn decl_rils_function_metadata(input: TokenStream) -> TokenStream {
    decl_rils::function_definition::expand_metadata(input)
}

#[proc_macro]
pub fn decl_rils_function_native(input: TokenStream) -> TokenStream {
    decl_rils::function_definition::expand_native(input)
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

#[proc_macro]
pub fn decl_rils_layout(input: TokenStream) -> TokenStream {
    decl_rils::expand_layout(input)
}
