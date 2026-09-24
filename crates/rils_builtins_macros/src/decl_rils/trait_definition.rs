//! Rust-backed declarations for Rils built-in traits.

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Error, FnArg, ItemTrait, Path, ReturnType, Token, TraitItem, TraitItemFn, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

use crate::type_patterns;

struct Header {
    module: Path,
}

impl Parse for Header {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let module = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("unexpected trait binding arguments"));
        }
        Ok(Self { module })
    }
}

struct Input {
    header: Header,
    item: ItemTrait,
}

impl Parse for Input {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let module = input.parse()?;
        input.parse::<Token![;]>()?;
        let item = input.parse()?;
        Ok(Self {
            header: Header { module },
            item,
        })
    }
}

impl Input {
    fn validate(&self) -> syn::Result<()> {
        let name = self.item.ident.to_string();
        let module = self
            .header
            .module
            .to_token_stream()
            .to_string()
            .replace(' ', "");
        let rust = self.rust_binding()?;
        let rust = rust.to_token_stream().to_string().replace(' ', "");
        let valid = match name.as_str() {
            "Clone" => module == "core::clone::clone" && rust == "::core::clone::Clone",
            "Copy" => module == "core::clone::copy" && rust == "::core::marker::Copy",
            "Default" => module == "core::default::default" && rust == "::core::default::Default",
            "Eq" => module == "core::cmp::eq" && rust == "::core::cmp::Eq",
            "Hash" => module == "core::hash::hash" && rust == "::core::hash::Hash",
            "BitFlags" => module == "core::bit_flags::bit_flags" && rust == "super::BitFlagsMarker",
            _ => self
                .item
                .attrs
                .iter()
                .any(|attribute| attribute.path().is_ident("rils_trait")),
        };
        if !valid {
            return Err(Error::new_spanned(
                &self.item,
                "unsupported built-in trait binding",
            ));
        }
        if !matches!(self.item.vis, syn::Visibility::Public(_)) {
            return Err(Error::new_spanned(
                &self.item,
                "built-in trait must be public",
            ));
        }
        if !self.item.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &self.item,
                "built-in trait must not declare generics",
            ));
        }
        let methods = self.methods()?;
        if (matches!(name.as_str(), "Clone" | "Default") && methods.len() != 1)
            || (matches!(name.as_str(), "Copy" | "Eq" | "Hash" | "BitFlags") && !methods.is_empty())
        {
            return Err(Error::new_spanned(
                &self.item,
                "unexpected built-in trait methods",
            ));
        }
        if let Some(method) = methods.first() {
            let valid_signature = match name.as_str() {
                "Clone" => {
                    method.sig.ident == "clone"
                        && method.sig.inputs.len() == 1
                        && matches!(method.sig.inputs.first(), Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() && receiver.mutability.is_none())
                }
                "Default" => method.sig.ident == "default" && method.sig.inputs.is_empty(),
                _ => true,
            };
            if !valid_signature
                || !method.sig.generics.params.is_empty()
                || method.sig.generics.where_clause.is_some()
                || (matches!(name.as_str(), "Clone" | "Default")
                    && !matches!(&method.sig.output, ReturnType::Type(_, ty) if matches!(ty.as_ref(), Type::Path(path) if path.path.is_ident("Self"))))
            {
                return Err(Error::new_spanned(
                    method,
                    "expected the bound Rust trait's required method signature",
                ));
            }
        }
        Ok(())
    }

    fn rust_binding(&self) -> syn::Result<&Path> {
        let mut matching = self.item.supertraits.iter().filter_map(|bound| {
            let syn::TypeParamBound::Trait(bound) = bound else {
                return None;
            };
            let path = &bound.path;
            (path.segments.len() > 1
                && path.segments.last().is_some_and(|part| {
                    part.ident == self.item.ident
                        || (self.item.ident == "BitFlags" && part.ident == "BitFlagsMarker")
                }))
            .then_some(path)
        });
        let binding = matching.next().ok_or_else(|| {
            Error::new_spanned(
                &self.item.supertraits,
                "expected a qualified Rust trait path in the supertraits",
            )
        })?;
        if matching.next().is_some() {
            return Err(Error::new_spanned(
                &self.item.supertraits,
                "ambiguous Rust trait binding",
            ));
        }
        Ok(binding)
    }

    fn methods(&self) -> syn::Result<Vec<&TraitItemFn>> {
        self.item
            .items
            .iter()
            .map(|item| match item {
                TraitItem::Fn(method) if method.default.is_none() => Ok(method),
                _ => Err(Error::new_spanned(
                    item,
                    "only required trait methods are supported until native defaults are registered",
                )),
            })
            .collect()
    }

    fn source(&self) -> String {
        let mut source = String::new();
        for line in super::documentation(&self.item.attrs).lines() {
            source.push_str(&format!("/// {line}\n"));
        }
        let methods = self.methods().expect("validated trait");
        let rust = self.rust_binding().expect("validated binding");
        let bounds = self
            .item
            .supertraits
            .iter()
            .filter_map(|bound| {
                let syn::TypeParamBound::Trait(bound) = bound else {
                    return None;
                };
                (bound.path.to_token_stream().to_string() != rust.to_token_stream().to_string())
                    .then(|| bound.path.segments.last().unwrap().ident.to_string())
            })
            .collect::<Vec<_>>();
        if methods.is_empty() {
            if bounds.is_empty() {
                source.push_str(&format!("pub trait {} {{}}\n", self.item.ident));
            } else {
                source.push_str(&format!(
                    "pub trait {}: {} {{}}\n",
                    self.item.ident,
                    bounds.join(" + ")
                ));
            }
            return source;
        }
        if bounds.is_empty() {
            source.push_str(&format!("pub trait {} {{\n", self.item.ident));
        } else {
            source.push_str(&format!(
                "pub trait {}: {} {{\n",
                self.item.ident,
                bounds.join(" + ")
            ));
        }
        for method in methods {
            for line in super::documentation(&method.attrs).lines() {
                source.push_str(&format!("    /// {line}\n"));
            }
            source.push_str(&format!(
                "    {};\n",
                method
                    .sig
                    .to_token_stream()
                    .to_string()
                    .replace(" (", "(")
                    .replace("& self", "&self")
            ));
        }
        source.push_str("}\n");
        source
    }

    fn metadata(&self) -> syn::Result<proc_macro2::TokenStream> {
        let name = self.item.ident.to_string();
        let docs = super::documentation(&self.item.attrs);
        let rust = self.rust_binding()?;
        let supertraits = self
            .item
            .supertraits
            .iter()
            .filter_map(|bound| {
                let syn::TypeParamBound::Trait(bound) = bound else {
                    return None;
                };
                (bound.path.to_token_stream().to_string() != rust.to_token_stream().to_string())
                    .then(|| bound.path.segments.last().unwrap().ident.to_string())
            })
            .collect::<Vec<_>>();
        let methods = self.methods()?.into_iter().map(|method| {
            let name = method.sig.ident.to_string();
            let docs = super::documentation(&method.attrs);
            let has_receiver = matches!(method.sig.inputs.first(), Some(FnArg::Receiver(_)));
            let parameters = method.sig.inputs.iter().skip(usize::from(has_receiver)).map(|argument| {
                let FnArg::Typed(argument) = argument else {
                    return Err(Error::new_spanned(argument, "unexpected receiver"));
                };
                type_patterns::tokens(&argument.ty)
            }).collect::<syn::Result<Vec<_>>>()?;
            let result = match &method.sig.output {
                ReturnType::Default => quote!(crate::TypePattern::Unit),
                ReturnType::Type(_, ty) => type_patterns::tokens(ty)?,
            };
            let kind = if has_receiver { quote!(crate::BuiltinMemberKind::Method) } else { quote!(crate::BuiltinMemberKind::AssociatedFunction) };
            let receiver = if has_receiver { quote!(Some(crate::ReceiverMode::Shared)) } else { quote!(None) };
            let builtin_id = if name == "clone" { quote!(Some(builtin_id!("core::clone"))) } else { quote!(None) };
            Ok(quote! {
                crate::BuiltinMember {
                    name: #name,
                    kind: #kind,
                    signature: Some(crate::BuiltinSignature { parameters: &[#(#parameters),*], result: #result, variadic: false }),
                    value_type: None,
                    receiver: #receiver,
                    builtin_id: #builtin_id,
                    runtime_import: None,
                    required: true,
                    type_parameters: &[],
                    documentation: #docs,
                }
            })
        }).collect::<syn::Result<Vec<_>>>()?;
        let backend = if name == "Clone" {
            quote!(crate::BuiltinBackend::Runtime)
        } else {
            quote!(crate::BuiltinBackend::Metadata)
        };
        let pattern_import = if methods.is_empty() {
            quote!()
        } else {
            quote!(
                use crate::TypePattern;
            )
        };
        Ok(quote! {
            #pattern_import
            pub const DECLARATION: crate::BuiltinDeclaration = crate::BuiltinDeclaration {
                path: #name,
                kind: crate::BuiltinKind::Trait,
                supertraits: &[#(#supertraits),*],
                type_parameters: &[],
                members: &[#(#methods),*],
                signature: None,
                backend: #backend,
                documentation: #docs,
            };
        })
    }
}

pub(super) fn mixed_trait_binding(module: Path, item: ItemTrait) -> syn::Result<Path> {
    let input = Input {
        header: Header { module },
        item,
    };
    input.validate()?;
    Ok(input.rust_binding()?.clone())
}

pub(crate) fn expand_source(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    match input.validate() {
        Ok(()) => {
            let source = input.source();
            quote!(#source).into()
        }
        Err(error) => error.into_compile_error().into(),
    }
}

pub(crate) fn expand_metadata(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    match input.validate().and_then(|()| input.metadata()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_binding_generates_the_existing_rils_contract() {
        let input = Input {
            header: syn::parse_quote!(core::clone::clone),
            item: syn::parse_quote! {
                /// Explicit owned duplication.
                pub trait Clone: ::core::clone::Clone {
                    /// Explicitly duplicates an owned value.
                    fn clone(&self) -> Self;
                }
            },
        };
        input.validate().unwrap();
        assert_eq!(
            input.source(),
            "/// Explicit owned duplication.\npub trait Clone {\n    /// Explicitly duplicates an owned value.\n    fn clone(&self) -> Self;\n}\n"
        );
        assert!(
            input
                .metadata()
                .unwrap()
                .to_string()
                .contains("BuiltinKind :: Trait")
        );
    }

    #[test]
    fn rejects_a_trait_binding_with_a_different_rust_contract() {
        let input = Input {
            header: syn::parse_quote!(core::clone::copy),
            item: syn::parse_quote!(
                pub trait Copy: ::core::clone::Clone {}
            ),
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn default_binding_generates_associated_constructor() {
        let input = Input {
            header: syn::parse_quote!(core::default::default),
            item: syn::parse_quote! {
                pub trait Default: ::core::default::Default {
                    fn default() -> Self;
                }
            },
        };
        input.validate().unwrap();
        assert!(input.source().contains("fn default() -> Self;"));
        let metadata = input.metadata().unwrap().to_string();
        assert!(metadata.contains("BuiltinMemberKind :: AssociatedFunction"));
        assert!(metadata.contains("BuiltinBackend :: Metadata"));
    }

    #[test]
    fn marker_traits_keep_rils_supertraits() {
        for (module, item, expected) in [
            (
                syn::parse_quote!(core::clone::copy),
                syn::parse_quote!(
                    pub trait Copy: Clone + ::core::marker::Copy {}
                ),
                "pub trait Copy: Clone {}",
            ),
            (
                syn::parse_quote!(core::cmp::eq),
                syn::parse_quote!(
                    pub trait Eq: ::core::cmp::Eq {}
                ),
                "pub trait Eq {}",
            ),
            (
                syn::parse_quote!(core::hash::hash),
                syn::parse_quote!(
                    pub trait Hash: ::core::hash::Hash {}
                ),
                "pub trait Hash {}",
            ),
            (
                syn::parse_quote!(core::bit_flags::bit_flags),
                syn::parse_quote!(
                    pub trait BitFlags: super::BitFlagsMarker {}
                ),
                "pub trait BitFlags {}",
            ),
        ] {
            let input = Input {
                header: Header { module },
                item,
            };
            input.validate().unwrap();
            assert_eq!(input.source(), format!("{expected}\n"));
        }
    }
}
