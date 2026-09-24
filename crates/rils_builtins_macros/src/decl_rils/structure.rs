//! Shared declaration generation for opaque Rust-backed Rils structs.

use proc_macro::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Error, FnArg, ImplItem, ImplItemFn, Item, ItemMod, ItemStruct, Path, ReturnType, Type};

use crate::type_patterns;

struct Definition {
    path: Path,
    item: ItemStruct,
    methods: Vec<ImplItemFn>,
    traits: Vec<Path>,
}

pub(super) fn contains_struct(module: &ItemMod) -> bool {
    module
        .content
        .as_ref()
        .is_some_and(|(_, items)| items.iter().any(|item| matches!(item, Item::Struct(_))))
}

impl Definition {
    fn parse(path: Path, module: ItemMod) -> syn::Result<Self> {
        let (_, items) = module
            .content
            .as_ref()
            .ok_or_else(|| Error::new_spanned(&module, "standard-library module must be inline"))?;
        let mut structs = items.iter().filter_map(|item| match item {
            Item::Struct(item) => Some(item.clone()),
            _ => None,
        });
        let item = structs
            .next()
            .ok_or_else(|| Error::new_spanned(&module, "expected a struct"))?;
        if structs.next().is_some() {
            return Err(Error::new_spanned(&module, "expected exactly one struct"));
        }
        if !item
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("rils_opaque"))
        {
            return Err(Error::new_spanned(
                &item,
                "native struct must use #[rils_opaque] until field projection is supported",
            ));
        }
        let traits = super::trait_impls::parse(&item.attrs)?;
        let mut methods = Vec::new();
        for implementation in items.iter().filter_map(|item| match item {
            Item::Impl(item) if item.trait_.is_none() => Some(item),
            _ => None,
        }) {
            let Type::Path(target) = implementation.self_ty.as_ref() else {
                continue;
            };
            if target
                .path
                .segments
                .last()
                .is_none_or(|part| part.ident != item.ident)
            {
                continue;
            }
            for member in &implementation.items {
                let ImplItem::Fn(method) = member else {
                    continue;
                };
                if !method
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("export_rils"))
                {
                    continue;
                }
                if !matches!(method.vis, syn::Visibility::Public(_))
                    || method.block.stmts.is_empty()
                {
                    return Err(Error::new_spanned(
                        method,
                        "exported struct member requires a public Rust body",
                    ));
                }
                methods.push(method.clone());
            }
        }
        if methods.is_empty() {
            return Err(Error::new_spanned(
                &module,
                "definition needs a #[export_rils] method",
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for method in &methods {
            if !names.insert(method.sig.ident.to_string()) {
                return Err(Error::new_spanned(method, "duplicate exported method"));
            }
        }
        Ok(Self {
            path,
            item,
            methods,
            traits,
        })
    }

    fn source(&self) -> String {
        let name = &self.item.ident;
        let type_parameters = self
            .item
            .generics
            .type_params()
            .map(|parameter| parameter.ident.to_string())
            .collect::<Vec<_>>();
        let generics = if type_parameters.is_empty() {
            String::new()
        } else {
            format!("<{}>", type_parameters.join(", "))
        };
        let mut source = String::new();
        for line in super::documentation(&self.item.attrs).lines() {
            source.push_str(&format!("/// {line}\n"));
        }
        source.push_str(&format!(
            "pub struct {name}{generics};\n\nimpl{generics} {name}{generics} {{\n"
        ));
        for method in &self.methods {
            for line in super::documentation(&method.attrs).lines() {
                source.push_str(&format!("    /// {line}\n"));
            }
            let mut signature = method.sig.clone();
            signature.generics.where_clause = None;
            let signature = signature
                .to_token_stream()
                .to_string()
                .replace(" (", "(")
                .replace(" < ", "<")
                .replace(" >", ">")
                .replace("& self", "&self")
                .replace("& mut self", "&mut self")
                .replace("& mut", "&mut");
            source.push_str(&format!("    {signature} {{}}\n"));
        }
        source.push_str("}\n");
        source
    }
}

pub(super) fn expand_definition(path: Path, module: ItemMod) -> TokenStream {
    let definition = match Definition::parse(path.clone(), module.clone()) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let mut emitted = module.clone();
    if let Some((_, items)) = &mut emitted.content {
        for item in items {
            match item {
                Item::Struct(structure) => structure.attrs.retain(|attribute| {
                    !attribute.path().is_ident("rils_opaque")
                        && !attribute.path().is_ident("rils_impl")
                }),
                Item::Impl(implementation) => {
                    for member in &mut implementation.items {
                        if let ImplItem::Fn(method) = member {
                            method
                                .attrs
                                .retain(|attribute| !attribute.path().is_ident("export_rils"));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let name = &definition.item.ident;
    let macro_name = format_ident!("{}_definition", name.to_string().to_lowercase());
    let module_name = &module.ident;
    let item_type: Type = syn::parse_quote!(#module_name::#name);
    let checks = super::trait_impls::checks(&item_type, &definition.traits);
    quote! {
        #emitted
        #checks
        #[macro_export]
        macro_rules! #macro_name {
            ($emit:ident) => { $emit! { #path; #module } };
        }
    }
    .into()
}

pub(super) fn expand_source(path: Path, module: ItemMod) -> TokenStream {
    match Definition::parse(path, module) {
        Ok(definition) => {
            let source = definition.source();
            quote!(#source).into()
        }
        Err(error) => error.into_compile_error().into(),
    }
}

pub(super) fn expand_metadata(path: Path, module: ItemMod) -> TokenStream {
    let definition = match Definition::parse(path, module) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    match metadata_tokens(&definition) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn metadata_tokens(definition: &Definition) -> syn::Result<proc_macro2::TokenStream> {
    let name = definition.item.ident.to_string();
    let docs = super::documentation(&definition.item.attrs);
    let module = &definition.path;
    let type_parameters = definition
        .item
        .generics
        .type_params()
        .map(|parameter| parameter.ident.to_string())
        .collect::<Vec<_>>();
    let methods = definition
        .methods
        .iter()
        .map(|method| {
            let method_name = method.sig.ident.to_string();
            let method_docs = super::documentation(&method.attrs);
            let id_path = format!(
                "{}::{method_name}",
                quote!(#module).to_string().replace(' ', "")
            );
            let (kind, receiver_mode, parameter_start) =
                if let Some(receiver) = method.sig.receiver() {
                    let mode = if receiver.reference.is_some() {
                        if receiver.mutability.is_some() {
                            quote!(crate::ReceiverMode::Mutable)
                        } else {
                            quote!(crate::ReceiverMode::Shared)
                        }
                    } else {
                        quote!(crate::ReceiverMode::Owned)
                    };
                    (
                        quote!(crate::BuiltinMemberKind::Method),
                        quote!(Some(#mode)),
                        1,
                    )
                } else {
                    (
                        quote!(crate::BuiltinMemberKind::AssociatedFunction),
                        quote!(None),
                        0,
                    )
                };
            let parameters = method
                .sig
                .inputs
                .iter()
                .skip(parameter_start)
                .map(|input| match input {
                    FnArg::Typed(parameter) => type_patterns::tokens(&parameter.ty),
                    _ => Err(Error::new_spanned(input, "unexpected receiver")),
                })
                .collect::<syn::Result<Vec<_>>>()?;
            let result = match &method.sig.output {
                ReturnType::Default => quote!(TypePattern::Unit),
                ReturnType::Type(_, ty) => type_patterns::tokens(ty)?,
            };
            let generics = method
                .sig
                .generics
                .type_params()
                .map(|parameter| parameter.ident.to_string())
                .collect::<Vec<_>>();
            Ok(quote! {
                crate::BuiltinMember {
                    name: #method_name,
                    kind: #kind,
                    signature: Some(crate::BuiltinSignature {
                        parameters: &[#(#parameters),*],
                        result: #result,
                        variadic: false,
                    }),
                    value_type: None,
                    receiver: #receiver_mode,
                    builtin_id: Some(builtin_id!(#id_path)),
                    runtime_import: None,
                    required: true,
                    type_parameters: &[#(#generics),*],
                    documentation: #method_docs,
                }
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote! {
        use crate::TypePattern;
        pub const DECLARATION: crate::BuiltinDeclaration = crate::BuiltinDeclaration {
            path: #name,
            kind: crate::BuiltinKind::Struct,
            supertraits: &[],
            type_parameters: &[#(#type_parameters),*],
            members: &[#(#methods),*],
            signature: None,
            backend: crate::BuiltinBackend::Runtime,
            documentation: #docs,
        };
    })
}

pub(super) fn expand_trait_impls(path: Path, module: ItemMod) -> TokenStream {
    match Definition::parse(path, module) {
        Ok(definition) => {
            let type_name = definition.item.ident.to_string();
            let traits = definition.traits.iter().map(|path| {
                let trait_name = path.segments[0].ident.to_string();
                quote!(crate::BuiltinTraitImpl { type_name: #type_name, trait_name: #trait_name, requirements: &[] })
            });
            quote!(pub const TRAIT_IMPLS: &[crate::BuiltinTraitImpl] = &[#(#traits),*];).into()
        }
        Err(error) => error.into_compile_error().into(),
    }
}

pub(super) fn expand_native(path: Path, module: ItemMod) -> TokenStream {
    let definition = match Definition::parse(path, module) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    Error::new_spanned(
        definition.item,
        "opaque struct native bridge must provide an explicit runtime value adapter",
    )
    .into_compile_error()
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_struct_source_hides_rust_storage_and_keeps_methods() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                #[rils_opaque]
                pub struct Range<T> { current: T, end: T }
                impl<T> Range<T> {
                    #[export_rils]
                    pub fn new() -> Self { loop {} }
                    #[export_rils]
                    pub fn next(&mut self) -> Option<T> { loop {} }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::range), module).unwrap();
        let source = definition.source();
        assert!(source.contains("pub struct Range<T>;"));
        assert!(source.contains("fn new() -> Self"));
        assert!(source.contains("fn next(&mut self) -> Option<T>"));
        assert!(!source.contains("current"));
        assert!(metadata_tokens(&definition).is_ok());
    }
}
