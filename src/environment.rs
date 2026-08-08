use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::{types::Type, value::Value};

pub type EnvironmentRef = Rc<RefCell<Environment>>;
pub type StorageRef = Rc<RefCell<StorageSlot>>;

pub struct StorageSlot {
    value: Option<Value>,
    mutable: bool,
    type_annotation: Option<Type>,
    references: usize,
}

impl StorageSlot {
    pub(crate) fn uninitialized(mutable: bool) -> Self {
        Self {
            value: None,
            mutable,
            type_annotation: None,
            references: 0,
        }
    }

    pub(crate) fn initialize(&mut self, value: Value) {
        self.value = Some(value);
    }

    pub(crate) fn clear(&mut self) {
        self.value = None;
    }

    pub fn read(&self) -> Result<Value, AccessError> {
        self.value.clone().ok_or(AccessError::Moved)
    }

    pub fn take(&mut self) -> Result<Value, AccessError> {
        let value = self.value.as_ref().ok_or(AccessError::Moved)?;
        if matches!(value, Value::Reference(_)) {
            return Ok(value.clone());
        }
        if value.is_partially_moved() {
            return Err(AccessError::PartiallyMoved);
        }
        if value.is_copy() {
            return Ok(value
                .clone_owned()
                .expect("Copy values can always be duplicated"));
        }
        if self.references > 0 {
            return Err(AccessError::Borrowed);
        }
        if value.has_active_references() {
            return Err(AccessError::Borrowed);
        }
        self.value.take().ok_or(AccessError::Moved)
    }

    pub fn is_mutable(&self) -> bool {
        self.mutable
    }

    pub fn add_reference(&mut self) {
        self.references += 1;
    }

    pub fn remove_reference(&mut self) {
        self.references = self.references.saturating_sub(1);
    }

    pub(crate) fn assign(&mut self, mut value: Value) -> Result<(), AssignError> {
        if !self.mutable {
            return Err(AssignError::Immutable);
        }
        if self
            .value
            .as_ref()
            .is_some_and(Value::has_active_references)
        {
            return Err(AssignError::BorrowedTarget);
        }
        if let Some(expected) = &self.type_annotation {
            value = expected
                .constrain(&value)
                .ok_or_else(|| AssignError::TypeMismatch(expected.clone()))?;
        } else if matches!(&value, Value::Option { .. }) {
            return Err(AssignError::OptionRequiresAnnotation);
        }
        self.value = Some(value);
        Ok(())
    }

    pub fn assign_through_reference(&mut self, mut value: Value) -> Result<(), AssignError> {
        if self
            .value
            .as_ref()
            .is_some_and(Value::has_active_references)
        {
            return Err(AssignError::BorrowedTarget);
        }
        if let Some(expected) = &self.type_annotation {
            value = expected
                .constrain(&value)
                .ok_or_else(|| AssignError::TypeMismatch(expected.clone()))?;
        }
        self.value = Some(value);
        Ok(())
    }
}

pub struct Environment {
    values: HashMap<String, StorageRef>,
    parent: Option<EnvironmentRef>,
}

impl Environment {
    pub fn global() -> EnvironmentRef {
        Rc::new(RefCell::new(Self {
            values: HashMap::new(),
            parent: None,
        }))
    }

    pub fn child(parent: EnvironmentRef) -> EnvironmentRef {
        Rc::new(RefCell::new(Self {
            values: HashMap::new(),
            parent: Some(parent),
        }))
    }

    pub fn define(
        &mut self,
        name: impl Into<String>,
        value: Value,
        mutable: bool,
        type_annotation: Option<Type>,
    ) {
        let type_annotation = type_annotation.or_else(|| match Type::of_value(&value) {
            Some(inferred @ (Type::Option(_) | Type::Result(_, _))) => Some(inferred),
            _ => None,
        });
        self.values.insert(
            name.into(),
            Rc::new(RefCell::new(StorageSlot {
                value: Some(value),
                mutable,
                type_annotation,
                references: 0,
            })),
        );
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        self.slot(name).and_then(|slot| slot.borrow().read().ok())
    }

    pub fn take(&self, name: &str) -> Result<Value, AccessError> {
        let slot = self.slot(name).ok_or(AccessError::Undefined)?;
        slot.borrow_mut().take()
    }

    pub fn slot(&self, name: &str) -> Option<StorageRef> {
        self.values.get(name).cloned().or_else(|| {
            self.parent
                .as_ref()
                .and_then(|parent| parent.borrow().slot(name))
        })
    }

    pub fn contains_local(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub fn assign(&mut self, name: &str, value: Value) -> Result<(), AssignError> {
        if let Some(slot) = self.values.get(name) {
            return slot.borrow_mut().assign(value);
        }
        if value.contains_reference() {
            return Err(AssignError::ReferenceEscape);
        }
        if let Some(parent) = &self.parent {
            return parent.borrow_mut().assign(name, value);
        }
        Err(AssignError::Undefined)
    }

    pub fn has_local_reference(&self) -> bool {
        self.values.values().any(|slot| {
            slot.borrow()
                .value
                .as_ref()
                .is_some_and(Value::contains_reference)
        })
    }

    pub fn has_visible_reference(&self) -> bool {
        self.has_local_reference()
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.borrow().has_visible_reference())
    }

    pub fn has_local_function(&self) -> bool {
        self.values.values().any(|slot| {
            slot.borrow()
                .value
                .as_ref()
                .is_some_and(|value| matches!(value, Value::Function(_)))
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessError {
    Undefined,
    Moved,
    Borrowed,
    PartiallyMoved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignError {
    Undefined,
    Immutable,
    TypeMismatch(Type),
    OptionRequiresAnnotation,
    ReferenceEscape,
    BorrowedTarget,
}
