//! Static Self results are constructed by the declared standard-library body.
use super::*;

pub(super) enum Input {
    Empty,
    Array,
}

pub(super) fn input(method: &ImplItemFn, parameters: &[syn::Ident]) -> Option<Input> {
    if method.sig.receiver().is_some() {
        return None;
    }
    let output = method
        .attrs
        .iter()
        .find(|a| a.path().is_ident("rils_return"))
        .and_then(|a| a.parse_args::<Type>().ok())
        .or_else(|| match &method.sig.output {
            ReturnType::Type(_, ty) => Some(*ty.clone()),
            _ => None,
        })?;
    if !matches!(output, Type::Path(path) if path.path.is_ident("Self")) {
        return None;
    }
    if method.sig.inputs.is_empty() && method.sig.generics.params.is_empty() {
        return Some(Input::Empty);
    }
    let Some(FnArg::Typed(argument)) = method.sig.inputs.first() else {
        return None;
    };
    let Type::Array(array) = argument.ty.as_ref() else {
        return None;
    };
    let Some(syn::GenericParam::Const(parameter)) = method.sig.generics.params.first() else {
        return None;
    };
    (method.sig.inputs.len() == 1
        && method.sig.generics.params.len() == 1
        && parameters.len() == 1
        && signature::parameter(&array.elem, parameters) == Some(0)
        && matches!(&array.len, syn::Expr::Path(path) if path.path.is_ident(&parameter.ident))
        && matches!(&parameter.ty, Type::Path(path) if path.path.is_ident("usize")))
    .then_some(Input::Array)
}

pub(super) fn call(
    input: Input,
    method: &ImplItemFn,
    receiver: &Path,
    owner: &str,
    generics: &[proc_macro2::TokenStream],
) -> proc_macro2::TokenStream {
    let name = &method.sig.ident;
    if matches!(input, Input::Array) {
        let syn::GenericParam::Const(parameter) = &method.sig.generics.params[0] else {
            unreachable!()
        };
        if storage::mentions(method.block.to_token_stream(), &parameter.ident)
            || method
                .sig
                .generics
                .where_clause
                .as_ref()
                .is_some_and(|clause| storage::mentions(clause.to_token_stream(), &parameter.ident))
        {
            return Error::new_spanned(&method.block,
                "native array construction cannot read the erased const length; query the backend input length instead")
                .into_compile_error();
        }
    }
    match input {
        Input::Empty => quote! {
            super::super::sequence_access::construct_empty(arguments, &SCHEMA, context, expected, || {
                #receiver::<#(#generics,)* rils_stdlib::storage::OwnedSequence>::#name().into_storage()
            })
        },
        Input::Array => quote! {
            super::super::sequence_access::construct_array(arguments, #owner, context, expected, |items| {
                #receiver::<#(#generics,)* rils_stdlib::storage::OwnedSequence>::#name::<0>(items)
                    .map(|value| value.into_storage())
            })
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn array_constructor_requires_a_matching_signature_and_rejects_erased_length_reads() {
        let method: ImplItemFn = syn::parse_quote! {
            #[rils_return(Self)]
            fn collect<const N: usize>(values: [E; N]) -> S::Output<Self> {
                ArrayInput::build(values, Self::with_storage)
            }
        };
        let element = syn::parse_quote!(E);
        assert!(matches!(input(&method, &[element]), Some(Input::Array)));
        assert!(input(&method, &[syn::parse_quote!(Other)]).is_none());
        let generated = call(
            Input::Array,
            &method,
            &syn::parse_quote!(Buffer),
            "Buffer",
            &[quote!(rils_value::DynamicValue)],
        );
        assert!(!generated.to_string().contains("compile_error"));
        let mut invalid = method;
        invalid.block = syn::parse_quote!({
            assert!(N > 0);
            todo!()
        });
        let generated = call(
            Input::Array,
            &invalid,
            &syn::parse_quote!(Buffer),
            "Buffer",
            &[quote!(rils_value::DynamicValue)],
        );
        assert!(generated.to_string().contains("compile_error"));
    }
}
