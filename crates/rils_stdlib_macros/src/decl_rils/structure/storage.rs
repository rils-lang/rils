//! Explicit implementation-only storage parameters for exported declarations.
use super::*;
use syn::{
    GenericParam, Generics, Ident, Token,
    parse::{Parse, ParseStream},
};

struct Spec {
    kind: Ident,
    element: Option<Type>,
    receiver: Path,
}
impl Parse for Spec {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let kind = input.parse()?;
        let element = if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let element = content.parse()?;
            if !content.is_empty() {
                return Err(content.error("expected one sequence element type"));
            }
            Some(element)
        } else {
            None
        };
        input.parse::<Token![,]>()?;
        let receiver = input.parse()?;
        Ok(Self {
            kind,
            element,
            receiver,
        })
    }
}

struct StorageParameter {
    backend: Ident,
    receiver: Path,
    element: Type,
}

fn specification(item: &ItemStruct) -> syn::Result<Option<StorageParameter>> {
    let mut result = None;
    for parameter in item.generics.type_params() {
        for attr in &parameter.attrs {
            if !attr.path().is_ident("rils_storage") {
                continue;
            }
            if result.is_some() {
                return Err(Error::new_spanned(
                    attr,
                    "only one storage backend is supported",
                ));
            }
            let spec: Spec = attr.parse_args()?;
            if spec.kind != "sequence" {
                return Err(Error::new_spanned(
                    attr,
                    "expected sequence storage protocol",
                ));
            }
            if parameter.default.is_none() {
                return Err(Error::new_spanned(
                    parameter,
                    "storage backend requires a Rust default",
                ));
            }
            if item.generics.type_params().last().map(|p| &p.ident) != Some(&parameter.ident) {
                return Err(Error::new_spanned(
                    parameter,
                    "storage backend must be the last type parameter",
                ));
            }
            if item.generics.params.len() != item.generics.type_params().count()
                || item.generics.params.len() < 2
            {
                return Err(Error::new_spanned(
                    &item.generics,
                    "sequence storage requires Rils type parameters followed by one backend parameter",
                ));
            }
            let parameters = item
                .generics
                .type_params()
                .filter(|entry| entry.ident != parameter.ident)
                .map(|entry| entry.ident.clone())
                .collect::<Vec<_>>();
            let element = match spec.element {
                Some(element) => element,
                None if parameters.len() == 1 => {
                    let element = &parameters[0];
                    syn::parse_quote!(#element)
                }
                None => {
                    return Err(Error::new_spanned(
                        attr,
                        "multiple Rils parameters require an explicit sequence element type",
                    ));
                }
            };
            validate_element(&element, &parameters)?;
            result = Some(StorageParameter {
                backend: parameter.ident.clone(),
                receiver: spec.receiver,
                element,
            });
        }
    }
    Ok(result)
}

fn validate_element(element: &Type, parameters: &[Ident]) -> syn::Result<()> {
    match element {
        Type::Path(path)
            if path.qself.is_none() && parameters.iter().any(|name| path.path.is_ident(name)) =>
        {
            Ok(())
        }
        Type::Tuple(tuple) if !tuple.elems.is_empty() => {
            for element in &tuple.elems {
                validate_element(element, parameters)?;
            }
            Ok(())
        }
        Type::Paren(ty) => validate_element(&ty.elem, parameters),
        Type::Group(ty) => validate_element(&ty.elem, parameters),
        _ => Err(Error::new_spanned(
            element,
            "sequence element must be a declared Rils type parameter or a tuple of those parameters",
        )),
    }
}

pub(in crate::decl_rils) fn parameter(item: &ItemStruct) -> syn::Result<Option<(Ident, Path)>> {
    Ok(specification(item)?.map(|spec| (spec.backend, spec.receiver)))
}

pub(in crate::decl_rils) fn element_type(item: &ItemStruct) -> syn::Result<Type> {
    specification(item)?
        .map(|spec| spec.element)
        .ok_or_else(|| Error::new_spanned(item, "expected a storage-backed declaration"))
}

pub(in crate::decl_rils) fn language_parameters(item: &ItemStruct) -> syn::Result<Vec<Ident>> {
    Ok(language_generics(item)?
        .type_params()
        .map(|parameter| parameter.ident.clone())
        .collect())
}

pub(in crate::decl_rils) fn language_generics(item: &ItemStruct) -> syn::Result<Generics> {
    let backend = parameter(item)?.map(|(name, _)| name);
    let mut generics = item.generics.clone();
    generics.params = generics
        .params
        .into_iter()
        .filter(|p| !matches!(p, GenericParam::Type(t) if Some(&t.ident) == backend.as_ref()))
        .collect();
    Ok(generics)
}

pub(in crate::decl_rils) fn strip_markers(generics: &mut Generics) {
    for parameter in generics.type_params_mut() {
        parameter
            .attrs
            .retain(|a| !a.path().is_ident("rils_storage"));
    }
}

/// A backend is an implementation detail, never part of a script-facing signature.
pub(super) fn validate_surface(item: &ItemStruct, methods: &[ImplItemFn]) -> syn::Result<()> {
    let Some((backend, _)) = parameter(item)? else {
        return Ok(());
    };
    for field in public_fields(item) {
        if mentions(field.ty.to_token_stream(), &backend) {
            return Err(Error::new_spanned(
                field,
                "storage backend cannot appear in a public Rils field",
            ));
        }
    }
    for method in methods {
        let mut signature = method.sig.clone();
        if let Some(attr) = method
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("rils_return"))
        {
            let ty: Type = attr.parse_args()?;
            signature.output = syn::parse_quote!(-> #ty);
        }
        if mentions(signature.to_token_stream(), &backend) {
            return Err(Error::new_spanned(
                &method.sig,
                "storage backend cannot appear in an exported Rils signature",
            ));
        }
    }
    Ok(())
}

pub(super) fn mentions(tokens: proc_macro2::TokenStream, name: &Ident) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => ident == *name,
        proc_macro2::TokenTree::Group(group) => mentions(group.stream(), name),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_storage_declarations_are_rejected() {
        for item in [
            syn::parse_quote!(
                struct Buffer<T, #[rils_storage(unknown, crate::Buffer)] S = Vec<T>>(S);
            ),
            syn::parse_quote!(
                struct Buffer<T, #[rils_storage(sequence, crate::Buffer)] S>(S);
            ),
            syn::parse_quote!(
                struct Buffer<#[rils_storage(sequence, crate::Buffer)] S = Vec<()>, T = ()>(S, T);
            ),
            syn::parse_quote!(
                struct Buffer<T, U, #[rils_storage(sequence, crate::Buffer)] S = Vec<T>>(S, U);
            ),
            syn::parse_quote!(
                struct Buffer<T, U, #[rils_storage(sequence((T, S)), crate::Buffer)] S = Vec<T>>(
                    S,
                    U,
                );
            ),
            syn::parse_quote!(
                struct Buffer<T, U, #[rils_storage(sequence((T, Unknown)), crate::Buffer)] S = Vec<T>>(
                    S,
                    U,
                );
            ),
            syn::parse_quote!(
                struct Buffer<T, U, #[rils_storage(sequence((&T, U)), crate::Buffer)] S = Vec<T>>(
                    S,
                    U,
                );
            ),
            syn::parse_quote!(
                struct Buffer<
                    T,
                    #[rils_storage(sequence, crate::Buffer)]
                    #[rils_storage(sequence, crate::Buffer)] S = Vec<T>,
                >(S);
            ),
        ] {
            assert!(parameter(&item).is_err());
        }
    }

    #[test]
    fn backend_is_omitted_from_language_generics_and_rejected_in_public_types() {
        let item = syn::parse_quote!(
            struct Buffer<T, #[rils_storage(sequence, crate::Buffer)] S = Vec<T>>(S);
        );
        assert_eq!(
            language_generics(&item)
                .unwrap()
                .to_token_stream()
                .to_string(),
            "< T >"
        );
        let method = syn::parse_quote!(
            pub fn leak(&self) -> S {
                unreachable!()
            }
        );
        assert!(validate_surface(&item, &[method]).is_err());
        let item = syn::parse_quote!(
            struct Buffer<T, #[rils_storage(sequence, crate::Buffer)] S = Vec<T>> {
                pub storage: S,
            }
        );
        assert!(validate_surface(&item, &[]).is_err());
    }

    #[test]
    fn declared_element_types_accept_reordering_nesting_and_single_parameter_projection() {
        for (element, expected) in [(quote!((C, (A, B))), "(C , (A , B))"), (quote!(B), "B")] {
            let item = syn::parse_quote!(
                struct Buffer<A, B, C, #[rils_storage(sequence(#element), crate::Buffer)] S = Vec<C>>(
                    S,
                );
            );
            assert_eq!(
                element_type(&item).unwrap().to_token_stream().to_string(),
                expected
            );
            assert_eq!(
                language_parameters(&item)
                    .unwrap()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                ["A", "B", "C"]
            );
            assert_eq!(parameter(&item).unwrap().unwrap().0, "S");
        }
    }
}

mod trait_impl;
pub(super) use trait_impl::language_impl;

mod parameters;
pub(in crate::decl_rils) use parameters::{
    language_method, strip_parameter_markers, validate_impl_parameters, validate_module_parameters,
};
