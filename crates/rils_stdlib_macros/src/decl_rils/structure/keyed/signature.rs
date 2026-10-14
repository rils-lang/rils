//! Reject declaration changes which the native storage protocol cannot yet encode.

use super::super::*;

pub(super) fn validate(method: &ImplItemFn, map: bool) -> syn::Result<()> {
    let name = method.sig.ident.to_string();
    let parameters = method
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(input) => Some(input.ty.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let (count, mutable, owned) = match name.as_str() {
        "new" => (0, false, false),
        "into_iter" => (0, false, true),
        "insert" => (if map { 2 } else { 1 }, true, false),
        "push" | "push_front" | "push_back" => (1, true, false),
        "pop" | "pop_front" | "pop_back" => (0, true, false),
        "clear" => (0, true, false),
        "remove" => (1, true, false),
        "contains"
        | "contains_key"
        | "get_cloned"
        | "is_subset"
        | "is_superset"
        | "is_disjoint"
        | "union"
        | "intersection"
        | "difference"
        | "symmetric_difference" => (1, false, false),
        "peek_cloned" | "front_cloned" | "back_cloned" | "len" | "is_empty" | "iter"
        | "first_cloned" | "last_cloned" | "first_key_cloned" | "last_key_cloned"
        | "keys_cloned" | "values_cloned" => (0, false, false),
        _ => {
            return Err(Error::new_spanned(
                &method.sig,
                "native keyed method has no generated bridge",
            ));
        }
    };
    let receiver_matches = if name == "new" {
        method.sig.receiver().is_none()
    } else {
        method
            .sig
            .receiver()
            .is_some_and(|r| r.reference.is_some() != owned && r.mutability.is_some() == mutable)
    };
    if !receiver_matches
        || parameters.len() != count
        || parameters.iter().any(|ty| {
            if matches!(
                name.as_str(),
                "insert" | "push" | "push_front" | "push_back"
            ) {
                matches!(ty, Type::Reference(_))
            } else {
                !matches!(ty, Type::Reference(reference) if reference.mutability.is_none())
            }
        })
    {
        return Err(Error::new_spanned(
            &method.sig,
            "native keyed method signature does not match its storage protocol",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_ownership_or_arity_is_rejected_at_generation_time() {
        for method in [
            syn::parse_quote!(
                fn insert(&self, key: K, value: V) -> Option<V> {
                    todo!()
                }
            ),
            syn::parse_quote!(
                fn insert(&mut self, key: &K, value: V) -> Option<V> {
                    todo!()
                }
            ),
            syn::parse_quote!(
                fn remove(&mut self) -> Option<V> {
                    todo!()
                }
            ),
            syn::parse_quote!(
                fn into_iter(&self) -> Self::IntoIter {
                    todo!()
                }
            ),
            syn::parse_quote!(
                fn get_cloned(&self, key: &mut K) -> Option<V> {
                    todo!()
                }
            ),
        ] {
            assert!(validate(&method, true).is_err());
        }
        assert!(
            validate(
                &syn::parse_quote!(
                    fn insert(&mut self, key: K, value: V) -> Option<V> {
                        todo!()
                    }
                ),
                true
            )
            .is_ok()
        );
    }
}
