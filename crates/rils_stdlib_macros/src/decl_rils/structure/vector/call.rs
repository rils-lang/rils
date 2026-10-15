//! Invoke the declared method after generic ownership and layout conversion.
use super::{
    signature::{Input, Output, Signature},
    *,
};

pub(super) fn tokens(
    method: &ImplItemFn,
    receiver: &Path,
    signature: &Signature,
    parameters: &[syn::Ident],
) -> proc_macro2::TokenStream {
    let name = &method.sig.ident;
    let mutable = signature.mutable;
    let output = signature.output.tokens();
    let owns_arguments = signature.owns_arguments();
    let mutability = owns_arguments.then(|| quote!(mut));
    let generics = parameters
        .iter()
        .enumerate()
        .map(|(index, _)| {
            if signature.inputs.contains(&Input::Query(index)) {
                quote!(_)
            } else {
                quote!(rils_value::DynamicValue)
            }
        })
        .collect::<Vec<_>>();
    let mut conversions = Vec::new();
    let mut arguments = Vec::new();
    let mut pair = None;
    for (index, input) in signature.inputs.iter().enumerate() {
        let argument = quote::format_ident!("argument_{index}");
        let position = index + 1;
        let raw = if owns_arguments {
            let raw = quote::format_ident!("input_{index}");
            conversions.push(quote!(let #raw = inputs.next().expect("arity checked");));
            quote!(#raw)
        } else {
            quote!(arguments[#position])
        };
        match input {
            Input::Index => {
                conversions.push(quote!(let #argument = #raw.get_cloned::<usize>()?;));
                arguments.push(quote!(#argument));
            }
            Input::Element(parameter) => {
                conversions.push(quote!(let #argument = call.encode(#raw, #parameter, context)?;));
                arguments.push(quote!(#argument));
            }
            Input::Query(_) => {
                conversions.push(
                    quote!(let #argument = super::super::sequence_access::Query::new(&#raw);),
                );
                arguments.push(quote!(&#argument));
            }
            Input::Owned => {
                conversions.push(quote!(let mut #argument = call.consume(#raw)?;));
                arguments.push(
                    quote!(#receiver::<#(#generics,)* _>::with_storage(#argument.access(&SCHEMA)?)),
                );
            }
            Input::Shared => {
                pair = Some(quote!(&#raw));
                arguments.push(quote!(&#receiver::<#(#generics,)* _>::with_storage(other)));
            }
        }
    }
    let invoke =
        quote!(#receiver::<#(#generics,)* _>::with_storage(storage).#name(#(#arguments),*));
    let invoke = if signature.output == Output::Owned {
        if signature.fallible {
            quote!(#invoke.map(|value| value.into_storage()))
        } else {
            quote!(Ok(#invoke.into_storage()))
        }
    } else if signature.fallible {
        invoke
    } else {
        quote!(Ok(#invoke))
    };
    let invoke = if let Some(other) = pair {
        quote!(call.pair(#other, #output, |storage, other| { #invoke }))
    } else if mutable {
        quote!(call.mutable(#output, |storage| { #invoke }))
    } else {
        quote!(call.shared(#output, |storage| { #invoke }))
    };
    let inputs = owns_arguments.then(|| quote!(let mut inputs = arguments.into_iter().skip(1);));
    quote! { (|| {
        let #mutability call = super::super::sequence_access::Call::prepare(&arguments[0], &SCHEMA, #mutable)?;
        #inputs
        #(#conversions)*
        #invoke
    })() }
}
