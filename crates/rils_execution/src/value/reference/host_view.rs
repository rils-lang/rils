//! Host type mappings borrow either standalone wrappers or composed leaves.

use super::super::borrowed::Read;
use super::ReferenceValue;
use crate::host_value::{RilsHostType, with_host_value};

impl ReferenceValue {
    pub(crate) fn with_host_ref<T: RilsHostType, R>(
        &self,
        callback: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        self.with_borrowed_target(|value| match value {
            Read::View(view) => T::with_composed_ref(view, callback),
            Read::Leaf(leaf) => T::with_leaf_ref(&leaf, callback),
            Read::Legacy(value) => with_host_value(value, callback),
        })?
    }
}
