use rils_execution::{Type, Value};
use rils_stdlib::stdlib::string::String as NativeString;

#[test]
fn exact_native_string_constraint_transfers_the_unique_payload() {
    let value = Value::from_string("owned");
    let constrained = value.constrain_owned(&Type::String).unwrap();
    let Value::Native(object) = constrained else {
        panic!("string should keep its native representation");
    };
    let text = object
        .into_rust::<NativeString>()
        .unwrap_or_else(|_| panic!("the consumed string should not have another shared owner"));
    assert_eq!(std::string::String::from(text), "owned");
}
