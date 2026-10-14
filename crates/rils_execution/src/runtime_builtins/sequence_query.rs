//! Invoke declared Rust scalar queries without a runtime method-name table.
use crate::{Type, Value, value::native_receiver::NativeReceiver};
use rils_stdlib::stdlib::collections::vector::Vec as NativeVec;
use rils_value::DynamicValue;

pub(super) trait Output {
    fn into_value(self) -> Value;
}
impl Output for usize {
    fn into_value(self) -> Value {
        crate::value::native_usize(self).into()
    }
}
impl Output for bool {
    fn into_value(self) -> Value {
        crate::value::native_bool(self).into()
    }
}

pub(super) fn with_shared<R: Output>(
    name: &str,
    arguments: &[Value],
    call: impl FnOnce(&NativeVec<DynamicValue>) -> R,
) -> Result<Value, String> {
    let [value] = arguments else {
        return Err("sequence query expects one receiver".into());
    };
    let receiver =
        NativeReceiver::from_value(value)?.ok_or("query requires native sequence storage")?;
    if let Type::Array { length, .. } = receiver.descriptor().layout().rils_type() {
        return match name {
            "len" => Ok(crate::value::native_usize(*length).into()),
            "is_empty" => Ok(crate::value::native_bool(*length == 0).into()),
            _ => Err(format!("array does not support `{name}`")),
        };
    }
    if !crate::value::native_layouts::vec::matches(receiver.descriptor().layout().rils_type()) {
        return Err("query requires a Vec receiver".into());
    }
    receiver.with(|view| {
        view.with_sequence_storage(|items| call(NativeVec::from_storage(items)).into_value())
    })?
}
