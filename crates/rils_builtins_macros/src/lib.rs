mod builtin_files;
mod builtin_ids;
mod catalog_files;
mod numeric_files;
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
pub fn type_pattern(input: TokenStream) -> TokenStream {
    type_patterns::expand(input)
}
