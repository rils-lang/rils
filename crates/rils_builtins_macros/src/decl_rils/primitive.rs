//! Specializes Rust numeric method templates for the existing Rils primitives.

use proc_macro::TokenStream;
use std::collections::BTreeSet;

use proc_macro2::{Group, TokenStream as Tokens, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::{
    Error, FnArg, Ident, ImplItem, ImplItemFn, Item, ItemImpl, ItemMod, Path, ReturnType, Token,
    Type,
    parse::{Parse, ParseStream, Parser},
};

use crate::type_patterns;

struct Mapping {
    primitive: Ident,
}

impl Parse for Mapping {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Ok(Self {
            primitive: input.parse()?,
        })
    }
}

impl Mapping {
    fn variant(&self) -> Ident {
        let primitive = self.primitive.to_string();
        let mut chars = primitive.chars();
        let first = chars
            .next()
            .expect("validated integer primitive")
            .to_ascii_uppercase();
        format_ident!("{first}{}", chars.as_str())
    }
}

struct Definition {
    path: Path,
    float: bool,
    family: Vec<Mapping>,
    module: ItemMod,
    methods: Vec<ImplItemFn>,
}

pub(super) fn contains_mapping(module: &ItemMod) -> bool {
    module.content.as_ref().is_some_and(|(_, items)| {
        items.iter().any(|item| matches!(item, Item::Macro(item) if item.mac.path.is_ident("primitive_integer_family") || item.mac.path.is_ident("primitive_float_family")))
    })
}

