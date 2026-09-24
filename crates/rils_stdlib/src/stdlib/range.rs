//! Half-open integer ranges shared by the runtime's numeric range values.

use rils_builtins_macros::decl_rils;

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

#[decl_rils(core::range)]
mod native {
    use super::{Option, RangeStep};

    /// A half-open integer range.
    pub struct Range<T> {
        current: T,
        end: T,
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

        /// Advances the range.
        #[allow(clippy::should_implement_trait)] // Rils uses its own Option wrapper.
        #[export_rils]
        pub fn next(&mut self) -> Option<T> {
            match self.step() {
                Some(value) => Option::Some(value),
                None => Option::None,
            }
        }

        /// Consumes the range and creates its iterator.
        #[allow(clippy::should_implement_trait)] // This method is part of the Rils API.
        #[export_rils]
        pub fn into_iter(self) -> Self {
            self
        }
    }

    impl<T: RangeStep> std::iter::Iterator for Range<T> {
        type Item = T;

        fn next(&mut self) -> std::option::Option<Self::Item> {
            self.step()
        }
    }
}

pub use native::Range;
