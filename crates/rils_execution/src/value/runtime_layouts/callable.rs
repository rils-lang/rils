//! Call targets embedded in composed storage retain their existing identity.

use std::rc::Rc;

use crate::environment::EnvironmentRef;

use super::super::{
    BoundMethod, BuiltinBoundMethod, BytecodeFunctionValue, HostBoundMethod, HostFunction,
    NativeFunction, TraitMethodSelector, UserFunction, Value, VariantConstructor,
};

#[derive(Clone)]
pub(super) enum Callable {
    User(Rc<UserFunction>),
    Bytecode(Rc<BytecodeFunctionValue>),
    Native(Rc<NativeFunction>),
    Host(Rc<HostFunction>),
    HostMethod(Rc<HostBoundMethod>),
    Constructor(Rc<VariantConstructor>),
    Method(Rc<BoundMethod>),
    BuiltinMethod(Rc<BuiltinBoundMethod>),
    TraitMethod(Rc<TraitMethodSelector>),
}

impl Callable {
    pub(super) fn from_value(value: Value) -> Result<Self, String> {
        Ok(match value {
            Value::Function(value) => Self::User(value),
            Value::BytecodeFunction(value) => Self::Bytecode(value),
            Value::NativeFunction(value) => Self::Native(Rc::new(value)),
            Value::HostFunction(value) => Self::Host(value),
            Value::HostBoundMethod(value) => Self::HostMethod(value),
            Value::VariantConstructor(value) => Self::Constructor(value),
            Value::BoundMethod(value) => Self::Method(value),
            Value::BuiltinBoundMethod(value) => Self::BuiltinMethod(value),
            Value::TraitMethodSelector(value) => Self::TraitMethod(value),
            value => return Err(format!("{} is not a call target", value.type_name())),
        })
    }

    pub(super) fn into_value(self) -> Value {
        match self {
            Self::User(value) => Value::Function(value),
            Self::Bytecode(value) => Value::BytecodeFunction(value),
            Self::Native(value) => Value::NativeFunction(
                Rc::try_unwrap(value).unwrap_or_else(|value| value.as_ref().clone()),
            ),
            Self::Host(value) => Value::HostFunction(value),
            Self::HostMethod(value) => Value::HostBoundMethod(value),
            Self::Constructor(value) => Value::VariantConstructor(value),
            Self::Method(value) => Value::BoundMethod(value),
            Self::BuiltinMethod(value) => Value::BuiltinBoundMethod(value),
            Self::TraitMethod(value) => Value::TraitMethodSelector(value),
        }
    }

    pub(super) fn contains_reference(&self) -> bool {
        match self {
            Self::Bytecode(value) => Value::BytecodeFunction(value.clone()).contains_reference(),
            Self::HostMethod(value) => value.receiver.contains_reference(),
            Self::Method(value) => value.receiver.contains_reference(),
            Self::BuiltinMethod(value) => value.receiver.contains_reference(),
            _ => false,
        }
    }

    pub(super) fn contains_local_reference(&self, environment: &EnvironmentRef) -> bool {
        match self {
            Self::Bytecode(value) => {
                Value::BytecodeFunction(value.clone()).contains_local_reference(environment)
            }
            Self::HostMethod(value) => value.receiver.contains_local_reference(environment),
            Self::Method(value) => value.receiver.contains_local_reference(environment),
            Self::BuiltinMethod(value) => value.receiver.contains_local_reference(environment),
            _ => false,
        }
    }
}
