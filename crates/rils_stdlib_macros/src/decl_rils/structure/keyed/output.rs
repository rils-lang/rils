//! Return conversion derived from the declared Rust signature.

use super::super::*;

pub(crate) fn tokens(method: &ImplItemFn, owner: &syn::Ident) -> proc_macro2::TokenStream {
    let ReturnType::Type(_, ty) = &method.sig.output else {
        return quote! { let () = result; Ok(crate::Value::from(crate::value::native_unit())) };
    };
    let Type::Path(path) = ty.as_ref() else {
        return Error::new_spanned(ty, "unsupported native keyed output").into_compile_error();
    };
    let segment = path.path.segments.last().expect("type path");
    match segment.ident.to_string().as_str() {
        "bool" => quote!(Ok(crate::Value::from(crate::value::native_bool(result)))),
        "usize" => quote!(Ok(crate::Value::from(crate::value::native_usize(result)))),
        "T" => quote!(object.codec().from_native(result)),
        "Option" | "Iterator" => {
            let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return Error::new_spanned(ty, "native collection output requires an element type")
                    .into_compile_error();
            };
            let Some(GenericArgument::Type(Type::Path(item))) = arguments.args.first() else {
                return Error::new_spanned(
                    ty,
                    "native collection output requires a type parameter",
                )
                .into_compile_error();
            };
            let field = if item.path.is_ident("K") {
                quote!(Some(0))
            } else if item.path.is_ident("V") {
                quote!(Some(1))
            } else if item.path.is_ident("T") {
                quote!(None)
            } else {
                return Error::new_spanned(item, "unmapped native collection output parameter")
                    .into_compile_error();
            };
            if segment.ident == "Option" {
                quote!(object.option_for(COLLECTION.item_layout(&object, #field)?)?.construct_native(result))
            } else {
                quote!(Ok(crate::Value::from_owned_iterator(crate::value::OwnedIteratorValue::from_native(
                    result, COLLECTION.item_layout(&object, #field)?, object.codec(),
                )?)))
            }
        }
        _ if segment.ident == *owner || segment.ident == "Self" => quote! {
            crate::value::native_instance::from_native(
                rils_value::DynamicValue::sequence(object.descriptor().layout_handle(), result)?,
                std::rc::Rc::new(object.codec()),
            )
        },
        _ => Error::new_spanned(ty, "unsupported native keyed output").into_compile_error(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_layout_follows_declared_type_parameter() {
        for (ty, expected) in [
            (quote!(Option<K>), "Some (0)"),
            (quote!(Option<V>), "Some (1)"),
            (quote!(Option<T>), "None"),
            (quote!(Iterator<V>), "Some (1)"),
        ] {
            let method = syn::parse_quote!(fn query(&self) -> #ty { todo!() });
            let output = tokens(&method, &syn::parse_quote!(Renamed)).to_string();
            assert!(output.contains(expected), "{output}");
            assert!(!output.contains("compile_error"), "{output}");
        }
        let method = syn::parse_quote!(
            fn query(&self) -> Option<Unknown> {
                todo!()
            }
        );
        assert!(
            tokens(&method, &syn::parse_quote!(Renamed))
                .to_string()
                .contains("compile_error")
        );
    }
}
