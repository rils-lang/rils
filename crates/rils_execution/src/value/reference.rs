use std::rc::Rc;

use rils_stdlib::stdlib::cell::ErasedRefCell;
use rils_value::{CompactDynamicObject, DynamicValueRef, SequenceItemLease};

use crate::environment::{AssignError, EnvironmentRef, StorageRef};

use super::record_codec::NativeRecordCodec;

#[path = "reference/host_view.rs"]
mod host_view;
#[path = "reference/inspection.rs"]
mod inspection;
#[path = "reference/native_path.rs"]
mod native_path;
#[path = "reference/operations.rs"]
mod operations;
#[path = "reference/value.rs"]
mod value;
use super::{DynamicObject, EnumType, StructType, Value};
use native_path::NativePath;

pub struct ReferenceValue {
    pub mutable: bool,
    target: ReferenceTarget,
    _guard: Option<Rc<ReferenceValue>>,
}

enum ReferenceTarget {
    Storage(StorageRef),
    DynamicIndexedElement {
        sequence: CompactDynamicObject<Value>,
        index: usize,
        field: Option<usize>,
        _lease: SequenceItemLease,
        codec: Option<Rc<NativeRecordCodec>>,
    },
    DynamicCell {
        cell: Rc<super::native_receiver::NativeReceiver>,
        codec: Rc<NativeRecordCodec>,
    },
    DynamicField(Box<NativePath>),
}

impl ReferenceValue {
    pub fn is_local_to(&self, environment: &EnvironmentRef) -> bool {
        self._guard
            .as_ref()
            .is_some_and(|guard| guard.is_local_to(environment))
            || match &self.target {
                ReferenceTarget::Storage(target) => environment.borrow().owns_storage(target),
                ReferenceTarget::DynamicIndexedElement { .. }
                | ReferenceTarget::DynamicCell { .. }
                | ReferenceTarget::DynamicField(_) => false,
            }
    }

    pub fn new_storage(target: StorageRef, mutable: bool) -> Self {
        target.borrow_mut().add_reference();
        Self {
            mutable,
            target: ReferenceTarget::Storage(target),
            _guard: None,
        }
    }

    /// A Rils reference stores an index and a lease, never a Rust reference
    /// into the dynamic sequence. Each access borrows the payload briefly.
    pub fn new_guarded_dynamic_indexed_element(
        sequence: DynamicObject,
        index: usize,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        Self::new_guarded_dynamic_indexed_field(sequence, index, None, mutable, guard)
    }

    pub fn new_guarded_dynamic_indexed_element_with_codec(
        sequence: DynamicObject,
        index: usize,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
        codec: Option<Rc<NativeRecordCodec>>,
    ) -> Result<Self, String> {
        Self::new_guarded_dynamic_indexed_field_with_codec(
            sequence, index, None, mutable, guard, codec,
        )
    }

    pub fn new_guarded_dynamic_indexed_field(
        sequence: DynamicObject,
        index: usize,
        field: Option<usize>,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        Self::new_guarded_dynamic_indexed_field_with_codec(
            sequence, index, field, mutable, guard, None,
        )
    }

    pub fn new_guarded_dynamic_indexed_field_with_codec(
        sequence: DynamicObject,
        index: usize,
        field: Option<usize>,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
        codec: Option<Rc<NativeRecordCodec>>,
    ) -> Result<Self, String> {
        sequence
            .descriptor()
            .layout()
            .sequence_item()
            .ok_or("dynamic value is not an indexed sequence")?;
        if let Some(field) = field {
            sequence.with(|value| {
                value.with_sequence_item(index, |item| item.view().field(field).map(|_| ()))
            })???;
        }
        let ledger = sequence.with(|value| value.sequence_borrows())??;
        let lease = ledger.reference(index)?;
        Ok(Self {
            mutable,
            target: ReferenceTarget::DynamicIndexedElement {
                sequence: sequence.into_compact(),
                index,
                field,
                _lease: lease,
                codec,
            },
            _guard: guard,
        })
    }

    pub fn new_dynamic_cell(value: &Value, mutable: bool) -> Result<Self, String> {
        let cell = super::native_receiver::NativeReceiver::from_value(value)?
            .ok_or("expected native RefCell receiver")?;
        let codec = Rc::new(cell.codec());
        cell.with(|payload| {
            payload.with_rust::<ErasedRefCell, _>(|value| {
                value.references.set(value.references.get() + 1);
            })
        })??;
        Ok(Self {
            mutable,
            target: ReferenceTarget::DynamicCell {
                cell: Rc::new(cell),
                codec,
            },
            _guard: value.as_reference(),
        })
    }

