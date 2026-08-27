//! Static descriptions of APIs shipped as part of Rils.
//!
//! Nothing in this crate performs I/O or executes script code. Tooling and
//! runtimes share these declarations while choosing their own implementation
//! strategy for runtime, intrinsic and host-backed items.

use crate::BuiltinId;
pub use rils_syntax::IntegerType;

/// A recursive type expression independent of the frontend's concrete `Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypePattern {
    SelfType,
    Generic(&'static str),
    AnyInteger,
    Unknown,
    Unit,
    Bool,
    Char,
    String,
    F32,
    F64,
    U32,
    U8,
    Usize,
    Named {
        path: &'static str,
        arguments: &'static [TypePattern],
    },
    Option(&'static TypePattern),
    Result {
        ok: &'static TypePattern,
        error: &'static TypePattern,
    },
    Tuple(&'static [TypePattern]),
    Function {
        parameters: &'static [TypePattern],
        result: &'static TypePattern,
    },
    Reference {
        mutable: bool,
        inner: &'static TypePattern,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinSignature {
    pub parameters: &'static [TypePattern],
    pub result: TypePattern,
    pub variadic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntrinsicKind {
    Method,
    AssociatedFunction,
}

#[derive(Clone, Copy, Debug)]
pub struct IntrinsicDeclaration {
    pub id: BuiltinId,
    pub name: &'static str,
    pub kind: IntrinsicKind,
    pub signature: BuiltinSignature,
    pub documentation: &'static str,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum IntegerConstantId {
    Min,
    Max,
    Bits,
}

#[derive(Clone, Copy, Debug)]
pub struct IntegerConstantDeclaration {
    pub id: IntegerConstantId,
    pub name: &'static str,
    pub value_type: TypePattern,
    pub documentation: &'static str,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum FloatConstantId {
    Min,
    Max,
    Epsilon,
    MinPositive,
    Nan,
    Infinity,
    NegInfinity,
}

#[derive(Clone, Copy, Debug)]
pub struct FloatConstantDeclaration {
    pub id: FloatConstantId,
    pub name: &'static str,
    pub documentation: &'static str,
}

rils_builtins_macros::builtin_numeric_file! {
    "builtin_ids.toml";
    "stdlib/core/integer.rils";
    complete "core::integer";
    family Integer;
    pub const INTEGER_INTRINSICS, INTEGER_CONSTANTS;
}

rils_builtins_macros::builtin_numeric_file! {
    "builtin_ids.toml";
    "stdlib/core/float.rils";
    complete "core::float";
    family Float;
    pub const FLOAT_INTRINSICS, FLOAT_CONSTANTS;
}

pub fn integer_constant(name: &str) -> Option<&'static IntegerConstantDeclaration> {
    INTEGER_CONSTANTS.iter().find(|item| item.name == name)
}

pub fn integer_method(name: &str) -> Option<&'static IntrinsicDeclaration> {
    INTEGER_INTRINSICS
        .iter()
        .find(|item| item.kind == IntrinsicKind::Method && item.name == name)
}

pub fn integer_associated_function(name: &str) -> Option<&'static IntrinsicDeclaration> {
    INTEGER_INTRINSICS
        .iter()
        .find(|item| item.kind == IntrinsicKind::AssociatedFunction && item.name == name)
}

pub fn float_method(name: &str) -> Option<&'static IntrinsicDeclaration> {
    FLOAT_INTRINSICS.iter().find(|item| item.name == name)
}

pub fn float_constant(name: &str) -> Option<&'static FloatConstantDeclaration> {
    FLOAT_CONSTANTS.iter().find(|item| item.name == name)
}

pub fn intrinsic(id: BuiltinId) -> Option<&'static IntrinsicDeclaration> {
    INTEGER_INTRINSICS
        .iter()
        .chain(FLOAT_INTRINSICS)
        .find(|item| item.id == id)
}
