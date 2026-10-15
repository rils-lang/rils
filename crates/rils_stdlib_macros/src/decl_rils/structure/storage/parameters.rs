//! Project implementation-only parameter types into exported language signatures.
use super::*;

const SCOPE_ERROR: &str =
    "#[rils_param] requires an exported storage-backed inherent method parameter";

fn project_signature(signature: &syn::Signature, allowed: bool) -> syn::Result<syn::Signature> {
    let mut result = signature.clone();
    for input in &mut result.inputs {
        let attrs = match &*input {
            FnArg::Receiver(receiver) => &receiver.attrs,
            FnArg::Typed(parameter) => &parameter.attrs,
        };
        let markers = attrs
            .iter()
            .filter(|attr| attr.path().is_ident("rils_param"))
            .collect::<Vec<_>>();
        let Some(marker) = markers.first() else {
            continue;
        };
        if !allowed || matches!(input, FnArg::Receiver(_)) {
            return Err(Error::new_spanned(marker, SCOPE_ERROR));
        }
        if markers.len() != 1 {
            return Err(Error::new_spanned(
                marker,
                "only one #[rils_param(Type)] is allowed per parameter",
            ));
        }
        let ty = marker.parse_args::<Type>()?;
        let FnArg::Typed(parameter) = input else {
            unreachable!("receiver markers were rejected");
        };
        *parameter.ty = ty;
        parameter
            .attrs
            .retain(|attr| !attr.path().is_ident("rils_param"));
    }
    Ok(result)
}

pub(in crate::decl_rils) fn language_method(
    item: &ItemStruct,
    method: &ImplItemFn,
) -> syn::Result<ImplItemFn> {
    let allowed = parameter(item)?.is_some()
        && method
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("export_rils"));
    let mut result = method.clone();
    result.sig = project_signature(&method.sig, allowed)?;
    validate_surface(item, std::slice::from_ref(&result))?;
    Ok(result)
}

pub(in crate::decl_rils) fn validate_impl_parameters(
    item: Option<&ItemStruct>,
    implementation: &ItemImpl,
) -> syn::Result<()> {
    for member in &implementation.items {
        let ImplItem::Fn(method) = member else {
            continue;
        };
        if implementation.trait_.is_none()
            && method
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("export_rils"))
            && let Some(item) = item
        {
            language_method(item, method)?;
        } else {
            project_signature(&method.sig, false)?;
        }
    }
    Ok(())
}

