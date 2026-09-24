//! Explicitly exported declarations in a Rust module.

use std::collections::BTreeSet;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Error, ImplItem, Item, ItemImpl, ItemMod, Path, Type};

use super::{trait_definition, trait_impls};

const MARKERS: [&str; 4] = ["rils_struct", "rils_enum", "rils_trait", "rils_fn"];

pub(super) fn has_export_markers(module: &ItemMod) -> bool {
    module.content.as_ref().is_some_and(|(_, items)| {
        items.iter().any(|item| {
            item_attrs(item)
                .is_some_and(|attrs| MARKERS.iter().any(|marker| has_attr(attrs, marker)))
        })
    })
}

fn item_attrs(item: &Item) -> Option<&[Attribute]> {
    match item {
        Item::Struct(item) => Some(&item.attrs),
        Item::Enum(item) => Some(&item.attrs),
        Item::Trait(item) => Some(&item.attrs),
        Item::Impl(item) => Some(&item.attrs),
        Item::Fn(item) => Some(&item.attrs),
        _ => None,
    }
}

fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

fn snake_case(name: &str) -> String {
    let characters = name.chars().collect::<Vec<_>>();
    let mut result = String::new();
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_uppercase()
            && index > 0
            && (characters[index - 1].is_lowercase()
                || characters[index - 1].is_ascii_digit()
                || characters
                    .get(index + 1)
                    .is_some_and(|next| next.is_lowercase()))
        {
            result.push('_');
        }
        result.extend(character.to_lowercase());
    }
    if let Some(rest) = result.strip_prefix("b_tree_") {
        format!("btree_{rest}")
    } else {
        result
    }
}

fn target_name(item: &ItemImpl) -> Option<&syn::Ident> {
    let Type::Path(target) = item.self_ty.as_ref() else {
        return None;
    };
    if target.qself.is_some() || target.path.segments.len() != 1 {
        return None;
    }
    target.path.segments.last().map(|segment| &segment.ident)
}

