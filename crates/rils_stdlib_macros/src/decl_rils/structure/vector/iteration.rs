//! Iterator bridges selected by declared receiver and item shapes.
use super::*;

pub(super) fn supports(method: &ImplItemFn, parameters: &[syn::Ident]) -> bool {
    if method.sig.inputs.len() != 1
        || !method
            .sig
            .receiver()
            .is_some_and(|r| r.reference.is_some() && r.mutability.is_none())
    {
        return false;
    }
    let Ok((Type::Path(path), _)) = signature::language_output(method) else {
        return false;
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return false;
    }
    let segment = &path.path.segments[0];
    if segment.ident != "Iter" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    args.args.len() == 1
        && matches!(args.args.first(), Some(GenericArgument::Type(ty)) if shared_item(ty, parameters))
}

fn shared_item(ty: &Type, parameters: &[syn::Ident]) -> bool {
    match ty {
        Type::Reference(reference) => {
            reference.mutability.is_none()
                && signature::parameter(&reference.elem, parameters).is_some()
        }
        Type::Tuple(tuple) => {
            !tuple.elems.is_empty() && tuple.elems.iter().all(|ty| shared_item(ty, parameters))
        }
        Type::Paren(ty) => shared_item(&ty.elem, parameters),
        Type::Group(ty) => shared_item(&ty.elem, parameters),
        _ => false,
    }
}

pub(super) fn owned(
    method: &ImplItemFn,
    implementation: Option<&ItemImpl>,
    parameters: &[syn::Ident],
) -> bool {
    if method.sig.inputs.len() != 1 || !method.sig.receiver().is_some_and(|r| r.reference.is_none())
    {
        return false;
    }
    let Some(implementation) = implementation else {
        return false;
    };
    if !implementation.trait_.as_ref().is_some_and(|(_, path, _)| {
        path.segments
            .last()
            .is_some_and(|segment| segment.ident == "IntoIterator")
    }) {
        return false;
    }
    let ReturnType::Type(_, output) = &method.sig.output else {
        return false;
    };
    let Ok(Type::Path(path)) = normalize_trait_type(output, implementation) else {
        return false;
    };
    if path.qself.is_some() || path.path.segments.len() != 1 {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &path.path.segments[0].arguments else {
        return false;
    };
    !args.args.is_empty()
        && args
            .args
            .iter()
            .all(|arg| matches!(arg, GenericArgument::Type(ty) if owned_item(ty, parameters)))
}

fn owned_item(ty: &Type, parameters: &[syn::Ident]) -> bool {
    signature::parameter(ty, parameters).is_some()
        || match ty {
            Type::Tuple(tuple) => {
                !tuple.elems.is_empty() && tuple.elems.iter().all(|ty| owned_item(ty, parameters))
            }
            Type::Paren(ty) => owned_item(&ty.elem, parameters),
            Type::Group(ty) => owned_item(&ty.elem, parameters),
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrowed_iteration_checks_every_declared_reference_parameter() {
        let parameters = [syn::parse_quote!(K), syn::parse_quote!(V)];
        for method in [
            syn::parse_quote!(
                #[rils_return(Iter<&K>)]
                fn walk(&self) -> S::IterOutput<'_> {
                    todo!()
                }
            ),
            syn::parse_quote!(
                #[rils_return(Iter<(&V, (&K, &V))>)]
                fn walk(&self) -> S::IterOutput<'_> {
                    todo!()
                }
            ),
        ] {
            assert!(supports(&method, &parameters));
        }
        for method in [
            syn::parse_quote!(
                #[rils_return(Iter<K>)]
                fn iter(&self) -> S::IterOutput<'_> {
                    todo!()
                }
            ),
            syn::parse_quote!(
                #[rils_return(Iter<&Other>)]
                fn iter(&self) -> S::IterOutput<'_> {
                    todo!()
                }
            ),
            syn::parse_quote!(
                #[rils_return(Iter<(&K, &mut V)>)]
                fn iter(&self) -> S::IterOutput<'_> {
                    todo!()
                }
            ),
            syn::parse_quote!(
                #[rils_return(Iter<&K>)]
                fn iter(self) -> S::IterOutput<'static> {
                    todo!()
                }
            ),
            syn::parse_quote!(
                #[rils_return(Iter<&K>)]
                fn iter(&mut self) -> S::IterOutput<'_> {
                    todo!()
                }
            ),
        ] {
            assert!(!supports(&method, &parameters));
        }
    }
}