pub(in crate::decl_rils) fn validate_module_parameters(items: &[Item]) -> syn::Result<()> {
    for item in items {
        match item {
            Item::Impl(implementation) => {
                let structure = items.iter().find_map(|item| {
                    let Item::Struct(item) = item else {
                        return None;
                    };
                    let Type::Path(target) = implementation.self_ty.as_ref() else {
                        return None;
                    };
                    (target.qself.is_none()
                        && target.path.segments.len() == 1
                        && target.path.segments[0].ident == item.ident
                        && item
                            .attrs
                            .iter()
                            .any(|attr| attr.path().is_ident("rils_struct")))
                    .then_some(item)
                });
                validate_impl_parameters(structure, implementation)?;
            }
            Item::Fn(function) => {
                project_signature(&function.sig, false)?;
            }
            Item::Trait(item) => {
                for member in &item.items {
                    if let syn::TraitItem::Fn(method) = member {
                        project_signature(&method.sig, false)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(in crate::decl_rils) fn strip_parameter_markers(signature: &mut syn::Signature) {
    for input in &mut signature.inputs {
        let attrs = match input {
            FnArg::Receiver(receiver) => &mut receiver.attrs,
            FnArg::Typed(parameter) => &mut parameter.attrs,
        };
        attrs.retain(|attr| !attr.path().is_ident("rils_param"));
    }
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
    fn parameter_projection_preserves_rust_type_and_const_parameter() {
        let method = syn::parse_quote!(
            #[export_rils]
            #[rils_return(Self)]
            pub fn from<const N: usize>(
                #[rils_param([T; N])] values: S::Array<N>,
            ) -> S::Output<Self> {
                todo!()
            }
        );
        let projected = language_method(&declaration(), &method).unwrap();
        assert!(
            projected
                .sig
                .to_token_stream()
                .to_string()
                .contains("values : [T ; N]")
        );
        assert!(
            !projected
                .sig
                .to_token_stream()
                .to_string()
                .contains("rils_param")
        );
        let mut rust = method.clone();
        strip_parameter_markers(&mut rust.sig);
        assert!(
            rust.sig
                .to_token_stream()
                .to_string()
                .contains("values : S :: Array < N >")
        );
        assert!(
            !rust
                .sig
                .to_token_stream()
                .to_string()
                .contains("rils_param")
        );
    }

    #[test]
    fn invalid_parameter_mappings_are_rejected() {
        for method in [
            syn::parse_quote!(
                #[export_rils]
                pub fn from(
                    #[rils_param(T)]
                    #[rils_param(T)]
                    value: S,
                ) -> Self {
                    todo!()
                }
            ),
            syn::parse_quote!(
                #[export_rils]
                pub fn from(#[rils_param(S)] value: S) -> Self {
                    todo!()
                }
            ),
            syn::parse_quote!(
                #[export_rils]
                pub fn from(#[rils_param()] value: S) -> Self {
                    todo!()
                }
            ),
            syn::parse_quote!(
                #[export_rils]
                pub fn from(#[rils_param(T)] &self) -> Self {
                    todo!()
                }
            ),
            syn::parse_quote!(
                pub fn from(#[rils_param(T)] value: S) -> Self {
                    todo!()
                }
            ),
        ] {
            assert!(language_method(&declaration(), &method).is_err());
        }
        let plain = syn::parse_quote!(
            struct Buffer<T>(Vec<T>);
        );
        let method = syn::parse_quote!(
            #[export_rils]
            pub fn from(#[rils_param(T)] value: T) -> Self {
                todo!()
            }
        );
        assert!(language_method(&plain, &method).is_err());
    }

    #[test]
    fn projected_parameter_drives_source_and_metadata() {
        let fixture: syn::File = syn::parse_str(include_str!(
            "../../../../tests/fixtures/parameter_storage.rs"
        ))
        .unwrap();
        let module = fixture
            .items
            .into_iter()
            .find_map(|item| match item {
                Item::Mod(module) => Some(module),
                _ => None,
            })
            .unwrap();
        let definition =
            Definition::parse(syn::parse_quote!(core::fixture::buffer), module).unwrap();
        let source = definition.source().unwrap();
        assert!(source.replace(' ', "").contains("values:[T;N]"), "{source}");
        assert!(!source.contains("S::Array"));
        let metadata = metadata_tokens(&definition).unwrap().to_string();
        assert!(metadata.contains("TypePattern :: Array"));
        assert!(metadata.contains("ArrayParameter"));
        assert!(!metadata.contains("S :: Array"));
    }

    #[test]
    fn module_validation_rejects_parameter_mapping_outside_exported_storage_methods() {
        let storage: Item = syn::parse_quote!(
            #[rils_struct]
            pub struct Buffer<T, #[rils_storage(sequence, crate::Buffer)] S = Vec<T>>(S);
        );
        for item in [
            syn::parse_quote!(
                fn helper(#[rils_param(i32)] value: i32) {}
            ),
            syn::parse_quote!(
                trait Helper {
                    fn call(#[rils_param(i32)] value: i32);
                }
            ),
            syn::parse_quote!(
                impl<T, S> Buffer<T, S> {
                    pub fn helper(#[rils_param(T)] value: T) {}
                }
            ),
            syn::parse_quote!(
                impl<T, S> Other<T, S> {
                    #[export_rils]
                    pub fn helper(#[rils_param(T)] value: T) {}
                }
            ),
            syn::parse_quote!(
                #[rils_impl]
                impl<T, S> From<T> for Buffer<T, S> {
                    fn from(#[rils_param(T)] value: T) -> Self {
                        todo!()
                    }
                }
            ),
        ] {
            let error = validate_module_parameters(&[storage.clone(), item]).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("exported storage-backed inherent method")
            );
        }
    }
}