impl Definition {
    fn parse(path: Path, module: ItemMod) -> syn::Result<Self> {
        let float = match path.to_token_stream().to_string().replace(' ', "").as_str() {
            "core::integer" => false,
            "core::float" => true,
            _ => {
                return Err(Error::new_spanned(
                    path,
                    "numeric family requires core::integer or core::float",
                ));
            }
        };
        let (_, items) = module
            .content
            .as_ref()
            .ok_or_else(|| Error::new_spanned(&module, "standard-library module must be inline"))?;
        let mut families = items.iter().filter_map(|item| match item {
            Item::Macro(item)
                if item.mac.path.is_ident("primitive_integer_family")
                    || item.mac.path.is_ident("primitive_float_family") =>
            {
                Some(item)
            }
            _ => None,
        });
        let item = families.next().expect("numeric family was detected");
        let expected_family = if float {
            "primitive_float_family"
        } else {
            "primitive_integer_family"
        };
        if !item.mac.path.is_ident(expected_family) {
            return Err(Error::new_spanned(
                item,
                format!("expected {expected_family}!"),
            ));
        }
        if families.next().is_some() {
            return Err(Error::new_spanned(
                &module,
                "expected exactly one integer family",
            ));
        }
        let family = syn::punctuated::Punctuated::<Mapping, Token![,]>::parse_terminated
            .parse2(item.mac.tokens.clone())?
            .into_iter()
            .collect::<Vec<_>>();
        if family.is_empty() {
            return Err(Error::new_spanned(item, "integer family cannot be empty"));
        }
        let mut primitive_names = BTreeSet::new();
        for entry in &family {
            let primitive = entry.primitive.to_string();
            let valid = if float {
                matches!(primitive.as_str(), "f32" | "f64")
            } else {
                matches!(
                    primitive.as_str(),
                    "i8" | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "isize"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "usize"
                )
            };
            if !valid {
                return Err(Error::new_spanned(
                    &entry.primitive,
                    "unexpected numeric primitive",
                ));
            }
            if !primitive_names.insert(primitive) {
                return Err(Error::new_spanned(
                    &entry.primitive,
                    "duplicate integer primitive",
                ));
            }
        }
        let mut methods = Vec::new();
        let mut implementations = items.iter().filter_map(|item| match item {
            Item::Impl(item) => Some(item),
            _ => None,
        });
        let implementation = implementations.next().ok_or_else(|| {
            Error::new_spanned(&module, "missing impl<TNum> Number<TNum> template")
        })?;
        if implementations.next().is_some() {
            return Err(Error::new_spanned(
                &module,
                "expected exactly one integer implementation template",
            ));
        }
        check_impl(implementation)?;
        for member in &implementation.items {
            let ImplItem::Fn(method) = member else {
                continue;
            };
            if method
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("export_rils"))
            {
                if method.block.stmts.is_empty() {
                    return Err(Error::new_spanned(
                        method,
                        "native method needs a Rust body",
                    ));
                }
                if !matches!(method.vis, syn::Visibility::Public(_)) {
                    return Err(Error::new_spanned(method, "exported method must be public"));
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
        Ok(Self {
            path,
            float,
            family,
            module,
            methods,
        })
    }

    fn rils_source(&self) -> String {
        let mut source = String::new();
        let mut mappings = self.family.iter().collect::<Vec<_>>();
        const INTEGER_ORDER: &[&str] = &[
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        ];
        let order = if self.float {
            &["f32", "f64"][..]
        } else {
            INTEGER_ORDER
        };
        mappings.sort_by_key(|mapping| {
            order
                .iter()
                .position(|name| mapping.primitive == *name)
                .expect("validated numeric primitive")
        });
        for mapping in mappings {
            source.push_str(&format!("impl {} {{\n", mapping.primitive));
            for method in &self.methods {
                let docs = super::documentation(&method.attrs);
                for line in docs.lines() {
                    source.push_str(&format!("    /// {line}\n"));
                }
                if method
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("constant"))
                {
                    source.push_str("    #[constant]\n");
                }
                let mut signature = method.sig.clone();
                signature.generics.where_clause = None;
                let signature = signature
                    .to_token_stream()
                    .to_string()
                    .replace("String", "string")
                    .replace("Integer", "integer");
                source.push_str(&format!("    {signature} {{}}\n"));
            }
            source.push_str("}\n");
        }
        source
    }
}

fn check_impl(implementation: &ItemImpl) -> syn::Result<()> {
    let Type::Path(target) = implementation.self_ty.as_ref() else {
        return Err(Error::new_spanned(
            &implementation.self_ty,
            "expected Number<TNum> template",
        ));
    };
    let valid_target = target.path.segments.len() == 1
        && target.path.segments[0].ident == "Number"
        && matches!(&target.path.segments[0].arguments, syn::PathArguments::AngleBracketed(arguments)
            if arguments.args.len() == 1 && matches!(&arguments.args[0], syn::GenericArgument::Type(Type::Path(inner)) if inner.path.is_ident("TNum")));
    let valid_generic = implementation.generics.params.len() == 1
        && matches!(&implementation.generics.params[0], syn::GenericParam::Type(parameter) if parameter.ident == "TNum");
    if !valid_target || !valid_generic || implementation.trait_.is_some() {
        return Err(Error::new_spanned(
            target,
            "integer template must be `impl<TNum> Number<TNum>`",
        ));
    }
    Ok(())
}

pub(super) fn expand_definition(path: Path, module: ItemMod) -> TokenStream {
    let definition = match Definition::parse(path, module) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let mut emitted = definition.module.clone();
    if let Some((_, items)) = &mut emitted.content {
        let template = items
            .iter()
            .find_map(|item| match item {
                Item::Impl(item) => Some(item.clone()),
                _ => None,
            })
            .expect("validated integer template");
        items.retain(|item| {
            !matches!(item, Item::Impl(_))
                && !matches!(item, Item::Macro(inner) if inner.mac.path.is_ident("primitive_integer_family") || inner.mac.path.is_ident("primitive_float_family"))
        });
        items.push(syn::parse_quote!(
            pub struct Number<T>(pub T);
        ));
        for mapping in &definition.family {
            let primitive = &mapping.primitive;
            let mut cloned = template.clone();
            cloned.generics.params.clear();
            cloned.self_ty = Box::new(syn::parse_quote!(Number<#primitive>));
            for member in &mut cloned.items {
                let replaced =
                    replace_ident(member.to_token_stream(), &format_ident!("TNum"), primitive);
                *member = match syn::parse2(replaced) {
                    Ok(value) => value,
                    Err(error) => return error.into_compile_error().into(),
                };
                if let ImplItem::Fn(method) = member {
                    method.attrs.retain(|attr| {
                        !attr.path().is_ident("export_rils") && !attr.path().is_ident("constant")
                    });
                }
            }
            items.push(Item::Impl(cloned));
        }
    }
    let original = &definition.module;
    let path = &definition.path;
    let name = if definition.float {
        format_ident!("float_definition")
    } else {
        format_ident!("integer_definition")
    };
    quote! {
        #emitted
        #[macro_export]
        macro_rules! #name {
            ($emit:ident) => { $emit! { #path; #original } };
        }
    }
    .into()
}

fn replace_ident(tokens: Tokens, from: &Ident, to: &Ident) -> Tokens {
    tokens
        .into_iter()
        .map(|token| match token {
            TokenTree::Ident(ident) if ident == *from => TokenTree::Ident(to.clone()),
            TokenTree::Group(group) => {
                let mut output =
                    Group::new(group.delimiter(), replace_ident(group.stream(), from, to));
                output.set_span(group.span());
                TokenTree::Group(output)
            }
            other => other,
        })
        .collect()
}

pub(super) fn expand_metadata(path: Path, module: ItemMod) -> TokenStream {
    let definition = match Definition::parse(path, module) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let methods = definition
        .methods
        .iter()
        .filter(|method| !method.attrs.iter().any(|attr| attr.path().is_ident("constant")))
        .map(|method| {
            let name = method.sig.ident.to_string();
            let id_path = format!("{}::{name}", if definition.float { "core::float" } else { "core::integer" });
            let documentation = super::documentation(&method.attrs);
            let receiver = method.sig.receiver();
            let kind = if receiver.is_some() {
                quote!(crate::IntrinsicKind::Method)
            } else {
                quote!(crate::IntrinsicKind::AssociatedFunction)
            };
            let parameters = method
                .sig
                .inputs
                .iter()
                .skip(usize::from(receiver.is_some()))
                .map(|input| {
                    let FnArg::Typed(parameter) = input else {
                        return Err(Error::new_spanned(input, "unexpected receiver"));
                    };
                    if matches!(parameter.ty.as_ref(), Type::Path(path) if path.path.is_ident("Integer")) {
                        Ok(quote!(TypePattern::AnyInteger))
                    } else {
                        type_patterns::tokens(&parameter.ty)
                    }
                })
                .collect::<syn::Result<Vec<_>>>()?;
            let result = match &method.sig.output {
                ReturnType::Default => quote!(crate::TypePattern::Unit),
                ReturnType::Type(_, ty) => type_patterns::tokens(ty)?,
            };
            Ok(quote! {
                crate::IntrinsicDeclaration {
                    id: builtin_id!(#id_path),
                    name: #name,
                    kind: #kind,
                    signature: crate::BuiltinSignature {
                        parameters: &[#(#parameters),*], result: #result, variadic: false,
                    },
                    documentation: #documentation,
                }
            })
        })
        .collect::<syn::Result<Vec<_>>>();
    let constants = definition
        .methods
        .iter()
        .filter(|method| {
            method
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("constant"))
        })
        .map(|method| {
            let name = method.sig.ident.to_string();
            let variant_name = match name.as_str() {
                "MIN" => "Min",
                "MAX" => "Max",
                "BITS" if !definition.float => "Bits",
                "EPSILON" if definition.float => "Epsilon",
                "MIN_POSITIVE" if definition.float => "MinPositive",
                "NAN" if definition.float => "Nan",
                "INFINITY" if definition.float => "Infinity",
                "NEG_INFINITY" if definition.float => "NegInfinity",
                _ => return Err(Error::new_spanned(method, "unsupported numeric constant")),
            };
            let variant = format_ident!("{variant_name}");
            let documentation = super::documentation(&method.attrs);
            let ReturnType::Type(_, ty) = &method.sig.output else {
                return Err(Error::new_spanned(
                    method,
                    "constant requires a return type",
                ));
            };
            let value_type = type_patterns::tokens(ty)?;
            let value_field = if definition.float {
                quote!()
            } else {
                quote!(value_type: #value_type,)
            };
            let (declaration, id) = if definition.float {
                (
                    quote!(crate::FloatConstantDeclaration),
                    quote!(crate::FloatConstantId),
                )
            } else {
                (
                    quote!(crate::IntegerConstantDeclaration),
                    quote!(crate::IntegerConstantId),
                )
            };
            Ok(quote! {
                #declaration {
                    id: #id::#variant,
                    name: #name,
                    #value_field
                    documentation: #documentation,
                }
            })
        })
        .collect::<syn::Result<Vec<_>>>();
    let constant_type = if definition.float {
        quote!(crate::FloatConstantDeclaration)
    } else {
        quote!(crate::IntegerConstantDeclaration)
    };
    match (methods, constants) {
        (Ok(methods), Ok(constants)) => quote! {
            use crate::TypePattern;
            pub const INTRINSICS: &[crate::IntrinsicDeclaration] = &[#(#methods),*];
            pub const CONSTANTS: &[#constant_type] = &[#(#constants),*];
        }
        .into(),
        (Err(error), _) | (_, Err(error)) => error.into_compile_error().into(),
    }
}

pub(super) fn expand_source(path: Path, module: ItemMod) -> TokenStream {
    match Definition::parse(path, module) {
        Ok(definition) => {
            let source = definition.rils_source();
            quote!(#source).into()
        }
        Err(error) => error.into_compile_error().into(),
    }
}

pub(super) fn expand_native(path: Path, module: ItemMod) -> TokenStream {
    let definition = match Definition::parse(path, module) {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let float = definition.float;
    let mappings = definition.family.iter().collect::<Vec<_>>();
    let bindings = mappings.iter().map(|mapping| {
    let primitive = &mapping.primitive;
    let call_name = format_ident!("call_{}", primitive);
    let constant_name = format_ident!("constant_{}", primitive);
    let numeric_module = if float { format_ident!("float") } else { format_ident!("integer") };
    let rust_type = quote!(rils_stdlib::stdlib::#numeric_module::Number<#primitive>);
    let rust_path = quote!(rils_stdlib::stdlib::#numeric_module::Number::<#primitive>);
    let methods = definition.methods.iter()
        .filter(|method| !method.attrs.iter().any(|attr| attr.path().is_ident("constant")))
        .map(|method| {
            let name = &method.sig.ident;
            let id_path = format!("{}::{name}", if float { "core::float" } else { "core::integer" });
            let receiver = method.sig.receiver();
            if receiver.is_some_and(|receiver| receiver.reference.is_some()) {
                return Err(Error::new_spanned(method, "primitive native bridge requires an owned receiver"));
            }
            let arity = method.sig.inputs.len();
            let receiver_input = receiver.map(|_| quote! {
                let native_self: #rust_type = super::NativeInput::from_value(&arguments[0])?;
            });
            let mut argument_inputs = Vec::new();
            let mut argument_names = Vec::new();
            for (index, input) in method.sig.inputs.iter().enumerate().skip(usize::from(receiver.is_some())) {
                let FnArg::Typed(parameter) = input else { return Err(Error::new_spanned(input, "unexpected receiver")); };
                let ty = match parameter.ty.as_ref() {
                    Type::Path(path) if path.path.is_ident("Self") => rust_type.clone(),
                    Type::Path(path) if path.path.is_ident("u32") => quote!(u32),
                    Type::Path(path) if path.path.is_ident("Integer") => quote!(rils_stdlib::stdlib::integer::Integer),
                    _ => return Err(Error::new_spanned(&parameter.ty, "unsupported primitive argument")),
                };
                let argument = format_ident!("arg_{index}");
                argument_inputs.push(quote! {
                    let #argument: #ty = super::NativeInput::from_value(&arguments[#index])?;
                });
                argument_names.push(argument);
            }
            let preflight = match name.to_string().as_str() {
                "clamp" if float => quote! {
                    if arg_1.0.is_nan() || arg_2.0.is_nan() || arg_1.0 > arg_2.0 {
                        return Err("float clamp requires non-NaN bounds with min <= max".into());
                    }
                },
                "pow" if !float => quote! {
                    if native_self.0.checked_pow(arg_1).is_none() { return Err("integer overflow".into()); }
                },
                "abs" if primitive.to_string().starts_with('i') => quote! {
                    if native_self.0.checked_abs().is_none() { return Err("integer overflow".into()); }
                },
                "div_euclid" if !float => quote! {
                    if arg_1.0 == 0 { return Err("division by zero".into()); }
                    if native_self.0.checked_div_euclid(arg_1.0).is_none() { return Err("integer overflow".into()); }
                },
                "rem_euclid" if !float => quote! {
                    if arg_1.0 == 0 { return Err("division by zero".into()); }
                    if native_self.0.checked_rem_euclid(arg_1.0).is_none() { return Err("integer overflow".into()); }
                },
                _ => quote!(),
            };
            let call = if receiver.is_some() {
                quote!(native_self.#name(#(#argument_names),*))
            } else {
                quote!(#rust_path::#name(#(#argument_names),*))
            };
            Ok(quote! {
                id if id == rils_builtins::builtin_id!(#id_path) => Some((|| -> Result<crate::Value, String> {
                    if arguments.len() != #arity {
                        return Err(format!("{} expects {} arguments, found {}", stringify!(#name), #arity, arguments.len()));
                    }
                    #receiver_input
                    #(#argument_inputs)*
                    #preflight
                    super::NativeOutput::into_value(#call)
                })())
            })
        }).collect::<syn::Result<Vec<_>>>();
    let constants = definition
        .methods
        .iter()
        .filter(|method| {
            method
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("constant"))
        })
        .map(|method| {
            let name = &method.sig.ident;
            let variant = match name.to_string().as_str() {
                "MIN" => format_ident!("Min"),
                "MAX" => format_ident!("Max"),
                "BITS" if !float => format_ident!("Bits"),
                "EPSILON" if float => format_ident!("Epsilon"),
                "MIN_POSITIVE" if float => format_ident!("MinPositive"),
                "NAN" if float => format_ident!("Nan"),
                "INFINITY" if float => format_ident!("Infinity"),
                "NEG_INFINITY" if float => format_ident!("NegInfinity"),
                _ => return Err(Error::new_spanned(method, "unsupported numeric constant")),
            };
            let id_type = if float { quote!(rils_builtins::FloatConstantId) } else { quote!(rils_builtins::IntegerConstantId) };
            let id = quote!(#id_type::#variant);
            Ok(quote!(#id => Some(super::NativeOutput::into_value(#rust_path::#name())),))
        })
        .collect::<syn::Result<Vec<_>>>();
    let constant_id_type = if float { quote!(rils_builtins::FloatConstantId) } else { quote!(rils_builtins::IntegerConstantId) };
    match (methods, constants) {
        (Ok(methods), Ok(constants)) => Ok(quote! {
            fn #call_name(id: rils_builtins::BuiltinId, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
                match id { #(#methods,)* _ => None }
            }
            fn #constant_name(id: #constant_id_type) -> Option<Result<crate::Value, String>> {
                match id { #(#constants)* }
            }
        }),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
    }).collect::<syn::Result<Vec<_>>>();
    let bindings = match bindings {
        Ok(value) => value,
        Err(error) => return error.into_compile_error().into(),
    };
    let receiver_dispatch = mappings.iter().map(|mapping| {
        let wrapper = mapping.variant();
        let call_name = format_ident!("call_{}", mapping.primitive);
        quote!(Some(crate::Value::#wrapper(_)) => #call_name(id, arguments),)
    });
    let target_dispatch = mappings.iter().map(|mapping| {
        let wrapper = mapping.variant();
        let call_name = format_ident!("call_{}", mapping.primitive);
        quote!(Some(rils_builtins::IntegerType::#wrapper) => #call_name(id, arguments),)
    });
    let constant_dispatch = mappings.iter().map(|mapping| {
        let wrapper = mapping.variant();
        let constant_name = format_ident!("constant_{}", mapping.primitive);
        let target_type = if float {
            quote!(crate::FloatType)
        } else {
            quote!(rils_builtins::IntegerType)
        };
        quote!(#target_type::#wrapper => #constant_name(id),)
    });
    if float {
        return quote! {
            #(#bindings)*
            pub fn call(id: rils_builtins::BuiltinId, arguments: &[crate::Value]) -> Option<Result<crate::Value, String>> {
                match arguments.first() { #(#receiver_dispatch)* _ => None }
            }
            pub fn constant(target: crate::FloatType, id: rils_builtins::FloatConstantId) -> Option<Result<crate::Value, String>> {
                match target { #(#constant_dispatch)* }
            }
        }.into();
    }
    quote! {
        #(#bindings)*
        pub fn call(
            id: rils_builtins::BuiltinId,
            target: Option<rils_builtins::IntegerType>,
            arguments: &[crate::Value],
        ) -> Option<Result<crate::Value, String>> {
            if id == rils_builtins::BuiltinId::IntegerTryFrom {
                match target { #(#target_dispatch)* _ => None }
            } else {
                match arguments.first() { #(#receiver_dispatch)* _ => None }
            }
        }
        pub fn constant(
            target: rils_builtins::IntegerType,
            id: rils_builtins::IntegerConstantId,
        ) -> Option<Result<crate::Value, String>> {
            match target { #(#constant_dispatch)* }
        }
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_can_include_i32_without_a_special_template_mapping() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                primitive_integer_family!(i32);
                impl<TNum> Number<TNum> {
                    #[export_rils]
                    pub fn wrapping_add(self, other: Self) -> Self {
                        Self(self.0.wrapping_add(other.0))
                    }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::integer), module).unwrap();
        let source = definition.rils_source();
        assert!(source.contains("impl i32"));
        assert!(source.contains("wrapping_add"));
        assert!(!source.contains("struct Number"));
        assert!(!source.contains("export_rils"));
    }

    #[test]
    fn family_mapping_reuses_rust_method_body_without_rils_wrapper_types() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                primitive_integer_family!(i8, i32, u8);
                impl<TNum> Number<TNum> {
                    #[export_rils]
                    pub fn wrapping_add(self, other: Self) -> Self {
                        Self(self.0.wrapping_add(other.0))
                    }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::integer), module).unwrap();
        let source = definition.rils_source();
        assert!(source.contains("impl i8"));
        assert!(source.contains("impl i32"));
        assert!(source.contains("impl u8"));
        assert!(!source.contains("struct Number"));
        let cloned = replace_ident(
            quote!(Self(TNum::MAX)),
            &format_ident!("TNum"),
            &format_ident!("i8"),
        );
        assert!(cloned.to_string().contains("i8 :: MAX"));
    }

    #[test]
    fn integer_variants_follow_primitive_names() {
        for (primitive, variant) in [
            ("i8", "I8"),
            ("i32", "I32"),
            ("isize", "Isize"),
            ("u128", "U128"),
            ("usize", "Usize"),
        ] {
            let mapping = Mapping {
                primitive: format_ident!("{primitive}"),
            };
            assert_eq!(mapping.variant().to_string(), variant);
        }
    }
}
