use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::{types::Type, value::Value};

mod declarations;
use declarations::TypeDeclarations;

pub type EnvironmentRef = Rc<RefCell<Environment>>;
pub type StorageRef = Rc<RefCell<StorageSlot>>;

pub struct StorageSlot {
    value: Option<Value>,
    mutable: bool,
    type_annotation: Option<Type>,
    native_declaration: Option<crate::value::storage::NativeDeclaration>,
    references: usize,
}

impl StorageSlot {
    pub fn uninitialized(mutable: bool) -> Self {
        Self {
            value: None,
            mutable,
            type_annotation: None,
            native_declaration: None,
            references: 0,
        }
    }

    pub fn initialize(&mut self, value: Value) {
        self.native_declaration = crate::value::storage::native_declaration(&value);
        self.type_annotation = match Type::of_value(&value) {
            Some(ty @ (Type::Option(_) | Type::Result(_, _))) => Some(ty),
            _ => self
                .native_declaration
                .as_ref()
                .map(|declaration| declaration.rils_type().clone()),
        };
        self.value = Some(value);
    }

    pub fn clear(&mut self) {
        self.value = None;
        self.native_declaration = None;
        self.type_annotation = None;
    }

    pub fn read(&self) -> Result<Value, AccessError> {
        self.value.clone().ok_or(AccessError::Moved)
    }

    pub(crate) fn with_value<R>(
        &self,
        callback: impl FnOnce(&Value) -> Result<R, String>,
    ) -> Result<R, String> {
        callback(
            self.value
                .as_ref()
                .ok_or("reference target has been moved")?,
        )
    }

    pub(crate) fn with_value_mut<R>(
        &mut self,
        callback: impl FnOnce(&mut Value) -> Result<R, String>,
    ) -> Result<R, String> {
        callback(
            self.value
                .as_mut()
                .ok_or("reference target has been moved")?,
        )
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

    pub fn assign(&mut self, mut value: Value) -> Result<(), AssignError> {
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
            value = crate::value::storage::constrain_assignment(
                value,
                expected,
                self.native_declaration.clone(),
            )
            .map_err(|_| AssignError::TypeMismatch(expected.clone()))?;
        } else if matches!(&value, Value::Option { .. }) {
            return Err(AssignError::OptionRequiresAnnotation);
        }
        if let Some(declaration) = crate::value::storage::native_declaration(&value) {
            self.native_declaration = Some(declaration);
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
            value = crate::value::storage::constrain_assignment(
                value,
                expected,
                self.native_declaration.clone(),
            )
            .map_err(|_| AssignError::TypeMismatch(expected.clone()))?;
        }
        if let Some(declaration) = crate::value::storage::native_declaration(&value) {
            self.native_declaration = Some(declaration);
        }
        self.value = Some(value);
        Ok(())
    }
}

pub struct Environment {
    values: HashMap<String, StorageRef>,
    type_bindings: HashMap<String, Type>,
    parent: Option<EnvironmentRef>,
    module: bool,
    module_path: Vec<String>,
    declarations: Rc<TypeDeclarations>,
}

impl Environment {
    pub fn set_type_bindings(&mut self, bindings: HashMap<String, Type>) {
        self.type_bindings = bindings;
    }

    pub fn type_bindings(&self) -> HashMap<String, Type> {
        let mut bindings = self
            .parent
            .as_ref()
            .map(|parent| parent.borrow().type_bindings())
            .unwrap_or_default();
        bindings.extend(self.type_bindings.clone());
        bindings
    }

    pub fn visible_host_definitions(&self) -> Vec<Rc<crate::value::HostType>> {
        self.declarations.host_definitions()
    }

    pub fn visible_type_definitions(
        &self,
    ) -> (
        Vec<Rc<crate::value::StructType>>,
        Vec<Rc<crate::value::EnumType>>,
    ) {
        self.declarations.definitions()
    }

