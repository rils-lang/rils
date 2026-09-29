use rils_execution::runtime_builtins::{
    NativeOwnedContext, call_native_owned_symbol, call_native_symbol, requires_owned_native_call,
};

#[test]
fn every_native_member_has_a_bridge_and_rejects_missing_arguments() {
    for declaration in rils_builtins::BUILTINS {
        for member in declaration.members {
            let Some(symbol) = member.native_symbol else {
                continue;
            };
            assert!(
                member.builtin_id.is_none() || requires_owned_native_call(symbol),
                "{symbol}"
            );
            assert!(member.runtime_import.is_none(), "{symbol}");
            let result = if requires_owned_native_call(symbol) {
                call_native_owned_symbol(
                    symbol,
                    Vec::new(),
                    &NativeOwnedContext {
                        structs: Vec::new(),
                        enums: Vec::new(),
                    },
                )
            } else {
                call_native_symbol(symbol, &[])
            }
            .unwrap_or_else(|| panic!("missing generated native bridge for {symbol}"));
            assert!(result.is_err(), "{symbol} accepted a missing receiver");
        }
    }
}
