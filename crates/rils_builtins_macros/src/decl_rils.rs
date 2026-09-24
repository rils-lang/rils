use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{ToTokens, quote};
use syn::{
    Error, FnArg, ImplItem, ImplItemFn, Item, ItemEnum, ItemImpl, ItemMod, Meta, Path, ReturnType,
    Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

use crate::type_patterns;

mod export_module;
mod primitive;
mod string;
mod structure;
pub(crate) mod trait_definition;
mod trait_impls;

struct Definition {
    module: Path,
    item: ItemEnum,
    methods: Vec<ImplItemFn>,
    traits: Vec<Path>,
    trait_impls: Vec<trait_impls::ConditionalImpl>,
}

struct DefinitionInput {
    path: Path,
    item: ItemMod,
}

impl Parse for DefinitionInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let path = input.parse()?;
        input.parse::<Token![;]>()?;
        let item = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after module"));
        }
        Ok(Self { path, item })
    }
}

impl Definition {
    fn parse(path: Path, module: &ItemMod) -> syn::Result<Self> {
        let (_, items) = module
            .content
            .as_ref()
            .ok_or_else(|| Error::new_spanned(module, "standard-library module must be inline"))?;
        let mut enums = items.iter().filter_map(|item| match item {
            Item::Enum(item) => Some(item.clone()),
            _ => None,
        });
        let item = enums
            .next()
            .ok_or_else(|| Error::new_spanned(module, "expected an enum"))?;
        if enums.next().is_some() {
            return Err(Error::new_spanned(module, "expected exactly one enum"));
        }
        let traits = trait_impls::parse(&item.attrs)?;
        if !traits.is_empty() && !item.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &item,
                "generic types need a marked trait impl",
            ));
        }
        let mut methods = Vec::new();
        let mut trait_impls = Vec::new();
        for implementation in items.iter().filter_map(|item| match item {
            Item::Impl(item) => Some(item),
            _ => None,
        }) {
            if implementation.trait_.is_some() {
                if let Some(parsed) = trait_impls::parse_impl(
                    implementation,
                    &item.ident,
                    &item.generics,
                    item.attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("rils_enum")),
                )? {
                    trait_impls.push(parsed);
                }
                if implementation.items.iter().any(|item| {
                    matches!(item, ImplItem::Fn(method) if method.attrs.iter().any(|attr| attr.path().is_ident("export_rils")))
                }) {
                    return Err(Error::new_spanned(
                        implementation,
                        "#[export_rils] requires an inherent implementation",
                    ));
                }
                continue;
            }
            Self::check_impl(&item, implementation)?;
            for member in &implementation.items {
                let ImplItem::Fn(method) = member else {
                    continue;
                };
                let Some(export_attribute) = method
                    .attrs
                    .iter()
                    .find(|attr| attr.path().is_ident("export_rils"))
                else {
                    continue;
                };
                if !matches!(export_attribute.meta, Meta::Path(_)) {
                    return Err(Error::new_spanned(
                        export_attribute,
                        "#[export_rils] does not take arguments",
                    ));
                }
                if method.block.stmts.is_empty() {
                    return Err(Error::new_spanned(
                        method,
                        "native method needs a Rust body",
                    ));
                }
                methods.push(method.clone());
            }
        }
        if methods.is_empty()
            && !item
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("rils_enum"))
        {
            return Err(Error::new_spanned(
                module,
                "definition needs a #[export_rils] method",
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for name in traits
            .iter()
            .map(|path| path.segments[0].ident.to_string())
            .chain(trait_impls.iter().map(|item| item.trait_name.clone()))
        {
            if !seen.insert(name.clone()) {
                return Err(Error::new_spanned(
                    module,
                    format!("duplicate {name} trait marker"),
                ));
            }
        }
        if seen.contains("Copy") && !seen.contains("Clone") {
            return Err(Error::new_spanned(
                module,
                "Copy requires a marked Clone implementation",
            ));
        }
        Ok(Self {
            module: path,
            item,
            methods,
            traits,
            trait_impls,
        })
    }

    fn check_impl(item: &ItemEnum, implementation: &ItemImpl) -> syn::Result<()> {
        let Type::Path(target) = implementation.self_ty.as_ref() else {
            return Err(Error::new_spanned(
                &implementation.self_ty,
                "expected enum impl target",
            ));
        };
        if target
            .path
            .segments
            .last()
            .is_none_or(|part| part.ident != item.ident)
            || implementation.generics.type_params().count() != item.generics.type_params().count()
        {
            return Err(Error::new_spanned(
                target,
                "impl must target the declared enum",
            ));
        }
        Ok(())
    }
}

pub(crate) fn expand_definition(attribute: TokenStream, item: TokenStream) -> TokenStream {
    let path = match syn::parse::<Path>(attribute) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let original = match syn::parse::<ItemMod>(item) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    if primitive::contains_mapping(&original) {
        if export_module::has_export_markers(&original) {
            return Error::new_spanned(
                &original,
                "primitive family cannot share a Rils declaration module",
            )
            .into_compile_error()
            .into();
        }
        return primitive::expand_definition(path, original);
    }
    export_module::expand_definition(path, original)
}

fn rils_source(definition: &Definition) -> String {
    fn docs(attributes: &[syn::Attribute], indent: &str) -> String {
        documentation(attributes)
            .lines()
            .map(|line| format!("{indent}/// {line}\n"))
            .collect()
    }

    let item = &definition.item;
    let generic_names = item
        .generics
        .type_params()
        .map(|parameter| parameter.ident.to_string())
        .collect::<Vec<_>>();
    let generics = if generic_names.is_empty() {
        String::new()
    } else {
        format!("<{}>", generic_names.join(", "))
    };
    let mut source = docs(&item.attrs, "");
    source.push_str(&format!("pub enum {}{} {{\n", item.ident, generics));
    for variant in &item.variants {
        source.push_str(&docs(&variant.attrs, "    "));
        source.push_str(&format!("    {}", variant.ident));
        if let syn::Fields::Unnamed(fields) = &variant.fields {
            let field_types = fields
                .unnamed
                .iter()
                .map(|field| field.ty.to_token_stream().to_string())
                .collect::<Vec<_>>();
            source.push_str(&format!("({})", field_types.join(", ")));
        }
        source.push_str(",\n");
    }
    source.push_str("}\n\n");
    source.push_str(&format!("impl{} {}{} {{\n", generics, item.ident, generics));
    for method in &definition.methods {
        source.push_str(&docs(&method.attrs, "    "));
        let mut signature = method.sig.clone();
        signature.generics.where_clause = None;
        let signature = signature
            .to_token_stream()
            .to_string()
            .replace("String", "string");
        source.push_str(&format!("    {signature} {{}}\n\n"));
    }
    source.push_str("}\n");
    source
}

pub(crate) fn expand_metadata(input: TokenStream) -> TokenStream {
    let source = parse_macro_input!(input as DefinitionInput);
    if string::is_string(&source.path) {
        return string::expand_metadata(source.path, source.item);
    }
    if primitive::contains_mapping(&source.item) {
        return primitive::expand_metadata(source.path, source.item);
    }
    if structure::contains_struct(&source.item) {
        return structure::expand_metadata(source.path, source.item);
    }
    let definition = match Definition::parse(source.path, &source.item) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    match metadata_tokens(&definition) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

pub(crate) fn expand_source(input: TokenStream) -> TokenStream {
    let source = parse_macro_input!(input as DefinitionInput);
    if string::is_string(&source.path) {
        return string::expand_source(source.path, source.item);
    }
    if primitive::contains_mapping(&source.item) {
        return primitive::expand_source(source.path, source.item);
    }
    if structure::contains_struct(&source.item) {
        return structure::expand_source(source.path, source.item);
    }
    match Definition::parse(source.path, &source.item) {
        Ok(definition) => {
            let source = rils_source(&definition);
            quote!(#source).into()
        }
        Err(error) => error.into_compile_error().into(),
    }
}

pub(crate) fn expand_trait_impls(input: TokenStream) -> TokenStream {
    let source = parse_macro_input!(input as DefinitionInput);
    if string::is_string(&source.path) {
        return string::expand_trait_impls(source.path, source.item);
    }
    if primitive::contains_mapping(&source.item) {
        return primitive::expand_trait_impls(source.path, source.item);
    }
    if structure::contains_struct(&source.item) {
        return structure::expand_trait_impls(source.path, source.item);
    }
    let definition = match Definition::parse(source.path, &source.item) {
        Ok(definition) => definition,
        Err(error) => return error.into_compile_error().into(),
    };
    let type_name = definition.item.ident.to_string();
    let direct = definition.traits.iter().map(|path| {
        let trait_name = path.segments[0].ident.to_string();
        quote!(crate::BuiltinTraitImpl { type_name: #type_name, trait_name: #trait_name, requirements: &[] })
    });
    let conditional = definition.trait_impls.iter().map(|implementation| {
        let trait_name = &implementation.trait_name;
        let requirements = implementation.requirements.iter().map(|(parameter, bound)| quote!((#parameter, #bound)));
        quote!(crate::BuiltinTraitImpl { type_name: #type_name, trait_name: #trait_name, requirements: &[#(#requirements),*] })
    });
    quote!(pub const TRAIT_IMPLS: &[crate::BuiltinTraitImpl] = &[#(#direct,)* #(#conditional),*];)
        .into()
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
                    native_symbol: None,
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
            if !matches!(method.vis, syn::Visibility::Public(_)) {
                return Err(Error::new_spanned(
                    &method.sig,
                    "standard-library methods must be public",
                ));
            }
            let name = method.sig.ident.to_string();
            let documentation = documentation(&method.attrs);
            let id_path = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
            let direct_native = supports_direct_bridge(&definition.item, method);
            let native_symbol = if direct_native {
                quote!(Some(#id_path))
            } else {
                quote!(None)
            };
            let builtin_id = if direct_native {
                quote!(None)
            } else {
                quote!(Some(legacy_builtin_id!(#id_path)))
            };
            let receiver = method.sig.receiver().ok_or_else(|| {
                Error::new_spanned(&method.sig, "native methods require a receiver")
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
                .sig
                .inputs
                .iter()
                .skip(1)
                .map(|input| match input {
                    FnArg::Typed(parameter) => type_patterns::tokens(&parameter.ty),
                    _ => Err(Error::new_spanned(input, "unexpected receiver")),
                })
                .collect::<syn::Result<Vec<_>>>()?;
            let result = match &method.sig.output {
                ReturnType::Default => quote!(TypePattern::Unit),
                ReturnType::Type(_, ty) => type_patterns::tokens(ty)?,
            };
            let type_parameters = method
                .sig
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
                    builtin_id: #builtin_id,
                    runtime_import: None,
                    native_symbol: #native_symbol,
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
            supertraits: &[],
            type_parameters: &[#(#type_generics),*],
            members: &[#(#variants,)* #(#methods),*],
            signature: None,
            native_symbol: None,
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

fn supports_direct_bridge(item: &ItemEnum, method: &ImplItemFn) -> bool {
    if !matches!(item.ident.to_string().as_str(), "Option" | "Result")
        || method.sig.inputs.len() != 1
    {
        return false;
    }
    let Some(receiver) = method.sig.receiver() else {
        return false;
    };
    let ReturnType::Type(_, result) = &method.sig.output else {
        return false;
    };
    if receiver.reference.is_some() {
        return receiver.mutability.is_none()
            && matches!(result.as_ref(), Type::Path(path) if path.path.is_ident("bool"));
    }
    if item.ident != "Result" {
        return false;
    }
    let Type::Path(path) = result.as_ref() else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident != "Option" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return false;
    };
    matches!(arguments.args.first(), Some(syn::GenericArgument::Type(Type::Path(path))) if path.path.is_ident("T") || path.path.is_ident("E"))
}

pub(crate) fn expand_native(input: TokenStream) -> TokenStream {
    let source = parse_macro_input!(input as DefinitionInput);
    if string::is_string(&source.path) {
        return string::expand_native(source.path, source.item);
    }
    if primitive::contains_mapping(&source.item) {
        return primitive::expand_native(source.path, source.item);
    }
    if structure::contains_struct(&source.item) {
        return structure::expand_native(source.path, source.item);
    }
    let definition = match Definition::parse(source.path, &source.item) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    match native_tokens(&definition) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn native_tokens(definition: &Definition) -> syn::Result<Tokens> {
    if definition.item.ident != "Option" && definition.item.ident != "Result" {
        return Err(Error::new_spanned(
            &definition.item.ident,
            "native bridge supports Option and Result",
        ));
    }
    let module = &definition.module;
    let implementations = definition.methods.iter().map(|method| {
        let name = &method.sig.ident;
        let id_path = format!("{}::{name}", quote!(#module).to_string().replace(' ', ""));
        if !supports_direct_bridge(&definition.item, method) {
            let arity = method.sig.inputs.len();
            return Ok(quote! {
                #id_path => Some(
                    if arguments.len() == #arity {
                        super::super::option_result::call(
                            rils_builtins::legacy_builtin_id!(#id_path),
                            arguments,
                        )
                    } else {
                        Err(format!("native method expects {} arguments, found {}", #arity, arguments.len()))
                    }
                )
            });
        }
        if method.sig.inputs.len() != 1 {
            return Err(Error::new_spanned(&method.sig, "native bridge supports methods without arguments"));
        }
        let receiver = method.sig.receiver().ok_or_else(|| Error::new_spanned(&method.sig, "native method needs a receiver"))?;
        let is_shared = receiver.reference.is_some() && receiver.mutability.is_none();
        let is_owned = receiver.reference.is_none();
        let is_bool = matches!(method.sig.output, ReturnType::Type(_, ref ty) if matches!(ty.as_ref(), Type::Path(path) if path.path.is_ident("bool")));
        let result_code = if is_bool && is_shared {
            quote! {
                let result: bool = native_self.#name();
                Ok(crate::Value::Bool(result))
            }
        } else if definition.item.ident == "Result" && is_owned {
            let ReturnType::Type(_, ty) = &method.sig.output else {
                return Err(Error::new_spanned(&method.sig.output, "expected Option<T> or Option<E> result"));
            };
            let Type::Path(result_type) = ty.as_ref() else {
                return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result"));
            };
            let Some(segment) = result_type.path.segments.last() else {
                return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result"));
            };
            if segment.ident != "Option" {
                return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result"));
            }
            let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result"));
            };
            let element_type = match arguments.args.first() {
                Some(syn::GenericArgument::Type(Type::Path(path))) if path.path.is_ident("T") => quote!(ok_type),
                Some(syn::GenericArgument::Type(Type::Path(path))) if path.path.is_ident("E") => quote!(error_type),
                _ => return Err(Error::new_spanned(ty, "expected Option<T> or Option<E> result")),
            };
            quote! {
                let result: rils_stdlib::stdlib::option::Option<_> = native_self.#name();
                let value = match result {
                    rils_stdlib::stdlib::option::Option::Some(value) => Some(value),
                    rils_stdlib::stdlib::option::Option::None => None,
                };
                Ok(crate::Value::Option { value, element_type: #element_type })
            }
        } else {
            return Err(Error::new_spanned(&method.sig, "native bridge supports shared bool methods and owned Result to Option methods"));
        };
        let conversion = if definition.item.ident == "Option" {
            quote! {
                let native_self = match receiver {
                    crate::Value::Option { value: Some(value), .. } => rils_stdlib::stdlib::option::Option::Some(value),
                    crate::Value::Option { value: None, .. } => rils_stdlib::stdlib::option::Option::None,
                    value => return Err(format!("`{}` expects Option, found {}", stringify!(#name), value.type_name())),
                };
            }
        } else {
            quote! {
                let (native_self, ok_type, error_type) = match receiver {
                    crate::Value::Result { value: Ok(value), ok_type, error_type } =>
                        (rils_stdlib::stdlib::result::Result::Ok(value), ok_type, error_type),
                    crate::Value::Result { value: Err(value), ok_type, error_type } =>
                        (rils_stdlib::stdlib::result::Result::Err(value), ok_type, error_type),
                    value => return Err(format!("`{}` expects Result, found {}", stringify!(#name), value.type_name())),
                };
                let _ = (&ok_type, &error_type);
            }
        };
        Ok(quote! {
            #id_path => Some((|| -> Result<crate::Value, String> {
                if arguments.len() != 1 {
                    return Err(format!("native method expects one receiver, found {} arguments", arguments.len()));
                }
                let receiver = arguments.first().ok_or_else(|| "missing native receiver".to_owned())?;
                let receiver = match receiver {
                    crate::Value::Reference(reference) => reference.read()?,
                    value => value.clone(),
                };
                #conversion
                #result_code
            })())
        })
    }).collect::<syn::Result<Vec<_>>>()?;
    Ok(quote! {
        pub fn call_symbol(
            symbol: &str,
            arguments: &[crate::Value],
        ) -> Option<Result<crate::Value, String>> {
            match symbol { #(#implementations,)* _ => None }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_bridge_eligibility_follows_the_method_signature() {
        let option: ItemEnum = syn::parse_quote!(
            pub enum Option<T> {
                Some(T),
                None,
            }
        );
        let result: ItemEnum = syn::parse_quote!(
            pub enum Result<T, E> {
                Ok(T),
                Err(E),
            }
        );
        let state_query: ImplItemFn = syn::parse_quote!(
            #[export_rils]
            pub fn is_some(&self) -> bool {
                true
            }
        );
        let extraction: ImplItemFn = syn::parse_quote!(
            #[export_rils]
            pub fn ok(self) -> Option<T> {
                loop {}
            }
        );
        let mutable: ImplItemFn = syn::parse_quote!(
            #[export_rils]
            pub fn take(&mut self) -> Self {
                loop {}
            }
        );
        let generic: ImplItemFn = syn::parse_quote!(
            #[export_rils]
            pub fn unwrap(self) -> T {
                loop {}
            }
        );
        assert!(supports_direct_bridge(&option, &state_query));
        assert!(supports_direct_bridge(&result, &state_query));
        assert!(supports_direct_bridge(&result, &extraction));
        assert!(!supports_direct_bridge(&option, &mutable));
        assert!(!supports_direct_bridge(&result, &generic));

        let module: ItemMod = syn::parse_quote! {
            mod native {
                pub enum Option<T> { Some(T), None }
                impl<T> Option<T> {
                    #[export_rils]
                    pub fn take(&mut self) -> Self { loop {} }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::option), &module).unwrap();
        assert!(
            metadata_tokens(&definition)
                .unwrap()
                .to_string()
                .contains("legacy_builtin_id")
        );
    }

    #[test]
    fn parses_native_definition_and_rejects_placeholder_body() {
        let valid: Tokens = r#"
            core::option;
            pub mod option {
                pub enum Option<T> { Some(T), None }
                impl<T> Option<T> {
                    #[export_rils]
                    pub fn is_some(&self) -> bool {
                        self.has_value()
                    }
                    fn has_value(&self) -> bool { matches!(self, Self::Some(_)) }
                }
            }
        "#
        .parse()
        .unwrap();
        let input: DefinitionInput = syn::parse2(valid).unwrap();
        let definition = Definition::parse(input.path, &input.item).unwrap();
        assert_eq!(definition.methods.len(), 1);
        assert_eq!(definition.methods[0].sig.ident, "is_some");
        assert!(metadata_tokens(&definition).is_ok());
        assert!(native_tokens(&definition).is_ok());

        let placeholder = quote! {
            core::option;
            pub mod option {
                pub enum Option<T> { Some(T), None }
                impl<T> Option<T> { #[export_rils] pub fn is_some(&self) -> bool {} }
            }
        };
        let input: DefinitionInput = syn::parse2(placeholder).unwrap();
        assert!(Definition::parse(input.path, &input.item).is_err());
    }

    #[test]
    fn helper_trait_impl_does_not_become_an_inherent_rils_method() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                pub enum Option<T> { Some(T), None }
                impl<T> Option<T> {
                    #[export_rils]
                    pub fn is_some(&self) -> bool { true }
                }
                impl<T> Clone for Option<T> {
                    fn clone(&self) -> Self { Self::None }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::option), &module).unwrap();
        assert_eq!(definition.methods.len(), 1);
        assert!(!rils_source(&definition).contains("fn clone"));
    }

    #[test]
    fn definition_without_exported_method_is_rejected() {
        let source = quote! {
            core::option;
            pub mod option {
                pub enum Option<T> { Some(T), None }
                impl<T> Option<T> { pub fn is_some(&self) -> bool { true } }
            }
        };
        let input: DefinitionInput = syn::parse2(source).unwrap();
        assert!(
            Definition::parse(input.path, &input.item)
                .err()
                .unwrap()
                .to_string()
                .contains("#[export_rils]")
        );
    }
}
