//! Rils `Copy` declaration bound to Rust's `Copy` trait.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::copy)]
mod native {
    use rils_syntax::{ast::Stmt, parser::ParseError, quote::QuotedStatement, rils_quote};

    /// Values duplicated by ordinary reads.
    pub trait Copy: super::Clone + ::core::marker::Copy {}

    /// Generates the marker implementation; field eligibility is checked by Rils.
    #[rils_derive]
    fn derive_copy(statement: &Stmt) -> Result<Option<QuotedStatement>, ParseError> {
        let Stmt::Struct {
            name,
            generic_parameters,
            span,
            ..
        } = statement
        else {
            return Err(ParseError {
                message: "Copy can currently only be derived for structs".into(),
                span: match statement {
                    Stmt::Enum { span, .. } => *span,
                    _ => Default::default(),
                },
            });
        };
        if !generic_parameters.is_empty() {
            return Err(ParseError {
                message: "deriving Copy for generic structs requires conditional trait impls"
                    .into(),
                span: *span,
            });
        }
        Ok(Some(rils_quote! { impl Copy for #name {} }))
    }
}

pub use native::Copy;
pub use native::DERIVE;
