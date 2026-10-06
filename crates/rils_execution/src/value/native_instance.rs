//! Native user instances and owner projections, independent of either backend.

use std::rc::Rc;

use rils_value::{DynamicPathStep, DynamicType, DynamicValue, DynamicValueRef};

use super::{DynamicObject, ReferenceValue, Value, record_codec::NativeRecordCodec};

/// Retain declaration identities alongside bytes, without reconstructing
/// StructFields or enum payload slots. Copy instances still need a stable
/// owner so lexical references observe later field writes.
pub fn from_native(value: DynamicValue, codec: Rc<NativeRecordCodec>) -> Result<Value, String> {
    let descriptor = Rc::new(DynamicType::new(value.layout_handle()).register_metadata(codec));
    with_descriptor(descriptor, value).map(Value::Dynamic)
}

pub(super) fn with_descriptor(
    descriptor: Rc<DynamicType<Value>>,
    value: DynamicValue,
) -> Result<DynamicObject, String> {
    let nominal = descriptor
        .metadata::<NativeRecordCodec>()
        .is_some_and(|codec| {
            codec
                .nominal_definition(descriptor.layout().rils_type())
                .is_some()
        });
    if nominal {
        DynamicObject::new_shared(descriptor, value)
    } else {
        DynamicObject::new(descriptor, value)
    }
}

pub fn definition(object: &DynamicObject) -> Option<Value> {
    object
        .descriptor()
        .metadata::<NativeRecordCodec>()?
        .nominal_definition(object.descriptor().layout().rils_type())
}

fn expose(value: DynamicValue, codec: Rc<NativeRecordCodec>) -> Result<Value, String> {
    if codec
        .nominal_definition(value.descriptor().rils_type())
        .is_some()
    {
        from_native(value, codec)
    } else {
        codec.from_native(value)
    }
}

/// An owner place carries a checked path, but does not acquire a lexical
/// reference. Moving one field therefore cannot block itself with a lease.
/// Callers enforce source mutability; borrow() adds the actual lexical lease.
#[derive(Clone)]
pub struct NativeInstancePlace {
    object: DynamicObject,
    path: Vec<DynamicPathStep>,
    codec: Rc<NativeRecordCodec>,
}

impl NativeInstancePlace {
    pub fn new(object: DynamicObject) -> Result<Self, String> {
        if object.is_inline() {
            return Err("native owner place requires shared storage".into());
        }
        let codec = object
            .descriptor()
            .metadata::<NativeRecordCodec>()
            .ok_or("native instance has no declaration context")?;
        Ok(Self {
            object,
            path: vec![],
            codec,
        })
    }

    pub fn with_view<R>(
        &self,
        callback: impl FnOnce(DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        self.object
            .with(|value| Ok(callback(value.view_path(&self.path)?)))?
    }

    pub fn field(&self, name: &str) -> Result<Self, String> {
        let layout = self.with_view(|view| view.layout())??;
        let index = layout
            .record_field_index(name)
            .ok_or_else(|| format!("{} has no field `{name}`", layout.rils_type()))?;
        // Resolve against declaration metadata, including a moved final field.
        let mut projected = self.clone();
        projected.path.push(DynamicPathStep::Field(index));
        Ok(projected)
    }

    /// Project an active sum or sequence without materializing its parents.
    pub fn project(&self, step: DynamicPathStep) -> Result<Self, String> {
        let mut projected = self.clone();
        projected.path.push(step);
        projected.with_view(|_| ())?;
        Ok(projected)
    }

    pub fn take(&self) -> Result<Value, String> {
        let layout = self.with_view(|view| {
            if view.is_partially_moved()? {
                return Err("cannot move a partially moved native value".into());
            }
            view.layout()
        })??;
        let payload = if layout.is_copy() {
            self.with_view(|view| view.copy_owned())??
        } else {
            self.object
                .with_mut(|value| value.take_path_field(&self.path))??
        };
        expose(payload, self.codec.clone())
    }

    pub fn assign(&self, value: Value) -> Result<(), String> {
        let layout = self
            .object
            .with(|value| value.path_field_layout(&self.path))??;
        let value = self
            .codec
            .as_ref()
            .clone()
            .with_owned_conversion()
            .into_native(value, layout)?;
        self.object
            .with_mut(|payload| payload.replace_path_field(&self.path, value))??;
        Ok(())
    }

    pub fn borrow(
        &self,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<ReferenceValue, String> {
        if self.with_view(|view| view.is_partially_moved())?? {
            return Err("cannot reference a partially moved native value".into());
        }
        ReferenceValue::new_native_path(
            self.object.clone(),
            self.path.clone(),
            mutable,
            guard,
            self.codec.clone(),
        )
    }
}
