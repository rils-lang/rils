//! Rils `Clone` declaration bound to Rust's `Clone` trait.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::clone)]
mod native {
    use rils_syntax::{
        ast::{EnumVariant, Stmt},
        parser::ParseError,
        quote::QuotedStatement,
        rils_quote, rils_quote_tokens,
    };

    /// Explicit owned duplication.
    pub trait Clone: ::core::clone::Clone {
        /// Explicitly duplicates an owned value.
        fn clone(&self) -> Self;
    }

    /// Generates a fieldwise `Clone` implementation for a struct or enum.
    #[rils_derive]
    fn derive_clone(statement: &Stmt) -> Result<Option<QuotedStatement>, ParseError> {
        let (name, generic_parameters, body) = match statement {
            Stmt::Struct {
                name,
                generic_parameters,
                fields,
                ..
            } => {
                let field_clones = fields.iter().map(|field| {
                    let field_name = &field.name;
                    let field_type = &field.type_annotation;
                    rils_quote_tokens!(#field_name: <#field_type as Clone>::clone(&self.#field_name))
                });
                (
                    name,
                    generic_parameters,
                    rils_quote_tokens!(#name { #(#field_clones),* }),
                )
            }
            Stmt::Enum {
                name,
                generic_parameters,
                variants,
                ..
            } => {
                let constructor = if generic_parameters.is_empty() {
                    name.as_str()
                } else {
                    "Self"
                };
                let arms = variants
                    .iter()
                    .map(|variant| clone_arm(name, constructor, variant))
                    .collect::<Vec<_>>();
                (
                    name,
                    generic_parameters,
                    rils_quote_tokens!(match self { #(#arms),* }),
                )
            }
            _ => unreachable!("derive attributes occur on types"),
        };
        let names = generic_parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>();
        let impl_generics = if names.is_empty() {
            String::new()
        } else {
            format!("<{}>", names.join(", "))
        };
        let type_arguments = impl_generics.clone();
        Ok(Some(rils_quote! {
            impl #impl_generics Clone for #name #type_arguments {
                fn clone(&self) -> Self {
                    #body
                }
            }
        }))
    }

    fn clone_arm(owner: &str, constructor: &str, variant: &EnumVariant) -> String {
        match variant {
            EnumVariant::Unit { name, .. } => {
                rils_quote_tokens!(#owner::#name => #constructor::#name)
            }
            EnumVariant::Tuple { name, fields, .. } => {
                let bindings = (0..fields.len())
                    .map(|index| format!("__rils_field_{index}"))
                    .collect::<Vec<_>>();
                let clones = fields
                    .iter()
                    .zip(&bindings)
                    .map(|(ty, binding)| rils_quote_tokens!(<#ty as Clone>::clone(#binding)));
                let patterns = &bindings;
                rils_quote_tokens!(#owner::#name(#(#patterns),*) => #constructor::#name(#(#clones),*))
            }
            EnumVariant::Record { name, fields, .. } => {
                let bindings = (0..fields.len())
                    .map(|index| format!("__rils_field_{index}"))
                    .collect::<Vec<_>>();
                let patterns = fields.iter().zip(&bindings).map(|(field, binding)| {
                    let field_name = &field.name;
                    rils_quote_tokens!(#field_name: #binding)
                });
                let clones = fields.iter().zip(&bindings).map(|(field, binding)| {
                    let field_name = &field.name;
                    let field_type = &field.type_annotation;
                    rils_quote_tokens!(#field_name: <#field_type as Clone>::clone(#binding))
                });
                rils_quote_tokens!(#owner::#name { #(#patterns),* } => #constructor::#name { #(#clones),* })
            }
        }
    }
}

pub use native::Clone;
pub use native::DERIVE;