fn marker_path(
    attrs: &[Attribute],
    marker: &str,
    module: &Path,
    name: &syn::Ident,
) -> syn::Result<Path> {
    let matching = attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident(marker))
        .collect::<Vec<_>>();
    let attribute = matching.first().expect("export marker is present");
    if matching.len() != 1 {
        return Err(Error::new_spanned(
            attribute,
            "duplicate Rils export marker",
        ));
    }
    if !matches!(attribute.meta, syn::Meta::Path(_)) {
        return Err(Error::new_spanned(
            attribute,
            "Rils export marker does not accept arguments",
        ));
    }
    let segment_name = snake_case(&name.to_string());
    let segment = syn::parse_str::<syn::Ident>(&segment_name)
        .unwrap_or_else(|_| format_ident!("r#{segment_name}"));
    Ok(syn::parse_quote!(#module::#segment))
}

pub(super) fn expand_definition(path: Path, module: ItemMod) -> TokenStream {
    match expand(path, module) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand(path: Path, module: ItemMod) -> syn::Result<proc_macro2::TokenStream> {
    let Some((_, items)) = &module.content else {
        return Err(Error::new_spanned(
            &module,
            "Rils declaration module must be inline",
        ));
    };
    let mut callbacks = Vec::new();
    let mut exported = BTreeSet::new();
    let mut trait_aliases = Vec::new();
    let mut derived = BTreeSet::new();
    let mut derive_constants = Vec::new();
    let mut trait_checks = Vec::new();

    for item in items {
        let Some(attrs) = item_attrs(item) else {
            continue;
        };
        let markers = MARKERS
            .iter()
            .filter(|marker| has_attr(attrs, marker))
            .collect::<Vec<_>>();
        if markers.len() > 1 {
            return Err(Error::new_spanned(
                item,
                "one Rils export marker is allowed per item",
            ));
        }
        let Some(marker) = markers.first() else {
            continue;
        };
        let marker = **marker;
        let (name, is_trait, is_function) = match (marker, item) {
            ("rils_struct", Item::Struct(value)) => (&value.ident, false, false),
            ("rils_enum", Item::Enum(value)) => (&value.ident, false, false),
            ("rils_trait", Item::Trait(value)) => (&value.ident, true, false),
            ("rils_fn", Item::Fn(value)) => (&value.sig.ident, false, true),
            _ => {
                return Err(Error::new_spanned(
                    item,
                    "Rils export marker does not match item kind",
                ));
            }
        };
        let public = match item {
            Item::Struct(value) => matches!(value.vis, syn::Visibility::Public(_)),
            Item::Enum(value) => matches!(value.vis, syn::Visibility::Public(_)),
            Item::Trait(value) => matches!(value.vis, syn::Visibility::Public(_)),
            Item::Fn(value) => matches!(value.vis, syn::Visibility::Public(_)),
            _ => unreachable!(),
        };
        if !public {
            return Err(Error::new_spanned(
                item,
                "exported Rils declarations must be public",
            ));
        }
        if !exported.insert(name.to_string()) {
            return Err(Error::new_spanned(item, "duplicate exported Rils name"));
        }
        let id_path = marker_path(attrs, marker, &path, name)?;
        let callback = if is_function {
            let parts = id_path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>();
            format_ident!("{}_definition", parts.join("_"))
        } else {
            format_ident!("{}_definition", name.to_string().to_lowercase())
        };
        if is_trait {
            let Item::Trait(trait_item) = item else {
                unreachable!()
            };
            let rust = trait_definition::mixed_trait_binding(id_path.clone(), trait_item.clone())?;
            trait_aliases.push((name.clone(), rust));
            callbacks.push(quote! {
                #[macro_export]
                macro_rules! #callback {
                    ($emit:ident) => { $emit! { #id_path; #trait_item } };
                }
            });
        } else if is_function {
            let Item::Fn(function) = item else {
                unreachable!()
            };
            if function.block.stmts.is_empty() {
                return Err(Error::new_spanned(
                    function,
                    "exported Rils function requires a Rust body",
                ));
            }
            callbacks.push(quote! {
                #[macro_export]
                macro_rules! #callback {
                    ($emit:ident) => { $emit! { #id_path; #function } };
                }
            });
        } else {
            let traits = trait_impls::parse(attrs)?;
            if !traits.is_empty() {
                let module_name = &module.ident;
                let item_type: Type = syn::parse_quote!(#module_name::#name);
                trait_checks.push(trait_impls::checks(&item_type, &traits));
            }
            let selected = items
                .iter()
                .filter(|candidate| match candidate {
                    Item::Impl(implementation) => target_name(implementation) == Some(name),
                    _ => std::ptr::eq(*candidate, item),
                })
                .cloned()
                .collect::<Vec<_>>();
            let mut filtered = module.clone();
            filtered.content.as_mut().expect("inline module").1 = selected;
            callbacks.push(quote! {
                #[macro_export]
                macro_rules! #callback {
                    ($emit:ident) => { $emit! { #id_path; #filtered } };
                }
            });
        }
    }
    if exported.is_empty() {
        return Err(Error::new_spanned(
            &module,
            "expected a marked Rils declaration",
        ));
    }
    for item in items {
        if let Item::Struct(value) = item {
            if has_attr(&value.attrs, "rils_impl") && !has_attr(&value.attrs, "rils_struct") {
                return Err(Error::new_spanned(
                    value,
                    "#[rils_impl] type must be marked #[rils_struct]",
                ));
            }
        }
        if let Item::Enum(value) = item {
            if has_attr(&value.attrs, "rils_impl") && !has_attr(&value.attrs, "rils_enum") {
                return Err(Error::new_spanned(
                    value,
                    "#[rils_impl] type must be marked #[rils_enum]",
                ));
            }
        }
        let Item::Impl(implementation) = item else {
            continue;
        };
        let Some(target) = target_name(implementation) else {
            continue;
        };
        if has_attr(&implementation.attrs, "rils_impl") {
            if !exported.contains(&target.to_string())
                || trait_aliases.iter().any(|(name, _)| name == target)
            {
                return Err(Error::new_spanned(
                    implementation,
                    "#[rils_impl] target must be an exported type",
                ));
            }
            let trait_path = &implementation
                .trait_
                .as_ref()
                .ok_or_else(|| {
                    Error::new_spanned(implementation, "#[rils_impl] requires a trait impl")
                })?
                .1;
            if trait_path.get_ident().is_none() {
                return Err(Error::new_spanned(
                    trait_path,
                    "expected a simple Rils trait name",
                ));
            }
        }
        if implementation.trait_.is_some()
            && implementation.items.iter().any(|member| {
                matches!(member, ImplItem::Fn(method) if has_attr(&method.attrs, "export_rils"))
            })
        {
            return Err(Error::new_spanned(
                implementation,
                "#[export_rils] requires an inherent implementation",
            ));
        }
        if implementation.trait_.is_none()
            && !exported.contains(&target.to_string())
            && implementation.items.iter().any(|member| matches!(member, ImplItem::Fn(method) if has_attr(&method.attrs, "export_rils")))
        {
            return Err(Error::new_spanned(implementation, "#[export_rils] target must be exported"));
        }
    }

    let mut emitted = module.clone();
    let (_, emitted_items) = emitted.content.as_mut().expect("inline module");
    for item in emitted_items.iter_mut() {
        match item {
            Item::Struct(value) => value.attrs.retain(|attr| {
                !attr.path().is_ident("rils_struct") && !attr.path().is_ident("rils_impl")
            }),
            Item::Enum(value) => value.attrs.retain(|attr| {
                !attr.path().is_ident("rils_enum") && !attr.path().is_ident("rils_impl")
            }),
            Item::Trait(value) if has_attr(&value.attrs, "rils_trait") => {
                let (name, rust) = trait_aliases
                    .iter()
                    .find(|(name, _)| name == &value.ident)
                    .expect("validated trait");
                *item = syn::parse_quote!(pub use #rust as #name;);
            }
            Item::Impl(value) => {
                value
                    .attrs
                    .retain(|attr| !attr.path().is_ident("rils_impl"));
                for member in &mut value.items {
                    if let ImplItem::Fn(method) = member {
                        method.attrs.retain(|attr| {
                            !attr.path().is_ident("export_rils")
                                && !attr.path().is_ident("rils_import")
                                && !attr.path().is_ident("rils_legacy_id")
                                && !attr.path().is_ident("rils_any")
                                && !attr.path().is_ident("rils_ref_any")
                                && !attr.path().is_ident("rils_return")
                        });
                    }
                }
            }
            Item::Fn(function) if has_attr(&function.attrs, "rils_derive") => {
                let markers = function
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("rils_derive"))
                    .collect::<Vec<_>>();
                let attr = markers[0];
                if markers.len() != 1 {
                    return Err(Error::new_spanned(
                        attr,
                        "one #[rils_derive] marker is allowed per function",
                    ));
                }
                let trait_name: syn::Ident = attr.parse_args()?;
                if !exported.contains(&trait_name.to_string())
                    || !trait_aliases.iter().any(|(name, _)| name == &trait_name)
                {
                    return Err(Error::new_spanned(
                        attr,
                        "derive target must be an exported trait",
                    ));
                }
                if !derived.insert(trait_name.to_string()) {
                    return Err(Error::new_spanned(attr, "duplicate derive for trait"));
                }
                let const_name = format_ident!("DERIVE_{}", trait_name.to_string().to_uppercase());
                let function_name = &function.sig.ident;
                let trait_string = trait_name.to_string();
                derive_constants.push(quote! {
                    pub const #const_name: rils_syntax::derive::NativeDeriveDefinition =
                        rils_syntax::derive::NativeDeriveDefinition {
                            name: #trait_string,
                            expand: #function_name,
                        };
                });
                function
                    .attrs
                    .retain(|attr| !attr.path().is_ident("rils_derive"));
            }
            Item::Fn(function) => {
                function.attrs.retain(|attr| {
                    !attr.path().is_ident("rils_fn")
                        && !attr.path().is_ident("rils_any")
                        && !attr.path().is_ident("rils_variadic")
                });
            }
            _ => {}
        }
    }
    for derived in derive_constants {
        emitted_items.push(syn::parse2(derived)?);
    }
    Ok(quote! { #emitted #(#trait_checks)* #(#callbacks)* })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_path_uses_snake_case_type_name() {
        assert_eq!(snake_case("BinaryHeap"), "binary_heap");
        assert_eq!(snake_case("VecDeque"), "vec_deque");
        assert_eq!(snake_case("I32"), "i32");
        assert_eq!(snake_case("BTreeMap"), "btree_map");
    }

    #[test]
    fn marked_trait_impl_cannot_target_a_rust_helper() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                #[rils_struct]
                pub struct Exported;
                pub struct Helper;
                #[rils_impl]
                impl Clone for Helper {
                    fn clone(&self) -> Self { Self }
                }
            }
        };
        let error = expand(syn::parse_quote!(core::fixture), module).unwrap_err();
        assert!(error.to_string().contains("exported type"));
    }

    #[test]
    fn mixed_trait_derive_requires_an_explicit_target() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                #[rils_trait]
                pub trait Marker: super::Marker {}

                #[rils_derive]
                fn derive_marker() {}
            }
        };
        assert!(expand(syn::parse_quote!(core::fixture), module).is_err());
    }

    #[test]
    fn single_declarations_require_export_markers_too() {
        for module in [
            syn::parse_quote! {
                mod native { pub struct Buffer; }
            },
            syn::parse_quote! {
                mod native { pub enum State { Ready } }
            },
            syn::parse_quote! {
                mod native { pub trait Marker: super::Marker {} }
            },
        ] {
            let error = expand(syn::parse_quote!(core::fixture), module).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("expected a marked Rils declaration")
            );
        }
    }

    #[test]
    fn derive_target_must_be_a_marked_trait_even_in_a_single_declaration_module() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                #[rils_trait]
                pub trait Clone: ::core::clone::Clone {
                    fn clone(&self) -> Self;
                }

                #[rils_derive(Copy)]
                fn derive_copy() {}
            }
        };
        let error = expand(syn::parse_quote!(core::clone), module).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("derive target must be an exported trait")
        );
    }

    #[test]
    fn helper_type_cannot_claim_a_rils_trait_implementation() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                #[rils_struct]
                pub struct Exported;

                #[rils_impl(Clone)]
                pub struct Helper;
            }
        };
        let error = expand(syn::parse_quote!(core::fixture), module).unwrap_err();
        assert!(error.to_string().contains("must be marked #[rils_struct]"));
    }

    #[test]
    fn trait_impl_methods_cannot_be_exported_as_inherent_methods() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                #[rils_struct]
                pub struct Exported;

                impl Clone for Exported {
                    #[export_rils]
                    fn clone(&self) -> Self { Self }
                }
            }
        };
        let error = expand(syn::parse_quote!(core::fixture), module).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires an inherent implementation")
        );
    }
}
