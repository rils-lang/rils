//! Metadata and declaration text for exported Rust free functions.

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Error, FnArg, GenericArgument, ItemFn, Path, PathArguments, ReturnType, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

use crate::type_patterns;

pub(crate) mod callback_signature;
mod native;

struct Input {
    path: Path,
    function: ItemFn,
    native_callback: bool,
    shadow_callback: bool,
}

impl Parse for Input {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let path = input.parse()?;
        input.parse::<Token![;]>()?;
        let function = input.parse()?;
        let (function, native_callback, shadow_callback) = callback_signature::expose(function)?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after exported function"));
        }
        Ok(Self {
            path,
            function,
            native_callback,
            shadow_callback,
        })
    }
}

impl Input {
    fn variadic(&self) -> bool {
        self.function
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("rils_variadic"))
    }

    fn any_parameters(&self) -> syn::Result<Vec<syn::Ident>> {
        self.function
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("rils_any"))
            .map(|attr| attr.parse_args())
            .collect()
    }

    fn validate(&self) -> syn::Result<()> {
        let name = &self.function.sig.ident;
        if self
            .path
            .segments
            .last()
            .is_none_or(|segment| segment.ident != *name)
        {
            return Err(Error::new_spanned(
                name,
                "exported function path must match its name",
            ));
        }
        if !matches!(self.function.vis, syn::Visibility::Public(_))
            || self.function.block.stmts.is_empty()
        {
            return Err(Error::new_spanned(
                &self.function,
                "exported function requires a public Rust body",
            ));
        }
        if self
            .function
            .sig
            .generics
            .params
            .iter()
            .any(|parameter| !matches!(parameter, syn::GenericParam::Type(_)))
            || (!self.native_callback && !self.function.sig.generics.params.is_empty())
            || self.function.sig.generics.where_clause.is_some()
            || self.function.sig.asyncness.is_some()
            || self.function.sig.unsafety.is_some()
            || self.function.sig.abi.is_some()
            || self.function.sig.variadic.is_some()
        {
            return Err(Error::new_spanned(
                &self.function.sig,
                "this exported function signature is not supported",
            ));
        }
        for argument in &self.function.sig.inputs {
            if !matches!(argument, FnArg::Typed(_)) {
                return Err(Error::new_spanned(
                    argument,
                    "free function cannot have a receiver",
                ));
            }
        }
        if self.variadic() && self.function.sig.inputs.len() != 1 {
            return Err(Error::new_spanned(
                &self.function.sig,
                "variadic Rust implementation expects one slice parameter",
            ));
        }
        let parameters = self.parameters()?;
        for any in self.any_parameters()? {
            let name = any.to_string();
            if !parameters.iter().any(|(parameter, _)| parameter == &name) {
                return Err(Error::new_spanned(any, "unknown exported parameter"));
            }
        }
        Ok(())
    }

    fn module_segments(&self) -> Vec<syn::PathSegment> {
        self.path
            .segments
            .iter()
            .take(self.path.segments.len() - 1)
            .cloned()
            .collect()
    }

    fn resolve_type(&self, ty: &Type) -> syn::Result<Type> {
        let mut ty = ty.clone();
        self.rewrite_type(&mut ty)?;
        Ok(ty)
    }

    fn rewrite_type(&self, ty: &mut Type) -> syn::Result<()> {
        match ty {
            Type::Path(path) if path.qself.is_none() => {
                let mut segments = path.path.segments.iter().cloned().collect::<Vec<_>>();
                if segments.first().is_some_and(|segment| {
                    segment.ident == "crate" || segment.ident == "rils_stdlib"
                }) && segments
                    .get(1)
                    .is_some_and(|segment| segment.ident == "stdlib")
                {
                    segments.drain(0..2);
                    if segments.len() == 2
                        && segments[0].ident == "string"
                        && segments[1].ident == "String"
                    {
                        segments.remove(0);
                    } else {
                        segments.insert(0, syn::parse_quote!(std));
                    }
                    path.path.leading_colon = None;
                    path.path.segments =
                        segments.into_iter().collect::<Punctuated<_, Token![::]>>();
                } else if segments
                    .first()
                    .is_some_and(|segment| segment.ident == "super")
                {
                    let mut module = self.module_segments();
                    while segments
                        .first()
                        .is_some_and(|segment| segment.ident == "super")
                    {
                        if module.pop().is_none() {
                            return Err(Error::new_spanned(
                                &path.path,
                                "super path escapes Rils module",
                            ));
                        }
                        segments.remove(0);
                    }
                    module.extend(segments);
                    path.path.leading_colon = None;
                    path.path.segments = module.into_iter().collect::<Punctuated<_, Token![::]>>();
                }
                for segment in &mut path.path.segments {
                    if let PathArguments::AngleBracketed(arguments) = &mut segment.arguments {
                        for argument in &mut arguments.args {
                            if let GenericArgument::Type(inner) = argument {
                                self.rewrite_type(inner)?;
                            }
                        }
                    }
                }
            }
            Type::Reference(reference) => self.rewrite_type(&mut reference.elem)?,
            Type::Tuple(tuple) => {
                for element in &mut tuple.elems {
                    self.rewrite_type(element)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn parameters(&self) -> syn::Result<Vec<(String, Type)>> {
        self.function
            .sig
            .inputs
            .iter()
            .map(|argument| {
                let FnArg::Typed(argument) = argument else {
                    unreachable!()
                };
                let syn::Pat::Ident(name) = argument.pat.as_ref() else {
                    return Err(Error::new_spanned(
                        &argument.pat,
                        "exported parameters need simple names",
                    ));
                };
                Ok((name.ident.to_string(), self.resolve_type(&argument.ty)?))
            })
            .collect()
    }

    fn result(&self) -> syn::Result<Type> {
        match &self.function.sig.output {
            ReturnType::Default => Ok(syn::parse_quote!(())),
            ReturnType::Type(_, ty) => self.resolve_type(ty),
        }
    }

    fn source(&self) -> syn::Result<String> {
        let mut source = String::new();
        for line in super::documentation(&self.function.attrs).lines() {
            source.push_str(&format!("/// {line}\n"));
        }
        let any = self
            .any_parameters()?
            .into_iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let parameters = if self.variadic() {
            Vec::new()
        } else {
            self.parameters()?
                .into_iter()
                .map(|(name, ty)| {
                    if any.contains(&name) {
                        return format!("{name}: _");
                    }
                    format!(
                        "{name}: {}",
                        ty.to_token_stream().to_string().replace("String", "string")
                    )
                })
                .collect::<Vec<_>>()
        };
        let result = self
            .result()?
            .to_token_stream()
            .to_string()
            .replace("String", "string");
        let name = &self.function.sig.ident;
        let generic_names = self
            .function
            .sig
            .generics
            .type_params()
            .map(|parameter| parameter.ident.to_string())
            .collect::<Vec<_>>();
        let generics = if generic_names.is_empty() {
            String::new()
        } else {
            format!("<{}>", generic_names.join(", "))
        };
        if self.variadic() {
            source.push_str("#[variadic]\n");
        }
        source.push_str(&format!(
            "pub fn {name}{generics}({}) -> {result} {{}}\n",
            parameters.join(", ")
        ));
        Ok(source)
    }

    fn metadata(&self) -> syn::Result<proc_macro2::TokenStream> {
        let path = self.path.to_token_stream().to_string().replace(' ', "");
        let capability = self
            .module_segments()
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        let backend = if capability.starts_with("std::") {
            quote!(crate::BuiltinBackend::Host(#capability))
        } else {
            quote!(crate::BuiltinBackend::Runtime)
        };
        let docs = super::documentation(&self.function.attrs);
        let any = self
            .any_parameters()?
            .into_iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let parameters = if self.variadic() {
            Vec::new()
        } else {
            self.parameters()?
                .iter()
                .map(|(name, ty)| {
                    if any.contains(name) {
                        Ok(quote!(TypePattern::Unknown))
                    } else {
                        type_patterns::tokens(ty)
                    }
                })
                .collect::<syn::Result<Vec<_>>>()?
        };
        let variadic = self.variadic();
        let result = type_patterns::tokens(&self.result()?)?;
        let type_parameters = self
            .function
            .sig
            .generics
            .type_params()
            .map(|parameter| parameter.ident.to_string())
            .collect::<Vec<_>>();
        let native_symbol = if self.native_callback {
            quote!(Some(#path))
        } else {
            quote!(None)
        };
        Ok(quote! {
            use crate::TypePattern;
            pub const DECLARATION: crate::BuiltinDeclaration = crate::BuiltinDeclaration {
                path: #path,
                kind: crate::BuiltinKind::Function,
                supertraits: &[],
                type_parameters: &[#(#type_parameters),*],
                members: &[],
                signature: Some(crate::BuiltinSignature {
                    parameters: &[#(#parameters),*],
                    result: #result,
                    variadic: #variadic,
                }),
                native_symbol: #native_symbol,
                backend: #backend,
                documentation: #docs,
            };
        })
    }
}

pub(crate) fn expand_native(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    match input.validate().and_then(|()| native::tokens(&input)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

pub(crate) fn expand_metadata(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    match input.validate().and_then(|()| input.metadata()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

pub(crate) fn expand_source(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    match input.validate().and_then(|()| input.source()) {
        Ok(source) => quote!(#source).into(),
        Err(error) => error.into_compile_error().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_function_resolves_proxy_paths() {
        let input = Input {
            path: syn::parse_quote!(std::fs::read_to_string),
            native_callback: false,
            shadow_callback: false,
            function: syn::parse_quote! {
                /// Reads a file.
                pub fn read_to_string(path: String) -> Result<String, crate::stdlib::io::Error> {
                    loop {}
                }
            },
        };
        input.validate().unwrap();
        let metadata = input.metadata().unwrap().to_string();
        assert!(metadata.contains("std::fs::read_to_string"));
        assert!(metadata.contains("std::io::Error"));
        let source = input.source().unwrap();
        assert!(source.contains("Result < string , std :: io :: Error >"));
    }

    #[test]
    fn exported_function_needs_a_public_body() {
        let input = Input {
            path: syn::parse_quote!(std::fs::write),
            native_callback: false,
            shadow_callback: false,
            function: syn::parse_quote!(
                pub fn write() {}
            ),
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn explicit_any_and_variadic_markers_change_only_rils_signatures() {
        let write = Input {
            path: syn::parse_quote!(std::io::write),
            native_callback: false,
            shadow_callback: false,
            function: syn::parse_quote! {
                #[rils_any(value)]
                pub fn write(value: String) -> Result<(), crate::stdlib::io::Error> { loop {} }
            },
        };
        write.validate().unwrap();
        assert!(write.source().unwrap().contains("value: _"));
        assert!(
            write
                .metadata()
                .unwrap()
                .to_string()
                .contains("TypePattern :: Unknown")
        );

        let print = Input {
            path: syn::parse_quote!(std::io::print),
            native_callback: false,
            shadow_callback: false,
            function: syn::parse_quote! {
                #[rils_variadic]
                pub fn print(values: &[String]) { loop {} }
            },
        };
        print.validate().unwrap();
        assert!(
            print
                .source()
                .unwrap()
                .contains("#[variadic]\npub fn print()")
        );
        assert!(
            print
                .metadata()
                .unwrap()
                .to_string()
                .contains("variadic : true")
        );
    }

    #[test]
    fn fallible_rust_callbacks_generate_one_rils_signature() {
        let input: Input = syn::parse_quote! {
            core::ops::chain;
            pub fn chain<T, U, V, E, F, G>(first: F, value: T, second: G) -> std::result::Result<V, E>
            where
                F: FnOnce(T) -> std::result::Result<U, E>,
                G: FnMut(U) -> std::result::Result<V, E>,
            {
                second(first(value)?)
            }
        };
        input.validate().unwrap();
        assert!(input.native_callback);
        let source = input.source().unwrap();
        assert!(source.contains("pub fn chain<T, U, V>"));
        assert!(source.contains("first: fn (T) -> U"));
        assert!(source.contains("second: fn (U) -> V"));
        assert!(source.contains("-> V"));
        let metadata = input.metadata().unwrap().to_string();
        assert!(metadata.contains("native_symbol : Some"));
        assert!(
            native::tokens(&input)
                .unwrap()
                .to_string()
                .contains("stdlib :: ops :: chain")
        );
    }

    #[test]
    fn callback_exports_reject_unrepresented_rust_bounds() {
        let error = syn::parse2::<Input>(quote! {
            core::ops::run;
            pub fn run<T, E, F>(value: T, callback: F) -> std::result::Result<T, E>
            where
                T: Clone,
                F: FnOnce(T) -> std::result::Result<T, E>,
            {
                callback(value)
            }
        })
        .err()
        .expect("Rust-only bound must not disappear from the Rils signature");
        assert!(error.to_string().contains("callback exports only support"));
    }

    #[test]
    fn method_callback_bound_exports_without_a_wrapper() {
        let method: syn::ImplItemFn = syn::parse_quote! {
            #[export_rils]
            pub fn map<U, E, F>(self, callback: F) -> std::result::Result<Option<U>, E>
            where
                F: FnOnce(T) -> std::result::Result<U, E>,
            {
                match self { Self::Some(value) => callback(value).map(Option::Some), Self::None => Ok(Option::None) }
            }
        };
        let exposed = callback_signature::expose_method(&method).unwrap();
        assert_eq!(exposed.sig.generics.type_params().count(), 1);
        assert!(exposed.sig.generics.where_clause.is_none());
        assert!(
            exposed
                .sig
                .to_token_stream()
                .to_string()
                .contains("callback : fn (T) -> U")
        );
        assert!(
            exposed
                .sig
                .to_token_stream()
                .to_string()
                .contains("-> Option < U >")
        );
        assert!(
            method
                .sig
                .to_token_stream()
                .to_string()
                .contains("Result < Option < U > , E >")
        );
    }

    #[test]
    fn plain_callback_body_generates_hidden_error_path() {
        let function: ItemFn = syn::parse_quote! {
            pub fn apply_twice<T, F>(value: T, mut callback: F) -> T
            where F: FnMut(T) -> T
            {
                let next = callback(value);
                callback(next)
            }
        };
        let (exposed, native_callback, shadow_callback) =
            callback_signature::expose(function.clone()).unwrap();
        assert!(native_callback && shadow_callback);
        assert!(
            exposed
                .sig
                .to_token_stream()
                .to_string()
                .contains("callback : fn (T) -> T")
        );
        let shadow = callback_signature::shadow_function(&function)
            .unwrap()
            .unwrap();
        assert_eq!(shadow.sig.ident, "__rils_try_apply_twice");
        let generated = shadow.to_token_stream().to_string();
        assert!(generated.contains("std :: result :: Result"));
        assert!(generated.contains("callback (value)) ?"));

        let unsupported: ItemFn = syn::parse_quote! {
            pub fn escape<T, F>(value: T, callback: F) -> T
            where F: FnOnce(T) -> T
            {
                return callback(value);
            }
        };
        assert!(callback_signature::shadow_function(&unsupported).is_err());

        let indirect: ItemFn = syn::parse_quote! {
            pub fn alias<T, F>(value: T, callback: F) -> T
            where F: FnOnce(T) -> T
            {
                let renamed = callback;
                renamed(value)
            }
        };
        let error = match callback_signature::shadow_function(&indirect) {
            Err(error) => error,
            Ok(_) => panic!("indirect callback calls must fail"),
        };
        assert!(error.to_string().contains("called directly"));
    }
}
