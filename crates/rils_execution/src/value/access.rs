//! Typed access delegates payload mappings to standard-library declarations.
use super::Value;
use crate::RilsHostType;

impl Value {
    /// Borrows a composed layout without cloning or decoding its children.
    pub fn with_native_view<R>(
        &self,
        callback: impl FnOnce(rils_value::DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        crate::host_value::with_native_value(self, callback)
    }

    pub fn with_ref<T: RilsHostType, R>(
        &self,
        callback: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        crate::host_value::with_host_value(self, callback)
    }

    /// Explicitly clones a typed payload; non-Copy values are never copied implicitly.
    pub fn get_cloned<T: RilsHostType + Clone>(&self) -> Result<T, String> {
        self.with_ref::<T, _>(Clone::clone)
    }
}
