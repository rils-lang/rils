//! Shared declaration generation for Rust-backed Rils structs.

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Error, Fields, FnArg, GenericArgument, ImplItem, ImplItemFn, Item, ItemImpl, ItemMod,
    ItemStruct, Path, PathArguments, ReturnType, Type, fold::Fold,
};

use crate::type_patterns;

mod native;

pub(super) fn expand_native(path: Path, module: ItemMod) -> TokenStream {
    native::expand(path, module)
}

pub(super) fn expand_layout(path: Path, module: ItemMod) -> TokenStream {
    let definition = match Definition::parse(path, module) {
        Ok(definition) => definition,
        Err(error) => return error.into_compile_error().into(),
    };
    let Fields::Unnamed(fields) = &definition.item.fields else {
        return Error::new_spanned(&definition.item, "sequence layout needs one tuple field")
            .into_compile_error()
            .into();
    };
    let storage = fields.unnamed.iter().next();
    if fields.unnamed.len() != 1
        || definition.item.generics.type_params().count() != 1
        || storage.is_none_or(|field| {
            field.ty.to_token_stream().to_string().replace(' ', "") != "std::vec::Vec<T>"
        })
    {
        return Error::new_spanned(
            &definition.item,
            "sequence layout requires one type parameter and std::vec::Vec<T> storage",
        )
        .into_compile_error()
        .into();
    }
    let name = definition.item.ident.to_string();
    quote! {
        pub fn matches(ty: &crate::Type) -> bool {
            matches!(ty, crate::Type::Named { name, arguments }
                if name == #name && arguments.len() == 1)
        }

        pub fn layout(
            ty: &crate::Type,
            resolve_child: &mut dyn FnMut(&crate::Type) -> Result<std::rc::Rc<rils_value::DynamicLayout>, String>,
        ) -> Option<Result<std::rc::Rc<rils_value::DynamicLayout>, String>> {
            let crate::Type::Named { arguments, .. } = ty else {
                return None;
            };
            if !matches(ty) {
                return None;
            }
            Some(resolve_child(&arguments[0])
                .map(|item| rils_value::DynamicLayout::sequence(ty.clone(), item)))
        }
    }
    .into()
}

struct Definition {
    path: Path,
    item: ItemStruct,
    methods: Vec<ImplItemFn>,
    traits: Vec<Path>,
    trait_impls: Vec<super::trait_impls::ConditionalImpl>,
    exported_impls: Vec<ItemImpl>,
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
        if let Fields::Unnamed(fields) = &item.fields
            && fields
                .unnamed
                .iter()
                .any(|field| matches!(field.vis, syn::Visibility::Public(_)))
        {
            return Err(Error::new_spanned(
                &item,
                "public tuple fields are not supported in Rils structs; use named fields",
            ));
        }
        let traits = super::trait_impls::parse(&item.attrs)?;
        if !traits.is_empty() && !item.generics.params.is_empty() {
            return Err(Error::new_spanned(
                &item,
                "generic types need a marked trait impl",
            ));
        }
        let mut methods = Vec::new();
        let mut trait_impls = Vec::new();
        let mut exported_impls = Vec::new();
        for implementation in items.iter().filter_map(|item| match item {
            Item::Impl(item) => Some(item),
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
            if implementation.trait_.is_some() {
                if let Some(parsed) = super::trait_impls::parse_impl(
                    implementation,
                    &item.ident,
                    &item.generics,
                    item.attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("rils_struct")),
                )? {
                    if let Some(method) =
                        implementation.items.iter().find_map(|member| match member {
                            ImplItem::Fn(method)
                                if method
                                    .attrs
                                    .iter()
                                    .any(|attr| attr.path().is_ident("rils_return")) =>
                            {
                                Some(method)
                            }
                            _ => None,
                        })
                    {
                        return Err(Error::new_spanned(
                            method,
                            "trait impl return types are derived from the Rust signature and associated types; remove #[rils_return]",
                        ));
                    }
                    trait_impls.push(parsed);
                    exported_impls.push(implementation.clone());
                }
            }
            if implementation.trait_.is_some() {
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
                if (implementation.trait_.is_none()
                    && !matches!(method.vis, syn::Visibility::Public(_)))
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
        if methods.is_empty()
            && exported_impls.is_empty()
            && !item
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("rils_struct"))
        {
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
            trait_impls,
            exported_impls,
        })
    }

    fn source(&self) -> syn::Result<String> {
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
        let public_fields = public_fields(&self.item);
        if public_fields.is_empty() {
            source.push_str(&format!("pub struct {name}{generics};\n"));
        } else {
            source.push_str(&format!("pub struct {name}{generics} {{\n"));
            for field in public_fields {
                for line in super::documentation(&field.attrs).lines() {
                    source.push_str(&format!("    /// {line}\n"));
                }
                let field_name = field.ident.as_ref().expect("named public field");
                let ty = rils_field_type(&field.ty)?;
                source.push_str(&format!("    {field_name}: {ty},\n"));
            }
            source.push_str("}\n");
        }
        source.push_str(&format!("\nimpl{generics} {name}{generics} {{\n"));
        for method in &self.methods {
            write_method(&mut source, method, None)?;
        }
        source.push_str("}\n");
        for implementation in &self.exported_impls {
            let trait_name = &implementation
                .trait_
                .as_ref()
                .expect("trait impl")
                .1
                .segments
                .last()
                .expect("trait path")
                .ident;
            source.push_str(&format!(
                "\nimpl{generics} {trait_name} for {name}{generics} {{\n"
            ));
            for member in &implementation.items {
                match member {
                    ImplItem::Type(associated) => {
                        let ty = rils_field_type(&associated.ty)?;
                        source.push_str(&format!("    type {} = {ty};\n", associated.ident));
                    }
                    ImplItem::Fn(method) => {
                        write_method(&mut source, method, Some(implementation))?
                    }
                    _ => {}
                }
            }
            source.push_str("}\n");
        }
        Ok(source)
    }
}

