//! Owned assignment into a place with an established native declaration.

use super::*;

pub fn native_declaration(value: &Value) -> Option<Rc<DynamicType<Value>>> {
    match value {
        Value::Dynamic(value) => Some(value.descriptor_handle()),
        _ => None,
    }
}

pub fn constrain_assignment(
    value: Value,
    expected: &Type,
    declaration: Option<Rc<DynamicType<Value>>>,
) -> Result<Value, String> {
    let expected = declarations::storage_type(expected);
    let value = value
        .constrain_owned(&expected)
        .ok_or_else(|| format!("assigned value does not match {expected}"))?;
    let Some(declaration) = declaration else {
        return Ok(value);
    };
    if declaration.layout().rils_type() != &expected
        || !matches!(expected, Type::Option(_) | Type::Result(_, _))
    {
        return Ok(value);
    }
    let payload = NativeRecordCodec::new().into_native(value, declaration.layout_handle())?;
    DynamicObject::new(declaration, payload).map(Value::Dynamic)
}
