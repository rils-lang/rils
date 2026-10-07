//! Declaration-owned equality of borrowed Rust leaves, independent of Value.

use rils_value::{DynamicLayout, DynamicValueRef, NativeLeafRef};

use crate::NativeRegistry;

pub struct EqualityRegistration {
    operation: Operation,
}

pub type EqualChildren = fn(&DynamicValueRef<'_>, &DynamicValueRef<'_>) -> Result<bool, String>;
pub type ViewEquality =
    fn(&DynamicValueRef<'_>, &DynamicValueRef<'_>, EqualChildren) -> Result<bool, String>;

enum Operation {
    Leaf {
        matches: fn(&NativeLeafRef<'_>) -> bool,
        equal: fn(&NativeLeafRef<'_>, &NativeLeafRef<'_>) -> Result<bool, String>,
    },
    View {
        matches: fn(&DynamicLayout) -> bool,
        equal: ViewEquality,
    },
}

impl EqualityRegistration {
    /// Compare a wrapper and its registered leaf through a safe borrowed mapping.
    pub const fn projected<T: AsRef<U> + 'static, U: PartialEq + 'static>() -> Self {
        fn matches<T: 'static, U: 'static>(leaf: &NativeLeafRef<'_>) -> bool {
            leaf.is_rust_type::<T>() || leaf.is_rust_type::<U>()
        }
        fn read<T: AsRef<U> + 'static, U: 'static, R>(
            leaf: &NativeLeafRef<'_>,
            callback: impl FnOnce(&U) -> R,
        ) -> Result<R, String> {
            if leaf.is_rust_type::<U>() {
                leaf.with_rust::<U, _>(callback)
            } else {
                leaf.with_rust::<T, _>(|wrapper| callback(wrapper.as_ref()))
            }
        }
        fn equal<T: AsRef<U> + 'static, U: PartialEq + 'static>(
            left: &NativeLeafRef<'_>,
            right: &NativeLeafRef<'_>,
        ) -> Result<bool, String> {
            read::<T, U, _>(left, |left| read::<T, U, _>(right, |right| left == right))?
        }
        Self {
            operation: Operation::Leaf {
                matches: matches::<T, U>,
                equal: equal::<T, U>,
            },
        }
    }

    pub const fn of<T: PartialEq + 'static>() -> Self {
        fn matches<T: 'static>(leaf: &NativeLeafRef<'_>) -> bool {
            leaf.is_rust_type::<T>()
        }
        fn equal<T: PartialEq + 'static>(
            left: &NativeLeafRef<'_>,
            right: &NativeLeafRef<'_>,
        ) -> Result<bool, String> {
            left.with_rust::<T, _>(|left| right.with_rust::<T, _>(|right| left == right))?
        }
        Self {
            operation: Operation::Leaf {
                matches: matches::<T>,
                equal: equal::<T>,
            },
        }
    }

    pub const fn view(matches: fn(&DynamicLayout) -> bool, equal: ViewEquality) -> Self {
        Self {
            operation: Operation::View { matches, equal },
        }
    }
}

impl NativeRegistry {
    pub fn equal_leaf(
        &self,
        left: &NativeLeafRef<'_>,
        right: &NativeLeafRef<'_>,
    ) -> Result<bool, String> {
        if left.rils_type() != right.rils_type() {
            return Ok(false);
        }
        let equal = self
            .equalities
            .iter()
            .find_map(|registration| match registration.operation {
                Operation::Leaf { matches, equal } if matches(left) && matches(right) => {
                    Some(equal)
                }
                _ => None,
            })
            .ok_or_else(|| format!("{} has no registered native equality", left.rils_type()))?;
        equal(left, right)
    }

    pub fn equal_view(
        &self,
        left: &DynamicValueRef<'_>,
        right: &DynamicValueRef<'_>,
        children: EqualChildren,
    ) -> Option<Result<bool, String>> {
        let layouts = left
            .layout()
            .and_then(|left| right.layout().map(|right| (left, right)));
        let (left_layout, right_layout) = match layouts {
            Ok(layouts) => layouts,
            Err(error) => return Some(Err(error)),
        };
        self.equalities
            .iter()
            .find_map(|registration| match registration.operation {
                Operation::View { matches, equal }
                    if matches(&left_layout) && matches(&right_layout) =>
                {
                    Some(equal(left, right, children))
                }
                _ => None,
            })
    }
}
