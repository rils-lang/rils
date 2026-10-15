//! Sequence layouts derived from declared element types and language parameters.
use super::*;

pub(super) fn tokens(definition: &Definition) -> syn::Result<proc_macro2::TokenStream> {
    let (arity, boxed) = shape(&definition.item).ok_or_else(|| Error::new_spanned(
        &definition.item,
        "native collection layout requires declared sequence storage or a supported storage field",
    ))?;
    let name = definition.item.ident.to_string();
    let parameters = storage::language_parameters(&definition.item)?;
    let element = if storage::parameter(&definition.item)?.is_some() {
        storage::element_type(&definition.item)?
    } else if arity == 1 {
        let parameter = &parameters[0];
        syn::parse_quote!(#parameter)
    } else {
        syn::parse_quote!((#(#parameters),*))
    };
    let item_layout = element_layout(&element, &parameters)?;
    let layout = if boxed {
        quote! {
            Some(Ok(rils_value::DynamicLayout::of::<std::boxed::Box<dyn std::any::Any>>(ty.clone())))
        }
    } else {
        quote! {
            Some(#item_layout.map(|item| rils_value::DynamicLayout::sequence(ty.clone(), item)))
        }
    };
    Ok(quote! {
        pub fn matches(ty: &crate::Type) -> bool {
            matches!(ty, crate::Type::Named { name, arguments }
                if name == #name && arguments.len() == #arity)
        }
        pub fn layout(
            ty: &crate::Type,
            resolve_child: &mut dyn FnMut(&crate::Type) -> Result<std::rc::Rc<rils_value::DynamicLayout>, String>,
        ) -> Option<Result<std::rc::Rc<rils_value::DynamicLayout>, String>> {
            let crate::Type::Named { arguments, .. } = ty else { return None; };
            if !matches(ty) { return None; }
            let _ = resolve_child;
            #layout
        }
    })
}

fn parameter_index(ty: &Type, parameters: &[syn::Ident]) -> Option<usize> {
    let Type::Path(path) = ty else { return None };
    if path.qself.is_some() {
        return None;
    }
    parameters
        .iter()
        .position(|parameter| path.path.is_ident(parameter))
}

fn element_type(ty: &Type, parameters: &[syn::Ident]) -> syn::Result<proc_macro2::TokenStream> {
    if let Some(index) = parameter_index(ty, parameters) {
        return Ok(quote!(arguments[#index].clone()));
    }
    match ty {
        Type::Tuple(tuple) => {
            let fields = tuple
                .elems
                .iter()
                .map(|ty| element_type(ty, parameters))
                .collect::<syn::Result<Vec<_>>>()?;
            Ok(quote!(crate::Type::Tuple(vec![#(#fields),*])))
        }
        Type::Paren(ty) => element_type(&ty.elem, parameters),
        Type::Group(ty) => element_type(&ty.elem, parameters),
        _ => Err(Error::new_spanned(
            ty,
            "unsupported declared sequence element type",
        )),
    }
}

fn element_layout(ty: &Type, parameters: &[syn::Ident]) -> syn::Result<proc_macro2::TokenStream> {
    if let Some(index) = parameter_index(ty, parameters) {
        return Ok(quote!(resolve_child(&arguments[#index])));
    }
    match ty {
        Type::Tuple(tuple) => {
            let fields = tuple
                .elems
                .iter()
                .enumerate()
                .map(|(index, ty)| {
                    let name = index.to_string();
                    let layout = element_layout(ty, parameters)?;
                    Ok(quote!((#name.into(), #layout?)))
                })
                .collect::<syn::Result<Vec<_>>>()?;
            let ty = element_type(ty, parameters)?;
            Ok(quote!((|| {
                rils_value::DynamicLayout::aggregate(#ty, vec![#(#fields),*])
            })()))
        }
        Type::Paren(ty) => element_layout(&ty.elem, parameters),
        Type::Group(ty) => element_layout(&ty.elem, parameters),
        _ => Err(Error::new_spanned(
            ty,
            "unsupported declared sequence element layout",
        )),
    }
}

pub(super) fn shape(item: &ItemStruct) -> Option<(usize, bool)> {
    if storage::parameter(item).ok().flatten().is_some() {
        return Some((storage::language_parameters(item).ok()?.len(), false));
    }
    let Fields::Unnamed(fields) = &item.fields else {
        return None;
    };
    if fields.unnamed.len() != 1 {
        return None;
    }
    let storage = fields
        .unnamed
        .first()?
        .ty
        .to_token_stream()
        .to_string()
        .replace(' ', "");
    let type_count = item.generics.type_params().count();
    (type_count == 1 && storage == "std::boxed::Box<T>").then_some((1, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Definition {
        let fixture: syn::File = syn::parse_str(include_str!(
            "../../../tests/fixtures/multi_parameter_storage.rs"
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
        Definition::parse(syn::parse_quote!(core::fixture::reordered), module).unwrap()
    }

    #[test]
    fn multiple_parameters_project_source_metadata_and_trait_associated_types() {
        let definition = fixture();
        let source = definition.source().unwrap().replace(' ', "");
        assert!(source.contains("pubstructReordered<A,B>"));
        assert!(source.contains("impl<A,B>IntoIteratorforReordered<A,B>"));
        assert!(source.contains("typeItem=(B,A)"));
        assert!(source.contains("typeIntoIter=Entries<A,B>"));
        let metadata = metadata_tokens(&definition).unwrap().to_string();
        assert!(
            metadata.contains("type_parameters : & [\"A\" , \"B\"]"),
            "{metadata}"
        );
        assert!(metadata.contains("TypePattern :: Generic (\"A\")"));
        assert!(metadata.contains("TypePattern :: Generic (\"B\")"));
        assert!(!metadata.contains("S :: IntoIter"));
        assert_eq!(shape(&definition.item), Some((2, false)));
        let bridge = vector::tokens(&definition).unwrap().to_string();
        assert!(bridge.contains("EntryShape :: Tuple"), "{bridge}");
        assert!(bridge.contains("Parameter (1usize)"), "{bridge}");
        assert!(bridge.contains("Parameter (0usize)"), "{bridge}");
        assert!(!bridge.contains("compile_error"), "{bridge}");
    }

    #[test]
    fn repeated_parameter_layout_rejects_ambiguous_native_projections() {
        let definition = Definition::parse(syn::parse_quote!(core::fixture::pairs), syn::parse_quote! {
            mod native {
                #[rils_struct]
                pub struct Pairs<T, #[rils_storage(sequence((T, T)), crate::Pairs)] S = Vec<(T, T)>>(S);
                impl<T, S> Pairs<T, S> {
                    #[export_rils]
                    pub fn len(&self) -> usize { todo!() }
                }
            }
        }).unwrap();
        let layout = tokens(&definition).unwrap().to_string();
        syn::parse2::<syn::File>(tokens(&definition).unwrap()).unwrap();
        assert!(layout.contains("Type :: Tuple"));
        assert_eq!(shape(&definition.item), Some((1, false)));
        let bridge = vector::tokens(&definition).unwrap().to_string();
        assert!(bridge.contains("entry schema repeats a language type parameter"));
        let legacy_heap = syn::parse_quote!(
            struct Heap<T>(std::collections::BinaryHeap<T>);
        );
        assert_eq!(shape(&legacy_heap), None);
    }
}
