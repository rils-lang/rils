//! Half-open integer ranges shared by the runtime's numeric range values.

use rils_stdlib_macros::decl_rils;

use super::prelude::Option;

/// Advances one value without depending on unstable integer stepping traits.
pub trait RangeStep: Copy + Ord {
    fn next_value(self) -> Option<Self>;
}

macro_rules! integer_steps {
    ($($integer:ty),* $(,)?) => {
        $(
            impl RangeStep for $integer {
                fn next_value(self) -> Option<Self> {
                    match self.checked_add(1) {
                        Some(value) => Option::Some(value),
                        None => Option::None,
                    }
                }
            }
        )*
    };
}

integer_steps!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

#[decl_rils(core::iter)]
mod native {
    use super::{Option, RangeStep};

    /// A half-open integer range.
    #[rils_struct]
    #[derive(Clone, PartialEq)]
    pub struct Range<T> {
        current: T,
        end: T,
    }

    impl<T: std::fmt::Display> std::fmt::Display for Range<T> {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "{}..{}", self.current, self.end)
        }
    }

    impl<T: RangeStep> Range<T> {
        pub fn from_bounds(current: T, end: T) -> Self {
            Self { current, end }
        }

        pub fn current(&self) -> T {
            self.current
        }

        fn step(&mut self) -> std::option::Option<T> {
            if self.current >= self.end {
                return None;
            }
            let value = self.current;
            self.current = match self.current.next_value() {
                Option::Some(next) => next,
                Option::None => unreachable!("a value below the exclusive end can advance"),
            };
            Some(value)
        }
    }

    #[rils_impl]
    impl<T: RangeStep> std::iter::Iterator for Range<T> {
        type Item = T;

        /// Advances the range.
        fn next(&mut self) -> std::option::Option<Self::Item> {
            self.step()
        }
    }
}

pub use native::Range;
