//! Native value storage shared by Rils execution backends.

mod dynamic;
mod native;
mod storage;

pub use dynamic::{DynamicCallContext, DynamicLayout, DynamicType, DynamicValue};
pub use native::{NativeCallContext, NativeChildren, NativeObject, NativeType};
