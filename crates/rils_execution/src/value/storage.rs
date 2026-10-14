//! The boundary between stored script data and execution-only values.

use std::collections::HashMap;
use std::rc::Rc;

use rils_value::DynamicType;

use crate::{Type, types::merge_types};

use super::{EnumType, HostType, StructType, Value, record_codec::NativeRecordCodec};

mod aggregate;
mod assignment;
mod construction;
mod declarations;
pub(crate) use declarations::storage_type;
mod sum;
pub use assignment::{NativeDeclaration, constrain_assignment, native_declaration};
pub(crate) use sum::PreparedOption;

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

    pub fn apply_declared(&self, value: Value, expected: &Type) -> Result<Value, String> {
        let expected = declarations::storage_type(expected);
        let expected = self.concrete_expected(&value, &expected)?;
        let expected = &expected;
        self.compose_variant(value, expected)
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
            let layout = self.layout(expected)?;
            let mut codec = NativeRecordCodec::with_definitions(self.structs, self.enums);
            let payload = codec.into_native(value, layout)?;
            return super::native_instance::from_native(payload, Rc::new(codec));
        }
        Ok(value)
    }
}
