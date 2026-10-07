//! Type witnesses and scoped borrows shared by generated callback conversions.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::{Type, Value, environment::StorageSlot, value::ReferenceValue};

pub(super) struct Types {
    bindings: HashMap<String, Type>,
    output: Type,
}

pub(super) fn check_signature(pattern: &Type, function: &Value) -> Result<(), String> {
    if let Some(
        actual @ Type::Function {
            parameters: Some(_),
            ..
        },
    ) = Type::of_value(function)
    {
        rils_frontend::types::infer_generic_arguments(pattern, &actual, &mut HashMap::new())
            .map_err(|message| format!("native callback type mismatch: {message}"))?;
    }
    Ok(())
}

impl Types {
    pub(super) fn new(
        input_pattern: &Type,
        input: &Type,
        output_pattern: &Type,
        expected: Option<&Type>,
    ) -> Result<Self, String> {
        let mut bindings = HashMap::new();
        rils_frontend::types::infer_generic_arguments(input_pattern, input, &mut bindings)?;
        if let Some(expected) = expected {
            rils_frontend::types::infer_generic_arguments(output_pattern, expected, &mut bindings)?;
        }
        let output = output_pattern.substitute(&bindings);
        if !output.is_concrete_type() {
            return Err(format!(
                "cannot infer the complete native callback result type: {output}"
            ));
        }
        Ok(Self { bindings, output })
    }

    pub(super) fn output(&self) -> &Type {
        &self.output
    }

    pub(super) fn resolve(&self, pattern: &Type) -> Result<Type, String> {
        let ty = pattern.substitute(&self.bindings);
        if !ty.is_concrete_type() {
            return Err(format!(
                "cannot infer the complete native callback type: {ty}"
            ));
        }
        Ok(ty)
    }
}

/// Retain a payload's native owner during a predicate call. This clones only
/// an identity handle (or a Copy leaf), never an owned payload.
pub(super) fn shared_argument(value: &Value) -> Value {
    let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(false)));
    storage.borrow_mut().initialize(value.clone());
    Value::Reference(Rc::new(ReferenceValue::new_storage(storage, false)))
}