fn write_method(
    source: &mut String,
    method: &ImplItemFn,
    implementation: Option<&ItemImpl>,
) -> syn::Result<()> {
    for line in super::documentation(&method.attrs).lines() {
        source.push_str(&format!("    /// {line}\n"));
    }
    let mut signature = method.sig.clone();
    signature.inputs.pop_punct();
    signature.generics.where_clause = None;
    if let Some(implementation) = implementation {
        for argument in &mut signature.inputs {
            if let FnArg::Typed(argument) = argument {
                *argument.ty = normalize_trait_type(&argument.ty, implementation)?;
            }
        }
        if let ReturnType::Type(_, ty) = &mut signature.output {
            **ty = normalize_trait_type(ty, implementation)?;
        }
    }
    if let Some(attribute) = method
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("rils_return"))
    {
        let ty: Type = attribute.parse_args()?;
        signature.output = syn::parse_quote!(-> #ty);
    }
    let signature = signature
        .to_token_stream()
        .to_string()
        .replace("std :: fmt", "core :: fmt")
        .replace(" (", "(")
        .replace(" < ", "<")
        .replace(" >", ">")
        .replace("& self", "&self")
        .replace("& mut self", "&mut self")
        .replace("& mut", "&mut");
    source.push_str(&format!("    {signature} {{}}\n"));
    Ok(())
}

struct TraitTypeNormalizer<'a> {
    implementation: &'a ItemImpl,
}

impl Fold for TraitTypeNormalizer<'_> {
    fn fold_type(&mut self, ty: Type) -> Type {
        if let Type::Path(path) = &ty {
            let associated = if let Some(qself) = &path.qself {
                matches!(qself.ty.as_ref(), Type::Path(base) if base.path.is_ident("Self"))
                    .then(|| path.path.segments.last().map(|segment| &segment.ident))
                    .flatten()
            } else if path.path.segments.len() == 2
                && path
                    .path
                    .segments
                    .first()
                    .is_some_and(|segment| segment.ident == "Self")
            {
                path.path.segments.last().map(|segment| &segment.ident)
            } else {
                None
            };
            if let Some(associated) = associated
                && let Some(value) = self
                    .implementation
                    .items
                    .iter()
                    .find_map(|item| match item {
                        ImplItem::Type(item) if item.ident == *associated => Some(&item.ty),
                        _ => None,
                    })
            {
                return self.fold_type(value.clone());
            }
        }
        let mut ty = syn::fold::fold_type(self, ty);
        if let Type::Path(path) = &mut ty
            && path.qself.is_none()
        {
            let full = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let mapped = match full.as_str() {
                "std::option::Option" | "core::option::Option" => Some("Option"),
                "std::result::Result" | "core::result::Result" => Some("Result"),
                "std::vec::Vec" | "alloc::vec::Vec" => Some("Vec"),
                "std::string::String" | "alloc::string::String" => Some("string"),
                _ => None,
            };
            if let Some(mapped) = mapped {
                let mut last = path
                    .path
                    .segments
                    .last()
                    .expect("type path segment")
                    .clone();
                last.ident = syn::parse_str(mapped).expect("mapped Rils type name");
                path.path = syn::parse_quote!(#last);
            }
        }
        ty
    }
}

