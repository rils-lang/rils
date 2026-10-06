//! Native value storage shared by Rils execution backends.

mod dynamic;
mod native;
mod storage;

pub use dynamic::{
    CompactDynamicObject, DynamicCallContext, DynamicField, DynamicLayout, DynamicObject,
    DynamicPathLease, DynamicPathStep, DynamicType, DynamicValue, DynamicValueRef,
    SequenceBorrowLedger, SequenceItemLease, SequenceIteratorLease,
};
pub use native::{NativeCallContext, NativeChildren, NativeObject, NativeType};
