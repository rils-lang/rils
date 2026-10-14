//! Backend-neutral native call handles. Concrete targets own their metadata.
use std::{any::Any, rc::Rc};

use rils_value::{DynamicLayout, DynamicType, DynamicValue};

use super::{DynamicObject, Value};
use crate::{Type, environment::EnvironmentRef};

mod targets;

/// Execution backends dispatch supported concrete targets. This interface
/// describes storage and captured values without depending on either backend.
pub trait CallTarget: Any {
    fn is_copy(&self) -> bool {
        true
    }
    fn signature(&self) -> Option<Type>;
    fn description(&self) -> String;
    fn visit_values(&self, _visit: &mut dyn FnMut(&Value)) {}
}

#[derive(Clone)]
pub struct Callable(Rc<dyn CallTarget>);

impl Callable {
    pub(crate) fn is_copy(&self) -> bool {
        self.0.is_copy()
    }
    pub fn downcast<T: CallTarget>(&self) -> Option<Rc<T>> {
        let target: Rc<dyn Any> = self.0.clone();
        target.downcast().ok()
    }

    pub fn description(&self) -> String {
        self.0.description()
    }

    pub(crate) fn into_value(self) -> Value {
        let ty = self.0.signature().unwrap_or_else(Type::opaque_function);
        let layout = DynamicLayout::copy_handle_of::<Self>(ty);
        let value = DynamicValue::from_rust(layout.clone(), self)
            .expect("callable layout matches its registered handle");
        Value::Dynamic(
            DynamicObject::new(Rc::new(DynamicType::new(layout)), value)
                .expect("callable descriptor matches its native payload"),
        )
    }

    pub(crate) fn contains_reference(&self) -> bool {
        let mut found = false;
        self.0
            .visit_values(&mut |value| found |= value.contains_reference());
        found
    }

    pub(crate) fn contains_local_reference(&self, environment: &EnvironmentRef) -> bool {
        let mut found = false;
        self.0
            .visit_values(&mut |value| found |= value.contains_local_reference(environment));
        found
    }
}

impl Value {
    pub fn from_callable<T: CallTarget>(target: Rc<T>) -> Self {
        Callable(target).into_value()
    }

    pub fn callable(&self) -> Option<Callable> {
        let Self::Dynamic(object) = self else {
            return None;
        };
        if !object.descriptor().layout().is_rust_type::<Callable>() {
            return None;
        }
        object
            .with(|value| value.view().with_rust::<Callable, _>(Clone::clone))
            .ok()?
            .ok()
    }

    pub fn as_callable<T: CallTarget>(&self) -> Option<Rc<T>> {
        self.callable()?.downcast()
    }

    pub fn into_callable<T: CallTarget>(self) -> Result<Rc<T>, Box<Self>> {
        match self.as_callable::<T>() {
            Some(target) => {
                drop(self);
                Ok(target)
            }
            None => Err(Box::new(self)),
        }
    }
}