fn normalize_trait_type(ty: &Type, implementation: &ItemImpl) -> syn::Result<Type> {
    Ok(TraitTypeNormalizer { implementation }.fold_type(ty.clone()))
}

fn public_fields(item: &ItemStruct) -> Vec<&syn::Field> {
    match &item.fields {
        Fields::Named(fields) => fields
            .named
            .iter()
            .filter(|field| matches!(field.vis, syn::Visibility::Public(_)))
            .collect(),
        _ => Vec::new(),
    }
}

fn rils_field_type(ty: &Type) -> syn::Result<String> {
    match ty {
        Type::Macro(value) if value.mac.path.is_ident("rils_type") => {
            let inner: Type = syn::parse2(value.mac.tokens.clone())?;
            rils_field_type(&inner)
        }
        Type::Path(value) if value.qself.is_none() => {
            let segments = &value.path.segments;
            let last = segments
                .last()
                .ok_or_else(|| Error::new_spanned(ty, "empty field type"))?;
            if segments
                .iter()
                .take(segments.len() - 1)
                .any(|segment| !matches!(segment.arguments, PathArguments::None))
            {
                return Err(Error::new_spanned(
                    ty,
                    "only the final field type segment may have type arguments",
                ));
            }
            let full = segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let name = match full.as_str() {
                "String" | "std::string::String" | "alloc::string::String" => "string",
                "std::option::Option" => "Option",
                "std::result::Result" => "Result",
                "std::vec::Vec" => "Vec",
                "std::collections::VecDeque" => "VecDeque",
                "std::collections::HashMap" => "HashMap",
                "std::collections::HashSet" => "HashSet",
                "std::collections::BTreeMap" => "BTreeMap",
                "std::collections::BTreeSet" => "BTreeSet",
                _ => full.as_str(),
            };
            let arguments = match &last.arguments {
                PathArguments::None => Vec::new(),
                PathArguments::AngleBracketed(args) => args
                    .args
                    .iter()
                    .map(|arg| match arg {
                        GenericArgument::Type(ty) => rils_field_type(ty),
                        _ => Err(Error::new_spanned(
                            arg,
                            "Rils fields only support type arguments",
                        )),
                    })
                    .collect::<syn::Result<Vec<_>>>()?,
                _ => return Err(Error::new_spanned(ty, "unsupported field type arguments")),
            };
            if arguments.is_empty() {
                Ok(name.to_owned())
            } else {
                Ok(format!("{name}<{}>", arguments.join(", ")))
            }
        }
        Type::Tuple(tuple) => Ok(format!(
            "({})",
            tuple
                .elems
                .iter()
                .map(rils_field_type)
                .collect::<syn::Result<Vec<_>>>()?
                .join(", ")
        )),
        Type::Paren(value) => rils_field_type(&value.elem),
        Type::Group(value) => rils_field_type(&value.elem),
        Type::Reference(_) => Err(Error::new_spanned(
            ty,
            "Rils struct fields cannot directly contain references",
        )),
        _ => Err(Error::new_spanned(
            ty,
            "unsupported public Rils struct field type",
        )),
    }
}

