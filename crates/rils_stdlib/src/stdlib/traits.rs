//! Basic traits shared by the host-independent standard library.

use rils_builtins_macros::decl_rils;

/// Marker for host-defined flag enums. The Rils implementation is registered
/// by the host, so this Rust trait has no blanket implementation.
pub trait BitFlagsMarker {}

#[decl_rils(core::default)]
pub(crate) mod default_native {
    use rils_syntax::{
        ast::{Block, Expr, ImplMethod, Stmt},
        default::{DefaultPlan, default_plan},
        parser::ParseError,
        quote::QuotedStatement,
        rils_quote, rils_quote_tokens,
        types::Type,
    };

    /// Types with a canonical default value.
    pub trait Default: ::core::default::Default {
        /// Constructs the default value for this type.
        fn default() -> Self;
    }

    /// Generates a fieldwise default constructor.
    #[rils_derive]
    fn derive_default(statement: &Stmt) -> Result<Option<QuotedStatement>, ParseError> {
        let Stmt::Struct {
            name,
            name_span,
            generic_parameters,
            fields,
            span,
            ..
        } = statement
        else {
            return Err(ParseError {
                message: "Default can currently only be derived for structs".into(),
                span: match statement {
                    Stmt::Enum { span, .. } => *span,
                    _ => Default::default(),
                },
            });
        };
        if fields.is_empty() {
            let target = Type::Named {
                name: name.clone(),
                arguments: generic_parameters
                    .iter()
                    .map(|parameter| Type::Variable(parameter.name.clone()))
                    .collect(),
            };
            return Ok(Some(QuotedStatement::from_statement(Stmt::Impl {
                generic_parameters: generic_parameters.clone(),
                trait_name: Some("Default".into()),
                target: target.clone(),
                associated_types: Vec::new(),
                methods: vec![ImplMethod {
                    attributes: Vec::new(),
                    name: "default".into(),
                    name_span: *name_span,
                    generic_parameters: Vec::new(),
                    parameters: Vec::new(),
                    return_type: Some(target),
                    body: Block {
                        statements: vec![Stmt::Expr {
                            expression: Expr::RecordLiteral {
                                path: vec![name.clone()],
                                fields: Vec::new(),
                                span: *span,
                            },
                            terminated: false,
                        }],
                        span: *span,
                    },
                    span: *span,
                }],
                span: *span,
            })));
        }
        let mut required = std::collections::HashSet::new();
        let field_defaults = fields
            .iter()
            .map(|field| {
                let ty = &field.type_annotation;
                let plan = default_plan(ty).ok_or_else(|| ParseError {
                    message: format!(
                        "cannot derive Default for `{name}`: field `{}` of type `{ty}` does not implement Default",
                        field.name
                    ),
                    span: field.span,
                })?;
                collect_required_bounds(&plan, &mut required);
                let field_name = &field.name;
                Ok(rils_quote_tokens!(#field_name: <#ty as Default>::default()))
            })
            .collect::<Result<Vec<_>, ParseError>>()?;
        let parameters = generic_parameters
            .iter()
            .map(|parameter| {
                let mut bounds = parameter.bounds.clone();
                if required.contains(&parameter.name)
                    && !bounds.iter().any(|bound| bound == "Default")
                {
                    bounds.push("Default".into());
                }
                if bounds.is_empty() {
                    parameter.name.clone()
                } else {
                    format!("{}: {}", parameter.name, bounds.join(" + "))
                }
            })
            .collect::<Vec<_>>();
        let names = generic_parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>();
        let impl_generics = if parameters.is_empty() {
            String::new()
        } else {
            format!("<{}>", parameters.join(", "))
        };
        let type_arguments = if names.is_empty() {
            String::new()
        } else {
            format!("<{}>", names.join(", "))
        };
        let body = rils_quote_tokens!(#name { #(#field_defaults),* });
        let quoted = rils_quote! {
            impl #impl_generics Default for #name #type_arguments {
                fn default() -> Self {
                    #body
                }
            }
        };
        Ok(Some(quoted))
    }

    fn collect_required_bounds(
        plan: &DefaultPlan,
        required: &mut std::collections::HashSet<String>,
    ) {
        match plan {
            DefaultPlan::TraitCall(Type::Variable(name)) => {
                required.insert(name.clone());
            }
            DefaultPlan::Tuple(elements) => {
                for element in elements {
                    collect_required_bounds(element, required);
                }
            }
            DefaultPlan::Array { element, .. } => collect_required_bounds(element, required),
            _ => {}
        }
    }
}

#[decl_rils(core::eq)]
pub(crate) mod eq_native {
    /// Values with reflexive equality suitable for hashed collections.
    pub trait Eq: ::core::cmp::Eq {}
}

#[decl_rils(core::hash)]
pub(crate) mod hash_native {
    /// Values that can be used as hash collection keys.
    pub trait Hash: ::core::hash::Hash {}
}

#[decl_rils(core::bit_flags)]
pub(crate) mod bit_flags_native {
    /// Enum values whose discriminants may be combined as a bit set.
    pub trait BitFlags: super::BitFlagsMarker {}
}

pub use bit_flags_native::BitFlags;
pub use default_native::DERIVE;
pub use default_native::Default;
pub use eq_native::Eq;
pub use hash_native::Hash;

#[decl_rils(core::clone)]
pub(crate) mod clone_native {
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

#[decl_rils(core::copy)]
pub(crate) mod copy_native {
    use rils_syntax::{ast::Stmt, parser::ParseError, quote::QuotedStatement, rils_quote};

    /// Values duplicated by ordinary reads.
    pub trait Copy: super::Clone + ::core::marker::Copy {}

    /// Generates the marker implementation; field eligibility is checked by Rils.
    #[rils_derive]
    fn derive_copy(statement: &Stmt) -> Result<Option<QuotedStatement>, ParseError> {
        let (name, generic_parameters, span) = match statement {
            Stmt::Struct {
                name,
                generic_parameters,
                span,
                ..
            }
            | Stmt::Enum {
                name,
                generic_parameters,
                span,
                ..
            } => (name, generic_parameters, span),
            _ => unreachable!("derive attributes occur on types"),
        };
        if !generic_parameters.is_empty() {
            return Err(ParseError {
                message: "deriving Copy for generic types requires conditional trait impls".into(),
                span: *span,
            });
        }
        Ok(Some(rils_quote! { impl Copy for #name {} }))
    }
}

pub use clone_native::Clone;
pub use copy_native::Copy;
