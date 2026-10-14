//! Queue bridges reuse signature-driven receiver and output conversions.
use super::super::*;

pub(super) fn call(
    method: &ImplItemFn,
    owner: &syn::Ident,
    heap: bool,
) -> proc_macro2::TokenStream {
    let name = method.sig.ident.to_string();
    if name == "into_iter" {
        return super::calls::owned(method, false, owner);
    }
    if matches!(name.as_str(), "len" | "is_empty" | "clear") {
        return super::calls::borrowed(method, false, owner);
    }
    let output = super::output::tokens(method, owner);
    let mutable = method
        .sig
        .receiver()
        .is_some_and(|r| r.mutability.is_some());
    let operation = match name.as_str() {
        "push" | "push_front" | "push_back" => {
            let push = if heap {
                quote!(rils_stdlib::native_collections::queue::heap_push(
                    view, item
                ))
            } else if name == "push_front" {
                quote!(view.push_sequence_front(item))
            } else {
                quote!(view.push_sequence_item(item))
            };
            // Heap takes its view by value; deque operations mutate the view directly.
            let view = if heap { quote!(view) } else { quote!(mut view) };
            quote! {
                let item = arguments.into_iter().nth(1).expect("arity checked");
                let mut codec = crate::value::record_codec::NativeRecordCodec::with_definitions(&context.structs, &context.enums).with_owned_conversion();
                let item = codec.into_native(item, COLLECTION.item_layout(&object, None)?)?;
                let result = object.with_mut(|#view| #push)?;
                let result = result.map_err(|error| COLLECTION.mutation_error(error))?;
                #output
            }
        }
        "pop" | "pop_front" | "pop_back" => {
            let back = name == "pop_back";
            let pop = if heap {
                quote!(rils_stdlib::native_collections::queue::heap_pop(view))
            } else {
                quote!(rils_stdlib::native_collections::queue::pop(view, #back))
            };
            quote! {
                let result = object.with_mut(|view| #pop)?.map_err(|error| COLLECTION.mutation_error(error))?;
                #output
            }
        }
        "peek_cloned" | "front_cloned" | "back_cloned" => {
            let back = name == "back_cloned";
            quote! {
                let result = object.with(|view| rils_stdlib::native_collections::queue::boundary(view, #back, &mut crate::value::runtime_layouts::clone_borrowed_view))??;
                #output
            }
        }
        _ => {
            return Error::new_spanned(&method.sig, "unsupported native queue operation")
                .into_compile_error();
        }
    };
    quote! { (|| {
        let object = COLLECTION.checked_receiver(&arguments[0], #mutable)?;
        #operation
    })() }
}
