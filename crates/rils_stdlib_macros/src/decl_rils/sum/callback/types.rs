//! Rust representations and Rils type patterns for a generated sum callback.

use quote::quote;
use syn::{Error, GenericArgument, ImplItemFn, ItemEnum, PathArguments, Type};

use crate::decl_rils::Tokens;

#[derive(Clone)]
pub(super) enum Shape {
    Value(String),
    Option(Box<Shape>),
    Result(Box<Shape>, Box<Shape>),
    Shared(Box<Shape>),
    Bool,
    Unit,
}

impl Shape {
    pub(super) fn parse(item: &ItemEnum, method: &ImplItemFn, ty: &Type) -> syn::Result<Self> {
        match ty {
            Type::Tuple(tuple) if tuple.elems.is_empty() => Ok(Self::Unit),
            Type::Reference(reference) if reference.mutability.is_none() => {
                let child = Self::parse(item, method, &reference.elem)?;
                if !matches!(child, Self::Value(_)) {
                    return Err(Error::new_spanned(
                        ty,
                        "callback borrow needs a generic item",
                    ));
                }
                Ok(Self::Shared(Box::new(child)))
            }
            Type::Path(path) if path.path.is_ident("Self") => Ok(Self::receiver(item)),
            Type::Path(path) if path.path.is_ident("bool") => Ok(Self::Bool),
            Type::Path(path)
                if item
                    .generics
                    .type_params()
                    .chain(method.sig.generics.type_params())
                    .any(|generic| path.path.is_ident(&generic.ident)) =>
            {
                Ok(Self::Value(path.path.segments[0].ident.to_string()))
            }
            Type::Path(path) => {
                let segment = path.path.segments.last().expect("type path");
                let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                    return Err(Error::new_spanned(ty, "unsupported callback value type"));
                };
                let children = arguments
                    .args
                    .iter()
                    .map(|argument| {
                        let GenericArgument::Type(ty) = argument else {
                            return Err(Error::new_spanned(
                                argument,
                                "callback needs type arguments",
                            ));
                        };
                        Self::parse(item, method, ty)
                    })
                    .collect::<syn::Result<Vec<_>>>()?;
                match (segment.ident.to_string().as_str(), children.as_slice()) {
                    ("Option", [child]) if matches!(child, Self::Value(_)) => {
                        Ok(Self::Option(Box::new(child.clone())))
                    }
                    ("Result", [ok, error])
                        if matches!((ok, error), (Self::Value(_), Self::Value(_))) =>
                    {
                        Ok(Self::Result(Box::new(ok.clone()), Box::new(error.clone())))
                    }
                    _ => Err(Error::new_spanned(ty, "unsupported callback sum type")),
                }
            }
            _ => Err(Error::new_spanned(ty, "unsupported callback value type")),
        }
    }

    pub(super) fn receiver(item: &ItemEnum) -> Self {
        let mut generics = item
            .generics
            .type_params()
            .map(|parameter| Self::Value(parameter.ident.to_string()));
        let first = generics.next().expect("sum has an item");
        if item.ident == "Option" {
            Self::Option(Box::new(first))
        } else {
            Self::Result(
                Box::new(first),
                Box::new(generics.next().expect("Result has an error")),
            )
        }
    }

    pub(super) fn rust_type(&self) -> Tokens {
        match self {
            Self::Value(_) => quote!(crate::Value),
            Self::Option(_) => quote!(rils_stdlib::stdlib::option::Option<crate::Value>),
            Self::Result(_, _) => {
                quote!(rils_stdlib::stdlib::result::Result<crate::Value, crate::Value>)
            }
            Self::Shared(_) => quote!(&crate::Value),
            Self::Bool => quote!(bool),
            Self::Unit => quote!(()),
        }
    }

    pub(super) fn pattern(&self) -> Tokens {
        match self {
            Self::Value(name) => quote!(crate::Type::Variable(#name.into())),
            Self::Option(child) => {
                let child = child.pattern();
                quote!(crate::Type::Option(Box::new(#child)))
            }
            Self::Result(ok, error) => {
                let ok = ok.pattern();
                let error = error.pattern();
                quote!(crate::Type::Result(Box::new(#ok), Box::new(#error)))
            }
            Self::Shared(child) => {
                let child = child.pattern();
                quote!(crate::Type::Reference { mutable: false, inner: Box::new(#child) })
            }
            Self::Bool => quote!(crate::Type::Bool),
            Self::Unit => quote!(crate::Type::Unit),
        }
    }

    pub(super) fn decode(&self, value: Tokens) -> Tokens {
        let pattern = self.pattern();
        let convert = match self {
            Self::Value(_) => quote!(Ok(value)),
            Self::Option(_) => quote!(bridge.for_type(&ty, context)?.import_option(value)),
            Self::Result(_, _) => quote!(bridge.for_type(&ty, context)?.import_result(value)),
            Self::Bool => quote!(
                <bool as crate::runtime_builtins::native_value::NativeValue>::from_owned_value(
                    value
                )
            ),
            Self::Unit => quote!(
                <() as crate::runtime_builtins::native_value::NativeValue>::from_owned_value(value)
            ),
            Self::Shared(_) => unreachable!("callbacks cannot return a temporary borrow"),
        };
        quote! {{
            let ty = types.resolve(&#pattern)?;
            let value = #value;
            if !ty.accepts(&value) {
                return Err(format!("native callback must return {}, found {}", ty, value.type_name()).into());
            }
            #convert
        }}
    }

    pub(super) fn encode(&self, value: Tokens) -> Tokens {
        let pattern = self.pattern();
        match self {
            Self::Value(_) => quote!(#value),
            Self::Shared(_) => quote!(super::super::callback::shared_argument(#value)),
            Self::Option(_) => {
                quote!(bridge.for_type(&types.resolve(&#pattern)?, context)?.export_option(#value, None)?)
            }
            Self::Result(_, _) => {
                quote!(bridge.for_type(&types.resolve(&#pattern)?, context)?.export_result(#value)?)
            }
            Self::Bool => quote!(crate::Value::Bool(#value)),
            Self::Unit => quote!({ #value; crate::Value::Unit }),
        }
    }
}
