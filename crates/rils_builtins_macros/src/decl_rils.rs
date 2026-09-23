use proc_macro::TokenStream;
use proc_macro2::{Group, Ident, TokenStream as Tokens, TokenTree};
use quote::{format_ident, quote};
use syn::{
    Error, FnArg, ItemEnum, Path, ReturnType, Signature, Token, Type, braced,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

use crate::type_patterns;

struct Method {
    attributes: Vec<syn::Attribute>,
    _visibility: syn::Visibility,
    signature: Signature,
    body: Tokens,
}

impl Parse for Method {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attributes = input.call(syn::Attribute::parse_outer)?;
        let visibility = input.parse()?;
        let signature = input.parse()?;
        let contents;
        braced!(contents in input);
        contents.parse::<Token![#]>()?;
        let marker: Ident = contents.parse()?;
        if marker != "rils" {
            return Err(Error::new(marker.span(), "expected `#rils { ... }`"));
        }
        let body_contents;
        braced!(body_contents in contents);
        let body = body_contents.parse()?;
        if !contents.is_empty() {
            return Err(contents.error("unexpected tokens after `#rils` body"));
        }
        Ok(Self {
            attributes,
            _visibility: visibility,
            signature,
            body,
        })
    }
}

struct Definition {
    module: Path,
    item: ItemEnum,
    methods: Vec<Method>,
}

impl Parse for Definition {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Token![pub]>()?;
        input.parse::<Token![mod]>()?;
        let module = input.parse()?;
        input.parse::<Token![;]>()?;
        let item: ItemEnum = input.parse()?;
        input.parse::<Token![impl]>()?;
        let generics: syn::Generics = input.parse()?;
        let target: Type = input.parse()?;
        let Type::Path(target) = &target else {
            return Err(Error::new_spanned(target, "expected enum impl target"));
        };
        if target
            .path
            .segments
            .last()
            .is_none_or(|part| part.ident != item.ident)
            || generics.type_params().count() != item.generics.type_params().count()
        {
            return Err(Error::new_spanned(
                target,
                "impl must target the declared enum",
            ));
        }
        let contents;
        braced!(contents in input);
        let mut methods = Vec::new();
        while !contents.is_empty() {
            methods.push(contents.parse()?);
        }
        if methods.is_empty() {
            return Err(Error::new_spanned(
                item,
                "definition needs an implemented method",
            ));
        }
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after enum impl"));
        }
        Ok(Self {
            module,
            item,
            methods,
        })
    }
}

