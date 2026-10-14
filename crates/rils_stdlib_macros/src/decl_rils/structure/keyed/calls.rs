//! Generate the execution boundary once for each declared native collection method.

use super::super::*;

pub(super) fn borrowed(
    method: &ImplItemFn,
    map: bool,
    owner: &syn::Ident,
) -> proc_macro2::TokenStream {
    let name = method.sig.ident.to_string();
    let mutable = method
        .sig
        .receiver()
        .is_some_and(|r| r.mutability.is_some());
    let backend = if map { quote!(map) } else { quote!(set) };
    let key_field = if map { quote!(Some(0)) } else { quote!(None) };
    let output = super::output::tokens(method, owner);
    let query = quote! {
        let key = crate::value::native_key::query(&arguments[1], COLLECTION.item_layout(&object, #key_field)?.as_ref(), COLLECTION.ordered)?;
    };
    let body = match name.as_str() {
        "len" => quote! { let result = object.with(|view| view.sequence_len())??; #output },
        "is_empty" => {
            quote! { let result = object.with(|view| view.sequence_len())?? == 0; #output }
        }
        "clear" => quote! {
            let result = object.with_mut(|mut view| view.clear_sequence())?.map_err(|error| COLLECTION.mutation_error(error))?;
            #output
        },
        "contains" | "contains_key" => quote! {
            #query
            let result = object.with(|view| operations::find(&view, &key))??.is_some();
            #output
        },
        "remove" => quote! {
            #query
            let result = object.with_mut(|view| operations::remove(view, &key))?.map_err(|error| COLLECTION.mutation_error(error))?;
            #output
        },
        "get_cloned" if map => quote! {
            #query
            let result = object.with(|view| operations::get_cloned(&view, &key, &mut clone_value))??;
            #output
        },
        "first_cloned" | "last_cloned" | "first_key_cloned" | "last_key_cloned" => {
            let last = name.starts_with("last");
            let operation = if map {
                quote!(boundary_key)
            } else {
                quote!(boundary)
            };
            quote! {
                let result = object.with(|view| operations::#operation(&view, #last, &mut clone_value))??;
                #output
            }
        }
        "keys_cloned" | "values_cloned" if map => {
            let field = usize::from(name == "values_cloned");
            quote! {
                let result = object.with(|view| operations::cloned_fields(&view, #field, &mut clone_value))??;
                #output
            }
        }
        "is_subset"
        | "is_superset"
        | "is_disjoint"
        | "union"
        | "intersection"
        | "difference"
        | "symmetric_difference"
            if !map =>
        {
            let operation = if name.starts_with("is_") {
                quote! {
                    let result = object.with(|left| other.with(|right| operations::relation(&left, &right, #name)))???;
                    #output
                }
            } else {
                quote! {
                    let result = object.with(|left| other.with(|right| operations::algebra(&left, &right, #name, &mut clone_value)))???;
                    #output
                }
            };
            quote! {
                let other = COLLECTION.receiver(&arguments[1])?;
                if !object.descriptor().layout().compatible_with(other.descriptor().layout()) {
                    return Err(format!("{} element types do not match", COLLECTION.name));
                }
                #operation
            }
        }
        _ => {
            return Error::new_spanned(&method.sig, "native keyed method has no generated bridge")
                .into_compile_error();
        }
    };
    // Imports are used only for operations which need the corresponding boundary.
    let operations = if matches!(name.as_str(), "len" | "is_empty" | "clear") {
        quote!()
    } else {
        quote!(use rils_stdlib::native_collections::#backend as operations;)
    };
    let clone = if name.ends_with("cloned")
        || matches!(
            name.as_str(),
            "union" | "intersection" | "difference" | "symmetric_difference"
        ) {
        quote!(let mut clone_value = crate::value::runtime_layouts::clone_borrowed_view;)
    } else {
        quote!()
    };
    quote! { (|| {
        #operations
        #clone
        let object = COLLECTION.checked_receiver(&arguments[0], #mutable)?;
        #body
    })() }
}

pub(super) fn owned(
    method: &ImplItemFn,
    map: bool,
    owner: &syn::Ident,
) -> proc_macro2::TokenStream {
    let name = method.sig.ident.to_string();
    let mutable = method
        .sig
        .receiver()
        .is_some_and(|r| r.mutability.is_some());
    if name == "into_iter" {
        return quote!(COLLECTION.into_iter(
            arguments.into_iter().next().expect("arity checked"),
            context
        ));
    }
    if name == "iter" {
        let operation = if map {
            quote!(borrow_map)
        } else {
            quote!(borrow)
        };
        return quote! { (|| {
            COLLECTION.checked_receiver(&arguments[0], #mutable)?;
            crate::runtime_builtins::indexed_iter::#operation(&arguments)
        })() };
    }
    let backend = if map { quote!(map) } else { quote!(set) };
    let key_field = if map { quote!(Some(0)) } else { quote!(None) };
    let output = super::output::tokens(method, owner);
    let input = if map {
        quote! {
            let value = codec.into_native(values.next().expect("arity checked"), COLLECTION.item_layout(&object, Some(1))?)?;
            let input = rils_value::DynamicValue::record(COLLECTION.item_layout(&object, None)?, vec![key, value])?;
        }
    } else {
        quote!(let input = key;)
    };
    quote! { (|| {
        let object = COLLECTION.checked_receiver(&arguments[0], #mutable)?;
        let mut values = arguments.into_iter();
        values.next();
        let mut codec = crate::value::record_codec::NativeRecordCodec::with_definitions(&context.structs, &context.enums).with_owned_conversion();
        let key = codec.into_native(values.next().expect("arity checked"), COLLECTION.item_layout(&object, #key_field)?)?;
        let identity = crate::value::native_key::native(key.view(), &codec, COLLECTION.ordered)?;
        #input
        let result = object.with_mut(|view| rils_stdlib::native_collections::#backend::insert(view, input, &identity, COLLECTION.ordered))?
            .map_err(|error| COLLECTION.mutation_error(error))?;
        #output
    })() }
}
