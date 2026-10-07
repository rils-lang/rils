//! The boundary between stored script data and execution-only values.

use std::collections::HashMap;
use std::rc::Rc;

use rils_value::DynamicType;

use crate::{Type, types::merge_types};

use super::{
    DynamicObject, EnumType, HostObject, HostType, NativeObject, StructType, Value,
    dynamic_sequence, record_codec::NativeRecordCodec,
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
    hosts: &'a [Rc<HostType>],
}

impl<'a> TypedStorageContext<'a> {
    pub fn new(structs: &'a [Rc<StructType>], enums: &'a [Rc<EnumType>]) -> Self {
        Self::with_hosts(structs, enums, &[])
    }

    pub fn with_hosts(
        structs: &'a [Rc<StructType>],
        enums: &'a [Rc<EnumType>],
        hosts: &'a [Rc<HostType>],
    ) -> Self {
        Self {
            structs,
            enums,
            hosts,
        }
    }

    pub fn layout(&self, ty: &Type) -> Result<Rc<rils_value::DynamicLayout>, String> {
        crate::runtime_builtins::resolve_layout(ty, self.structs, self.enums, self.hosts)
    }

    /// Consume a user struct/enum into native instance storage. Backends can
    /// switch their constructor and place paths together using this boundary.
    pub fn compose_nominal(&self, value: Value, expected: &Type) -> Result<Value, String> {
        let expected = declarations::storage_type(expected);
        let expected = self.concrete_expected(&value, &expected)?;
        let mut codec =
            NativeRecordCodec::with_definitions(self.structs, self.enums).with_owned_conversion();
        if codec.nominal_definition(&expected).is_none() {
            return Err(format!("no user instance declaration for {expected}"));
        }
        if !expected.accepts(&value) {
            return Err(format!(
                "declared {expected} does not accept {}",
                value.type_name()
            ));
        }
        let payload = codec.into_native(value, self.layout(&expected)?)?;
        super::native_instance::from_native(payload, Rc::new(codec))
    }

    pub fn apply_declared(&self, value: Value, expected: &Type) -> Result<Value, String> {
        let expected = declarations::storage_type(expected);
        let expected = self.concrete_expected(&value, &expected)?;
        let expected = &expected;
        if matches!(value, Value::Struct(_) | Value::Enum(_)) {
            return self.compose_nominal(value, expected);
        }
        let value = self.compose_indexed(value, expected)?;
        let value = self.attach_result_witness(value, expected)?;
        let value = self.compose_variant(value, expected)?;
        Ok(dynamic_sequence::promote_empty_with_context(
            value, expected, self,
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
        let expected = expected.substitute(&bindings);
        Ok(merge_types(&expected, &actual).unwrap_or(expected))
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
        if let Value::Dynamic(object) = &value
            && matches!(expected, Type::Option(_) | Type::Result(_, _))
            && merge_types(expected, object.descriptor().layout().rils_type()).is_none()
        {
            if !expected.accepts(&value) {
                return Err(format!(
                    "declared {expected} does not accept {}",
                    value.type_name()
                ));
            }
            let value = super::owned_sum::materialize(value, self.structs, self.enums)?;
            return self.compose_variant(value, expected);
        }
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
        let Ok(layout) = self.layout(expected) else {
            // Unresolved generic constructors retain their transitional form
            // until a concrete declaration is available.
            return Ok(value);
        };
        let payload = NativeRecordCodec::with_definitions(self.structs, self.enums)
            .into_native(value, layout.clone())?;
        let codec = Rc::new(NativeRecordCodec::with_definitions(
            self.structs,
            self.enums,
        ));
        let decode_codec = codec.clone();
        let descriptor = Rc::new(
            DynamicType::new(layout)
                .register_metadata(codec)
                .register_owned_operation(super::owned_sum::DECODE_OPERATION, move |value| {
                    decode_codec.from_native(value)
                }),
        );
        DynamicObject::new(descriptor, payload).map(Value::Dynamic)
    }
}
