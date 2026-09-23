//! Native methods for the built-in floating-point types.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::float)]
mod native {
    primitive_float_family!(f32, f64);

    #[allow(non_snake_case)]
    impl<TNum> Number<TNum> {
        /// Returns whether the value is NaN.
        #[export_rils]
        pub fn is_nan(self) -> bool {
            self.0.is_nan()
        }
        /// Returns whether the value is positive or negative infinity.
        #[export_rils]
        pub fn is_infinite(self) -> bool {
            self.0.is_infinite()
        }
        /// Returns whether the value is neither infinite nor NaN.
        #[export_rils]
        pub fn is_finite(self) -> bool {
            self.0.is_finite()
        }
        /// Returns whether the value is neither zero, subnormal, infinite nor NaN.
        #[export_rils]
        pub fn is_normal(self) -> bool {
            self.0.is_normal()
        }
        /// Returns whether the sign is positive, including positive zero and positive NaN.
        #[export_rils]
        pub fn is_sign_positive(self) -> bool {
            self.0.is_sign_positive()
        }
        /// Returns whether the sign is negative, including negative zero and negative NaN.
        #[export_rils]
        pub fn is_sign_negative(self) -> bool {
            self.0.is_sign_negative()
        }
        /// Returns the absolute value.
        #[export_rils]
        pub fn abs(self) -> Self {
            Self(self.0.abs())
        }
        /// Returns 1 or -1 according to the sign, preserving NaN.
        #[export_rils]
        pub fn signum(self) -> Self {
            Self(self.0.signum())
        }
        /// Returns the magnitude of self with the sign of the argument.
        #[export_rils]
        pub fn copysign(self, sign: Self) -> Self {
            Self(self.0.copysign(sign.0))
        }
        /// Returns the greatest integer less than or equal to the value.
        #[export_rils]
        pub fn floor(self) -> Self {
            Self(self.0.floor())
        }
        /// Returns the smallest integer greater than or equal to the value.
        #[export_rils]
        pub fn ceil(self) -> Self {
            Self(self.0.ceil())
        }
        /// Rounds to the nearest integer, with halfway cases away from zero.
        #[export_rils]
        pub fn round(self) -> Self {
            Self(self.0.round())
        }
        /// Returns the integer part of the value.
        #[export_rils]
        pub fn trunc(self) -> Self {
            Self(self.0.trunc())
        }
        /// Returns the fractional part of the value.
        #[export_rils]
        pub fn fract(self) -> Self {
            Self(self.0.fract())
        }
        /// Returns the square root, or NaN for a negative value.
        #[export_rils]
        pub fn sqrt(self) -> Self {
            Self(self.0.sqrt())
        }
        /// Returns the reciprocal.
        #[export_rils]
        pub fn recip(self) -> Self {
            Self(self.0.recip())
        }
        /// Returns the minimum, ignoring NaN when exactly one operand is NaN.
        #[export_rils]
        pub fn min(self, other: Self) -> Self {
            Self(self.0.min(other.0))
        }
        /// Returns the maximum, ignoring NaN when exactly one operand is NaN.
        #[export_rils]
        pub fn max(self, other: Self) -> Self {
            Self(self.0.max(other.0))
        }
        /// Restricts the value to the inclusive interval, failing for invalid bounds.
        #[export_rils]
        pub fn clamp(self, minimum: Self, maximum: Self) -> Self {
            Self(self.0.clamp(minimum.0, maximum.0))
        }
        /// Computes self * a + b with one rounding operation.
        #[export_rils]
        pub fn mul_add(self, multiplier: Self, addend: Self) -> Self {
            Self(self.0.mul_add(multiplier.0, addend.0))
        }

        /// The smallest finite value.
        #[export_rils]
        #[constant]
        pub fn MIN() -> Self {
            Self(TNum::MIN)
        }
        /// The largest finite value.
        #[export_rils]
        #[constant]
        pub fn MAX() -> Self {
            Self(TNum::MAX)
        }
        /// The difference between 1 and the next representable value.
        #[export_rils]
        #[constant]
        pub fn EPSILON() -> Self {
            Self(TNum::EPSILON)
        }
        /// The smallest positive normal value.
        #[export_rils]
        #[constant]
        pub fn MIN_POSITIVE() -> Self {
            Self(TNum::MIN_POSITIVE)
        }
        /// A not-a-number value.
        #[export_rils]
        #[constant]
        pub fn NAN() -> Self {
            Self(TNum::NAN)
        }
        /// Positive infinity.
        #[export_rils]
        #[constant]
        pub fn INFINITY() -> Self {
            Self(TNum::INFINITY)
        }
        /// Negative infinity.
        #[export_rils]
        #[constant]
        pub fn NEG_INFINITY() -> Self {
            Self(TNum::NEG_INFINITY)
        }
    }
}

pub use native::Number;
