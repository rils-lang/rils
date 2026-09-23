//! Trait markers attached to native standard-library type definitions.

use std::collections::BTreeSet;

use quote::{ToTokens, quote};
use syn::{Attribute, Error, GenericParam, ItemEnum, ItemImpl, Path, Token, Type, TypeParamBound};

pub(super) struct ConditionalImpl {
    pub trait_name: String,
    pub requirements: Vec<(String, String)>,
}

pub(super) fn parse_impl(
    item: &ItemImpl,
    target: &ItemEnum,
) -> syn::Result<Option<ConditionalImpl>> {
    let markers = item
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("rils_impl"))
        .collect::<Vec<_>>();
    let Some(marker) = markers.first() else {
        return Ok(None);
    };
    if markers.len() != 1 || !matches!(marker.meta, syn::Meta::Path(_)) {
        return Err(Error::new_spanned(
            marker,
            "use #[rils_impl] on a trait impl",
        ));
    }
    let Some((_, trait_path, _)) = &item.trait_ else {
        return Err(Error::new_spanned(
            item,
            "#[rils_impl] requires a trait impl",
        ));
    };
    let trait_name = trait_path.to_token_stream().to_string().replace(' ', "");
    if !supported_trait(&trait_name) {
        return Err(Error::new_spanned(
            trait_path,
            "unsupported native trait binding",
        ));
    }
    let Type::Path(ty) = item.self_ty.as_ref() else {
        return Err(Error::new_spanned(
            &item.self_ty,
            "expected declared enum target",
        ));
    };
    if ty
        .path
        .segments
        .last()
        .is_none_or(|segment| segment.ident != target.ident)
    {
        return Err(Error::new_spanned(
            &item.self_ty,
            "trait impl must target the declared enum",
        ));
    }
    let expected = target
        .generics
        .type_params()
        .map(|param| param.ident.to_string())
        .collect::<Vec<_>>();
    let actual = item
        .generics
        .type_params()
        .map(|param| param.ident.to_string())
        .collect::<Vec<_>>();
    let Some(segment) = ty.path.segments.last() else {
        unreachable!()
    };
    let arguments = match &segment.arguments {
        syn::PathArguments::AngleBracketed(arguments) => arguments
            .args
            .iter()
            .map(|argument| match argument {
                syn::GenericArgument::Type(Type::Path(path)) => {
                    path.path.get_ident().map(ToString::to_string)
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default(),
        syn::PathArguments::None => Vec::new(),
        _ => Vec::new(),
    };
    if expected.len() != actual.len() || arguments != actual {
        return Err(Error::new_spanned(
            &item.self_ty,
            "trait impl must cover every generic parameter of the declared enum",
        ));
    }
    let mut requirements = Vec::new();
    for parameter in &item.generics.params {
        let GenericParam::Type(parameter) = parameter else {
            return Err(Error::new_spanned(
                parameter,
                "only type parameters are supported",
            ));
        };
        for bound in &parameter.bounds {
            add_bound(&mut requirements, &parameter.ident.to_string(), bound)?;
        }
    }
    if let Some(where_clause) = &item.generics.where_clause {
        for predicate in &where_clause.predicates {
            let syn::WherePredicate::Type(predicate) = predicate else {
                return Err(Error::new_spanned(
                    predicate,
                    "only type parameter bounds are supported",
                ));
            };
            let Type::Path(path) = &predicate.bounded_ty else {
                return Err(Error::new_spanned(
                    &predicate.bounded_ty,
                    "expected type parameter",
                ));
            };
            let Some(parameter) = path.path.get_ident() else {
                return Err(Error::new_spanned(path, "expected type parameter"));
            };
            for bound in &predicate.bounds {
                add_bound(&mut requirements, &parameter.to_string(), bound)?;
            }
        }
    }
    if requirements
        .iter()
        .any(|(parameter, _)| !actual.contains(parameter))
    {
        return Err(Error::new_spanned(
            item,
            "trait bound must refer to a target type parameter",
        ));
    }
    Ok(Some(ConditionalImpl {
        trait_name,
        requirements,
    }))
}

fn add_bound(
    requirements: &mut Vec<(String, String)>,
    parameter: &str,
    bound: &TypeParamBound,
) -> syn::Result<()> {
    let TypeParamBound::Trait(bound) = bound else {
        return Err(Error::new_spanned(bound, "expected trait bound"));
    };
    let name = bound.path.to_token_stream().to_string().replace(' ', "");
    if !supported_trait(&name) {
        return Err(Error::new_spanned(bound, "unsupported native trait bound"));
    }
    requirements.push((parameter.to_owned(), name));
    Ok(())
}

pub(super) fn parse(attributes: &[Attribute]) -> syn::Result<Vec<Path>> {
    let mut names = BTreeSet::new();
    let mut traits = Vec::new();
    for attribute in attributes
        .iter()
        .filter(|attr| attr.path().is_ident("rils_impl"))
    {
        let paths = attribute
            .parse_args_with(syn::punctuated::Punctuated::<Path, Token![,]>::parse_terminated)?;
        if paths.is_empty() {
            return Err(Error::new_spanned(attribute, "expected a native trait"));
        }
        for path in paths {
            if !path
                .get_ident()
                .is_some_and(|name| supported_trait(&name.to_string()))
            {
                return Err(Error::new_spanned(
                    &path,
                    "unsupported native trait binding",
                ));
            }
            if !names.insert(path.segments[0].ident.to_string()) {
                return Err(Error::new_spanned(&path, "duplicate Rils trait"));
            }
            traits.push(path);
        }
    }
    if names.contains("Copy") && !names.contains("Clone") {
        return Err(Error::new_spanned(
            &traits[0],
            "Copy requires Clone in Rils; declare both traits",
        ));
    }
    Ok(traits)
}

pub(super) fn checks(ty: &syn::Type, traits: &[Path]) -> proc_macro2::TokenStream {
    let checks = traits.iter().map(|path| {
        let bound = match path.segments[0].ident.to_string().as_str() {
            "Clone" => quote!(::core::clone::Clone),
            "Copy" => quote!(::core::marker::Copy),
            "Default" => quote!(::core::default::Default),
            "Eq" => quote!(::core::cmp::Eq),
            "Hash" => quote!(::core::hash::Hash),
            _ => unreachable!("validated native trait"),
        };
        quote! { fn assert_trait<T: #bound>() {} let _ = assert_trait::<#ty>; }
    });
    quote! { #(const _: () = { #checks };)* }
}

fn supported_trait(name: &str) -> bool {
    matches!(name, "Clone" | "Copy" | "Default" | "Eq" | "Hash")
}

#[cfg(test)]
mod tests {
    use super::{parse, parse_impl};

    #[test]
    fn validates_supported_trait_markers() {
        let attrs = vec![syn::parse_quote!(#[rils_impl(Clone, Copy, Default, Eq, Hash)])];
        assert_eq!(parse(&attrs).unwrap().len(), 5);
        for attr in [
            syn::parse_quote!(#[rils_impl(Copy)]),
            syn::parse_quote!(#[rils_impl(Debug)]),
            syn::parse_quote!(#[rils_impl(Clone, Clone)]),
        ] {
            assert!(parse(&[attr]).is_err());
        }
    }

    #[test]
    fn marked_impl_captures_generic_bounds() {
        let target = syn::parse_quote!(
            pub enum Result<T, E> {
                Ok(T),
                Err(E),
            }
        );
        let item = syn::parse_quote!(
            #[rils_impl]
            impl<T: Clone, E> Clone for Result<T, E>
            where
                E: Clone,
            {
                fn clone(&self) -> Self {
                    unreachable!()
                }
            }
        );
        let parsed = parse_impl(&item, &target).unwrap().unwrap();
        assert_eq!(parsed.trait_name, "Clone");
        assert_eq!(
            parsed.requirements,
            [("T".into(), "Clone".into()), ("E".into(), "Clone".into())]
        );

        let wrong_target = syn::parse_quote!(
            #[rils_impl]
            impl<T: Clone, E: Clone> Clone for Result<T, i32> {
                fn clone(&self) -> Self {
                    unreachable!()
                }
            }
        );
        assert!(parse_impl(&wrong_target, &target).is_err());
    }
}
