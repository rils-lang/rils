//! Declaration shapes supported by the generic collection call boundary.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Input {
    Index,
    Element(usize),
    Query(usize),
    Owned,
    Shared,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Output {
    Unit,
    Bool,
    Usize,
    Element(usize),
    Optional(usize),
    Owned,
    Iterator(usize),
}
impl Output {
    pub fn tokens(self) -> proc_macro2::TokenStream {
        let shape = match self {
            Self::Unit => quote!(Unit),
            Self::Bool => quote!(Bool),
            Self::Usize => quote!(Usize),
            Self::Element(index) => quote!(Element(#index)),
            Self::Optional(index) => quote!(Optional(#index)),
            Self::Owned => quote!(Owned),
            Self::Iterator(index) => quote!(Iterator(#index)),
        };
        quote!(super::super::sequence_access::ResultShape::#shape)
    }
}

pub(super) struct Signature {
    pub mutable: bool,
    pub inputs: Vec<Input>,
    pub output: Output,
    pub fallible: bool,
}
impl Signature {
    pub fn parse(method: &ImplItemFn, parameters: &[syn::Ident]) -> syn::Result<Self> {
        let unsupported = || {
            Error::new_spanned(
                &method.sig,
                "sequence method signature has no generated storage bridge",
            )
        };
        let receiver = method
            .sig
            .receiver()
            .filter(|r| r.reference.is_some())
            .ok_or_else(unsupported)?;
        let inputs = method.sig.inputs.iter().skip(1).map(|input| {
            let FnArg::Typed(input) = input else { return None; };
            let ty = input.ty.as_ref();
            if let Some(index) = parameter(ty, parameters) { return Some(Input::Element(index)); }
            match ty {
                Type::Path(path) if path.path.is_ident("usize") => Some(Input::Index),
                Type::Path(path) if path.path.is_ident("Self") => Some(Input::Owned),
                Type::Reference(reference) if reference.mutability.is_none() => {
                    if let Some(index) = parameter(&reference.elem, parameters) { Some(Input::Query(index)) }
                    else if matches!(reference.elem.as_ref(), Type::Path(path) if path.path.is_ident("Self")) { Some(Input::Shared) }
                    else { None }
                }
                _ => None,
            }
        }).collect::<Option<Vec<_>>>().ok_or_else(unsupported)?;
        if inputs
            .iter()
            .filter(|input| matches!(input, Input::Shared))
            .count()
            > 1
            || (receiver.mutability.is_some() && inputs.contains(&Input::Shared))
        {
            return Err(unsupported());
        }
        let (ty, fallible) = language_output(method)?;
        let output = if let Some(index) = parameter(&ty, parameters) {
            Output::Element(index)
        } else {
            match &ty {
                Type::Tuple(tuple) if tuple.elems.is_empty() => Output::Unit,
                Type::Path(path) if path.path.is_ident("bool") => Output::Bool,
                Type::Path(path) if path.path.is_ident("usize") => Output::Usize,
                Type::Path(path) if path.path.is_ident("Self") => Output::Owned,
                Type::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => {
                    let segment = &path.path.segments[0];
                    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                        return Err(unsupported());
                    };
                    let Some(GenericArgument::Type(argument)) = arguments.args.first() else {
                        return Err(unsupported());
                    };
                    if arguments.args.len() != 1 {
                        return Err(unsupported());
                    }
                    let index = parameter(argument, parameters).ok_or_else(unsupported)?;
                    if segment.ident == "Option" {
                        Output::Optional(index)
                    } else if segment.ident == "Iterator" || segment.ident == "Iter" {
                        Output::Iterator(index)
                    } else {
                        return Err(unsupported());
                    }
                }
                _ => return Err(unsupported()),
            }
        };
        Ok(Self {
            mutable: receiver.mutability.is_some(),
            inputs,
            output,
            fallible,
        })
    }
    pub fn owns_arguments(&self) -> bool {
        self.inputs
            .iter()
            .any(|input| matches!(input, Input::Element(_) | Input::Owned))
    }
}

pub(super) fn parameter(ty: &Type, parameters: &[syn::Ident]) -> Option<usize> {
    match ty {
        Type::Path(path) if path.qself.is_none() => {
            parameters.iter().position(|name| path.path.is_ident(name))
        }
        Type::Paren(ty) => parameter(&ty.elem, parameters),
        Type::Group(ty) => parameter(&ty.elem, parameters),
        _ => None,
    }
}

pub(super) fn language_output(method: &ImplItemFn) -> syn::Result<(Type, bool)> {
    if let Some(attr) = method
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("rils_return"))
    {
        Ok((attr.parse_args()?, true))
    } else {
        Ok((
            match &method.sig.output {
                ReturnType::Default => syn::parse_quote!(()),
                ReturnType::Type(_, ty) => *ty.clone(),
            },
            false,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_parameter_identity_controls_queries_and_results() {
        let parameters = [syn::parse_quote!(K), syn::parse_quote!(V)];
        let method = syn::parse_quote!(
            #[rils_return(Option<V>)]
            fn lookup(&self, key: &K) -> S::Output {
                todo!()
            }
        );
        let parsed = Signature::parse(&method, &parameters).unwrap();
        assert_eq!(parsed.inputs, [Input::Query(0)]);
        assert_eq!(parsed.output, Output::Optional(1));
        assert!(!parsed.owns_arguments());
        let method = syn::parse_quote!(
            #[rils_return(Option<K>)]
            fn replace(&mut self, value: V, key: K) -> S::Output {
                todo!()
            }
        );
        let parsed = Signature::parse(&method, &parameters).unwrap();
        assert_eq!(parsed.inputs, [Input::Element(1), Input::Element(0)]);
        assert_eq!(parsed.output, Output::Optional(0));
        assert!(parsed.owns_arguments());
    }
}
