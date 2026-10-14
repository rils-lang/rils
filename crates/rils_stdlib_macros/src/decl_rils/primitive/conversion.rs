//! Signature conversion glue derived from one numeric family member.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

pub(super) fn execution(primitive: &Ident, module: &Ident) -> TokenStream {
    let constructor = format_ident!("native_{primitive}");
    let wrapper = quote!(rils_stdlib::stdlib::#module::Number<#primitive>);
    let mut spelling = primitive.to_string();
    spelling[..1].make_ascii_uppercase();
    let variant = format_ident!("{spelling}");
    let float = module == "float";
    let input = quote!(super::copy_input::<#primitive>(value).map(Self));
    let output_type = if float {
        quote!(crate::Type::Float(crate::FloatType::#variant))
    } else {
        quote!(crate::Type::Integer(crate::IntegerType::#variant))
    };
    quote! {
        impl super::NativeInput for #wrapper {
            fn from_value(value: &crate::Value) -> Result<Self, String> { #input }
        }
        impl super::NativeOutput for #wrapper {
            fn output_type() -> crate::Type { #output_type }
            fn into_value(self) -> Result<crate::Value, String> { Ok(#constructor(self.0).into()) }
        }
        impl super::NativeInput for #primitive {
            fn from_value(value: &crate::Value) -> Result<Self, String> { super::copy_input::<Self>(value) }
        }
        impl super::NativeOutput for #primitive {
            fn output_type() -> crate::Type { #output_type }
            fn into_value(self) -> Result<crate::Value, String> { Ok(#constructor(self).into()) }
        }
    }
}
