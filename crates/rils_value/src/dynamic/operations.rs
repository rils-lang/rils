//! Arbitrary operations attached to a runtime-composed layout.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    rc::Rc,
};

use super::{DynamicLayout, DynamicValue};

type Method<V> = Rc<dyn for<'a> Fn(&mut DynamicCallContext<'a, V>) -> Result<V, String>>;
type OwnedOperation<V> = Rc<dyn Fn(DynamicValue) -> Result<V, String>>;

/// A type's layout and callable operations, suitable for generated registration.
pub struct DynamicType<V> {
    layout: Rc<DynamicLayout>,
    methods: HashMap<String, Method<V>>,
    owned_operations: HashMap<String, OwnedOperation<V>>,
    metadata: HashMap<TypeId, Rc<dyn Any>>,
}

impl<V> DynamicType<V> {
    pub fn new(layout: Rc<DynamicLayout>) -> Self {
        Self {
            layout,
            methods: HashMap::new(),
            owned_operations: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Attach immutable declaration context to the operation table. This is
    /// type metadata, never the bytes or children of a stored value.
    pub fn register_metadata<T: 'static>(mut self, metadata: Rc<T>) -> Self {
        assert!(
            self.metadata.insert(TypeId::of::<T>(), metadata).is_none(),
            "dynamic metadata {} was registered twice",
            std::any::type_name::<T>()
        );
        self
    }

    pub fn metadata<T: 'static>(&self) -> Option<Rc<T>> {
        self.metadata
            .get(&TypeId::of::<T>())?
            .clone()
            .downcast()
            .ok()
    }

    pub fn register_method(
        mut self,
        name: impl Into<String>,
        method: impl for<'a> Fn(&mut DynamicCallContext<'a, V>) -> Result<V, String> + 'static,
    ) -> Self {
        let name = name.into();
        assert!(
            self.methods.insert(name.clone(), Rc::new(method)).is_none(),
            "dynamic native method {name} was registered twice"
        );
        self
    }

    pub fn call(
        &self,
        receiver: &mut DynamicValue,
        name: &str,
        arguments: &[V],
    ) -> Option<Result<V, String>> {
        let method = self.methods.get(name)?;
        if !receiver.has_layout(&self.layout) {
            return Some(Err("native receiver layout does not match its type".into()));
        }
        Some(method(&mut DynamicCallContext {
            receiver,
            arguments,
        }))
    }

    pub fn has_method(&self, name: &str) -> bool {
        self.methods.contains_key(name)
    }

    /// Register an operation that consumes the native bytes and their children.
    pub fn register_owned_operation(
        mut self,
        name: impl Into<String>,
        operation: impl Fn(DynamicValue) -> Result<V, String> + 'static,
    ) -> Self {
        let name = name.into();
        assert!(
            self.owned_operations
                .insert(name.clone(), Rc::new(operation))
                .is_none(),
            "dynamic owned operation {name} was registered twice"
        );
        self
    }

    pub fn has_owned_operation(&self, name: &str) -> bool {
        self.owned_operations.contains_key(name)
    }

    pub fn call_owned(&self, receiver: DynamicValue, name: &str) -> Result<V, String> {
        let operation = self
            .owned_operations
            .get(name)
            .ok_or_else(|| format!("native owned operation `{name}` is unavailable"))?;
        if !receiver.has_layout(&self.layout) {
            return Err("native receiver layout does not match its type".into());
        }
        operation(receiver)
    }

    pub fn layout(&self) -> &DynamicLayout {
        &self.layout
    }

    pub fn layout_handle(&self) -> Rc<DynamicLayout> {
        self.layout.clone()
    }
}

/// Typed receiver access and arguments for one dynamically registered method.
pub struct DynamicCallContext<'a, V> {
    receiver: &'a mut DynamicValue,
    arguments: &'a [V],
}

impl<V> DynamicCallContext<'_, V> {
    pub fn arguments(&self) -> &[V] {
        self.arguments
    }

    pub fn receiver<T: 'static, R>(&self, f: impl FnOnce(&T) -> R) -> Result<R, String> {
        self.receiver.with(f)
    }

    pub fn receiver_mut<T: 'static, R>(
        &mut self,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, String> {
        self.receiver.with_mut(f)
    }

    pub fn option_is_some(&self) -> Result<bool, String> {
        self.receiver.is_some()
    }

    pub fn record_field<T: 'static, R>(
        &self,
        index: usize,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        self.receiver.with_field(index, f)
    }

    pub fn record_field_mut<T: 'static, R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, String> {
        self.receiver.with_field_mut(index, f)
    }
}
