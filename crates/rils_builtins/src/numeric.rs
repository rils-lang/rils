//! Static descriptions of APIs shipped as part of Rils.
//!
//! Nothing in this crate performs I/O or executes script code. Tooling and
//! runtimes share these declarations while choosing their own implementation
//! strategy for runtime, intrinsic and host-backed items.

pub use rils_syntax::IntegerType;

/// A recursive type expression independent of the frontend's concrete `Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypePattern {
    SelfType,
    Generic(&'static str),
    BoundGeneric {
        name: &'static str,
        bounds: &'static [TypePattern],
    },
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
    Array {
        element: &'static TypePattern,
        length: usize,
    },
    ArrayParameter {
        element: &'static TypePattern,
        length: &'static str,
    },
    Slice(&'static TypePattern),
    Function {
        parameters: &'static [TypePattern],
        result: &'static TypePattern,
    },
    Reference {
        mutable: bool,
        inner: &'static TypePattern,
    },
    Associated {
        base: &'static TypePattern,
        trait_name: Option<&'static str>,
        name: &'static str,
        arguments: &'static [TypePattern],
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
    pub symbol: &'static str,
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

pub fn integer_constant(name: &str) -> Option<&'static IntegerConstantDeclaration> {
    crate::INTEGER_CONSTANTS
        .iter()
        .find(|item| item.name == name)
}

pub fn integer_method(name: &str) -> Option<&'static IntrinsicDeclaration> {
    crate::INTEGER_INTRINSICS
        .iter()
        .find(|item| item.kind == IntrinsicKind::Method && item.name == name)
}

pub fn integer_associated_function(name: &str) -> Option<&'static IntrinsicDeclaration> {
    crate::INTEGER_INTRINSICS
        .iter()
        .find(|item| item.kind == IntrinsicKind::AssociatedFunction && item.name == name)
}

pub fn float_method(name: &str) -> Option<&'static IntrinsicDeclaration> {
    crate::FLOAT_INTRINSICS
        .iter()
        .find(|item| item.name == name)
}

pub fn float_constant(name: &str) -> Option<&'static FloatConstantDeclaration> {
    crate::FLOAT_CONSTANTS.iter().find(|item| item.name == name)
}

pub fn intrinsic(symbol: &str) -> Option<&'static IntrinsicDeclaration> {
    crate::INTEGER_INTRINSICS
        .iter()
        .chain(crate::FLOAT_INTRINSICS)
        .find(|item| item.symbol == symbol)
}

/// Resolves a numeric operation path, including the target type of an
/// associated integer function.
pub fn intrinsic_by_symbol(
    symbol: &str,
) -> Option<(&'static IntrinsicDeclaration, Option<IntegerType>)> {
    if let Some(rest) = symbol.strip_prefix("core::integer::")
        && let Some((target, method)) = rest.split_once("::")
        && let Some(target) = IntegerType::from_name(target)
    {
        let declaration = integer_associated_function(method)?;
        return Some((declaration, Some(target)));
    }
    crate::INTEGER_INTRINSICS
        .iter()
        .chain(crate::FLOAT_INTRINSICS)
        .find(|declaration| declaration.symbol == symbol)
        .map(|declaration| (declaration, None))
}
