//! The boundary between stored script data and execution-only values.

use std::collections::HashMap;
use std::rc::Rc;

use rils_value::DynamicType;

use crate::{Type, types::merge_types};

use super::{
    DynamicObject, EnumType, HostObject, NativeObject, StructType, Value, dynamic_sequence,
    record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver,
};

mod assignment;
mod declarations;
mod indexed;
pub use assignment::{NativeDeclaration, constrain_assignment, native_declaration};

/// The three payload families that can represent a stored script value.
/// References, call targets, modules and declarations remain execution values.
pub(crate) enum StoredDataRef<'a> {
    Native(&'a NativeObject),
    Dynamic(&'a DynamicObject),
    Host(&'a HostObject),
}

impl Value {
    pub(crate) fn stored_data(&self) -> Option<StoredDataRef<'_>> {
        match self {
            Self::Native(value) => Some(StoredDataRef::Native(value)),
            Self::Dynamic(value) => Some(StoredDataRef::Dynamic(value)),
            Self::HostObject(value) => Some(StoredDataRef::Host(value)),
            _ => None,
        }
    }
}

/// Applies a concrete declaration to owned storage using the declarations
/// visible at that execution site. This is a storage operation, not a second
/// frontend type checker.
pub struct TypedStorageContext<'a> {
    structs: &'a [Rc<StructType>],
    enums: &'a [Rc<EnumType>],
}

impl<'a> TypedStorageContext<'a> {
    pub fn new(structs: &'a [Rc<StructType>], enums: &'a [Rc<EnumType>]) -> Self {
        Self { structs, enums }
    }

    pub fn apply_declared(&self, value: Value, expected: &Type) -> Result<Value, String> {
        let expected = declarations::storage_type(expected);
        let expected = self.concrete_expected(&value, &expected)?;
        let expected = &expected;
        let value = self.compose_indexed(value, expected)?;
        let value = self.attach_result_witness(value, expected)?;
        let value = self.compose_variant(value, expected)?;
        Ok(dynamic_sequence::promote_empty_with_definitions(
            value,
            expected,
            self.structs,
            self.enums,
        ))
    }

    fn concrete_expected(&self, value: &Value, expected: &Type) -> Result<Type, String> {
        let Some(actual) = Type::of_value(value) else {
            return Ok(expected.clone());
        };
        let mut bindings = HashMap::new();
        if crate::types::infer_generic_arguments(expected, &actual, &mut bindings).is_err() {
            // The frontend may have checked an alias against its underlying
            // nominal type. Storage only needs bindings it can infer here.
            return Ok(expected.clone());
        }
        bindings.retain(|_, ty| *ty != Type::Unknown);
        Ok(expected.substitute(&bindings))
    }

    fn attach_result_witness(&self, value: Value, expected: &Type) -> Result<Value, String> {
        let Type::Result(expected_ok, expected_error) = expected else {
            return Ok(value);
        };
        if matches!(value, Value::Result { .. }) && !expected.accepts(&value) {
            return Err(format!(
                "declared {expected} does not accept {}",
                value.type_name()
            ));
        }
        let Value::Result {
            value: branch,
            ok_type,
            error_type,
        } = value
        else {
            return Ok(value);
        };
        Ok(Value::Result {
            value: branch,
            ok_type: Some(
                merge_types(expected_ok, &ok_type.unwrap_or(Type::Unknown))
                    .ok_or("Result Ok type does not match declaration")?,
            ),
            error_type: Some(
                merge_types(expected_error, &error_type.unwrap_or(Type::Unknown))
                    .ok_or("Result Err type does not match declaration")?,
            ),
        })
    }

    fn compose_variant(&self, value: Value, expected: &Type) -> Result<Value, String> {
        if !matches!(expected, Type::Option(_) | Type::Result(_, _))
            || !matches!(value, Value::Option { .. } | Value::Result { .. })
        {
            return Ok(value);
        }
        if !expected.accepts(&value) {
            return Err(format!(
                "declared {expected} does not accept {}",
                value.type_name()
            ));
        }
        let Ok(layout) =
            RecordLayoutResolver::with_enums(self.structs, self.enums).resolve(expected)
        else {
            // A valid value can still contain a runtime-only child, such as a
            // reference or callable. Its representation is migrated later.
            return Ok(value);
        };
        let payload = NativeRecordCodec::with_definitions(self.structs, self.enums)
            .into_native(value, layout.clone())?;
        let codec = NativeRecordCodec::with_definitions(self.structs, self.enums);
        let descriptor = Rc::new(
            DynamicType::new(layout)
                .register_owned_operation(super::owned_sum::DECODE_OPERATION, move |value| {
                    codec.from_native(value)
                }),
        );
        DynamicObject::new(descriptor, payload).map(Value::Dynamic)
    }
}
