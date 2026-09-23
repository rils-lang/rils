//! Native implementations for the built-in integer types.

use rils_builtins_macros::decl_rils;

use super::prelude::{Option, Result};

/// A value from any Rils integer primitive, used by `try_from`.
pub enum Integer {
    Signed(i128, &'static str),
    Unsigned(u128, &'static str),
}

fn try_convert<T>(value: &Integer) -> std::option::Option<T>
where
    T: TryFrom<i128> + TryFrom<u128>,
{
    match value {
        Integer::Signed(value, _) => T::try_from(*value).ok(),
        Integer::Unsigned(value, _) => T::try_from(*value).ok(),
    }
}

trait IntegerMagnitude: Copy {
    fn checked_abs(self) -> std::option::Option<Self>;
    fn saturating_abs(self) -> Self;
    fn overflowing_abs(self) -> (Self, bool);
    fn saturating_neg(self) -> Self;
}

macro_rules! signed_magnitude {
    ($($ty:ty),* $(,)?) => {$(
        impl IntegerMagnitude for $ty {
            fn checked_abs(self) -> std::option::Option<Self> { self.checked_abs() }
            fn saturating_abs(self) -> Self { self.saturating_abs() }
            fn overflowing_abs(self) -> (Self, bool) { self.overflowing_abs() }
            fn saturating_neg(self) -> Self { self.saturating_neg() }
        }
    )*};
}

macro_rules! unsigned_magnitude {
    ($($ty:ty),* $(,)?) => {$(
        impl IntegerMagnitude for $ty {
            fn checked_abs(self) -> std::option::Option<Self> { Some(self) }
            fn saturating_abs(self) -> Self { self }
            fn overflowing_abs(self) -> (Self, bool) { (self, false) }
            fn saturating_neg(self) -> Self { 0 }
        }
    )*};
}

signed_magnitude!(i8, i16, i32, i64, i128, isize);
unsigned_magnitude!(u8, u16, u32, u64, u128, usize);

#[decl_rils(core::integer)]
mod native {
    use super::{Integer, IntegerMagnitude, Option, Result};

    #[rils_impl(Clone, Copy)]
    primitive_integer_family!(
        i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize,
    );

    #[allow(non_snake_case)]
    impl<TNum> Number<TNum> {
        fn optional(value: std::option::Option<TNum>) -> Option<Self> {
            match value {
                Some(value) => Option::Some(Self(value)),
                None => Option::None,
            }
        }

        fn overflowed((value, overflowed): (TNum, bool)) -> (Self, bool) {
            (Self(value), overflowed)
        }

        /// Converts an integer when its value is representable by the target type.
        #[export_rils]
        pub fn try_from(value: Integer) -> Result<Self, String> {
            let source_name = match &value {
                Integer::Signed(_, name) | Integer::Unsigned(_, name) => name,
            };
            let converted = super::try_convert::<TNum>(&value);
            match converted {
                Some(value) => Result::Ok(Self(value)),
                None => Result::Err(format!(
                    "value of type `{source_name}` is outside the `{}` range",
                    stringify!(TNum)
                )),
            }
        }

        /// Converts to f32, allowing IEEE-754 precision rounding.
        #[export_rils]
        pub fn to_f32(self) -> f32 {
            self.0 as f32
        }
        /// Converts to f64, allowing IEEE-754 precision rounding.
        #[export_rils]
        pub fn to_f64(self) -> f64 {
            self.0 as f64
        }

        /// Returns None on overflow.
        #[export_rils]
        pub fn checked_add(self, other: Self) -> Option<Self> {
            Self::optional(self.0.checked_add(other.0))
        }
        /// Returns None on overflow.
        #[export_rils]
        pub fn checked_sub(self, other: Self) -> Option<Self> {
            Self::optional(self.0.checked_sub(other.0))
        }
        /// Returns None on overflow.
        #[export_rils]
        pub fn checked_mul(self, other: Self) -> Option<Self> {
            Self::optional(self.0.checked_mul(other.0))
        }
        /// Returns None on division failure or overflow.
        #[export_rils]
        pub fn checked_div(self, other: Self) -> Option<Self> {
            Self::optional(self.0.checked_div(other.0))
        }
        /// Returns None on remainder failure or overflow.
        #[export_rils]
        pub fn checked_rem(self, other: Self) -> Option<Self> {
            Self::optional(self.0.checked_rem(other.0))
        }
        /// Returns the negated value, or None when it cannot be represented.
        #[export_rils]
        pub fn checked_neg(self) -> Option<Self> {
            Self::optional(self.0.checked_neg())
        }
        /// Returns the absolute value, or None on signed minimum overflow.
        #[export_rils]
        pub fn checked_abs(self) -> Option<Self> {
            Self::optional(IntegerMagnitude::checked_abs(self.0))
        }
        /// Raises to a power, returning None on overflow.
        #[export_rils]
        pub fn checked_pow(self, exponent: u32) -> Option<Self> {
            Self::optional(self.0.checked_pow(exponent))
        }
        /// Shifts left, returning None when the shift is at least the bit width.
        #[export_rils]
        pub fn checked_shl(self, shift: u32) -> Option<Self> {
            Self::optional(self.0.checked_shl(shift))
        }
        /// Shifts right, returning None when the shift is at least the bit width.
        #[export_rils]
        pub fn checked_shr(self, shift: u32) -> Option<Self> {
            Self::optional(self.0.checked_shr(shift))
        }

        /// Adds with two's-complement wrapping.
        #[export_rils]
        pub fn wrapping_add(self, other: Self) -> Self {
            Self(self.0.wrapping_add(other.0))
        }
        /// Subtracts with two's-complement wrapping.
        #[export_rils]
        pub fn wrapping_sub(self, other: Self) -> Self {
            Self(self.0.wrapping_sub(other.0))
        }
        /// Multiplies with two's-complement wrapping.
        #[export_rils]
        pub fn wrapping_mul(self, other: Self) -> Self {
            Self(self.0.wrapping_mul(other.0))
        }
        /// Negates with two's-complement wrapping.
        #[export_rils]
        pub fn wrapping_neg(self) -> Self {
            Self(self.0.wrapping_neg())
        }
        /// Raises to a power with wrapping arithmetic.
        #[export_rils]
        pub fn wrapping_pow(self, exponent: u32) -> Self {
            Self(self.0.wrapping_pow(exponent))
        }
        /// Shifts left after reducing the shift modulo the bit width.
        #[export_rils]
        pub fn wrapping_shl(self, shift: u32) -> Self {
            Self(self.0.wrapping_shl(shift))
        }
        /// Shifts right after reducing the shift modulo the bit width.
        #[export_rils]
        pub fn wrapping_shr(self, shift: u32) -> Self {
            Self(self.0.wrapping_shr(shift))
        }

        /// Adds while saturating at the numeric bounds.
        #[export_rils]
        pub fn saturating_add(self, other: Self) -> Self {
            Self(self.0.saturating_add(other.0))
        }
        /// Subtracts while saturating at the numeric bounds.
        #[export_rils]
        pub fn saturating_sub(self, other: Self) -> Self {
            Self(self.0.saturating_sub(other.0))
        }
        /// Multiplies while saturating at the numeric bounds.
        #[export_rils]
        pub fn saturating_mul(self, other: Self) -> Self {
            Self(self.0.saturating_mul(other.0))
        }
        /// Negates while saturating at the numeric bounds.
        #[export_rils]
        pub fn saturating_neg(self) -> Self {
            Self(IntegerMagnitude::saturating_neg(self.0))
        }
        /// Returns the absolute value, saturating signed minimum at MAX.
        #[export_rils]
        pub fn saturating_abs(self) -> Self {
            Self(IntegerMagnitude::saturating_abs(self.0))
        }
        /// Raises to a power while saturating at the numeric bounds.
        #[export_rils]
        pub fn saturating_pow(self, exponent: u32) -> Self {
            let mut result: TNum = 1;
            let mut base = self.0;
            let mut power = exponent;
            while power > 0 {
                if power & 1 == 1 {
                    result = result.saturating_mul(base);
                }
                power >>= 1;
                if power > 0 {
                    base = base.saturating_mul(base);
                }
            }
            Self(result)
        }

        /// Returns the wrapped sum and whether overflow occurred.
        #[export_rils]
        pub fn overflowing_add(self, other: Self) -> (Self, bool) {
            Self::overflowed(self.0.overflowing_add(other.0))
        }
        /// Returns the wrapped difference and whether overflow occurred.
        #[export_rils]
        pub fn overflowing_sub(self, other: Self) -> (Self, bool) {
            Self::overflowed(self.0.overflowing_sub(other.0))
        }
        /// Returns the wrapped product and whether overflow occurred.
        #[export_rils]
        pub fn overflowing_mul(self, other: Self) -> (Self, bool) {
            Self::overflowed(self.0.overflowing_mul(other.0))
        }
        /// Returns the wrapped negation and whether overflow occurred.
        #[export_rils]
        pub fn overflowing_neg(self) -> (Self, bool) {
            Self::overflowed(self.0.overflowing_neg())
        }
        /// Returns the wrapped absolute value and whether overflow occurred.
        #[export_rils]
        pub fn overflowing_abs(self) -> (Self, bool) {
            Self::overflowed(IntegerMagnitude::overflowing_abs(self.0))
        }
        /// Returns the wrapped power and whether overflow occurred.
        #[export_rils]
        pub fn overflowing_pow(self, exponent: u32) -> (Self, bool) {
            Self::overflowed(self.0.overflowing_pow(exponent))
        }
        /// Returns the wrapped left shift and whether the shift exceeded the bit width.
        #[export_rils]
        pub fn overflowing_shl(self, shift: u32) -> (Self, bool) {
            Self::overflowed(self.0.overflowing_shl(shift))
        }
        /// Returns the wrapped right shift and whether the shift exceeded the bit width.
        #[export_rils]
        pub fn overflowing_shr(self, shift: u32) -> (Self, bool) {
            Self::overflowed(self.0.overflowing_shr(shift))
        }

        /// Returns the number of one bits.
        #[export_rils]
        pub fn count_ones(self) -> u32 {
            self.0.count_ones()
        }
        /// Returns the number of zero bits.
        #[export_rils]
        pub fn count_zeros(self) -> u32 {
            self.0.count_zeros()
        }
        /// Returns the number of leading zero bits.
        #[export_rils]
        pub fn leading_zeros(self) -> u32 {
            self.0.leading_zeros()
        }
        /// Returns the number of trailing zero bits.
        #[export_rils]
        pub fn trailing_zeros(self) -> u32 {
            self.0.trailing_zeros()
        }
        /// Rotates bits to the left.
        #[export_rils]
        pub fn rotate_left(self, shift: u32) -> Self {
            Self(self.0.rotate_left(shift))
        }
        /// Rotates bits to the right.
        #[export_rils]
        pub fn rotate_right(self, shift: u32) -> Self {
            Self(self.0.rotate_right(shift))
        }
        /// Raises to a non-negative integer power, failing on overflow.
        #[export_rils]
        pub fn pow(self, exponent: u32) -> Self {
            Self(self.0.checked_pow(exponent).expect("integer overflow"))
        }
        /// Computes Euclidean division, failing on zero or overflow.
        #[export_rils]
        pub fn div_euclid(self, other: Self) -> Self {
            Self(
                self.0
                    .checked_div_euclid(other.0)
                    .expect("division by zero or integer overflow"),
            )
        }
        /// Computes the least non-negative remainder, failing on zero or overflow.
        #[export_rils]
        pub fn rem_euclid(self, other: Self) -> Self {
            Self(
                self.0
                    .checked_rem_euclid(other.0)
                    .expect("division by zero or integer overflow"),
            )
        }
        /// Returns the absolute value, failing on signed minimum overflow.
        #[export_rils]
        pub fn abs(self) -> Self {
            Self(IntegerMagnitude::checked_abs(self.0).expect("integer overflow"))
        }
        /// Reverses the byte order.
        #[export_rils]
        pub fn swap_bytes(self) -> Self {
            Self(self.0.swap_bytes())
        }
        /// Reverses the order of bits.
        #[export_rils]
        pub fn reverse_bits(self) -> Self {
            Self(self.0.reverse_bits())
        }

        /// The smallest value representable by this integer type.
        #[constant]
        #[export_rils]
        pub fn MIN() -> Self {
            Self(TNum::MIN)
        }
        /// The largest value representable by this integer type.
        #[constant]
        #[export_rils]
        pub fn MAX() -> Self {
            Self(TNum::MAX)
        }
        /// The width of this integer type in bits.
        #[constant]
        #[export_rils]
        pub fn BITS() -> u32 {
            TNum::BITS
        }
    }
}

pub use native::Number;
