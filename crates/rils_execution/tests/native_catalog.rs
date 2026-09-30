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
            if member.signature.is_some_and(|signature| {
                signature.parameters.is_empty() && member.receiver.is_none()
            }) {
                continue;
            }
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
