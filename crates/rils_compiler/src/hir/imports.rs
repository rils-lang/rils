//! Built-in import signatures understood by bytecode lowering.

use crate::types::{FunctionSignature, Type};

use super::ReceiverMode;
pub(super) fn core_import_signature(name: &str) -> Option<FunctionSignature> {
    Some(match name {
        "type_of" => FunctionSignature::fixed(vec![Type::Unknown], Type::String),
        "clone" => FunctionSignature::fixed(
            vec![Type::Reference {
                mutable: false,
                inner: Box::new(Type::Unknown),
            }],
            Type::Unknown,
        ),
        "is_ok" | "is_err" | "is_some" | "is_none" => {
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool)
        }
        "unwrap" => FunctionSignature::fixed(vec![Type::Unknown], Type::Unknown),
        "unwrap_or" => FunctionSignature::fixed(vec![Type::Unknown, Type::Unknown], Type::Unknown),
        _ => return None,
    })
}

pub(super) fn native_macro_import(
    name: &str,
) -> Option<(&'static str, FunctionSignature, &'static str)> {
    Some(match name {
        "#rils_native_print" => (
            "std::io::print",
            FunctionSignature::variadic(Type::Unit),
            "std::io",
        ),
        "#rils_native_println" => (
            "std::io::println",
            FunctionSignature::variadic(Type::Unit),
            "std::io",
        ),
        "#rils_native_assert" => (
            "core::assert",
            FunctionSignature::variadic(Type::Unit),
            "core",
        ),
        _ => return None,
    })
}

pub(super) fn collection_import_signature(name: &str) -> Option<(&'static str, FunctionSignature)> {
    Some(match name {
        "Vec::new" | "std::collections::Vec::new" => (
            "core::vec::new",
            FunctionSignature::fixed(
                Vec::new(),
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        "Vec::from" | "std::collections::Vec::from" => (
            "core::vec::from",
            FunctionSignature::fixed(
                vec![Type::Unknown],
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        _ => return None,
    })
}

pub(super) fn collection_method_import(
    name: &str,
) -> Option<(&'static str, FunctionSignature, ReceiverMode)> {
    let shared = || Type::Reference {
        mutable: false,
        inner: Box::new(Type::Unknown),
    };
    let mutable = || Type::Reference {
        mutable: true,
        inner: Box::new(Type::Unknown),
    };
    Some(match name {
        "len" => (
            "core::sequence::len",
            FunctionSignature::fixed(vec![shared()], Type::Int),
            ReceiverMode::Reference { mutable: false },
        ),
        "push" => (
            "core::vec::push",
            FunctionSignature::fixed(vec![mutable(), Type::Unknown], Type::Unit),
            ReceiverMode::Reference { mutable: true },
        ),
        "pop" => (
            "core::vec::pop",
            FunctionSignature::fixed(vec![mutable()], Type::Option(Box::new(Type::Unknown))),
            ReceiverMode::Reference { mutable: true },
        ),
        _ => return None,
    })
}
