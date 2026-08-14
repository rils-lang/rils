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
    U32,
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
    IntegerCheckedNeg = 21,
    IntegerCheckedAbs = 22,
    IntegerCheckedPow = 23,
    IntegerCheckedShl = 24,
    IntegerCheckedShr = 25,
    IntegerWrappingAdd = 32,
    IntegerWrappingSub = 33,
    IntegerWrappingMul = 34,
    IntegerWrappingNeg = 35,
    IntegerWrappingPow = 36,
    IntegerWrappingShl = 37,
    IntegerWrappingShr = 38,
    IntegerSaturatingAdd = 48,
    IntegerSaturatingSub = 49,
    IntegerSaturatingMul = 50,
    IntegerSaturatingNeg = 51,
    IntegerSaturatingAbs = 52,
    IntegerSaturatingPow = 53,
    IntegerOverflowingAdd = 64,
    IntegerOverflowingSub = 65,
    IntegerOverflowingMul = 66,
    IntegerOverflowingNeg = 67,
    IntegerOverflowingAbs = 68,
    IntegerOverflowingPow = 69,
    IntegerOverflowingShl = 70,
    IntegerOverflowingShr = 71,
    IntegerCountOnes = 80,
    IntegerCountZeros = 81,
    IntegerLeadingZeros = 82,
    IntegerTrailingZeros = 83,
    IntegerRotateLeft = 84,
    IntegerRotateRight = 85,
    IntegerPow = 86,
    IntegerDivEuclid = 87,
    IntegerRemEuclid = 88,
    IntegerAbs = 89,
    IntegerSwapBytes = 90,
    IntegerReverseBits = 91,
    FloatIsNan = 128,
    FloatIsInfinite = 129,
    FloatIsFinite = 130,
    FloatIsNormal = 131,
    FloatIsSignPositive = 132,
    FloatIsSignNegative = 133,
    FloatAbs = 134,
    FloatSignum = 135,
    FloatCopySign = 136,
    FloatFloor = 137,
    FloatCeil = 138,
    FloatRound = 139,
    FloatTrunc = 140,
    FloatFract = 141,
    FloatSqrt = 142,
    FloatRecip = 143,
    FloatMin = 144,
    FloatMax = 145,
    FloatClamp = 146,
    FloatMulAdd = 147,
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

