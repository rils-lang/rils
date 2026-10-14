//! Host type mappings borrow either standalone wrappers or composed leaves.

use super::super::borrowed::Read;
use super::ReferenceValue;
use crate::host_value::RilsHostType;

impl ReferenceValue {
    pub(crate) fn with_host_ref<T: RilsHostType, R>(
        &self,
        callback: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        self.with_borrowed_target(|value| match value {
            Read::View(view) if view.layout()?.is_rust_type::<std::rc::Rc<ReferenceValue>>() => {
                view.with_rust::<std::rc::Rc<ReferenceValue>, _>(|reference| {
                    reference.with_host_ref(callback)
                })?
            }
            Read::Leaf(leaf) if leaf.is_rust_type::<std::rc::Rc<ReferenceValue>>() => {
                leaf.with_rust::<std::rc::Rc<ReferenceValue>, _>(|reference| {
                    reference.with_host_ref(callback)
                })?
            }
            Read::View(view) => T::with_composed_ref(view, callback),
            Read::Leaf(leaf) => T::with_leaf_ref(&leaf, callback),
            Read::Host(object) => Err(format!(
                "host object {} has no registered Rust borrow view",
                object.type_definition.name
            )),
        })?
    }
}
