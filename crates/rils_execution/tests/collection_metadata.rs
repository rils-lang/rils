use rils_execution::{
    Type, Value,
    runtime_builtins::call_native_symbol,
    value::{
        StructType, native_instance, record_codec::NativeRecordCodec,
        record_layout::RecordLayoutResolver,
    },
};
use rils_value::{DynamicLayout, DynamicValue};
use std::rc::Rc;

fn key_definition(traits: &[&str]) -> Rc<StructType> {
    Rc::new(StructType {
        name: "Key".into(),
        opaque_native: false,
        generic_parameters: Vec::new(),
        fields: Vec::new(),
        field_indices: Default::default(),
        methods: Default::default(),
        trait_methods: Default::default(),
        implemented_traits: std::cell::RefCell::new(
            traits.iter().map(|name| (*name).into()).collect(),
        ),
        associated_types: Default::default(),
    })
}

fn key_layout(definition: &Rc<StructType>) -> Rc<DynamicLayout> {
    RecordLayoutResolver::new(std::slice::from_ref(definition))
        .resolve(&Type::named("Key"))
        .unwrap()
}

fn codec(definition: &Rc<StructType>) -> Rc<NativeRecordCodec> {
    Rc::new(NativeRecordCodec::with_definitions(
        std::slice::from_ref(definition),
        &[],
    ))
}

fn set(layout: Rc<DynamicLayout>, codec: Rc<NativeRecordCodec>) -> Value {
    let ty = Type::Named {
        name: "HashSet".into(),
        arguments: vec![Type::named("Key")],
    };
    native_instance::from_native(
        DynamicValue::sequence(
            DynamicLayout::sequence(ty, layout.clone()),
            vec![DynamicValue::record(layout, Vec::new()).unwrap()],
        )
        .unwrap(),
        codec,
    )
    .unwrap()
}

fn call(method: &str, arguments: &[Value]) -> Result<Value, String> {
    let symbol = rils_builtins::builtin_member("HashSet", method)
        .unwrap()
        .native_symbol
        .unwrap();
    call_native_symbol(symbol, arguments).unwrap()
}

#[test]
fn borrowed_key_queries_use_the_querys_declarations() {
    let stored_definition = key_definition(&["Eq", "Hash"]);
    let receiver = set(key_layout(&stored_definition), codec(&stored_definition));
    for traits in [vec![], vec!["Eq"], vec!["Hash"], vec!["Eq", "Hash"]] {
        let query_definition = key_definition(&traits);
        let query = native_instance::from_native(
            DynamicValue::record(key_layout(&query_definition), Vec::new()).unwrap(),
            codec(&query_definition),
        )
        .unwrap();
        let result = call("contains", &[receiver.clone(), query]);
        if traits.len() == 2 {
            assert_eq!(result.unwrap().get_cloned::<bool>(), Ok(true));
        } else {
            assert!(result.unwrap_err().contains("Eq + Hash"));
        }
    }
}

#[test]
fn set_binary_operations_do_not_borrow_missing_declarations_from_other_operand() {
    let definition = key_definition(&["Eq", "Hash", "Clone"]);
    let layout = key_layout(&definition);
    for method in [
        "is_subset",
        "is_superset",
        "is_disjoint",
        "union",
        "intersection",
        "difference",
        "symmetric_difference",
    ] {
        for missing_left in [false, true] {
            let valid = set(layout.clone(), codec(&definition));
            let missing = set(layout.clone(), Rc::new(NativeRecordCodec::new()));
            let arguments = if missing_left {
                [missing, valid]
            } else {
                [valid, missing]
            };
            let error = call(method, &arguments).unwrap_err();
            assert!(
                error.contains("Eq + Hash"),
                "{method}, missing_left={missing_left}: {error}"
            );
        }
    }
}