const SELF: TypePattern = TypePattern::SelfType;
const BOOL: TypePattern = TypePattern::Bool;
const U32: TypePattern = TypePattern::U32;
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
    intrinsic!(IntegerCheckedNeg, "checked_neg", Method, [] -> OPTION_SELF, "Returns the negated value, or None when it cannot be represented."),
    intrinsic!(IntegerCheckedAbs, "checked_abs", Method, [] -> OPTION_SELF, "Returns the absolute value, or None on signed minimum overflow."),
    intrinsic!(IntegerCheckedPow, "checked_pow", Method, [U32] -> OPTION_SELF, "Raises to a power, returning None on overflow."),
    intrinsic!(IntegerCheckedShl, "checked_shl", Method, [U32] -> OPTION_SELF, "Shifts left, returning None when the shift is at least the bit width."),
    intrinsic!(IntegerCheckedShr, "checked_shr", Method, [U32] -> OPTION_SELF, "Shifts right, returning None when the shift is at least the bit width."),
    intrinsic!(IntegerWrappingAdd, "wrapping_add", Method, [SELF] -> SELF, "Adds with two's-complement wrapping."),
    intrinsic!(IntegerWrappingSub, "wrapping_sub", Method, [SELF] -> SELF, "Subtracts with two's-complement wrapping."),
    intrinsic!(IntegerWrappingMul, "wrapping_mul", Method, [SELF] -> SELF, "Multiplies with two's-complement wrapping."),
    intrinsic!(IntegerWrappingNeg, "wrapping_neg", Method, [] -> SELF, "Negates with two's-complement wrapping."),
    intrinsic!(IntegerWrappingPow, "wrapping_pow", Method, [U32] -> SELF, "Raises to a power with wrapping arithmetic."),
    intrinsic!(IntegerWrappingShl, "wrapping_shl", Method, [U32] -> SELF, "Shifts left after reducing the shift modulo the bit width."),
    intrinsic!(IntegerWrappingShr, "wrapping_shr", Method, [U32] -> SELF, "Shifts right after reducing the shift modulo the bit width."),
    intrinsic!(IntegerSaturatingAdd, "saturating_add", Method, [SELF] -> SELF, "Adds while saturating at the numeric bounds."),
    intrinsic!(IntegerSaturatingSub, "saturating_sub", Method, [SELF] -> SELF, "Subtracts while saturating at the numeric bounds."),
    intrinsic!(IntegerSaturatingMul, "saturating_mul", Method, [SELF] -> SELF, "Multiplies while saturating at the numeric bounds."),
    intrinsic!(IntegerSaturatingNeg, "saturating_neg", Method, [] -> SELF, "Negates while saturating at the numeric bounds."),
    intrinsic!(IntegerSaturatingAbs, "saturating_abs", Method, [] -> SELF, "Returns the absolute value, saturating signed minimum at MAX."),
    intrinsic!(IntegerSaturatingPow, "saturating_pow", Method, [U32] -> SELF, "Raises to a power while saturating at the numeric bounds."),
    intrinsic!(IntegerOverflowingAdd, "overflowing_add", Method, [SELF] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped sum and whether overflow occurred."),
    intrinsic!(IntegerOverflowingSub, "overflowing_sub", Method, [SELF] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped difference and whether overflow occurred."),
    intrinsic!(IntegerOverflowingMul, "overflowing_mul", Method, [SELF] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped product and whether overflow occurred."),
    intrinsic!(IntegerOverflowingNeg, "overflowing_neg", Method, [] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped negation and whether overflow occurred."),
    intrinsic!(IntegerOverflowingAbs, "overflowing_abs", Method, [] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped absolute value and whether overflow occurred."),
    intrinsic!(IntegerOverflowingPow, "overflowing_pow", Method, [U32] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped power and whether overflow occurred."),
    intrinsic!(IntegerOverflowingShl, "overflowing_shl", Method, [U32] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped left shift and whether the shift exceeded the bit width."),
    intrinsic!(IntegerOverflowingShr, "overflowing_shr", Method, [U32] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped right shift and whether the shift exceeded the bit width."),
    intrinsic!(IntegerCountOnes, "count_ones", Method, [] -> U32, "Returns the number of one bits."),
    intrinsic!(IntegerCountZeros, "count_zeros", Method, [] -> U32, "Returns the number of zero bits."),
    intrinsic!(IntegerLeadingZeros, "leading_zeros", Method, [] -> U32, "Returns the number of leading zero bits."),
    intrinsic!(IntegerTrailingZeros, "trailing_zeros", Method, [] -> U32, "Returns the number of trailing zero bits."),
    intrinsic!(IntegerRotateLeft, "rotate_left", Method, [U32] -> SELF, "Rotates bits to the left."),
    intrinsic!(IntegerRotateRight, "rotate_right", Method, [U32] -> SELF, "Rotates bits to the right."),
    intrinsic!(IntegerPow, "pow", Method, [U32] -> SELF, "Raises to a non-negative integer power, failing on overflow."),
    intrinsic!(IntegerDivEuclid, "div_euclid", Method, [SELF] -> SELF, "Computes Euclidean division, failing on zero or overflow."),
    intrinsic!(IntegerRemEuclid, "rem_euclid", Method, [SELF] -> SELF, "Computes the least non-negative remainder, failing on zero or overflow."),
    intrinsic!(IntegerAbs, "abs", Method, [] -> SELF, "Returns the absolute value, failing on signed minimum overflow."),
    intrinsic!(IntegerSwapBytes, "swap_bytes", Method, [] -> SELF, "Reverses the byte order."),
    intrinsic!(IntegerReverseBits, "reverse_bits", Method, [] -> SELF, "Reverses the order of bits."),
];

pub const FLOAT_INTRINSICS: &[IntrinsicDeclaration] = &[
    intrinsic!(FloatIsNan, "is_nan", Method, [] -> BOOL, "Returns whether the value is NaN."),
    intrinsic!(FloatIsInfinite, "is_infinite", Method, [] -> BOOL, "Returns whether the value is positive or negative infinity."),
    intrinsic!(FloatIsFinite, "is_finite", Method, [] -> BOOL, "Returns whether the value is neither infinite nor NaN."),
    intrinsic!(FloatIsNormal, "is_normal", Method, [] -> BOOL, "Returns whether the value is neither zero, subnormal, infinite nor NaN."),
    intrinsic!(FloatIsSignPositive, "is_sign_positive", Method, [] -> BOOL, "Returns whether the sign is positive, including positive zero and positive NaN."),
    intrinsic!(FloatIsSignNegative, "is_sign_negative", Method, [] -> BOOL, "Returns whether the sign is negative, including negative zero and negative NaN."),
    intrinsic!(FloatAbs, "abs", Method, [] -> SELF, "Returns the absolute value."),
    intrinsic!(FloatSignum, "signum", Method, [] -> SELF, "Returns 1 or -1 according to the sign, preserving NaN."),
    intrinsic!(FloatCopySign, "copysign", Method, [SELF] -> SELF, "Returns the magnitude of self with the sign of the argument."),
    intrinsic!(FloatFloor, "floor", Method, [] -> SELF, "Returns the greatest integer less than or equal to the value."),
    intrinsic!(FloatCeil, "ceil", Method, [] -> SELF, "Returns the smallest integer greater than or equal to the value."),
    intrinsic!(FloatRound, "round", Method, [] -> SELF, "Rounds to the nearest integer, with halfway cases away from zero."),
    intrinsic!(FloatTrunc, "trunc", Method, [] -> SELF, "Returns the integer part of the value."),
    intrinsic!(FloatFract, "fract", Method, [] -> SELF, "Returns the fractional part of the value."),
    intrinsic!(FloatSqrt, "sqrt", Method, [] -> SELF, "Returns the square root, or NaN for a negative value."),
    intrinsic!(FloatRecip, "recip", Method, [] -> SELF, "Returns the reciprocal."),
    intrinsic!(FloatMin, "min", Method, [SELF] -> SELF, "Returns the minimum, ignoring NaN when exactly one operand is NaN."),
    intrinsic!(FloatMax, "max", Method, [SELF] -> SELF, "Returns the maximum, ignoring NaN when exactly one operand is NaN."),
    intrinsic!(FloatClamp, "clamp", Method, [SELF, SELF] -> SELF, "Restricts the value to the inclusive interval, failing for invalid bounds."),
    intrinsic!(FloatMulAdd, "mul_add", Method, [SELF, SELF] -> SELF, "Computes self * a + b with one rounding operation."),
];

pub const INTEGER_CONSTANTS: &[IntegerConstantDeclaration] = &[
    IntegerConstantDeclaration {
        id: IntegerConstantId::Min,
        name: "MIN",
        value_type: TypePattern::SelfType,
        documentation: "The smallest value representable by this integer type.",
    },
    IntegerConstantDeclaration {
        id: IntegerConstantId::Max,
        name: "MAX",
        value_type: TypePattern::SelfType,
        documentation: "The largest value representable by this integer type.",
    },
    IntegerConstantDeclaration {
        id: IntegerConstantId::Bits,
        name: "BITS",
        value_type: TypePattern::U32,
        documentation: "The width of this integer type in bits.",
    },
];

pub const FLOAT_CONSTANTS: &[FloatConstantDeclaration] = &[
    FloatConstantDeclaration {
        id: FloatConstantId::Min,
        name: "MIN",
        documentation: "The smallest finite value.",
    },
    FloatConstantDeclaration {
        id: FloatConstantId::Max,
        name: "MAX",
        documentation: "The largest finite value.",
    },
    FloatConstantDeclaration {
        id: FloatConstantId::Epsilon,
        name: "EPSILON",
        documentation: "The difference between 1 and the next representable value.",
    },
    FloatConstantDeclaration {
        id: FloatConstantId::MinPositive,
        name: "MIN_POSITIVE",
        documentation: "The smallest positive normal value.",
    },
    FloatConstantDeclaration {
        id: FloatConstantId::Nan,
        name: "NAN",
        documentation: "A not-a-number value.",
    },
    FloatConstantDeclaration {
        id: FloatConstantId::Infinity,
        name: "INFINITY",
        documentation: "Positive infinity.",
    },
    FloatConstantDeclaration {
        id: FloatConstantId::NegInfinity,
        name: "NEG_INFINITY",
        documentation: "Negative infinity.",
    },
];

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

pub fn intrinsic(id: IntrinsicId) -> Option<&'static IntrinsicDeclaration> {
    INTEGER_INTRINSICS
        .iter()
        .chain(FLOAT_INTRINSICS)
        .find(|item| item.id == id)
}
