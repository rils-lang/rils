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
pub(crate) mod function_definition;
mod primitive;
mod string;
mod sum;
use sum::{native_tokens, supports_direct_bridge, uses_sum_adapter};
pub(crate) mod structure;
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
                methods.push(function_definition::callback_signature::expose_method(
                    method,
                )?);
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
    let (path, backend) = declaration_identity(module, &definition.item.ident.to_string());
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
                    trait_name: None,
                    signature: None,
                    value_type: Some(#value_type),
                    receiver: None,
                    native_symbol: None,
                    native_bridge: false,
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
            if !supports_direct_bridge(&definition.item, method) {
                return Err(Error::new_spanned(
                    &method.sig,
                    "exported method signature has no native conversion; implement its bridge",
                ));
            }
            let native_symbol = quote!(Some(#id_path));
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
            let native_bridge = uses_sum_adapter(&definition.item, method);
            Ok(quote! {
                crate::BuiltinMember {
                    name: #name,
                    kind: crate::BuiltinMemberKind::Method,
                    trait_name: None,
                    signature: Some(crate::BuiltinSignature {
                        parameters: &[#(#parameters),*],
                        result: #result,
                        variadic: false,
                    }),
                    value_type: None,
                    receiver: Some(#receiver_mode),
                    native_symbol: #native_symbol,
                    native_bridge: #native_bridge,
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
            opaque_native: false,
            source: None,
            supertraits: &[],
            type_parameters: &[#(#type_generics),*],
            members: &[#(#variants,)* #(#methods),*],
            signature: None,
            native_symbol: None,
            backend: #backend,
            documentation: #type_documentation,
        };
        pub const DECLARATIONS: &[crate::BuiltinDeclaration] = &[DECLARATION];
    })
}

fn declaration_identity(module: &Path, name: &str) -> (String, Tokens) {
    let segments = module
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    if segments.first().is_some_and(|segment| segment == "std") {
        let capability = segments[..segments.len() - 1].join("::");
        (
            format!("{capability}::{name}"),
            quote!(crate::BuiltinBackend::Host(#capability)),
        )
    } else {
        (name.to_owned(), quote!(crate::BuiltinBackend::Runtime))
    }
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

pub(crate) fn expand_layout(input: TokenStream) -> TokenStream {
    let source = parse_macro_input!(input as DefinitionInput);
    if string::is_string(&source.path) {
        return string::expand_layout(source.path, source.item);
    }
    if primitive::contains_mapping(&source.item) {
        return primitive::expand_layout(source.path, source.item);
    }
    if structure::contains_struct(&source.item) {
        return structure::expand_layout(source.path, source.item);
    }
    let definition = match Definition::parse(source.path, &source.item) {
        Ok(definition) => definition,
        Err(error) => return error.into_compile_error().into(),
    };
    if definition.item.ident != "Option" {
        return Error::new_spanned(
            &definition.item,
            "dynamic native layout generation currently supports Option<T>",
        )
        .into_compile_error()
        .into();
    }
    let variants = &definition.item.variants;
    if variants.len() != 2
        || variants[0].ident != "None"
        || !matches!(variants[0].fields, syn::Fields::Unit)
        || variants[1].ident != "Some"
        || !matches!(&variants[1].fields, syn::Fields::Unnamed(fields)
            if fields.unnamed.len() == 1
                && matches!(&fields.unnamed[0].ty, Type::Path(path) if path.path.is_ident("T")))
    {
        return Error::new_spanned(
            &definition.item,
            "dynamic Option layout requires exactly None and Some(T)",
        )
        .into_compile_error()
        .into();
    }
    quote! {
        pub fn layout(
            item: std::rc::Rc<rils_value::DynamicLayout>,
        ) -> Result<std::rc::Rc<rils_value::DynamicLayout>, String> {
            rils_value::DynamicLayout::option(item)
        }
    }
    .into()
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
        let callback: ImplItemFn = syn::parse_quote!(
            #[export_rils]
            pub fn map<U>(self, transform: fn(T) -> U) -> Option<U> {
                loop {}
            }
        );
        assert!(supports_direct_bridge(&option, &state_query));
        assert!(supports_direct_bridge(&result, &state_query));
        assert!(supports_direct_bridge(&result, &extraction));
        assert!(supports_direct_bridge(&option, &mutable));
        assert!(supports_direct_bridge(&result, &generic));
        assert!(supports_direct_bridge(&option, &callback));
        let unsupported: ImplItemFn = syn::parse_quote!(
            #[export_rils]
            pub fn unknown(self) -> T {
                loop {}
            }
        );
        assert!(supports_direct_bridge(&option, &unsupported));

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
        assert!(metadata_tokens(&definition).is_ok());

        let module: ItemMod = syn::parse_quote! {
            mod native {
                pub enum Option<T> { Some(T), None }
                impl<T> Option<T> {
                    #[export_rils]
                    pub fn extract_or(self, fallback: T) -> T { loop {} }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::option), &module).unwrap();
        let generated = native_tokens(&definition).unwrap().to_string();
        assert!(generated.contains("native_self . extract_or (argument_1)"));
        assert!(!generated.contains("option_result"));
        assert!(!generated.contains("materialize_native_sum"));
    }

    #[test]
    fn renamed_callbacks_generate_signature_driven_owned_calls() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                pub enum Option<T> { Some(T), None }
                impl<T> Option<T> {
                    #[export_rils]
                    pub fn transform<U, F>(self, callback: F) -> Option<U>
                    where F: FnOnce(T) -> U {
                        match self {
                            Self::Some(value) => Option::Some(callback(value)),
                            Self::None => Option::None,
                        }
                    }
                    #[export_rils]
                    pub fn select<F>(self, callback: F) -> Self
                    where F: FnOnce(&T) -> bool { loop {} }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::option), &module).unwrap();
        let generated = native_tokens(&definition).unwrap().to_string();
        assert!(generated.contains("native_self . __rils_try_transform"));
        assert!(generated.contains("native_self . __rils_try_select"));
        assert!(generated.contains("shared_argument"));
        assert!(!generated.contains("Operation"));
        assert!(!generated.contains("materialize_native_sum"));
        assert!(!generated.contains("Value :: Option"));
        assert!(!generated.contains("Value :: Result"));
        assert!(!generated.contains("arguments ["));
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
