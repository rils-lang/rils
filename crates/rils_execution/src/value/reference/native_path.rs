//! Native projections retain the owner, layout, and path instead of a snapshot.

use std::rc::Rc;

use rils_value::{
    CompactDynamicObject, DynamicLayout, DynamicPathLease, DynamicPathStep, DynamicValueRef,
};

use super::{NativeRecordCodec, ReferenceTarget, ReferenceValue, Value};
use crate::environment::AssignError;

pub(super) struct NativePath {
    object: Rc<CompactDynamicObject<Value>>,
    path: Vec<DynamicPathStep>,
    codec: Rc<NativeRecordCodec>,
    _lease: DynamicPathLease,
}

impl NativePath {
    pub(super) fn new(
        object: CompactDynamicObject<Value>,
        path: Vec<DynamicPathStep>,
        codec: Rc<NativeRecordCodec>,
    ) -> Result<Self, String> {
        Self::from_shared(Rc::new(object), path, codec)
    }

    fn from_shared(
        object: Rc<CompactDynamicObject<Value>>,
        path: Vec<DynamicPathStep>,
        codec: Rc<NativeRecordCodec>,
    ) -> Result<Self, String> {
        let lease = object.with(|value| value.reference_path(&path))??;
        Ok(Self {
            object,
            path,
            codec,
            _lease: lease,
        })
    }

    pub(super) fn reborrow(&self) -> Result<Self, String> {
        Self::from_shared(self.object.clone(), self.path.clone(), self.codec.clone())
    }

    pub(super) fn with_view<R>(
        &self,
        callback: impl FnOnce(DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        self.object
            .with(|value| Ok(callback(value.view_path(&self.path)?)))?
    }

    pub(super) fn read(&self) -> Result<Value, String> {
        let value = self.with_view(crate::value::runtime_layouts::clone_borrowed_view)??;
        self.codec.from_native(value)
    }

    pub(super) fn copy(&self) -> Result<Value, String> {
        let value = self.with_view(|view| view.copy_owned())??;
        let value = self.codec.from_native(value)?;
        if !value.is_copy() {
            return Err("cannot move a non-Copy field through a reference".into());
        }
        Ok(value)
    }

    pub(super) fn write(&self, value: Value) -> Result<(), AssignError> {
        let layout = self
            .with_view(|view| view.layout())
            .map_err(|_| AssignError::Undefined)?
            .map_err(|_| AssignError::Undefined)?;
        let mut codec = self.codec.as_ref().clone();
        let value = codec
            .into_native(value, layout.clone())
            .map_err(|_| AssignError::TypeMismatch(layout.rils_type().clone()))?;
        self.object
            .with_mut(|payload| payload.replace_path_reference(&self.path, value))
            .map_err(|_| AssignError::BorrowedTarget)?
            .map_err(|_| AssignError::BorrowedTarget)?;
        Ok(())
    }

    fn project(mut self, step: DynamicPathStep) -> Result<Self, String> {
        self.path.push(step);
        Self::from_shared(self.object.clone(), self.path.clone(), self.codec.clone())
    }
}

impl ReferenceValue {
    fn native_path(&self) -> Result<Option<NativePath>, String> {
        let path = match &self.target {
            ReferenceTarget::DynamicField(path) => path.reborrow()?,
            ReferenceTarget::DynamicIndexedElement {
                sequence,
                index,
                field,
                codec,
                ..
            } => {
                let mut path = vec![DynamicPathStep::Index(*index)];
                if let Some(field) = field {
                    path.push(DynamicPathStep::Field(*field));
                }
                NativePath::new(sequence.clone(), path, codec.clone().unwrap_or_default())?
            }
            ReferenceTarget::DynamicCell {
                cell,
                structs,
                enums,
            } => {
                let object = cell.with(|payload| {
                    payload.with::<rils_stdlib::stdlib::cell::ErasedRefCell, _>(|value| {
                        value.value.rebind_shared::<Value>()
                    })
                })???;
                NativePath::new(
                    object.into_compact(),
                    vec![],
                    Rc::new(NativeRecordCodec::with_definitions(structs, enums)),
                )?
            }
            _ => {
                let value = match &self.target {
                    ReferenceTarget::Storage(target) => target
                        .borrow()
                        .read()
                        .map_err(|error| format!("{error:?}"))?,
                    ReferenceTarget::StructField { instance, index } => instance
                        .fields
                        .borrow()
                        .get_index(*index)
                        .and_then(|slot| slot.value.clone())
                        .ok_or("native projection target was moved")?,
                    ReferenceTarget::IndexedElement { sequence, index } => sequence
                        .elements
                        .borrow()
                        .get(*index)
                        .and_then(|slot| slot.value.clone())
                        .ok_or("native projection target was moved")?,
                    _ => return Ok(None),
                };
                match value {
                    Value::Reference(reference) => return reference.native_path(),
                    Value::Dynamic(object) if !object.is_inline() => {
                        let codec = object
                            .descriptor()
                            .metadata::<NativeRecordCodec>()
                            .unwrap_or_default();
                        NativePath::new(object.into_compact(), vec![], codec)?
                    }
                    _ => return Ok(None),
                }
            }
        };
        Ok(Some(path))
    }

    pub fn native_layout(&self) -> Result<Option<Rc<DynamicLayout>>, String> {
        self.native_path()?
            .map(|path| path.with_view(|view| view.layout())?)
            .transpose()
    }

    pub fn native_type_definition(&self) -> Result<Option<Value>, String> {
        let Some(path) = self.native_path()? else {
            return Ok(None);
        };
        let layout = path.with_view(|view| view.layout())??;
        Ok(path.codec.nominal_definition(layout.rils_type()))
    }

    pub fn project_native_field(self: &Rc<Self>, name: &str) -> Result<Option<Self>, String> {
        let Some(path) = self.native_path()? else {
            return Ok(None);
        };
        let layout = path.with_view(|view| view.layout())??;
        let Some(index) = layout.record_field_index(name) else {
            return Ok(None);
        };
        let path = path.project(DynamicPathStep::Field(index))?;
        Ok(Some(Self {
            mutable: self.mutable,
            target: ReferenceTarget::DynamicField(Box::new(path)),
            _guard: Some(self.clone()),
        }))
    }

    pub fn project_native_index(self: &Rc<Self>, index: usize) -> Result<Option<Self>, String> {
        let Some(path) = self.native_path()? else {
            return Ok(None);
        };
        let layout = path.with_view(|view| view.layout())??;
        let step = match layout.rils_type() {
            crate::Type::Array { .. } | crate::Type::Tuple(_) => DynamicPathStep::Field(index),
            crate::Type::Named { .. }
                if super::super::native_layouts::vec::matches(layout.rils_type()) =>
            {
                DynamicPathStep::Index(index)
            }
            _ => return Ok(None),
        };
        let path = path.project(step)?;
        Ok(Some(Self {
            mutable: self.mutable,
            target: ReferenceTarget::DynamicField(Box::new(path)),
            _guard: Some(self.clone()),
        }))
    }

    /// Copy a projected value according to both its physical layout and its
    /// Rils Copy rules, without cloning a non-Copy payload.
    pub fn copy_native(&self) -> Result<Option<Value>, String> {
        self.native_path()?.map(|path| path.copy()).transpose()
    }
}