pub(super) fn expand_source(path: Path, module: ItemMod) -> TokenStream {
    match Definition::parse(path, module) {
        Ok(definition) => match definition.source() {
            Ok(source) => quote!(#source).into(),
            Err(error) => error.into_compile_error().into(),
        },
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
    let (path, backend) = super::declaration_identity(&definition.path, &name);
    let docs = super::documentation(&definition.item.attrs);
    let module = &definition.path;
    let type_parameters = definition
        .item
        .generics
        .type_params()
        .map(|parameter| parameter.ident.to_string())
        .collect::<Vec<_>>();
    let fields = public_fields(&definition.item)
        .into_iter()
        .map(|field| {
            let name = field
                .ident
                .as_ref()
                .expect("named public field")
                .to_string();
            let docs = super::documentation(&field.attrs);
            let rils_ty = rils_field_type(&field.ty)?;
            let parsed_ty: Type = syn::parse_str(&rils_ty)?;
            let value_type = type_patterns::tokens(&parsed_ty)?;
            Ok(quote! {
                crate::BuiltinMember {
                    name: #name,
                    kind: crate::BuiltinMemberKind::Field,
                    signature: None,
                    value_type: Some(#value_type),
                    receiver: None,
                    builtin_id: None,
                    indexed_view: false,
                    runtime_import: None,
                    native_symbol: None,
                    required: false,
                    type_parameters: &[],
                    documentation: #docs,
                }
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let methods = definition
        .methods
        .iter()
        .map(|method| (method, None))
        .chain(
            exported_trait_methods(definition)
                .map(|(method, implementation)| (method, Some(implementation))),
        )
        .map(|(method, implementation)| {
            let method_name = method.sig.ident.to_string();
            let method_docs = super::documentation(&method.attrs);
            let id_path = format!(
                "{}::{method_name}",
                quote!(#module).to_string().replace(' ', "")
            );
            let indexed_view = method
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("rils_indexed_view"));
            let (builtin_id, runtime_import, native_symbol) =
                match super::method_binding::MethodBinding::parse(method)? {
                    super::method_binding::MethodBinding::Native => {
                        (quote!(None), quote!(None), quote!(Some(#id_path)))
                    }
                    super::method_binding::MethodBinding::Legacy(path) => {
                        (quote!(Some(builtin_id!(#path))), quote!(None), quote!(None))
                    }
                    super::method_binding::MethodBinding::Import(path) => {
                        (quote!(None), quote!(Some(#path)), quote!(None))
                    }
                };
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
                    FnArg::Typed(parameter) => {
                        let ty = if let Some(implementation) = implementation {
                            normalize_trait_type(&parameter.ty, implementation)?
                        } else {
                            *parameter.ty.clone()
                        };
                        type_patterns::with_generics(&ty, &method.sig.generics)
                    }
                    _ => Err(Error::new_spanned(input, "unexpected receiver")),
                })
                .collect::<syn::Result<Vec<_>>>()?;
            let result_type = method
                .attrs
                .iter()
                .find(|attr| attr.path().is_ident("rils_return"))
                .map(|attr| attr.parse_args::<Type>())
                .transpose()?;
            let result = if let Some(ty) = result_type.as_ref() {
                type_patterns::tokens(ty)?
            } else {
                match &method.sig.output {
                    ReturnType::Default => quote!(TypePattern::Unit),
                    ReturnType::Type(_, ty) => {
                        let ty = if let Some(implementation) = implementation {
                            normalize_trait_type(ty, implementation)?
                        } else {
                            *ty.clone()
                        };
                        type_patterns::tokens(&ty)?
                    }
                }
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
                    builtin_id: #builtin_id,
                    indexed_view: #indexed_view,
                    runtime_import: #runtime_import,
                    native_symbol: #native_symbol,
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
            path: #path,
            kind: crate::BuiltinKind::Struct,
            source: None,
            supertraits: &[],
            type_parameters: &[#(#type_parameters),*],
            members: &[#(#fields,)* #(#methods),*],
            signature: None,
            native_symbol: None,
            backend: #backend,
            documentation: #docs,
        };
    })
}

fn exported_trait_methods(
    definition: &Definition,
) -> impl Iterator<Item = (&ImplItemFn, &ItemImpl)> {
    definition
        .exported_impls
        .iter()
        .filter(|implementation| {
            implementation.trait_.as_ref().is_some_and(|(_, path, _)| {
                path.segments.last().is_some_and(|segment| {
                    segment.ident == "Iterator" || segment.ident == "IntoIterator"
                })
            })
        })
        .flat_map(|implementation| {
            implementation
                .items
                .iter()
                .filter_map(move |member| match member {
                    ImplItem::Fn(method) => Some((method, implementation)),
                    _ => None,
                })
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
            let conditional = definition.trait_impls.iter().map(|implementation| {
                let trait_name = &implementation.trait_name;
                let requirements = implementation.requirements.iter().map(|(parameter, bound)| quote!((#parameter, #bound)));
                quote!(crate::BuiltinTraitImpl { type_name: #type_name, trait_name: #trait_name, requirements: &[#(#requirements),*] })
            });
            quote!(pub const TRAIT_IMPLS: &[crate::BuiltinTraitImpl] = &[#(#traits,)* #(#conditional),*];).into()
        }
        Err(error) => error.into_compile_error().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_struct_storage_stays_hidden_and_methods_export() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
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
        let source = definition.source().unwrap();
        assert!(source.contains("pub struct Range<T>;"));
        assert!(source.contains("fn new() -> Self"));
        assert!(source.contains("fn next(&mut self) -> Option<T>"));
        assert!(!source.contains("current"));
        assert!(metadata_tokens(&definition).is_ok());
    }

    #[test]
    fn trait_impl_exports_its_identity_and_associated_type() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                #[rils_struct]
                pub struct Iter<T>(std::vec::IntoIter<T>);
                #[rils_impl]
                impl<T> std::iter::Iterator for Iter<T> {
                    type Item = T;
                    fn next(&mut self) -> std::option::Option<Self::Item> {
                        self.0.next()
                    }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::iter), module).unwrap();
        let source = definition.source().unwrap();
        assert!(source.contains("impl<T> Iterator for Iter<T>"));
        assert!(source.contains("type Item = T;"));
        assert!(source.contains("fn next(&mut self) -> Option<T>"));
        let tokens = rils_syntax::lex(&source).unwrap();
        rils_syntax::parser::parse_builtin_declarations(tokens).unwrap();
        let metadata = metadata_tokens(&definition).unwrap().to_string();
        assert!(metadata.contains("TypePattern :: Option"));
        assert!(metadata.contains("TypePattern :: Generic"));
    }

    #[test]
    fn trait_impl_rejects_return_override() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                #[rils_struct]
                pub struct Iter<T>(std::vec::IntoIter<T>);
                #[rils_impl]
                impl<T> std::iter::Iterator for Iter<T> {
                    type Item = T;
                    #[rils_return(Option<T>)]
                    fn next(&mut self) -> std::option::Option<Self::Item> {
                        self.0.next()
                    }
                }
            }
        };
        let error = Definition::parse(syn::parse_quote!(core::iter), module)
            .err()
            .expect("trait return override must be rejected");
        assert!(error.to_string().contains("remove #[rils_return]"));
    }

    #[test]
    fn into_iterator_impl_exports_the_rils_associated_type() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                #[rils_struct]
                pub struct Vec<T>(std::vec::Vec<T>);
                #[rils_impl]
                impl<T> std::iter::IntoIterator for Vec<T> {
                    type Item = T;
                    type IntoIter = Iter<T>;
                    fn into_iter(self) -> Self::IntoIter { loop {} }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::vec), module).unwrap();
        let source = definition.source().unwrap();
        assert!(source.contains("impl<T> IntoIterator for Vec<T>"));
        assert!(source.contains("type IntoIter = Iter<T>;"));
        assert!(source.contains("type Item = T;"));
        assert!(source.contains("fn into_iter(self) -> Iter<T>"));
        let tokens = rils_syntax::lex(&source).unwrap();
        rils_syntax::parser::parse_builtin_declarations(tokens).unwrap();
        let metadata = metadata_tokens(&definition).unwrap().to_string();
        assert!(metadata.contains("TypePattern :: Named"));
        assert!(metadata.contains("path : \"Iter\""));
    }

    #[test]
    fn public_fields_are_exported_with_rils_types() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                pub struct Record<T> {
                    /// An integer value.
                    pub value: rils_type![i32],
                    pub text: std::string::String,
                    pub optional: std::option::Option<T>,
                    hidden: T,
                }
                impl<T> Record<T> {
                    #[export_rils]
                    pub fn new(value: i32, text: String) -> Self { loop {} }
                }
            }
        };
        let definition = Definition::parse(syn::parse_quote!(core::record), module).unwrap();
        let source = definition.source().unwrap();
        assert!(source.contains("value: i32,"));
        assert!(source.contains("text: string,"));
        assert!(source.contains("optional: Option<T>,"));
        assert!(!source.contains("hidden:"));
        assert!(source.contains("/// An integer value."));
        let tokens = rils_syntax::lex(&source).unwrap();
        rils_syntax::parser::parse_builtin_declarations(tokens).unwrap();
        let metadata = metadata_tokens(&definition).unwrap().to_string();
        assert!(metadata.contains("name : \"value\""));
        assert!(metadata.contains("name : \"text\""));
        assert!(metadata.contains("name : \"optional\""));
        assert!(!metadata.contains("name : \"hidden\""));
    }

    #[test]
    fn public_tuple_fields_are_rejected() {
        let module: ItemMod = syn::parse_quote! {
            mod native {
                pub struct Record(pub i32);
                impl Record {
                    #[export_rils]
                    pub fn new(value: i32) -> Self { Self(value) }
                }
            }
        };
        assert!(Definition::parse(syn::parse_quote!(core::record), module).is_err());
    }
}
