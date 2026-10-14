//! Arithmetic implementations and dispatch derived from a primitive family.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Item};

use super::{Definition, Mapping};

pub(super) fn implementation(primitive: &Ident, float: bool) -> Item {
    let arithmetic = if float {
        quote! {
            Add => ArithmeticResult::Number(Self(left + right)),
            Subtract => ArithmeticResult::Number(Self(left - right)),
            Multiply => ArithmeticResult::Number(Self(left * right)),
            Divide if right == 0.0 => return Err("division by zero".into()),
            Divide => ArithmeticResult::Number(Self(left / right)),
            Remainder if right == 0.0 => return Err("division by zero".into()),
            Remainder => ArithmeticResult::Number(Self(left % right)),
        }
    } else {
        quote! {
            Add => ArithmeticResult::Number(Self(left.checked_add(right).ok_or("integer overflow")?)),
            Subtract => ArithmeticResult::Number(Self(left.checked_sub(right).ok_or("integer overflow")?)),
            Multiply => ArithmeticResult::Number(Self(left.checked_mul(right).ok_or("integer overflow")?)),
            Divide if right == 0 => return Err("division by zero".into()),
            Divide => ArithmeticResult::Number(Self(left.checked_div(right).ok_or("integer overflow")?)),
            Remainder if right == 0 => return Err("division by zero".into()),
            Remainder => ArithmeticResult::Number(Self(left.checked_rem(right).ok_or("integer overflow")?)),
        }
    };
    let negate = if float {
        quote!(Ok(Self(-self.0)))
    } else if primitive.to_string().starts_with('i') {
        quote!(
            self.0
                .checked_neg()
                .map(Self)
                .ok_or_else(|| "integer overflow".into())
        )
    } else {
        quote!(Err(format!(
            "unary `-` expects a signed number, found {}",
            stringify!(#primitive)
        )))
    };
    syn::parse_quote! {
        impl crate::arithmetic::PrimitiveArithmetic for Number<#primitive> {
            fn binary(self, operator: rils_syntax::ast::BinaryOp, right: Self) -> std::result::Result<crate::arithmetic::ArithmeticResult<Self>, std::string::String> {
                use crate::arithmetic::ArithmeticResult;
                use rils_syntax::ast::BinaryOp::*;
                let (left, right) = (self.0, right.0);
                Ok(match operator {
                    #arithmetic
                    Greater => ArithmeticResult::Bool(left > right),
                    GreaterEqual => ArithmeticResult::Bool(left >= right),
                    Less => ArithmeticResult::Bool(left < right),
                    LessEqual => ArithmeticResult::Bool(left <= right),
                    Equal => ArithmeticResult::Bool(left == right),
                    NotEqual => ArithmeticResult::Bool(left != right),
                })
            }
            fn negate(self) -> std::result::Result<Self, std::string::String> { #negate }
        }
    }
}

fn kind(mapping: &Mapping, float: bool) -> TokenStream {
    let variant = mapping.variant();
    if float {
        quote!(crate::FloatType::#variant)
    } else {
        quote!(crate::IntegerType::#variant)
    }
}

pub(super) fn dispatch(definition: &Definition) -> TokenStream {
    let float = definition.float;
    let module = if float {
        format_ident!("float")
    } else {
        format_ident!("integer")
    };
    let kind_type = if float {
        quote!(crate::FloatType)
    } else {
        quote!(crate::IntegerType)
    };
    let binary = definition.family.iter().map(|mapping| {
        let primitive = &mapping.primitive;
        let kind = kind(mapping, float);
        quote!(#kind => crate::numeric::operators::binary::<rils_stdlib::stdlib::#module::Number<#primitive>>(left, operator, right),)
    });
    let negate = definition.family.iter().map(|mapping| {
        let primitive = &mapping.primitive;
        let kind = kind(mapping, float);
        quote!(#kind => crate::numeric::operators::negate::<rils_stdlib::stdlib::#module::Number<#primitive>>(value),)
    });
    let conversion = if float {
        quote!()
    } else {
        integer_conversion(definition)
    };
    quote! {
        pub fn binary(kind: #kind_type, left: &crate::Value, operator: crate::ast::BinaryOp, right: &crate::Value) -> Result<crate::Value, String> {
            match kind { #(#binary)* }
        }
        pub fn negate(kind: #kind_type, value: &crate::Value) -> Result<crate::Value, String> {
            match kind { #(#negate)* }
        }
        #conversion
    }
}

fn integer_conversion(definition: &Definition) -> TokenStream {
    let sources = definition.family.iter().map(|mapping| {
        let primitive = &mapping.primitive;
        let kind = kind(mapping, false);
        let (variant, widened) = if primitive.to_string().starts_with('i') {
            (format_ident!("Signed"), quote!(i128))
        } else { (format_ident!("Unsigned"), quote!(u128)) };
        quote!(Some(crate::Type::Integer(#kind)) => Ok(rils_stdlib::stdlib::integer::Integer::#variant(value.get_cloned::<#primitive>()? as #widened, stringify!(#primitive))),)
    });
    let targets = definition.family.iter().map(|mapping| {
        let primitive = &mapping.primitive;
        let kind = kind(mapping, false);
        let constructor = format_ident!("native_{primitive}");
        quote! {
            #kind => match source {
                Integer::Signed(value, _) => #primitive::try_from(*value).map_err(|_| ()),
                Integer::Unsigned(value, _) => #primitive::try_from(*value).map_err(|_| ()),
            }.map(|value| crate::Value::from(#constructor(value))),
        }
    });
    quote! {
        pub fn integer_value(value: &crate::Value) -> Result<rils_stdlib::stdlib::integer::Integer, String> {
            match crate::Type::of_value(value) {
                #(#sources)*
                _ => Err(format!("expected an integer, found {}", value.type_name())),
            }
        }
        pub fn convert_integer(source: &rils_stdlib::stdlib::integer::Integer, target: crate::IntegerType) -> Result<crate::Value, ()> {
            use rils_stdlib::stdlib::integer::Integer;
            match target { #(#targets)* }
        }
    }
}
