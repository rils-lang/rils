//! Arbitrary operations attached to a runtime-composed layout.

use std::{collections::HashMap, rc::Rc};

use super::{DynamicLayout, DynamicValue};

type Method<V> = Rc<dyn for<'a> Fn(&mut DynamicCallContext<'a, V>) -> Result<V, String>>;

/// A type's layout and callable operations, suitable for generated registration.
pub struct DynamicType<V> {
    layout: Rc<DynamicLayout>,
    methods: HashMap<String, Method<V>>,
}

impl<V> DynamicType<V> {
    pub fn new(layout: Rc<DynamicLayout>) -> Self {
        Self {
            layout,
            methods: HashMap::new(),
        }
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

    pub fn layout(&self) -> &DynamicLayout {
        &self.layout
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
}
