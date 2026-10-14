use rils_execution::{
    Value,
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol, call_native_symbol},
    value::native_i32,
};

#[test]
fn shared_declarations_have_checked_bridges() {
    for owner in ["Rc", "Weak"] {
        for member in rils_builtins::builtin(owner).unwrap().members {
            let Some(symbol) = member.native_symbol else {
                continue;
            };
            let result = if member.name == "new" {
                call_native_owned_symbol(symbol, vec![], &NativeOwnedContext::default())
            } else {
                call_native_symbol(symbol, &[])
            };
            assert!(
                result
                    .unwrap_or_else(|| panic!("missing bridge {symbol}"))
                    .unwrap_err()
                    .contains("expects")
            );
            if member.name != "new" {
                assert!(
                    call_native_symbol(symbol, &[Value::from(native_i32(1))])
                        .unwrap()
                        .is_err()
                );
            }
        }
    }
}
