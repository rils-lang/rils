use std::rc::Rc;

use crate::environment::{AssignError, EnvironmentRef, StorageRef};

use super::{HashKey, MapCollection, SequenceValue, SetCollection, StructInstance, Value};

pub struct ReferenceValue {
    pub mutable: bool,
    target: ReferenceTarget,
    _guard: Option<Rc<ReferenceValue>>,
}

enum ReferenceTarget {
    Storage(StorageRef),
    StructField {
        instance: Rc<StructInstance>,
        name: String,
    },
    SequenceElement {
        sequence: Rc<SequenceValue>,
        index: usize,
    },
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
                ReferenceTarget::StructField { .. }
                | ReferenceTarget::SequenceElement { .. }
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

    pub fn new_struct_field(
        instance: Rc<StructInstance>,
        name: String,
        mutable: bool,
    ) -> Result<Self, String> {
        Self::new_guarded_struct_field(instance, name, mutable, None)
    }

    pub fn new_guarded_struct_field(
        instance: Rc<StructInstance>,
        name: String,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        let mut fields = instance.fields.borrow_mut();
        let field = fields
            .get_mut(&name)
            .ok_or_else(|| format!("unknown field `{name}`"))?;
        if field.value.is_none() {
            return Err(format!("cannot reference moved field `{name}`"));
        }
        field.references += 1;
        drop(fields);
        Ok(Self {
            mutable,
            target: ReferenceTarget::StructField { instance, name },
            _guard: guard,
        })
    }

    pub fn new_sequence_element(
        sequence: Rc<SequenceValue>,
        index: usize,
        mutable: bool,
    ) -> Result<Self, String> {
        Self::new_guarded_sequence_element(sequence, index, mutable, None)
    }

    pub fn new_guarded_sequence_element(
        sequence: Rc<SequenceValue>,
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
            target: ReferenceTarget::SequenceElement { sequence, index },
            _guard: guard,
        })
    }

    pub fn new_map_key(
        map: MapCollection,
        key: HashKey,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        if !map.contains_key(&key) {
            return Err("iterator map key no longer exists".into());
        }
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
        if map.value(&key).is_none() {
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
        if !set.contains(&key) {
            return Err("iterator set item no longer exists".into());
        }
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
            ReferenceTarget::StructField { instance, name } => Self::new_guarded_struct_field(
                instance.clone(),
                name.clone(),
                mutable,
                self._guard.clone(),
            ),
            ReferenceTarget::SequenceElement { sequence, index } => {
                Self::new_guarded_sequence_element(
                    sequence.clone(),
                    *index,
                    mutable,
                    self._guard.clone(),
                )
            }
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
            ReferenceTarget::StructField { instance, name } => instance
                .fields
                .borrow()
                .get(name)
                .and_then(|field| field.value.clone())
                .ok_or_else(|| format!("reference target field `{name}` has been moved")),
            ReferenceTarget::SequenceElement { sequence, index } => sequence
                .elements
                .borrow()
                .get(*index)
                .and_then(|slot| slot.value.clone())
                .ok_or_else(|| format!("reference target element {index} has been moved")),
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

    pub fn write(&self, value: Value) -> Result<(), AssignError> {
        if !self.mutable {
            return Err(AssignError::Immutable);
        }
        match &self.target {
            ReferenceTarget::Storage(target) => target.borrow_mut().assign_through_reference(value),
            ReferenceTarget::StructField { instance, name } => {
                let mut fields = instance.fields.borrow_mut();
                let field = fields.get_mut(name).ok_or(AssignError::Undefined)?;
                field.value = Some(
                    field
                        .type_annotation
                        .constrain(&value)
                        .ok_or_else(|| AssignError::TypeMismatch(field.type_annotation.clone()))?,
                );
                Ok(())
            }
            ReferenceTarget::SequenceElement { sequence, index } => {
                if sequence.active_iterators.get() > 0 {
                    return Err(AssignError::BorrowedTarget);
                }
                let mut elements = sequence.elements.borrow_mut();
                let slot = elements.get_mut(*index).ok_or(AssignError::Undefined)?;
                slot.value = Some(
                    slot.type_annotation
                        .constrain(&value)
                        .ok_or_else(|| AssignError::TypeMismatch(slot.type_annotation.clone()))?,
                );
                Ok(())
            }
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
            ReferenceTarget::StructField { instance, name } => {
                if let Some(field) = instance.fields.borrow_mut().get_mut(name) {
                    field.references = field.references.saturating_sub(1);
                }
            }
            ReferenceTarget::SequenceElement { sequence, index } => {
                if let Some(slot) = sequence.elements.borrow_mut().get_mut(*index) {
                    slot.references = slot.references.saturating_sub(1);
                }
            }
            ReferenceTarget::MapKey { map, .. } | ReferenceTarget::MapValue { map, .. } => {
                map.borrowed().set(map.borrowed().get().saturating_sub(1));
            }
            ReferenceTarget::SetItem { set, .. } => {
                set.borrowed().set(set.borrowed().get().saturating_sub(1));
            }
        }
    }
}
