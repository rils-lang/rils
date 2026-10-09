//! Borrow the original compatibility key without reconstructing a Value.
use super::HashKey;
use crate::value::{
    borrowed::{Read, with_read},
    record_codec::NativeRecordCodec,
};
use std::rc::Rc;
impl HashKey {
    pub(crate) fn with_native_view<R>(
        &self,
        callback: impl FnOnce(rils_value::DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        crate::host_value::with_native_value(&self.value, callback)
    }
    pub(crate) fn with_read<R>(
        &self,
        callback: impl for<'a> FnOnce(Read<'a>) -> R,
    ) -> Result<R, String> {
        with_read(&self.value, false, callback)
    }
    pub(crate) fn native_codec(&self) -> Result<Option<Rc<NativeRecordCodec>>, String> {
        crate::value::borrowed::native_codec(&self.value)
    }
}
