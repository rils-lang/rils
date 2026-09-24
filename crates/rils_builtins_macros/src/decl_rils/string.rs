//! Native declaration support for the built-in `string` primitive.

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{Error, FnArg, ImplItem, ImplItemFn, Item, ItemMod, ItemStruct, Path, ReturnType, Type};

use crate::type_patterns;

struct Definition {
    item: ItemStruct,
    methods: Vec<ImplItemFn>,
    traits: Vec<Path>,
}

pub(super) fn is_string(path: &Path) -> bool {
    path.to_token_stream().to_string().replace(' ', "") == "core::string"
}

impl Definition {
    fn parse(path: Path, module: ItemMod) -> syn::Result<Self> {
        if !is_string(&path) {
            return Err(Error::new_spanned(path, "expected core::string"));
        }
        let (_, items) = module
            .content
            .as_ref()
            .ok_or_else(|| Error::new_spanned(&module, "standard-library module must be inline"))?;
        let item = items
            .iter()
            .find_map(|item| match item {
                Item::Struct(item) if item.ident == "String" => Some(item.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::new_spanned(&module, "expected String wrapper"))?;
        let traits = super::trait_impls::parse(&item.attrs)?;
        let implementation = items.iter().find_map(|item| match item { Item::Impl(item) if matches!(item.self_ty.as_ref(), Type::Path(path) if path.path.is_ident("String")) => Some(item), _ => None })
            .ok_or_else(|| Error::new_spanned(&module, "expected impl String"))?;
        let methods = implementation
            .items
            .iter()
            .filter_map(|item| match item {
                ImplItem::Fn(method)
                    if method
                        .attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("export_rils")) =>
                {
                    Some(method.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if methods.is_empty() {
            return Err(Error::new_spanned(
                &module,
                "definition needs a #[export_rils] method",
            ));
        }
        for method in &methods {
            if !matches!(method.vis, syn::Visibility::Public(_))
                || method.block.stmts.is_empty()
                || method
                    .sig
                    .receiver()
                    .is_none_or(|receiver| receiver.reference.is_none())
            {
                return Err(Error::new_spanned(
                    method,
                    "exported string method requires a public Rust body and &self receiver",
                ));
            }
        }
        Ok(Self {
            item,
            methods,
            traits,
        })
    }

    fn source(&self) -> String {
        let mut source = String::new();
        for line in super::documentation(&self.item.attrs).lines() {
            source.push_str(&format!("/// {line}\n"));
        }
        source.push_str("impl string {\n");
        for method in &self.methods {
            for line in super::documentation(&method.attrs).lines() {
                source.push_str(&format!("    /// {line}\n"));
            }
            let signature = method
                .sig
                .to_token_stream()
                .to_string()
                .replace("String", "string");
            source.push_str(&format!("    {signature} {{}}\n"));
        }
        source.push_str("}\n");
        source
    }
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

pub(super) fn expand_trait_impls(path: Path, module: ItemMod) -> TokenStream {
    match Definition::parse(path, module) {
        Ok(definition) => {
            let traits = definition.traits.iter().map(|path| {
                let name = path.segments[0].ident.to_string();
                quote!(crate::BuiltinTraitImpl { type_name: "string", trait_name: #name, requirements: &[] })
            });
            quote!(pub const TRAIT_IMPLS: &[crate::BuiltinTraitImpl] = &[#(#traits),*];).into()
        }
        Err(error) => error.into_compile_error().into(),
    }
}

pub(super) fn expand_metadata(path: Path, module: ItemMod) -> TokenStream {
    let definition = match Definition::parse(path, module) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let documentation = super::documentation(&definition.item.attrs);
    let methods = definition.methods.iter().map(|method| {
        let name = method.sig.ident.to_string();
        let id_path = format!("core::string::{name}");
        let docs = super::documentation(&method.attrs);
        let parameters = method.sig.inputs.iter().skip(1).map(|input| {
            let FnArg::Typed(parameter) = input else { return Err(Error::new_spanned(input, "unexpected receiver")); };
            type_patterns::tokens(&parameter.ty)
        }).collect::<syn::Result<Vec<_>>>()?;
        let result = match &method.sig.output {
            ReturnType::Default => quote!(TypePattern::Unit),
            ReturnType::Type(_, ty) => type_patterns::tokens(ty)?,
        };
        Ok(quote! {
            crate::BuiltinMember {
                name: #name,
                kind: crate::BuiltinMemberKind::Method,
                signature: Some(crate::BuiltinSignature { parameters: &[#(#parameters),*], result: #result, variadic: false }),
                value_type: None,
                receiver: Some(crate::ReceiverMode::Shared),
                builtin_id: Some(builtin_id!(#id_path)),
                runtime_import: None,
                required: true,
                type_parameters: &[],
                documentation: #docs,
            }
        })
    }).collect::<syn::Result<Vec<_>>>();
    match methods {
        Ok(methods) => quote! {
            use crate::TypePattern;
            pub const DECLARATION: crate::BuiltinDeclaration = crate::BuiltinDeclaration {
                path: "string",
                kind: crate::BuiltinKind::Primitive,
                supertraits: &[],
                type_parameters: &[],
                members: &[#(#methods),*],
                signature: None,
                backend: crate::BuiltinBackend::Runtime,
                documentation: #documentation,
            };
        }
        .into(),
        Err(error) => error.into_compile_error().into(),
    }
}

pub(super) fn expand_native(path: Path, module: ItemMod) -> TokenStream {
    let definition = match Definition::parse(path, module) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let methods = definition.methods.iter().map(|method| {
        let name = &method.sig.ident;
        let id_path = format!("core::string::{name}");
        let arity = method.sig.inputs.len();
        let arguments = method.sig.inputs.iter().enumerate().skip(1).map(|(index, input)| {
            let FnArg::Typed(parameter) = input else { return Err(Error::new_spanned(input, "unexpected receiver")); };
            let value = match parameter.ty.as_ref() {
                Type::Path(path) if path.path.is_ident("String") => quote!(super::string_input(&arguments[#index])?),
                Type::Path(path) if path.path.is_ident("usize") => quote!(super::usize_input(&arguments[#index])?),
                _ => return Err(Error::new_spanned(&parameter.ty, "unsupported string argument")),
            };
            Ok(value)
        }).collect::<syn::Result<Vec<_>>>()?;
        Ok(quote! {
            id if id == rils_builtins::builtin_id!(#id_path) => Some((|| -> Result<crate::Value, String> {
                if arguments.len() != #arity {
                    return Err(format!("{} expects {} arguments, found {}", stringify!(#name), #arity, arguments.len()));
                }
                let receiver = super::string_input(&arguments[0])?;
                super::StringOutput::into_value(receiver.#name(#(#arguments),*))
            })()),
        })
    }).collect::<syn::Result<Vec<_>>>();
    match methods {
        Ok(methods) => quote! {
            pub fn call(id: rils_builtins::BuiltinId, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
                match id { #(#methods)* _ => None }
            }
        }.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
