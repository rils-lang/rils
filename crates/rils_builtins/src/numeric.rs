//! Static descriptions of APIs shipped as part of Rils.
//!
//! Nothing in this crate performs I/O or executes script code. Tooling and
//! runtimes share these declarations while choosing their own implementation
//! strategy for runtime, intrinsic and host-backed items.

use core::fmt;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum IntegerType {
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
}

impl IntegerType {
    pub const ALL: [Self; 12] = [
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::I128,
        Self::Isize,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::U128,
        Self::Usize,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::Isize => "isize",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::Usize => "usize",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub const fn is_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128 | Self::Isize
        )
    }
    pub const fn bits(self) -> u32 {
        match self {
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 => 32,
            Self::I64 | Self::U64 => 64,
            Self::I128 | Self::U128 => 128,
            Self::Isize | Self::Usize => usize::BITS,
        }
    }
    pub const fn can_cast_losslessly_to(self, target: Self) -> bool {
        match (self.is_signed(), target.is_signed()) {
            (true, true) | (false, false) | (true, false) => target.bits() >= self.bits(),
            (false, true) => target.bits() > self.bits(),
        }
    }
    pub const fn can_represent_all(self, source: Self) -> bool {
        source.can_cast_losslessly_to(self)
    }
}

impl fmt::Display for IntegerType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// A recursive type expression independent of the frontend's concrete `Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypePattern {
    SelfType,
    Generic(&'static str),
    AnyInteger,
    Unknown,
    Unit,
    Bool,
    String,
    F32,
    F64,
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

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(u16)]
pub enum IntrinsicId {
    IntegerTryFrom = 1,
    IntegerToF32 = 2,
    IntegerToF64 = 3,
    IntegerCheckedAdd = 16,
    IntegerCheckedSub = 17,
    IntegerCheckedMul = 18,
    IntegerCheckedDiv = 19,
    IntegerCheckedRem = 20,
    IntegerWrappingAdd = 32,
    IntegerWrappingSub = 33,
    IntegerWrappingMul = 34,
    IntegerSaturatingAdd = 48,
    IntegerSaturatingSub = 49,
    IntegerSaturatingMul = 50,
    IntegerOverflowingAdd = 64,
    IntegerOverflowingSub = 65,
    IntegerOverflowingMul = 66,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntrinsicKind {
    Method,
    AssociatedFunction,
}

#[derive(Clone, Copy, Debug)]
pub struct IntrinsicDeclaration {
    pub id: IntrinsicId,
    pub name: &'static str,
    pub kind: IntrinsicKind,
    pub signature: BuiltinSignature,
    pub documentation: &'static str,
}

const SELF: TypePattern = TypePattern::SelfType;
const BOOL: TypePattern = TypePattern::Bool;
const STRING: TypePattern = TypePattern::String;
const OPTION_SELF: TypePattern = TypePattern::Option(&SELF);
const RESULT_SELF_STRING: TypePattern = TypePattern::Result {
    ok: &SELF,
    error: &STRING,
};
const SELF_BOOL: &[TypePattern] = &[SELF, BOOL];

macro_rules! intrinsic {
    ($id:ident, $name:literal, $kind:ident, [$($parameter:expr),* $(,)?] -> $result:expr, $documentation:literal) => {
        IntrinsicDeclaration {
            id: IntrinsicId::$id,
            name: $name,
            kind: IntrinsicKind::$kind,
            signature: BuiltinSignature {
                parameters: &[$($parameter),*], result: $result, variadic: false,
            },
            documentation: $documentation,
        }
    };
}

pub const INTEGER_INTRINSICS: &[IntrinsicDeclaration] = &[
    intrinsic!(IntegerTryFrom, "try_from", AssociatedFunction, [TypePattern::AnyInteger] -> RESULT_SELF_STRING, "Converts an integer when its value is representable by the target type."),
    intrinsic!(IntegerToF32, "to_f32", Method, [] -> TypePattern::F32, "Converts to f32, allowing IEEE-754 precision rounding."),
    intrinsic!(IntegerToF64, "to_f64", Method, [] -> TypePattern::F64, "Converts to f64, allowing IEEE-754 precision rounding."),
    intrinsic!(IntegerCheckedAdd, "checked_add", Method, [SELF] -> OPTION_SELF, "Returns None on overflow."),
    intrinsic!(IntegerCheckedSub, "checked_sub", Method, [SELF] -> OPTION_SELF, "Returns None on overflow."),
    intrinsic!(IntegerCheckedMul, "checked_mul", Method, [SELF] -> OPTION_SELF, "Returns None on overflow."),
    intrinsic!(IntegerCheckedDiv, "checked_div", Method, [SELF] -> OPTION_SELF, "Returns None on division failure or overflow."),
    intrinsic!(IntegerCheckedRem, "checked_rem", Method, [SELF] -> OPTION_SELF, "Returns None on remainder failure or overflow."),
    intrinsic!(IntegerWrappingAdd, "wrapping_add", Method, [SELF] -> SELF, "Adds with two's-complement wrapping."),
    intrinsic!(IntegerWrappingSub, "wrapping_sub", Method, [SELF] -> SELF, "Subtracts with two's-complement wrapping."),
    intrinsic!(IntegerWrappingMul, "wrapping_mul", Method, [SELF] -> SELF, "Multiplies with two's-complement wrapping."),
    intrinsic!(IntegerSaturatingAdd, "saturating_add", Method, [SELF] -> SELF, "Adds while saturating at the numeric bounds."),
    intrinsic!(IntegerSaturatingSub, "saturating_sub", Method, [SELF] -> SELF, "Subtracts while saturating at the numeric bounds."),
    intrinsic!(IntegerSaturatingMul, "saturating_mul", Method, [SELF] -> SELF, "Multiplies while saturating at the numeric bounds."),
    intrinsic!(IntegerOverflowingAdd, "overflowing_add", Method, [SELF] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped sum and whether overflow occurred."),
    intrinsic!(IntegerOverflowingSub, "overflowing_sub", Method, [SELF] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped difference and whether overflow occurred."),
    intrinsic!(IntegerOverflowingMul, "overflowing_mul", Method, [SELF] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped product and whether overflow occurred."),
];

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