    pub fn new_dynamic_field(
        object: DynamicObject,
        field: usize,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
        structs: Vec<Rc<StructType>>,
        enums: Vec<Rc<EnumType>>,
    ) -> Result<Self, String> {
        let codec = object
            .descriptor()
            .metadata::<NativeRecordCodec>()
            .unwrap_or_else(|| Rc::new(NativeRecordCodec::with_definitions(&structs, &enums)));
        Self::new_native_path(
            object,
            vec![rils_value::DynamicPathStep::Field(field)],
            mutable,
            guard,
            codec,
        )
    }

    pub(super) fn new_native_path(
        object: DynamicObject,
        steps: Vec<rils_value::DynamicPathStep>,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
        codec: Rc<NativeRecordCodec>,
    ) -> Result<Self, String> {
        let path = NativePath::new(object.into_compact(), steps, codec)?;
        Ok(Self {
            mutable,
            target: ReferenceTarget::DynamicField(Box::new(path)),
            _guard: guard,
        })
    }

    pub fn reborrow(&self, mutable: bool) -> Result<Self, String> {
        if mutable && !self.mutable {
            return Err("cannot mutably borrow through an immutable reference".into());
        }
        match &self.target {
            ReferenceTarget::Storage(target) => Ok(Self::new_storage(target.clone(), mutable)),

            ReferenceTarget::DynamicIndexedElement {
                sequence,
                index,
                field,
                codec,
                ..
            } => {
                let ledger = sequence.with(|value| value.sequence_borrows())??;
                let lease = ledger.reference(*index)?;
                Ok(Self {
                    mutable,
                    target: ReferenceTarget::DynamicIndexedElement {
                        sequence: sequence.clone(),
                        index: *index,
                        field: *field,
                        _lease: lease,
                        codec: codec.clone(),
                    },
                    _guard: self._guard.clone(),
                })
            }
            ReferenceTarget::DynamicCell { cell, codec } => {
                cell.with(|payload| {
                    payload.with_rust::<ErasedRefCell, _>(|value| {
                        value.references.set(value.references.get() + 1);
                    })
                })??;
                Ok(Self {
                    mutable,
                    target: ReferenceTarget::DynamicCell {
                        cell: cell.clone(),
                        codec: codec.clone(),
                    },
                    _guard: self._guard.clone(),
                })
            }
            ReferenceTarget::DynamicField(path) => Ok(Self {
                mutable,
                target: ReferenceTarget::DynamicField(Box::new(path.reborrow()?)),
                _guard: self._guard.clone(),
            }),
        }
    }

    /// Copy the referent without moving it or implicitly invoking Clone.
    /// Non-Copy values must be observed through a borrowed view or cloned explicitly.
    pub fn read(&self) -> Result<Value, String> {
        if let ReferenceTarget::Storage(target) = &self.target {
            return target
                .try_borrow()
                .map_err(|_| "reference target is already mutably accessed".to_owned())?
                .with_value(|value| {
                    if !value.is_copy() {
                        return Err("cannot move a non-Copy value out of a reference".into());
                    }
                    value.clone_owned()
                });
        }
        let codec = self.native_codec()?.unwrap_or_default();
        let copied = self.with_native_view(|view| view.copy_owned())??;
        let value = codec.from_native(copied)?;
        if !value.is_copy() {
            return Err("cannot move a non-Copy value out of a reference".into());
        }
        Ok(value)
    }

