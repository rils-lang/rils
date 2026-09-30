use quote::quote;
use rils_syntax::{Span, Type};
use syn::{Error, LitStr};

pub(crate) fn type_tokens(ty: &Type) -> syn::Result<proc_macro2::TokenStream> {
    Ok(match ty {
        Type::Unit => quote!(TypePattern::Unit),
        Type::Bool => quote!(TypePattern::Bool),
        Type::Char => quote!(TypePattern::Char),
        Type::String => quote!(TypePattern::String),
        Type::Float(rils_syntax::FloatType::F32) => quote!(TypePattern::F32),
        Type::Float(rils_syntax::FloatType::F64) => quote!(TypePattern::F64),
        Type::Integer(rils_syntax::IntegerType::U32) => quote!(TypePattern::U32),
        Type::Integer(rils_syntax::IntegerType::U8) => quote!(TypePattern::U8),
        Type::Integer(rils_syntax::IntegerType::Usize) => quote!(TypePattern::Usize),
        Type::Variable(name) => {
            let name = LitStr::new(name, proc_macro2::Span::call_site());
            quote!(TypePattern::Generic(#name))
        }
        Type::Named { name, arguments } if name == "Self" && arguments.is_empty() => {
            quote!(TypePattern::SelfType)
        }
        Type::Named { name, arguments } if name == "integer" && arguments.is_empty() => {
            quote!(TypePattern::AnyInteger)
        }
        Type::Named { name, arguments }
            if rils_syntax::ast::is_builtin_signature_placeholder(name) && arguments.is_empty() =>
        {
            let name = LitStr::new(name, proc_macro2::Span::call_site());
            quote!(TypePattern::Generic(#name))
        }
        Type::Named { name, arguments } => {
            let path = if name == "Iterator" {
                "OwnedIterator"
            } else {
                name
            };
            let path = LitStr::new(path, proc_macro2::Span::call_site());
            let arguments = arguments
                .iter()
                .map(type_tokens)
                .collect::<syn::Result<Vec<_>>>()?;
            quote!(TypePattern::Named { path: #path, arguments: &[#(#arguments),*] })
        }
        Type::Option(inner) => {
            let inner = type_tokens(inner)?;
            quote!(TypePattern::Option(&#inner))
        }
        Type::Result(ok, error) => {
            let ok = type_tokens(ok)?;
            let error = type_tokens(error)?;
            quote!(TypePattern::Result { ok: &#ok, error: &#error })
        }
        Type::Tuple(elements) => {
            let elements = elements
                .iter()
                .map(type_tokens)
                .collect::<syn::Result<Vec<_>>>()?;
            quote!(TypePattern::Tuple(&[#(#elements),*]))
        }
        Type::Function {
            parameters: Some(parameters),
            return_type,
        } => {
            let parameters = parameters
                .iter()
                .map(type_tokens)
                .collect::<syn::Result<Vec<_>>>()?;
            let result = type_tokens(return_type)?;
            quote!(TypePattern::Function { parameters: &[#(#parameters),*], result: &#result })
        }
        Type::Reference { mutable, inner } => {
            let inner = type_tokens(inner)?;
            quote!(TypePattern::Reference { mutable: #mutable, inner: &#inner })
        }
        Type::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => {
            let base = type_tokens(base)?;
            let trait_name = trait_name
                .as_ref()
                .map(|name| {
                    let name = LitStr::new(name, proc_macro2::Span::call_site());
                    quote!(Some(#name))
                })
                .unwrap_or_else(|| quote!(None));
            let name = LitStr::new(name, proc_macro2::Span::call_site());
            let arguments = arguments
                .iter()
                .map(type_tokens)
                .collect::<syn::Result<Vec<_>>>()?;
            quote!(TypePattern::Associated {
                base: &#base,
                trait_name: #trait_name,
                name: #name,
                arguments: &[#(#arguments),*],
            })
        }
        Type::Unknown => quote!(TypePattern::Unknown),
        unsupported => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!("unsupported built-in type `{unsupported:?}`"),
            ));
        }
    })
}

pub(crate) fn documentation(source: &str, span: Span) -> String {
    let mut lines = source[..span.start.min(source.len())]
        .trim_end()
        .lines()
        .rev()
        .map(str::trim_start)
        .take_while(|line| line.starts_with("///"))
        .map(|line| line.trim_start_matches("///").trim().to_owned())
        .collect::<Vec<_>>();
    lines.reverse();
    lines.join("\n")
}
