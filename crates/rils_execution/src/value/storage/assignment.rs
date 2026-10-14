//! Owned assignment into a place with an established native declaration.

use super::*;

/// Payload-free storage declarations retained when an aggregate is moved out.
#[derive(Clone)]
pub struct NativeDeclaration(Rc<DynamicType<Value>>);

impl NativeDeclaration {
    pub fn rils_type(&self) -> &Type {
        self.0.layout().rils_type()
    }

    fn needs_conversion(&self, value: &Value) -> bool {
        match value {
            Value::Dynamic(value) => {
                let declaration = &self.0;
                !declaration
                    .layout()
                    .compatible_with(value.descriptor().layout())
                    || (declaration
                        .metadata::<NativeRecordCodec>()
                        .is_some_and(|codec| {
                            codec
                                .nominal_definition(declaration.layout().rils_type())
                                .is_some()
                        })
                        && value.descriptor().metadata::<NativeRecordCodec>().is_none())
            }
            _ => true,
        }
    }

    fn apply_owned(&self, value: Value, expected: &Type) -> Result<Value, String> {
        if !expected.accepts(&value) {
            return Err(format!("assigned value does not match {expected}"));
        }
        if !self.needs_conversion(&value) {
            return Ok(value);
        }
        let declaration = &self.0;
        if matches!(expected, Type::Option(_) | Type::Result(_, _))
            || declaration
                .metadata::<NativeRecordCodec>()
                .is_some_and(|codec| codec.nominal_definition(expected).is_some())
        {
            let mut codec = declaration
                .metadata::<NativeRecordCodec>()
                .map(|codec| codec.as_ref().clone())
                .unwrap_or_default();
            let payload = codec.into_native(value, declaration.layout_handle())?;
            super::super::native_instance::with_descriptor(declaration.clone(), payload)
                .map(Value::Dynamic)
        } else {
            value
                .constrain_owned(expected)
                .ok_or_else(|| format!("assigned value does not match {expected}"))
        }
    }
}

pub fn native_declaration(value: &Value) -> Option<NativeDeclaration> {
    match value {
        Value::Dynamic(value) => Some(NativeDeclaration(value.descriptor_handle())),
        _ => None,
    }
}

pub fn constrain_assignment(
    value: Value,
    expected: &Type,
    declaration: Option<NativeDeclaration>,
) -> Result<Value, String> {
    let expected = declarations::storage_type(expected);
    if let Some(declaration) = declaration
        && declaration.rils_type() == &expected
    {
        return declaration.apply_owned(value, &expected);
    }
    value
        .constrain_owned(&expected)
        .ok_or_else(|| format!("assigned value does not match {expected}"))
}
