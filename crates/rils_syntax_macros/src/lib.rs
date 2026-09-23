//! Procedural syntax helpers for Rils.
#![allow(linker_messages)]

mod quote;
mod registry;

use proc_macro::TokenStream;

#[proc_macro]
pub fn rils_quote(input: TokenStream) -> TokenStream {
    quote::expand(input)
}

#[proc_macro]
pub fn rils_quote_tokens(input: TokenStream) -> TokenStream {
    quote::expand_tokens(input)
}

#[proc_macro]
pub fn rils_derive_registry(input: TokenStream) -> TokenStream {
    registry::expand(input)
}
