//! Typed host access to registered native scalar payloads.

use super::Value;

macro_rules! integer_accessors {
    ($( $from:ident, $read:ident, $rust:ty, $constructor:path, $payload:path; )*) => {
        impl Value {
            $(
                /// Construct an integer in its registered native storage.
                pub fn $from(value: $rust) -> Self {
                    $constructor(value)
                }

                /// Read an integer from native or legacy storage.
                pub fn $read(&self) -> Option<$rust> {
                    $payload(self)
                }
            )*
        }
    };
}

integer_accessors! {
    from_i8, as_i8, i8, crate::numeric::native_i8, crate::numeric::i8_payload;
    from_i16, as_i16, i16, crate::numeric::native_i16, crate::numeric::i16_payload;
    from_i32, as_i32, i32, crate::numeric::native_i32, crate::numeric::i32_payload;
    from_i64, as_i64, i64, crate::numeric::native_i64, crate::numeric::i64_payload;
    from_i128, as_i128, i128, crate::numeric::native_i128, crate::numeric::i128_payload;
    from_isize, as_isize, isize, crate::numeric::native_isize, crate::numeric::isize_payload;
    from_u8, as_u8, u8, crate::numeric::native_u8, crate::numeric::u8_payload;
    from_u16, as_u16, u16, crate::numeric::native_u16, crate::numeric::u16_payload;
    from_u32, as_u32, u32, crate::numeric::native_u32, crate::numeric::u32_payload;
    from_u64, as_u64, u64, crate::numeric::native_u64, crate::numeric::u64_payload;
    from_u128, as_u128, u128, crate::numeric::native_u128, crate::numeric::u128_payload;
    from_usize, as_usize, usize, crate::numeric::native_usize, crate::numeric::usize_payload;
}

impl Value {
    /// Construct an `f32` in its registered native storage.
    pub fn from_f32(value: f32) -> Self {
        crate::numeric::native_f32(value)
    }

    /// Construct an `f64` in its registered native storage.
    pub fn from_f64(value: f64) -> Self {
        crate::numeric::native_f64(value)
    }

    /// Read an `f32` from native or legacy storage.
    pub fn as_f32(&self) -> Option<f32> {
        crate::numeric::f32_payload(self)
    }

    /// Read an `f64` from native or legacy storage.
    pub fn as_f64(&self) -> Option<f64> {
        crate::numeric::f64_payload(self)
    }
}
