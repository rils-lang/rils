use crate::{FunctionSignature, Type, Value, is_identifier, value};

pub type NativeFunctionHandler = fn(&[Value]) -> Result<Value, String>;

#[derive(Clone)]
pub struct NativeTypeHandle {
    pub(crate) definition: std::rc::Rc<value::HostType>,
}

impl NativeTypeHandle {
    pub fn value<T: 'static>(&self, payload: T) -> Value {
        Value::HostObject(std::rc::Rc::new(value::HostObject {
            type_definition: self.definition.clone(),
            payload: std::rc::Rc::new(payload),
        }))
    }

    pub fn register_method<F>(
        &self,
        name: &str,
        min_arity: usize,
        max_arity: usize,
        function: F,
    ) -> Result<(), String>
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        if !is_identifier(name) {
            return Err(format!("`{name}` is not a valid method name"));
        }
        if min_arity > max_arity {
            return Err("method minimum arity cannot exceed maximum arity".into());
        }
        if self.definition.methods.borrow().contains_key(name) {
            return Err(format!("method `{name}` is already registered"));
        }
        self.definition.methods.borrow_mut().insert(
            name.into(),
            std::rc::Rc::new(value::HostFunction {
                name: name.into(),
                min_arity,
                max_arity,
                signature: None,
                function: std::rc::Rc::new(function),
            }),
        );
        Ok(())
    }

    pub fn register_typed_method<F>(
        &self,
        name: &str,
        parameters: Vec<Type>,
        return_type: Type,
        function: F,
    ) -> Result<(), String>
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        if !is_identifier(name) {
            return Err(format!("`{name}` is not a valid method name"));
        }
        if self.definition.methods.borrow().contains_key(name) {
            return Err(format!("method `{name}` is already registered"));
        }
        let arity = parameters.len();
        self.definition.methods.borrow_mut().insert(
            name.into(),
            std::rc::Rc::new(value::HostFunction {
                name: name.into(),
                min_arity: arity,
                max_arity: arity,
                signature: Some(FunctionSignature::fixed(parameters, return_type)),
                function: std::rc::Rc::new(function),
            }),
        );
        Ok(())
    }
}