pub(crate) fn expand_definition(input: TokenStream) -> TokenStream {
    let source: Tokens = input.into();
    let definition = match syn::parse2::<Definition>(source.clone()) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let macro_name = format_ident!(
        "{}_definition",
        definition.item.ident.to_string().to_lowercase()
    );
    quote! {
        #[macro_export]
        macro_rules! #macro_name {
            ($emit:ident) => { $emit! { #source } };
        }
    }
    .into()
}

pub(crate) fn expand_metadata(input: TokenStream) -> TokenStream {
    let definition = parse_macro_input!(input as Definition);
    match metadata_tokens(&definition) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn metadata_tokens(definition: &Definition) -> syn::Result<Tokens> {
    let module = &definition.module;
    let path = definition.item.ident.to_string();
    let type_documentation = documentation(&definition.item.attrs);
    let type_generics = definition
        .item
        .generics
        .type_params()
        .map(|parameter| parameter.ident.to_string())
        .collect::<Vec<_>>();
    let variants = definition
        .item
        .variants
        .iter()
        .map(|variant| {
            let name = variant.ident.to_string();
            let documentation = documentation(&variant.attrs);
            let value_type = match &variant.fields {
                syn::Fields::Unit => quote!(TypePattern::Unit),
                syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    type_patterns::tokens(&fields.unnamed.first().expect("one field").ty)?
                }
                _ => {
                    return Err(Error::new_spanned(
                        variant,
                        "only unit and single-field tuple variants are supported",
                    ));
                }
            };
            Ok(quote! {
                crate::BuiltinMember {
                    name: #name,
                    kind: crate::BuiltinMemberKind::Variant,
                    signature: None,
                    value_type: Some(#value_type),
                    receiver: None,
                    builtin_id: None,
                    runtime_import: None,
                    required: false,
                    type_parameters: &[],
                    documentation: #documentation,
                }
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let methods = definition
        .methods
        .iter()
        .map(|method| {
            if !matches!(method._visibility, syn::Visibility::Public(_)) {
                return Err(Error::new_spanned(
                    &method.signature,
                    "standard-library methods must be public",
                ));
            }
            let name = method.signature.ident.to_string();
            let documentation = documentation(&method.attributes);
            let id_path = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
            let receiver = method.signature.receiver().ok_or_else(|| {
                Error::new_spanned(&method.signature, "native methods require a receiver")
            })?;
            let receiver_mode = if receiver.reference.is_some() {
                if receiver.mutability.is_some() {
                    quote!(crate::ReceiverMode::Mutable)
                } else {
                    quote!(crate::ReceiverMode::Shared)
                }
            } else {
                quote!(crate::ReceiverMode::Owned)
            };
            let parameters = method
                .signature
                .inputs
                .iter()
                .skip(1)
                .map(|input| match input {
                    FnArg::Typed(parameter) => type_patterns::tokens(&parameter.ty),
                    _ => Err(Error::new_spanned(input, "unexpected receiver")),
                })
                .collect::<syn::Result<Vec<_>>>()?;
            let result = match &method.signature.output {
                ReturnType::Default => quote!(TypePattern::Unit),
                ReturnType::Type(_, ty) => type_patterns::tokens(ty)?,
            };
            let type_parameters = method
                .signature
                .generics
                .type_params()
                .map(|parameter| parameter.ident.to_string())
                .collect::<Vec<_>>();
            Ok(quote! {
                crate::BuiltinMember {
                    name: #name,
                    kind: crate::BuiltinMemberKind::Method,
                    signature: Some(crate::BuiltinSignature {
                        parameters: &[#(#parameters),*],
                        result: #result,
                        variadic: false,
                    }),
                    value_type: None,
                    receiver: Some(#receiver_mode),
                    builtin_id: Some(builtin_id!(#id_path)),
                    runtime_import: None,
                    required: true,
                    type_parameters: &[#(#type_parameters),*],
                    documentation: #documentation,
                }
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote! {
        use crate::TypePattern;
        pub const DECLARATION: crate::BuiltinDeclaration = crate::BuiltinDeclaration {
            path: #path,
            kind: crate::BuiltinKind::Enum,
            type_parameters: &[#(#type_generics),*],
            members: &[#(#variants,)* #(#methods),*],
            signature: None,
            backend: crate::BuiltinBackend::Runtime,
            documentation: #type_documentation,
        };
        pub const DECLARATIONS: &[crate::BuiltinDeclaration] = &[DECLARATION];
    })
}

fn documentation(attributes: &[syn::Attribute]) -> String {
    attributes
        .iter()
        .filter_map(|attribute| {
            if !attribute.path().is_ident("doc") {
                return None;
            }
            let syn::Meta::NameValue(value) = &attribute.meta else {
                return None;
            };
            let syn::Expr::Lit(value) = &value.value else {
                return None;
            };
            let syn::Lit::Str(value) = &value.lit else {
                return None;
            };
            Some(value.value().trim().to_owned())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn expand_native(input: TokenStream) -> TokenStream {
    let definition = parse_macro_input!(input as Definition);
    match native_tokens(&definition) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn native_tokens(definition: &Definition) -> syn::Result<Tokens> {
    if definition.item.ident != "Option" {
        return Err(Error::new_spanned(
            &definition.item.ident,
            "the first native bridge supports Option",
        ));
    }
    let module = &definition.module;
    let wrapper = format_ident!("{}Wrapper", definition.item.ident);
    let arms = definition
        .item
        .variants
        .iter()
        .map(|variant| {
            let name = &variant.ident;
            match &variant.fields {
                syn::Fields::Unit => Ok(quote!(#name)),
                syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    Ok(quote!(#name(std::rc::Rc<crate::Value>)))
                }
                _ => Err(Error::new_spanned(
                    variant,
                    "only unit and single-field tuple variants are supported",
                )),
            }
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let implementations = definition.methods.iter().map(|method| {
        if !matches!(method.signature.output, ReturnType::Type(_, ref ty) if matches!(ty.as_ref(), Type::Path(path) if path.path.is_ident("bool"))) {
            return Err(Error::new_spanned(&method.signature.output, "the first native bridge supports bool results"));
        }
        if method.signature.inputs.len() != 1 || method.signature.receiver().is_none_or(|receiver| receiver.reference.is_none() || receiver.mutability.is_some()) {
            return Err(Error::new_spanned(&method.signature, "the first native bridge supports only &self methods without arguments"));
        }
        let name = &method.signature.ident;
        let id_path = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        let expression = replace_bindings(method.body.clone(), &definition.item.ident, &wrapper)?;
        let _: syn::Expr = syn::parse2(expression.clone())?;
        Ok(quote! {
            id if id == rils_builtins::builtin_id!(#id_path) => Some((|| -> Result<crate::Value, String> {
                if arguments.len() != 1 {
                    return Err(format!("native method expects one receiver, found {} arguments", arguments.len()));
                }
                let receiver = arguments.first().ok_or_else(|| "missing native receiver".to_owned())?;
                let receiver = match receiver {
                    crate::Value::Reference(reference) => reference.read()?,
                    value => value.clone(),
                };
                let __rils_self = match receiver {
                    crate::Value::Option { value: Some(value), .. } => #wrapper::Some(value),
                    crate::Value::Option { value: None, .. } => #wrapper::None,
                    value => return Err(format!("`{}` expects Option, found {}", stringify!(#name), value.type_name())),
                };
                let result: bool = { #expression };
                Ok(crate::Value::Bool(result))
            })())
        })
    }).collect::<syn::Result<Vec<_>>>()?;
    Ok(quote! {
        #[allow(dead_code)]
        enum #wrapper { #(#arms),* }
        pub fn call(
            id: rils_builtins::BuiltinId,
            arguments: &[crate::Value],
        ) -> Option<Result<crate::Value, String>> {
            match id { #(#implementations,)* _ => None }
        }
    })
}

fn replace_bindings(source: Tokens, enum_name: &Ident, wrapper: &Ident) -> syn::Result<Tokens> {
    let mut output = Tokens::new();
    let mut tokens = source.into_iter();
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Punct(punct) if punct.as_char() == '#' => {
                let Some(TokenTree::Ident(name)) = tokens.next() else {
                    return Err(Error::new(punct.span(), "expected name after `#`"));
                };
                if name == "self" {
                    output.extend(quote!(__rils_self));
                } else if name == *enum_name {
                    output.extend(quote!(#wrapper));
                } else {
                    return Err(Error::new(name.span(), "unknown `#rils` binding"));
                }
            }
            TokenTree::Group(group) => {
                let body = replace_bindings(group.stream(), enum_name, wrapper)?;
                output.extend([TokenTree::Group(Group::new(group.delimiter(), body))]);
            }
            token => output.extend([token]),
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_native_definition_and_rejects_placeholder_body() {
        let valid: Tokens = r#"
            pub mod core::option;
            pub enum Option<T> { Some(T), None }
            impl<T> Option<T> {
                pub fn is_some(&self) -> bool {
                    #rils { matches!(#self, #Option::Some(_)) }
                }
            }
        "#
        .parse()
        .unwrap();
        let definition: Definition = syn::parse2(valid).unwrap();
        assert_eq!(definition.methods[0].signature.ident, "is_some");
        assert!(metadata_tokens(&definition).is_ok());
        assert!(native_tokens(&definition).is_ok());

        let placeholder = quote! {
            pub mod core::option;
            pub enum Option<T> { Some(T), None }
            impl<T> Option<T> { pub fn is_some(&self) -> bool {} }
        };
        assert!(syn::parse2::<Definition>(placeholder).is_err());
    }

    #[test]
    fn unknown_native_binding_is_rejected() {
        let body = "#missing".parse().unwrap();
        let error = replace_bindings(
            body,
            &format_ident!("Option"),
            &format_ident!("OptionWrapper"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown"));
    }
}
