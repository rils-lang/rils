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
use super::{
    DynamicObject, EnumType, HashKey, IndexedStorage, MapCollection, SetCollection, StructType,
    Value,
};
use native_path::NativePath;

pub struct ReferenceValue {
    pub mutable: bool,
    target: ReferenceTarget,
    _guard: Option<Rc<ReferenceValue>>,
}

enum ReferenceTarget {
    Storage(StorageRef),
    IndexedElement {
        sequence: Rc<IndexedStorage>,
        index: usize,
    },
    DynamicIndexedElement {
        sequence: CompactDynamicObject<Value>,
        index: usize,
        field: Option<usize>,
        _lease: SequenceItemLease,
        codec: Option<Rc<NativeRecordCodec>>,
    },
    DynamicCell {
        cell: CompactDynamicObject<Value>,
        structs: Rc<Vec<Rc<StructType>>>,
        enums: Rc<Vec<Rc<EnumType>>>,
    },
    DynamicField(Box<NativePath>),
    MapKey {
        map: MapCollection,
        key: HashKey,
    },
    MapValue {
        map: MapCollection,
        key: HashKey,
    },
    SetItem {
        set: SetCollection,
        key: HashKey,
    },
}

impl ReferenceValue {
    pub fn is_local_to(&self, environment: &EnvironmentRef) -> bool {
        self._guard
            .as_ref()
            .is_some_and(|guard| guard.is_local_to(environment))
            || match &self.target {
                ReferenceTarget::Storage(target) => environment.borrow().owns_storage(target),
                ReferenceTarget::IndexedElement { .. }
                | ReferenceTarget::DynamicIndexedElement { .. }
                | ReferenceTarget::DynamicCell { .. }
                | ReferenceTarget::DynamicField(_)
                | ReferenceTarget::MapKey { .. }
                | ReferenceTarget::MapValue { .. }
                | ReferenceTarget::SetItem { .. } => false,
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

    pub fn new_indexed_element(
        sequence: Rc<IndexedStorage>,
        index: usize,
        mutable: bool,
    ) -> Result<Self, String> {
        Self::new_guarded_indexed_element(sequence, index, mutable, None)
    }

    pub fn new_guarded_indexed_element(
        sequence: Rc<IndexedStorage>,
        index: usize,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        let mut elements = sequence.elements.borrow_mut();
        let slot = elements
            .get_mut(index)
            .ok_or_else(|| format!("index {index} is out of bounds"))?;
        if slot.value.is_none() {
            return Err(format!("cannot reference moved element at index {index}"));
        }
        slot.references += 1;
        drop(elements);
        Ok(Self {
            mutable,
            target: ReferenceTarget::IndexedElement { sequence, index },
            _guard: guard,
        })
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

    pub fn new_dynamic_cell(
        cell: DynamicObject,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
        structs: Vec<Rc<StructType>>,
        enums: Vec<Rc<EnumType>>,
    ) -> Result<Self, String> {
        cell.with(|payload| {
            payload.with::<ErasedRefCell, _>(|value| {
                value.references.set(value.references.get() + 1);
            })
        })??;
        Ok(Self {
            mutable,
            target: ReferenceTarget::DynamicCell {
                cell: cell.into_compact(),
                structs: Rc::new(structs),
                enums: Rc::new(enums),
            },
            _guard: guard,
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

    pub fn new_map_key(
        map: MapCollection,
        key: HashKey,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        map.with_entry(&key, |_, _| ())?;
        map.borrowed().set(map.borrowed().get() + 1);
        Ok(Self {
            mutable: false,
            target: ReferenceTarget::MapKey { map, key },
            _guard: guard,
        })
    }

    pub fn new_map_value(
        map: MapCollection,
        key: HashKey,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        if !map.with_entry(&key, |_, slot| slot.value.is_some())? {
            return Err("iterator map value no longer exists".into());
        }
        map.borrowed().set(map.borrowed().get() + 1);
        Ok(Self {
            mutable: false,
            target: ReferenceTarget::MapValue { map, key },
            _guard: guard,
        })
    }

    pub fn new_set_item(
        set: SetCollection,
        key: HashKey,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        set.with_item(&key, |_| ())?;
        set.borrowed().set(set.borrowed().get() + 1);
        Ok(Self {
            mutable: false,
            target: ReferenceTarget::SetItem { set, key },
            _guard: guard,
        })
    }

    pub fn reborrow(&self, mutable: bool) -> Result<Self, String> {
        if mutable && !self.mutable {
            return Err("cannot mutably borrow through an immutable reference".into());
        }
        match &self.target {
            ReferenceTarget::Storage(target) => Ok(Self::new_storage(target.clone(), mutable)),

            ReferenceTarget::IndexedElement { sequence, index } => {
                Self::new_guarded_indexed_element(
                    sequence.clone(),
                    *index,
                    mutable,
                    self._guard.clone(),
                )
            }
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
            ReferenceTarget::DynamicCell {
                cell,
                structs,
                enums,
            } => {
                cell.with(|payload| {
                    payload.with::<ErasedRefCell, _>(|value| {
                        value.references.set(value.references.get() + 1);
                    })
                })??;
                Ok(Self {
                    mutable,
                    target: ReferenceTarget::DynamicCell {
                        cell: cell.clone(),
                        structs: structs.clone(),
                        enums: enums.clone(),
                    },
                    _guard: self._guard.clone(),
                })
            }
            ReferenceTarget::DynamicField(path) => Ok(Self {
                mutable,
                target: ReferenceTarget::DynamicField(Box::new(path.reborrow()?)),
                _guard: self._guard.clone(),
            }),
            ReferenceTarget::MapKey { map, key } => {
                Self::new_map_key(map.clone(), key.clone(), self._guard.clone())
            }
            ReferenceTarget::MapValue { map, key } => {
                Self::new_map_value(map.clone(), key.clone(), self._guard.clone())
            }
            ReferenceTarget::SetItem { set, key } => {
                Self::new_set_item(set.clone(), key.clone(), self._guard.clone())
            }
        }
    }

    pub fn read(&self) -> Result<Value, String> {
        match &self.target {
            ReferenceTarget::Storage(target) => target
                .borrow()
                .read()
                .map_err(|_| "reference target has been moved".into()),

            ReferenceTarget::IndexedElement { sequence, index } => sequence
                .elements
                .borrow()
                .get(*index)
                .and_then(|slot| slot.value.clone())
                .ok_or_else(|| format!("reference target element {index} has been moved")),
            ReferenceTarget::DynamicIndexedElement {
                sequence,
                index,
                field,
                codec,
                ..
            } => match field {
                Some(field) => sequence.with(|value| {
                    value.with_sequence_item(*index, |item| {
                        let view = item.view().field(*field)?;
                        let cloned = crate::value::runtime_layouts::clone_borrowed_view(view)?;
                        if let Some(codec) = codec {
                            codec.from_native(cloned)
                        } else {
                            super::record_codec::from_native(cloned)
                        }
                    })
                })??,
                None => {
                    let item = sequence.with(|payload| {
                        payload.with_sequence_item(*index, |item| {
                            crate::value::runtime_layouts::clone_borrowed_element(item)
                        })
                    })???;
                    if let Some(codec) = codec {
                        codec.from_native(item)
                    } else {
                        super::record_codec::from_native(item)
                    }
                }
            },
            ReferenceTarget::DynamicCell {
                cell,
                structs,
                enums,
            } => {
                let inner = cell.with(|payload| {
                    payload.with::<ErasedRefCell, _>(|value| value.value.rebind_shared::<Value>())
                })???;
                let layout = inner.descriptor().layout();
                let nominal_noncopy = match layout.rils_type() {
                    crate::Type::Named { name, .. } => {
                        structs
                            .iter()
                            .find(|definition| definition.name == *name)
                            .is_some_and(|definition| {
                                !definition.implemented_traits.borrow().contains("Copy")
                            })
                            || enums
                                .iter()
                                .find(|definition| definition.name == *name)
                                .is_some_and(|definition| {
                                    !definition.implemented_traits.borrow().contains("Copy")
                                })
                    }
                    _ => false,
                };
                if (layout.is_copy() && !nominal_noncopy)
                    || layout.rils_type() == &crate::Type::String
                    || super::runtime_layouts::is_execution_leaf(layout)
                {
                    let cloned = inner.with(|value| {
                        crate::value::runtime_layouts::clone_borrowed_element(value)
                    })??;
                    super::record_codec::NativeRecordCodec::with_definitions(structs, enums)
                        .from_native(cloned)
                } else {
                    Ok(Value::Dynamic(inner))
                }
            }
            ReferenceTarget::DynamicField(path) => path.read(),
            ReferenceTarget::MapKey { map, key } => map
                .contains_key(key)
                .then(|| key.to_value())
                .ok_or_else(|| "iterator map key no longer exists".into()),
            ReferenceTarget::MapValue { map, key } => map
                .value(key)
                .ok_or_else(|| "iterator map value no longer exists".into()),
            ReferenceTarget::SetItem { set, key } => set
                .contains(key)
                .then(|| key.to_value())
                .ok_or_else(|| "iterator set item no longer exists".into()),
        }
    }

    pub(crate) fn with_rust<T: 'static, R>(
        &self,
        callback: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        self.with_borrowed_target(|value| match value {
            super::borrowed::Read::View(view) => view.with_rust(callback),
            super::borrowed::Read::Leaf(leaf) => leaf.with_rust(callback),
            super::borrowed::Read::Legacy(value) => {
                crate::host_value::with_rust_value(value, callback)
            }
        })?
    }

    pub(crate) fn with_native_view<R>(
        &self,
        callback: impl FnOnce(DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        match &self.target {
            ReferenceTarget::Storage(target) => target
                .borrow()
                .with_value(|value| crate::host_value::with_native_value(value, callback)),

            ReferenceTarget::IndexedElement { sequence, index } => {
                let elements = sequence.elements.borrow();
                let value = elements
                    .get(*index)
                    .and_then(|slot| slot.value.as_ref())
                    .ok_or("native projection target was moved")?;
                crate::host_value::with_native_value(value, callback)
            }
            ReferenceTarget::DynamicIndexedElement {
                sequence,
                index,
                field,
                ..
            } => sequence.with(|value| {
                value.with_sequence_item(*index, |item| match field {
                    Some(field) => Ok(callback(item.view().field(*field)?)),
                    None => Ok(callback(item.view())),
                })
            })??,
            ReferenceTarget::DynamicCell { cell, .. } => cell.with(|payload| {
                payload.with::<ErasedRefCell, _>(|value| {
                    value.value.with(|item| callback(item.view()))
                })
            })??,
            ReferenceTarget::DynamicField(path) => path.with_view(callback),
            ReferenceTarget::MapKey { map, key } => {
                map.with_entry(key, |key, _| key.with_native_view(callback))?
            }
            ReferenceTarget::MapValue { map, key } => map.with_entry(key, |_, slot| {
                crate::host_value::with_native_value(
                    slot.value
                        .as_ref()
                        .ok_or("referenced map value was moved")?,
                    callback,
                )
            })?,
            ReferenceTarget::SetItem { set, key } => {
                set.with_item(key, |key| key.with_native_view(callback))?
            }
        }
    }

    pub fn write(&self, value: Value) -> Result<(), AssignError> {
        if !self.mutable {
            return Err(AssignError::Immutable);
        }
        match &self.target {
            ReferenceTarget::Storage(target) => target.borrow_mut().assign_through_reference(value),

            ReferenceTarget::IndexedElement { sequence, index } => {
                if sequence.active_iterators.get() > 0 {
                    return Err(AssignError::BorrowedTarget);
                }
                let mut elements = sequence.elements.borrow_mut();
                let slot = elements.get_mut(*index).ok_or(AssignError::Undefined)?;
                slot.assign(value)
                    .map_err(|_| AssignError::TypeMismatch(slot.type_annotation.clone()))?;
                Ok(())
            }
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
            ReferenceTarget::DynamicCell {
                cell,
                structs,
                enums,
            } => {
                let item_ty = cell.descriptor().layout().rils_type();
                let crate::Type::Named { arguments, .. } = item_ty else {
                    return Err(AssignError::Undefined);
                };
                let Some(item_ty) = arguments.first() else {
                    return Err(AssignError::Undefined);
                };
                let layout = cell
                    .with(|payload| {
                        payload.with::<ErasedRefCell, _>(|cell| {
                            cell.value.with(|value| value.layout_handle())
                        })
                    })
                    .map_err(|_| AssignError::BorrowedTarget)?
                    .map_err(|_| AssignError::BorrowedTarget)?
                    .map_err(|_| AssignError::BorrowedTarget)?;
                let native =
                    super::record_codec::NativeRecordCodec::with_definitions(structs, enums)
                        .into_native(value, layout)
                        .map_err(|_| AssignError::TypeMismatch(item_ty.clone()))?;
                cell.with(|payload| {
                    payload.with::<ErasedRefCell, _>(|value| {
                        value.value.with_mut(|item| std::mem::replace(item, native))
                    })
                })
                .map_err(|_| AssignError::BorrowedTarget)?
                .map_err(|_| AssignError::BorrowedTarget)?
                .map_err(|_| AssignError::BorrowedTarget)?;
                Ok(())
            }
            ReferenceTarget::DynamicField(path) => path.write(value),
            ReferenceTarget::MapKey { .. }
            | ReferenceTarget::MapValue { .. }
            | ReferenceTarget::SetItem { .. } => Err(AssignError::Immutable),
        }
    }
}

impl Drop for ReferenceValue {
    fn drop(&mut self) {
        match &self.target {
            ReferenceTarget::Storage(target) => target.borrow_mut().remove_reference(),

            ReferenceTarget::IndexedElement { sequence, index } => {
                if let Some(slot) = sequence.elements.borrow_mut().get_mut(*index) {
                    slot.references = slot.references.saturating_sub(1);
                }
            }
            ReferenceTarget::DynamicIndexedElement { .. } => {}
            ReferenceTarget::DynamicCell { cell, .. } => {
                let _ = cell.with(|payload| {
                    payload.with::<ErasedRefCell, _>(|value| {
                        value
                            .references
                            .set(value.references.get().saturating_sub(1));
                    })
                });
            }
            ReferenceTarget::DynamicField(_) => {}
            ReferenceTarget::MapKey { map, .. } | ReferenceTarget::MapValue { map, .. } => {
                map.borrowed().set(map.borrowed().get().saturating_sub(1));
            }
            ReferenceTarget::SetItem { set, .. } => {
                set.borrowed().set(set.borrowed().get().saturating_sub(1));
            }
        }
    }
}
