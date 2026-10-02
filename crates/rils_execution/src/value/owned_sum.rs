//! Consume native sum storage at boundaries that still require legacy wrappers.

use std::rc::Rc;

use crate::Type;

use super::{EnumType, StructType, Value, dynamic_option, dynamic_result};

pub(super) const DECODE_OPERATION: &str = "rils_execution::decode_owned_sum";

pub fn materialize(
    value: Value,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Result<Value, String> {
    let Value::Dynamic(object) = &value else {
        return Ok(value);
    };
    if object.descriptor().has_owned_operation(DECODE_OPERATION) {
        let Value::Dynamic(object) = value else {
            unreachable!()
        };
        return object.call_owned(DECODE_OPERATION);
    }
    match object.descriptor().layout().rils_type().clone() {
        Type::Option(item) => Ok(Value::Option {
            value: dynamic_option::take_owned_with_definitions(value, structs, enums)?.map(Rc::new),
            element_type: Some(*item),
        }),
        Type::Result(ok, error) => Ok(Value::Result {
            value: dynamic_result::take_owned_with_definitions(value, structs, enums)?
                .map(Rc::new)
                .map_err(Rc::new),
            ok_type: Some(*ok),
            error_type: Some(*error),
        }),
        _ => Ok(value),
    }
}
