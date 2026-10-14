//! Declaration-generated numeric calls and their shared conversion protocol.
use crate::Value;
use rils_stdlib::stdlib::integer::Integer;

pub(crate) trait NativeInput: Sized {
    fn from_value(value: &Value) -> Result<Self, String>;
}

pub(crate) use crate::runtime_builtins::native_output::NativeOutput;

/// Reads a Copy payload through the declaration's Rust mapping and reports
/// mismatched language types without exposing the erased Rust storage type.
fn copy_input<T: crate::RilsHostType + Copy + NativeOutput>(value: &Value) -> Result<T, String> {
    value.get_cloned::<T>().map_err(|error| {
        let expected = T::output_type();
        match crate::runtime_type::receiver_type(value) {
            Some(actual) if actual != expected => format!("expected {expected}, found {actual}"),
            _ => error,
        }
    })
}

impl NativeInput for Integer {
    fn from_value(value: &Value) -> Result<Self, String> {
        integer::integer_value(value)
    }
}

pub(super) mod integer {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::integer_definition!(decl_rils_native);
}

pub(super) mod float {
    use rils_stdlib_macros::decl_rils_native;
    rils_stdlib::float_definition!(decl_rils_native);
}
