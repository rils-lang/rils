//! Host payload mappings owned by standard-library declarations.
use rils_value::{DynamicValueRef, NativeLeafRef};

/// Maps a Rust host type to the exact Rust payload stored for its Rils type.
pub trait RilsHostType: Sized + 'static {
    type Native: 'static;

    fn from_native(value: Self::Native) -> Self;
    fn as_native_ref(value: &Self::Native) -> &Self;

    /// Borrow a composed leaf. A type may use a different native
    /// representation for an inline field than for its standalone wrapper.
    fn with_composed_ref<R>(
        view: DynamicValueRef<'_>,
        callback: impl FnOnce(&Self) -> R,
    ) -> Result<R, String> {
        Self::with_leaf_ref(&view.leaf()?, callback)
    }

    /// Read a checked leaf from standalone, composed, or compatibility storage.
    fn with_leaf_ref<R>(
        leaf: &NativeLeafRef<'_>,
        callback: impl FnOnce(&Self) -> R,
    ) -> Result<R, String> {
        leaf.with_rust::<Self::Native, _>(|value| callback(Self::as_native_ref(value)))
    }
}

macro_rules! direct_host_type {
    ($($ty:ty),* $(,)?) => {$(
        impl RilsHostType for $ty {
            type Native = Self;

            fn from_native(value: Self::Native) -> Self { value }
            fn as_native_ref(value: &Self::Native) -> &Self { value }
        }
    )*};
}

direct_host_type!((), bool, char);

impl RilsHostType for String {
    type Native = crate::stdlib::string::String;

    fn from_native(value: Self::Native) -> Self {
        value.into()
    }

    fn as_native_ref(value: &Self::Native) -> &Self {
        value.as_ref()
    }
}