    pub fn global() -> EnvironmentRef {
        Rc::new(RefCell::new(Self {
            values: HashMap::new(),
            type_bindings: HashMap::new(),
            parent: None,
            module: false,
            module_path: Vec::new(),
            declarations: Rc::default(),
        }))
    }

    pub fn child(parent: EnvironmentRef) -> EnvironmentRef {
        let module_path = parent.borrow().module_path.clone();
        let declarations = parent.borrow().declarations.clone();
        Rc::new(RefCell::new(Self {
            values: HashMap::new(),
            type_bindings: HashMap::new(),
            parent: Some(parent),
            module: false,
            module_path,
            declarations,
        }))
    }

    pub fn owns_storage(&self, target: &StorageRef) -> bool {
        self.values.values().any(|slot| Rc::ptr_eq(slot, target))
    }

    pub fn module_child(parent: EnvironmentRef) -> EnvironmentRef {
        let child = Self::child(parent);
        child.borrow_mut().module = true;
        child
    }

    pub fn named_module_child(parent: EnvironmentRef, name: &str) -> EnvironmentRef {
        let child = Self::module_child(parent);
        child.borrow_mut().module_path.push(name.to_owned());
        child
    }

    pub fn qualified_type_name(&self, name: &str) -> String {
        self.module_path
            .iter()
            .map(String::as_str)
            .chain([name])
            .collect::<Vec<_>>()
            .join("::")
    }

    pub fn set_declaration_types(
        &self,
        resolver: rils_frontend::semantic::DeclarationTypeResolver,
    ) {
        self.declarations.set_resolver(resolver);
    }

    pub fn resolve_declaration_type(&self, ty: &Type) -> Type {
        self.declarations.resolve(ty, &self.module_path)
    }

    pub fn is_declared_type(&self, name: &str) -> bool {
        self.declarations.is_declared_type(name)
    }

    pub fn declared_value_traits(&self, name: &str) -> std::collections::HashSet<String> {
        self.declarations.declared_value_traits(name)
    }

    pub fn inaccessible_type(&self, ty: &Type) -> Option<String> {
        self.declarations.inaccessible_type(ty, &self.module_path)
    }

    pub fn root(environment: &EnvironmentRef) -> EnvironmentRef {
        let mut current = environment.clone();
        loop {
            let parent = current.borrow().parent.clone();
            let Some(parent) = parent else {
                return current;
            };
            current = parent;
        }
    }

    pub fn current_module(environment: &EnvironmentRef) -> Option<EnvironmentRef> {
        let mut current = Some(environment.clone());
        while let Some(candidate) = current {
            if candidate.borrow().module {
                return Some(candidate);
            }
            current = candidate.borrow().parent.clone();
        }
        None
    }

    pub fn parent_module(environment: &EnvironmentRef) -> Option<EnvironmentRef> {
        let current = Self::current_module(environment)?;
        let mut candidate = current.borrow().parent.clone();
        while let Some(environment) = candidate {
            if environment.borrow().module {
                return Some(environment);
            }
            candidate = environment.borrow().parent.clone();
        }
        Some(Self::root(&current))
    }

    pub fn define(
        &mut self,
        name: impl Into<String>,
        value: Value,
        mutable: bool,
        type_annotation: Option<Type>,
    ) {
        self.declarations.register(&value);
        let native_declaration = crate::value::storage::native_declaration(&value);
        let type_annotation = type_annotation.or_else(|| match Type::of_value(&value) {
            Some(inferred @ (Type::Option(_) | Type::Result(_, _))) => Some(inferred),
            _ => native_declaration
                .as_ref()
                .map(|declaration| declaration.rils_type().clone()),
        });
        self.values.insert(
            name.into(),
            Rc::new(RefCell::new(StorageSlot {
                native_declaration,
                value: Some(value),
                mutable,
                type_annotation,
                references: 0,
            })),
        );
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        self.slot(name)
            .and_then(|slot| slot.borrow().read().ok())
            .or_else(|| {
                name.contains("::")
                    .then(|| self.declarations.get(name))
                    .flatten()
            })
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
