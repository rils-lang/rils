//! Storage and host bridges generated from a declared primitive family member.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

pub(super) fn host_mapping(primitive: &Ident) -> syn::Item {
    syn::parse_quote! {
        impl crate::host_value::RilsHostType for #primitive {
            type Native = Number<Self>;
            fn from_native(value: Self::Native) -> Self { value.0 }
            fn as_native_ref(value: &Self::Native) -> &Self { &value.0 }
            fn with_leaf_ref<R>(leaf: &rils_value::NativeLeafRef<'_>, callback: impl FnOnce(&Self) -> R) -> std::result::Result<R, std::string::String> {
                if leaf.is_rust_type::<Self>() { leaf.with_rust(callback) }
                else { leaf.with_rust::<Self::Native, _>(|value| callback(&value.0)) }
            }
        }
    }
}

pub(super) fn execution_bridge(primitive: &Ident, module: &Ident) -> TokenStream {
    let conversions = super::conversion::execution(primitive, module);
    let constructor = format_ident!("native_{primitive}");
    let payload = format_ident!("{primitive}_payload");
    let descriptor = format_ident!("descriptor_{primitive}");
    quote! {
        #conversions
        pub fn #constructor(value: #primitive) -> crate::value::NativeObject {
            crate::value::NativeObject::new(#descriptor(), rils_stdlib::stdlib::#module::Number(value))
                .expect("generated descriptor matches its declared Rust payload")
        }
        pub fn #payload(value: &crate::Value) -> Option<#primitive> {
            value.get_cloned::<#primitive>().ok()
        }
        impl crate::runtime_builtins::native_value::NativeValue for #primitive {
            fn from_value(value: &crate::Value) -> Result<Self, String> {
                value.get_cloned::<Self>()
            }
            fn into_value(self) -> crate::Value {
                #constructor(self).into()
            }
        }
    }
}