    pub(crate) fn with_rust<T: 'static, R>(
        &self,
        callback: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        self.with_borrowed_target(|value| match value {
            super::borrowed::Read::View(view)
                if view.layout()?.is_rust_type::<Rc<ReferenceValue>>() =>
            {
                view.with_rust::<Rc<ReferenceValue>, _>(|reference| reference.with_rust(callback))?
            }
            super::borrowed::Read::Leaf(leaf) if leaf.is_rust_type::<Rc<ReferenceValue>>() => {
                leaf.with_rust::<Rc<ReferenceValue>, _>(|reference| reference.with_rust(callback))?
            }
            super::borrowed::Read::View(view) => view.with_rust(callback),
            super::borrowed::Read::Leaf(leaf) => leaf.with_rust(callback),
            super::borrowed::Read::Host(object) => Err(format!(
                "host object {} has no registered Rust borrow view",
                object.type_definition.name
            )),
        })?
    }

    pub(crate) fn with_native_view<R>(
        &self,
        callback: impl FnOnce(DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        let mut callback = Some(callback);
        let mut output = None;
        self.with_native_view_erased(&mut |view| {
            output = Some(callback.take().expect("called once")(view));
        })?;
        Ok(output.expect("native view callback was called"))
    }

    fn with_native_view_erased(
        &self,
        callback: &mut dyn FnMut(DynamicValueRef<'_>),
    ) -> Result<(), String> {
        match &self.target {
            ReferenceTarget::Storage(target) => target
                .borrow()
                .with_value(|value| crate::host_value::with_native_value(value, callback)),

            ReferenceTarget::DynamicIndexedElement {
                sequence,
                index,
                field,
                ..
            } => sequence.with(|value| {
                value.with_sequence_item(*index, |item| {
                    callback(match field {
                        Some(field) => item.view().field(*field)?,
                        None => item.view(),
                    });
                    Ok(())
                })
            })??,
            ReferenceTarget::DynamicCell { cell, .. } => cell.with(|payload| {
                payload.with_rust::<ErasedRefCell, _>(|value| {
                    value.value.with(|item| callback(item.view()))
                })
            })??,
            ReferenceTarget::DynamicField(path) => path.with_view(callback),
        }
    }

    pub fn write(&self, value: Value) -> Result<(), AssignError> {
        if !self.mutable {
            return Err(AssignError::Immutable);
        }
        match &self.target {
            ReferenceTarget::Storage(target) => target.borrow_mut().assign_through_reference(value),

            ReferenceTarget::DynamicIndexedElement {
                sequence,
                index,
                field,
                ..
            } => {
                if field.is_some() {
                    return Err(AssignError::Immutable);
                }
                let layout = sequence
                    .descriptor()
                    .layout()
                    .sequence_item()
                    .ok_or(AssignError::Undefined)?;
                if !layout.rils_type().accepts(&value) {
                    return Err(AssignError::TypeMismatch(layout.rils_type().clone()));
                }
                let item = super::record_codec::into_native(value, layout.clone())
                    .map_err(|_| AssignError::TypeMismatch(layout.rils_type().clone()))?;
                sequence
                    .with_mut(|payload| payload.replace_sequence_item(*index, item))
                    .map_err(|_| AssignError::BorrowedTarget)?
                    .map_err(|_| AssignError::BorrowedTarget)?;
                Ok(())
            }
            ReferenceTarget::DynamicCell { cell, codec } => {
                let item_ty = cell.descriptor().layout().rils_type();
                let crate::Type::Named { arguments, .. } = item_ty else {
                    return Err(AssignError::Undefined);
                };
                let Some(item_ty) = arguments.first() else {
                    return Err(AssignError::Undefined);
                };
                let layout = cell
                    .with(|payload| {
                        payload.with_rust::<ErasedRefCell, _>(|cell| {
                            cell.value.with(|value| value.layout_handle())
                        })
                    })
                    .map_err(|_| AssignError::BorrowedTarget)?
                    .map_err(|_| AssignError::BorrowedTarget)?
                    .map_err(|_| AssignError::BorrowedTarget)?;
                let native = codec
                    .as_ref()
                    .clone()
                    .with_owned_conversion()
                    .into_native(value, layout)
                    .map_err(|_| AssignError::TypeMismatch(item_ty.clone()))?;
                cell.with(|payload| {
                    payload.with_rust::<ErasedRefCell, _>(|value| {
                        value.value.with_mut(|item| std::mem::replace(item, native))
                    })
                })
                .map_err(|_| AssignError::BorrowedTarget)?
                .map_err(|_| AssignError::BorrowedTarget)?
                .map_err(|_| AssignError::BorrowedTarget)?;
                Ok(())
            }
            ReferenceTarget::DynamicField(path) => path.write(value),
        }
    }
}

impl Drop for ReferenceValue {
    fn drop(&mut self) {
        match &self.target {
            ReferenceTarget::Storage(target) => target.borrow_mut().remove_reference(),

            ReferenceTarget::DynamicIndexedElement { .. } => {}
            ReferenceTarget::DynamicCell { cell, .. } => {
                let _ = cell.with(|payload| {
                    payload.with_rust::<ErasedRefCell, _>(|value| {
                        value
                            .references
                            .set(value.references.get().saturating_sub(1));
                    })
                });
            }
            ReferenceTarget::DynamicField(_) => {}
        }
    }
}
