//! Project backend-dependent Rust trait implementations into language declarations.
use super::*;

pub(in crate::decl_rils::structure) fn language_impl(
    item: &ItemStruct,
    implementation: &ItemImpl,
) -> syn::Result<ItemImpl> {
    let mut result = implementation.clone();
    if !implementation
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("rils_impl"))
    {
        return Ok(result);
    }
    let backend = parameter(item)?.map(|(name, _)| name);
    for member in &mut result.items {
        if let ImplItem::Type(associated) = member {
            let markers = associated
                .attrs
                .iter()
                .filter(|attr| attr.path().is_ident("rils_associated"))
                .collect::<Vec<_>>();
            if !markers.is_empty() {
                if backend.is_none() || markers.len() != 1 {
                    return Err(Error::new_spanned(
                        associated,
                        "one #[rils_associated(Type)] requires a storage-backed trait impl",
                    ));
                }
                associated.ty = markers[0].parse_args()?;
                associated
                    .attrs
                    .retain(|attr| !attr.path().is_ident("rils_associated"));
            }
        }
    }
    let Some(backend) = backend else {
        return Ok(result);
    };
    result.generics.params = result
        .generics
        .params
        .into_iter()
        .filter(|param| !matches!(param, GenericParam::Type(ty) if ty.ident == backend))
        .collect();
    if let Some(clause) = &mut result.generics.where_clause {
        clause.predicates = clause.predicates.clone().into_iter().filter(|predicate|
            !matches!(predicate, syn::WherePredicate::Type(ty) if matches!(&ty.bounded_ty, Type::Path(path) if path.path.is_ident(&backend))))
            .collect();
        if clause.predicates.is_empty() {
            result.generics.where_clause = None;
        }
    }
    if let Type::Path(target) = result.self_ty.as_mut()
        && let Some(segment) = target.path.segments.last_mut()
        && let PathArguments::AngleBracketed(arguments) = &mut segment.arguments
    {
        arguments.args = arguments.args.clone().into_iter().filter(|argument|
            !matches!(argument, GenericArgument::Type(Type::Path(path)) if path.path.is_ident(&backend))).collect();
    }
    if mentions(result.generics.to_token_stream(), &backend)
        || mentions(result.self_ty.to_token_stream(), &backend)
        || result
            .trait_
            .as_ref()
            .is_some_and(|(_, path, _)| mentions(path.to_token_stream(), &backend))
    {
        return Err(Error::new_spanned(
            implementation,
            "storage backend cannot appear in a Rils trait requirement",
        ));
    }
    for member in &result.items {
        let tokens = match member {
            ImplItem::Type(associated) => associated.ty.to_token_stream(),
            ImplItem::Fn(method) => {
                let mut signature = method.sig.clone();
                for argument in &mut signature.inputs {
                    if let FnArg::Typed(argument) = argument {
                        *argument.ty = normalize_trait_type(&argument.ty, &result)?;
                    }
                }
                if let ReturnType::Type(_, ty) = &mut signature.output {
                    **ty = normalize_trait_type(ty, &result)?;
                }
                signature.to_token_stream()
            }
            _ => continue,
        };
        if mentions(tokens, &backend) {
            return Err(Error::new_spanned(
                member,
                "storage backend cannot appear in a Rils trait signature; declare its associated language type",
            ));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn declaration() -> ItemStruct {
        syn::parse_quote!(
            struct Buffer<T, #[rils_storage(sequence, crate::Buffer)] S = Vec<T>>(S);
        )
    }
    #[test]
    fn backend_trait_parameters_and_associated_types_project_together() {
        for implementation in [
            syn::parse_quote!(
                #[rils_impl]
                impl<T, S: Backend<T>> IntoIterator for Buffer<T, S> {
                    type Item = T;
                    #[rils_associated(Iterator<T>)]
                    type IntoIter = S::IntoIter;
                    fn into_iter(self) -> Self::IntoIter {
                        self.0.into_iter()
                    }
                }
            ),
            syn::parse_quote!(
                #[rils_impl]
                impl<T, S> IntoIterator for Buffer<T, S>
                where
                    S: Backend<T>,
                {
                    type Item = T;
                    #[rils_associated(Iterator<T>)]
                    type IntoIter = S::IntoIter;
                    fn into_iter(self) -> Self::IntoIter {
                        self.0.into_iter()
                    }
                }
            ),
        ] {
            let projected = language_impl(&declaration(), &implementation).unwrap();
            assert_eq!(projected.generics.to_token_stream().to_string(), "< T >");
            assert_eq!(
                projected.self_ty.to_token_stream().to_string(),
                "Buffer < T >"
            );
            let method = projected
                .items
                .iter()
                .find_map(|item| {
                    if let ImplItem::Fn(method) = item {
                        Some(method)
                    } else {
                        None
                    }
                })
                .unwrap();
            let ReturnType::Type(_, output) = &method.sig.output else {
                panic!()
            };
            assert_eq!(
                normalize_trait_type(output, &projected)
                    .unwrap()
                    .to_token_stream()
                    .to_string(),
                "Iterator < T >"
            );
        }
    }
    #[test]
    fn invalid_associated_overrides_and_backend_leaks_are_rejected() {
        for implementation in [
            syn::parse_quote!(
                #[rils_impl]
                impl<T, S> IntoIterator for Buffer<T, S> {
                    type IntoIter = S::IntoIter;
                }
            ),
            syn::parse_quote!(
                #[rils_impl]
                impl<T, S> IntoIterator for Buffer<T, S> {
                    #[rils_associated(Iterator<S>)]
                    type IntoIter = S::IntoIter;
                }
            ),
            syn::parse_quote!(
                #[rils_impl]
                impl<T, S> IntoIterator for Buffer<T, S> {
                    #[rils_associated(Iterator<T>)]
                    #[rils_associated(Iterator<T>)]
                    type IntoIter = S::IntoIter;
                }
            ),
            syn::parse_quote!(
                #[rils_impl]
                impl<T: Bound<S>, S> IntoIterator for Buffer<T, S> {
                    type Item = T;
                }
            ),
            syn::parse_quote!(
                #[rils_impl]
                impl<T, S> IntoIterator for Buffer<T, S> {
                    fn leak(self, value: S) -> T {
                        todo!()
                    }
                }
            ),
        ] {
            assert!(language_impl(&declaration(), &implementation).is_err());
        }
        let plain = syn::parse_quote!(
            struct Buffer<T>(Vec<T>);
        );
        let implementation = syn::parse_quote!(
            #[rils_impl]
            impl<T> IntoIterator for Buffer<T> {
                #[rils_associated(Iterator<T>)]
                type IntoIter = Vec<T>;
            }
        );
        assert!(language_impl(&plain, &implementation).is_err());
    }
}
