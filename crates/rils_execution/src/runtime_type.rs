//! Runtime bridge between static [`Type`] descriptions and dynamically stored [`Value`]s.

use crate::{
    types::{RuntimeValue, Type, merge_types},
    value::Value,
};

/// Resolve a method receiver's type through lexical references, without reading its payload.
pub fn receiver_type(value: &Value) -> Option<Type> {
    let mut ty = Type::of_value(value)?;
    while let Type::Reference { inner, .. } = ty {
        ty = *inner;
    }
    Some(ty)
}

impl RuntimeValue for Value {
    fn is_accepted_by(&self, expected: &Type) -> bool {
        accepts(expected, self)
    }

    fn constrain_to(&self, expected: &Type) -> Option<Self> {
        constrain(expected, self)
    }

    fn runtime_type(&self) -> Option<Type> {
        type_of_value(self)
    }
}

impl Value {
    /// Apply declaration information needed by erased runtime storage after
    /// the shared frontend has checked the binding. Only values whose
    /// representation still needs a concrete type witness are changed.
    pub fn apply_declared_storage(self, expected: &Type) -> Result<Self, String> {
        crate::value::storage::TypedStorageContext::new(&[], &[]).apply_declared(self, expected)
    }

    /// Preserve unique ownership for an already concrete native or nominal
    /// value. Other cases still use the existing constraint logic, which may
    /// infer generic arguments or materialize a different representation.
    pub fn constrain_owned(self, expected: &Type) -> Option<Self> {
        if let Self::Dynamic(object) = &self
            && matches!(expected, Type::Option(_) | Type::Result(_, _))
            && merge_types(expected, object.descriptor().layout().rils_type()).is_none()
            && expected.accepts(&self)
        {
            // Preserve the owner here. The execution site's typed storage
            // context supplies nominal and host layouts for the conversion.
            return Some(self);
        }
        if Type::of_value(&self).as_ref() == Some(expected)
            && matches!(self, Self::Native(_) | Self::Dynamic(_))
            && expected.accepts(&self)
        {
            return Some(self);
        }
        expected.constrain(&self)
    }
}

fn accepts(expected: &Type, value: &Value) -> bool {
    match (expected, value) {
        (Type::Unknown | Type::Variable(_) | Type::BoundVariable { .. }, _) => true,
        (
            Type::Reference {
                mutable: expected_mutable,
                inner: expected_inner,
            },
            value,
        ) if value.as_reference().is_some() => {
            let reference = value.as_reference().unwrap();
            (!*expected_mutable || reference.mutable)
                && reference.accepts_referent(expected_inner).unwrap_or(false)
        }
        (_, Value::HostObject(object)) => accepts_host(expected, object),
        (expected, Value::Native(object)) => merge_types(
            &crate::value::storage::storage_type(expected),
            &crate::value::storage::storage_type(object.descriptor().rils_type()),
        )
        .is_some(),
        (expected, Value::Dynamic(object)) => {
            merge_types(expected, object.descriptor().layout().rils_type()).is_some()
                || object
                    .with(|value| {
                        crate::value::runtime_layouts::accepts_view(value.view(), expected)
                    })
                    .is_ok_and(|accepted| accepted.unwrap_or(false))
        }
    }
}

fn constrain(expected: &Type, value: &Value) -> Option<Value> {
    if !expected.accepts(value) {
        return None;
    }
    Some(value.clone())
}

fn type_of_value(value: &Value) -> Option<Type> {
    match value {
        __reference_value if __reference_value.as_reference().is_some() => {
            let reference = __reference_value
                .as_reference()
                .expect("checked reference handle");
            Some(Type::Reference {
                mutable: reference.mutable,
                inner: Box::new(reference.referent_type().ok()?),
            })
        }

        Value::HostObject(object) => Some(Type::named(object.type_definition.name.clone())),
        Value::Native(object) => Some(object.descriptor().rils_type().clone()),
        Value::Dynamic(object) => Some(object.descriptor().layout().rils_type().clone()),
    }
}

pub(crate) fn accepts_host(expected: &Type, object: &crate::value::HostObject) -> bool {
    match expected {
        Type::Unknown | Type::Variable(_) | Type::BoundVariable { .. } => true,
        Type::Named { name, arguments } => {
            arguments.is_empty()
                && (object.type_definition.name == *name
                    || object.type_definition.base_types.contains(name))
        }
        _ => false,
    }
}
