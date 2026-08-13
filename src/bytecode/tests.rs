use super::*;
use crate::FloatType;

fn assert_matches_interpreter(source: &str) {
    let interpreted = crate::eval(source).expect("source should interpret");
    let module = compile(source).expect("source should compile");
    let compiled = module.execute().expect("bytecode should execute");
    match (&compiled, &interpreted) {
        (Value::Range(left), Value::Range(right)) => assert_eq!(left, right),
        _ => assert_eq!(compiled, interpreted),
    }
}

#[test]
fn compiles_arithmetic_blocks_and_short_circuiting() {
    assert_matches_interpreter(
        r#"
                let mut value = 2;
                let selected = if true && (value < 4) {
                    value = value * 5;
                    value + 1
                } else {
                    0
                };
                selected
            "#,
    );
    assert_matches_interpreter(r#"let text = "ri" + "ls"; text"#);
}

#[test]
fn core_string_and_vec_helpers_match_interpreter() {
    assert_matches_interpreter(
        r#"
            let text = "  alpha beta alpha  ";
            let trimmed = text.trim();
            assert!(text.len() == 20usize);
            assert!(!text.is_empty());
            assert!(text.contains("beta"));
            assert!(text.starts_with("  alpha"));
            assert!(text.ends_with("alpha  "));
            assert!(unwrap(text.find("beta")) == 8usize);
            assert!(is_none("missing".find("x")));
            assert!(trimmed.replace("alpha", "rils") == "rils beta rils");
            assert!(trimmed == "alpha beta alpha");

            let empty: [i32; 0] = [];
            assert!(empty.is_empty());
            let mut values = Vec::from([1, 2, 3, 4]);
            values.truncate(2usize);
            assert!(values.len() == 2usize);
            values.clear();
            values.is_empty()
        "#,
    );
}

#[test]
fn option_and_result_helpers_match_interpreter() {
    assert_matches_interpreter(
        r#"
            let mut option = Some(40);
            let taken = option.take();
            assert!(is_none(option));
            assert!(taken.expect("expected a value") == 40);

            let ok: Result<i32, string> = Ok(2);
            let err: Result<i32, string> = Err("failed");
            assert!(ok.ok().expect("expected Ok") == 2);
            assert!(err.err().expect("expected Err") == "failed");
            true
        "#,
    );
}

#[test]
fn established_option_and_result_methods_match_interpreter() {
    assert_matches_interpreter(
        r#"
            let present = Some(40);
            assert!(present.is_some());
            let missing: Option<i32> = None;
            assert!(missing.is_none());
            assert!(missing.unwrap_or(2) == 2);

            let ok: Result<i32, string> = Ok(42);
            assert!(ok.is_ok());
            assert!(ok.unwrap() == 42);
            let err: Result<i32, string> = Err("failed");
            assert!(err.is_err());
            err.unwrap_or(7)
        "#,
    );
}

#[test]
fn compiles_while_break_and_continue() {
    assert_matches_interpreter(
        r#"
                let mut current = 0;
                let mut total = 0;
                while current < 8 {
                    current = current + 1;
                    if current % 2 == 0 { continue; }
                    if current > 5 { break; }
                    total = total + current;
                }
                total
            "#,
    );
}

#[test]
fn compiles_loop_values_and_moves() {
    assert_matches_interpreter(
        r#"
                let value = { loop { break 42; } };
                value
            "#,
    );
    assert_matches_interpreter(r#"let text = "owned"; let moved = text; moved"#);
}

#[test]
fn compiled_modules_are_reusable_and_limit_steps() {
    let module = compile("let value = 40; value + 2").unwrap();
    assert_eq!(module.execute().unwrap(), Value::I32(42));
    assert_eq!(module.execute().unwrap(), Value::I32(42));
    assert!(module.instruction_count() > 0);
    assert!(module.register_count() > 0);

    let endless = compile("loop {}").unwrap();
    let error = endless.execute_with_limit(16).unwrap_err();
    assert!(error.message.contains("step limit"));

    let recursion = compile(
        r#"
                fn recurse(value: i32) -> i32 { recurse(value + 1) }
                recurse(0)
            "#,
    )
    .unwrap();
    let error = recursion.execute_with_limit(10_000).unwrap_err();
    assert!(error.message.contains("call stack"));
}

#[test]
fn calls_named_functions_with_arguments() {
    let module = compile("pub fn add(left: i32, right: i32) -> i32 { left + right }").unwrap();
    assert_eq!(
        module
            .call("add", vec![Value::I32(20), Value::I32(22)])
            .unwrap(),
        Value::I32(42)
    );

    let error = module.call("missing", Vec::new()).unwrap_err();
    assert!(error.message.contains("unknown exported function"));
    let error = module.call("add", vec![Value::I32(1)]).unwrap_err();
    assert!(error.message.contains("expects 2 arguments"));

    let private = compile("fn hidden() -> i32 { 42 }").unwrap();
    let error = private.call("hidden", Vec::new()).unwrap_err();
    assert!(error.message.contains("unknown exported function"));
}

#[test]
fn compiles_functions_recursion_and_early_return() {
    let source = r#"
            fn factorial(n: i32) -> i32 {
                if n <= 1 { return 1; }
                n * factorial(n - 1)
            }

            fn choose(flag: bool, value: i32) -> i32 {
                if flag { return value; }
                0
            }

            factorial(6) + choose(true, 2)
        "#;
    assert_matches_interpreter(source);
    let module = compile(source).unwrap();
    assert_eq!(module.function_count(), 2);
    assert_eq!(module.execute().unwrap(), Value::I32(722));
}

#[test]
fn compiles_top_level_function_values_and_indirect_calls() {
    assert_matches_interpreter(
        r#"
                fn apply(transform: fn(i32) -> i32, value: i32) -> i32 {
                    transform(value)
                }

                fn double(value: i32) -> i32 { value * 2 }

                let transform = double;
                apply(transform, 21)
            "#,
    );
    assert_matches_interpreter(
        r#"
                fn answer() -> i32 { 42 }
                fn select() -> fn() -> i32 { answer }
                let selected = select();
                selected()
            "#,
    );
    assert_matches_interpreter(
        r#"
                fn answer() -> i32 { 42 }
                fn select() -> fn() -> i32 { answer }
                select()()
            "#,
    );
    assert_matches_interpreter(
        r#"
                fn left() -> i32 { 20 }
                fn right() -> i32 { 22 }
                (if true { left } else { right })() + right()
            "#,
    );
}

#[test]
fn compiles_bound_methods_and_general_receivers() {
    assert_matches_interpreter(
        r#"
                struct Number { value: i32 }
                impl Copy for Number {}
                impl Number {
                    fn add(self, amount: i32) -> i32 { self.value + amount }
                    fn read(&self) -> i32 { self.value }
                }

                let add = Number { value: 40 }.add;
                add(2) + Number { value: 0 }.read()
            "#,
    );
    assert_matches_interpreter(
        r#"
                struct Counter { value: i32 }
                struct State { counter: Counter }
                impl Counter {
                    fn increment(&mut self) { self.value = self.value + 1; }
                }

                let mut state = State { counter: Counter { value: 40 } };
                state.counter.increment();
                state.counter.increment();
                state.counter.value
            "#,
    );
}

#[test]
fn compiles_qualified_method_values() {
    assert_matches_interpreter(
        r#"
                trait Read { fn read(&self) -> i32; }
                struct Number { value: i32 }
                impl Read for Number {
                    fn read(&self) -> i32 { self.value }
                }

                let read = <Number as Read>::read;
                let number = Number { value: 42 };
                read(&number)
            "#,
    );
}

#[test]
fn compiles_places_rooted_in_references() {
    assert_matches_interpreter(
        r#"
                struct Number { value: i32 }
                fn update(number: &mut Number) {
                    (*number).value = 40;
                    let value = &mut (*number).value;
                    *value = *value + 2;
                    let again = &mut *value;
                    *again = *again + 1;
                }

                let mut number = Number { value: 0 };
                update(&mut number);
                number.value - 1
            "#,
    );
}

#[test]
fn compiles_closures_with_shared_mutable_captures() {
    assert_matches_interpreter(
        r#"
                fn make_counter() -> fn() -> i32 {
                    let mut count = 0;
                    fn next() -> i32 {
                        count = count + 1;
                        count
                    }
                    next
                }

                let counter = make_counter();
                counter();
                counter()
            "#,
    );
}

#[test]
fn links_and_executes_core_imports() {
    let source = r#"
            let text = "rils";
            let copied = clone(&text);
            let option = Some(type_of(copied));
            if is_some(option) && is_none(None) {
                unwrap_or(Some(40), 0) + unwrap_or(Err("missing"), 2)
            } else {
                0
            }
        "#;
    assert_matches_interpreter(source);

    let module = compile(source).unwrap();
    let names = module
        .imports()
        .iter()
        .map(|import| import.name.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(
        names,
        HashSet::from(["clone", "type_of", "is_some", "is_none", "unwrap_or"])
    );
    assert!(
        module
            .imports()
            .iter()
            .all(|import| import.abi_version == BYTECODE_HOST_ABI_VERSION)
    );
}

#[test]
fn compiles_and_executes_standard_native_macros() {
    let module = compile(
        r#"
            print!();
            println!();
            assert!(true, "must remain true");
            42
        "#,
    )
    .unwrap();
    let mut host = BytecodeHost::standard();
    host.enable_standard_io().unwrap();
    assert_eq!(module.execute_with_host(&host).unwrap(), Value::I32(42));

    let failure = compile("assert!(false, \"expected failure\")")
        .unwrap()
        .execute()
        .unwrap_err();
    assert!(failure.message.contains("expected failure"));
}

#[test]
fn compiles_vec_construction_methods_and_owned_iteration() {
    assert_matches_interpreter(
        r#"
                let mut values = Vec::from([1, 2]);
                values.push(3);
                let popped = values.pop();
                assert!(values.len() == 2);
                let mut total = unwrap(popped) + 2;
                for value in values {
                    total = total + value;
                }
                total
            "#,
    );
    assert_matches_interpreter(
        r#"
                let mut values: Vec<i32> = Vec::new();
                values.push(40);
                values.push(2);
                unwrap(values.pop())
            "#,
    );
}

#[test]
fn compiles_custom_iterator_and_into_iterator_traits() {
    assert_matches_interpreter(
        r#"
                struct CounterRange { current: i32, end: i32 }

                impl Iterator for CounterRange {
                    type Item = i32;
                    fn next(&mut self) -> Option<i32> {
                        if self.current < self.end {
                            let value = self.current;
                            let end = self.end;
                            *self = CounterRange { current: value + 1, end: end };
                            Some(value)
                        } else {
                            None
                        }
                    }
                }

                let mut total = 0;
                for value in CounterRange { current: 1, end: 7 } {
                    total = total + value;
                }
                total
            "#,
    );
    assert_matches_interpreter(
        r#"
                struct CounterRange { current: i32, end: i32 }
                struct CountTo { end: i32 }

                impl Iterator for CounterRange {
                    type Item = i32;
                    fn next(&mut self) -> Option<i32> {
                        if self.current < self.end {
                            let value = self.current;
                            let end = self.end;
                            *self = CounterRange { current: value + 1, end: end };
                            Some(value)
                        } else {
                            None
                        }
                    }
                }

                impl IntoIterator for CountTo {
                    type IntoIter = CounterRange;
                    fn into_iter(self) -> CounterRange {
                        CounterRange { current: 0, end: self.end }
                    }
                }

                let mut total = 0;
                for value in CountTo { end: 5 } {
                    total = total + value;
                }
                total
            "#,
    );
}

#[test]
fn rejects_unlinked_unauthorized_and_incompatible_imports() {
    let module = compile("type_of(42)").unwrap();

    let host = BytecodeHost::new(BYTECODE_HOST_ABI_VERSION);
    let error = module.execute_with_host(&host).unwrap_err();
    assert!(error.message.contains("not authorized"));

    let mut missing = BytecodeHost::new(BYTECODE_HOST_ABI_VERSION);
    missing.allow_capability("core");
    let error = module.execute_with_host(&missing).unwrap_err();
    assert!(error.message.contains("missing bytecode import"));

    let mut incompatible = BytecodeHost::new(BYTECODE_HOST_ABI_VERSION);
    incompatible.allow_capability("core");
    incompatible
        .register_function(
            "type_of",
            FunctionSignature::fixed(Vec::new(), Type::String),
            "core",
            |_| Ok(Value::String(Rc::from("invalid"))),
        )
        .unwrap();
    let error = module.execute_with_host(&incompatible).unwrap_err();
    assert!(error.message.contains("signature mismatch"));

    let mut wrong_abi = BytecodeHost::new(BYTECODE_HOST_ABI_VERSION + 1);
    wrong_abi.allow_capability("core");
    let error = module.execute_with_host(&wrong_abi).unwrap_err();
    assert!(error.message.contains("requires host ABI"));
}

#[test]
fn compiles_validates_and_executes_custom_host_contract_imports() {
    let mut contract = HostContract::new();
    contract
        .register_function(
            100,
            "unity_engine::math::add",
            FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
            "unity.math",
        )
        .unwrap();

    let module = compile_with_host("use unity_engine::math::add; add(20, 22)", &contract).unwrap();
    assert_eq!(module.imports().len(), 1);
    assert_eq!(module.imports()[0].name, "unity_engine::math::add");
    assert_eq!(module.imports()[0].capability, "unity.math");

    let mut host = BytecodeHost::new(BYTECODE_HOST_ABI_VERSION);
    host.allow_capability("unity.math");
    host.register_function(
        "unity_engine::math::add",
        FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
        "unity.math",
        |arguments| match arguments {
            [Value::I32(left), Value::I32(right)] => Ok(Value::I32(left + right)),
            _ => Err("unexpected arguments".into()),
        },
    )
    .unwrap();

    module.validate_host(&host).unwrap();
    assert_eq!(module.execute_with_host(&host).unwrap(), Value::I32(42));

    let image = module.to_bytes().unwrap();
    let loaded = BytecodeModule::from_bytes(&image).unwrap();
    loaded.validate_host(&host).unwrap();
    assert_eq!(loaded.execute_with_host(&host).unwrap(), Value::I32(42));
}

#[test]
fn custom_host_contract_participates_in_static_type_checking() {
    let mut contract = HostContract::new();
    contract
        .register_function(
            101,
            "unity_engine::time::scale",
            FunctionSignature::fixed(
                vec![Type::Float(FloatType::F32)],
                Type::Float(FloatType::F32),
            ),
            "unity.time",
        )
        .unwrap();

    let module = compile_with_host("unity_engine::time::scale(2.5)", &contract).unwrap();
    let mut host = BytecodeHost::new(BYTECODE_HOST_ABI_VERSION);
    host.allow_capability("unity.time");
    host.register_function(
        "unity_engine::time::scale",
        FunctionSignature::fixed(
            vec![Type::Float(FloatType::F32)],
            Type::Float(FloatType::F32),
        ),
        "unity.time",
        |arguments| match arguments {
            [Value::F32(value)] => Ok(Value::F32(value * 2.0)),
            _ => Err("unexpected arguments".into()),
        },
    )
    .unwrap();
    assert_eq!(module.execute_with_host(&host).unwrap(), Value::F32(5.0));

    let error = match compile_with_host("unity_engine::time::scale(true)", &contract) {
        Ok(_) => panic!("host argument type mismatch should fail"),
        Err(error) => error,
    };
    assert!(error.message.contains("argument"));
}

#[test]
fn rejects_host_contract_with_incompatible_abi_before_lowering() {
    let contract = HostContract::with_versions(BYTECODE_HOST_ABI_VERSION + 1, 1).unwrap();
    let error = match compile_with_host("40 + 2", &contract) {
        Ok(_) => panic!("incompatible host ABI should fail"),
        Err(error) => error,
    };
    assert!(error.message.contains("incompatible"));
}

#[test]
fn standard_fs_imports_require_explicit_capability() {
    let source = r#"
            let missing = std::fs::read_to_string(
                "rils-bytecode-file-that-should-never-exist.txt"
            );
            is_err(missing)
        "#;
    let module = compile(source).unwrap();
    assert!(
        module.imports().iter().any(
            |import| import.name == "std::fs::read_to_string" && import.capability == "std::fs"
        )
    );

    let error = module.execute().unwrap_err();
    assert!(error.message.contains("std::fs") && error.message.contains("not authorized"));

    let mut host = BytecodeHost::standard();
    host.enable_standard_fs().unwrap();
    let compiled = module.execute_with_host(&host).unwrap();
    assert_eq!(compiled, crate::eval(source).unwrap());
}

#[test]
fn compiles_nested_recursive_functions() {
    assert_matches_interpreter(
        r#"
                fn calculate(value: i32) -> i32 {
                    fn factorial(value: i32) -> i32 {
                        if value <= 1 { return 1; }
                        value * factorial(value - 1)
                    }
                    factorial(value)
                }
                calculate(6)
            "#,
    );
}

#[test]
fn compiles_nested_calls_and_owned_arguments() {
    assert_matches_interpreter(
        r#"
                fn add_one(n: i32) -> i32 { n + 1 }
                fn twice(n: i32) -> i32 { add_one(add_one(n)) }
                twice(40)
            "#,
    );
    assert_matches_interpreter(
        r#"
                fn identity(value: string) -> string { value }
                let text = "owned";
                identity(text)
            "#,
    );
}

#[test]
fn compiles_tuple_array_repeat_and_range_values() {
    assert_matches_interpreter("(1, true, \"value\")");
    assert_matches_interpreter("[1, 2, 3]");
    assert_matches_interpreter("[7; 4]");
    assert_matches_interpreter("2..8");
    assert_matches_interpreter(
        r#"
                let tuple = (10, 20, 30);
                let values = [1, 2, 3];
                tuple.1 + values[2]
            "#,
    );
}

#[test]
fn compiles_for_ranges_arrays_break_and_continue() {
    assert_matches_interpreter(
        r#"
                let mut total = 0;
                for value in 0..10 {
                    if value == 7 { break; }
                    if value % 2 == 0 { continue; }
                    total = total + value;
                }
                total
            "#,
    );
    assert_matches_interpreter(
        r#"
                let found = {
                    for value in [2, 4, 6, 8] {
                        if value > 5 { break value; }
                    }
                };
                found
            "#,
    );
}

#[test]
fn compiles_array_index_and_tuple_field_assignment() {
    assert_matches_interpreter(
        r#"
                let mut values = [1, 2, 3];
                values[1] = 9;
                values[1]
            "#,
    );
    assert_matches_interpreter(
        r#"
                let mut pair = ("left", "right");
                pair.1 = "updated";
                pair.1
            "#,
    );

    let module = compile("let values = [1]; values[2]").unwrap();
    let error = module.execute().unwrap_err();
    assert!(error.message.contains("out of bounds"));
}

#[test]
fn compiles_option_result_and_question_mark() {
    assert_matches_interpreter("Some(42)");
    assert_matches_interpreter("None");
    assert_matches_interpreter("Ok(\"value\")");
    assert_matches_interpreter("Err(7)");

    let source = r#"
            fn read(flag: bool) -> Result<i32, string> {
                if flag { Ok(20) } else { Err("missing") }
            }

            fn double(flag: bool) -> Result<i32, string> {
                let value = read(flag)?;
                Ok(value * 2)
            }

            double(true)
        "#;
    assert_matches_interpreter(source);
    assert_matches_interpreter(&source.replace("double(true)", "double(false)"));
}

#[test]
fn compiles_match_literals_options_results_and_bindings() {
    assert_matches_interpreter(
        r#"
                let value = Some(21);
                match value {
                    Some(inner) => inner * 2,
                    None => 0,
                }
            "#,
    );
    assert_matches_interpreter(
        r#"
                let value = Err("failed");
                match value {
                    Ok(inner) => inner,
                    Err(message) => message,
                }
            "#,
    );
    assert_matches_interpreter(
        r#"
                let value = 2;
                match value {
                    0 => "zero",
                    1 => "one",
                    other => if other > 1 { "many" } else { "negative" },
                }
            "#,
    );
}

#[test]
fn compiles_type_aliases_and_type_erased_generic_functions() {
    assert_matches_interpreter(
        r#"
                type Number = i32;
                type Alias<T> = T;

                fn identity<T: Clone>(value: T) -> T { value }
                let left: Number = identity(20);
                let right: Alias<i32> = identity(22);
                (left, right)
            "#,
    );
}

#[test]
fn compiles_local_and_element_borrows_and_dereference_assignment() {
    assert_matches_interpreter(
        r#"
                fn increment(value: &mut i32) { *value = *value + 1; }
                fn run() -> i32 {
                    let mut value = 40;
                    increment(&mut value);
                    let shared = &value;
                    *shared + 1
                }
                run()
            "#,
    );
    assert_matches_interpreter(
        r#"
                fn run() -> i32 {
                    let mut values = [1, 2, 3];
                    let reference = &mut values[1];
                    *reference = 9;
                    values[1]
                }
                run()
            "#,
    );
    assert_matches_interpreter(
        r#"
                fn run() -> string {
                    let text = "owned";
                    { let reference = &text; }
                    text
                }
                run()
            "#,
    );
}

#[test]
fn compiles_struct_fields_borrows_and_enum_patterns() {
    assert_matches_interpreter(
        r#"
                struct Point { x: i32, y: i32 }
                fn run() -> i32 {
                    let mut point = Point { x: 1, y: 2 };
                    let x = &mut point.x;
                    *x = 10;
                    point.y = 5;
                    point.x + point.y
                }
                run()
            "#,
    );
    assert_matches_interpreter(
        r#"
                enum Message {
                    Quit,
                    Value(i32),
                    Move { x: i32, y: i32 },
                }
                let first = Message::Value(21);
                let second = Message::Move { x: 10, y: 11 };
                let left = match first {
                    Message::Value(value) => value,
                    _ => 0,
                };
                let right = match second {
                    Message::Move { x, y } => x + y,
                    _ => 0,
                };
                left + right
            "#,
    );
    assert_matches_interpreter(
        r#"
                enum State { Ready, Waiting }
                match State::Ready {
                    State::Ready => 1,
                    State::Waiting => 0,
                }
            "#,
    );
}

#[test]
fn compiles_nested_struct_field_places() {
    assert_matches_interpreter(
        r#"
            struct Leaf { value: i32 }
            struct Branch { leaf: Leaf }
            struct Tree { branch: Branch }

            let mut tree = Tree {
                branch: Branch { leaf: Leaf { value: 20 } }
            };
            let first = tree.branch.leaf.value;
            tree.branch.leaf.value = first + 1;
            {
                let value = &mut tree.branch.leaf.value;
                *value = *value + 21;
            }
            tree.branch.leaf.value
        "#,
    );
}

#[test]
fn compiles_mixed_nested_field_and_index_places() {
    assert_matches_interpreter(
        r#"
            struct Group { values: [i32; 2] }

            let mut groups = Vec::from([
                Group { values: [1, 2] },
                Group { values: [3, 4] }
            ]);
            let group = 1;
            let item = 0;
            groups[group].values[item] = 20;
            {
                let selected = &mut groups[group].values[1];
                *selected = *selected + 18;
            }
            groups[group].values[item] + groups[group].values[1]
        "#,
    );
}

#[test]
fn compiles_associated_functions_and_all_self_receivers() {
    assert_matches_interpreter(
        r#"
                struct Counter { value: i32 }
                impl Counter {
                    fn new(value: i32) -> Self { Counter { value: value } }
                    fn get(&self) -> i32 { self.value }
                    fn add(&mut self, amount: i32) {
                        let next = self.value + amount;
                        *self = Counter { value: next };
                    }
                    fn into_value(self) -> i32 { self.value }
                }

                let mut counter = Counter::new(10);
                counter.add(5);
                let current = counter.get();
                current + counter.into_value()
            "#,
    );
    assert_matches_interpreter(
        r#"
                enum Number { Value(i32) }
                impl Number {
                    fn read(self) -> i32 {
                        match self { Number::Value(value) => value }
                    }
                }
                let number = Number::Value(42);
                number.read()
            "#,
    );
}

#[test]
fn compiles_trait_impl_dispatch_and_ufcs() {
    assert_matches_interpreter(
        r#"
                trait Value { fn value(&self) -> i32; }
                struct Number { inner: i32 }
                impl Value for Number {
                    fn value(&self) -> i32 { self.inner }
                }

                let number = Number { inner: 21 };
                number.value() + <Number as Value>::value(&number)
            "#,
    );
}

#[test]
fn compiles_inline_modules_qualified_calls_and_use_aliases() {
    assert_matches_interpreter(
        r#"
                mod math {
                    fn increment(value: i32) -> i32 { value + 1 }
                    pub fn add(left: i32, right: i32) -> i32 {
                        increment(left + right - 1)
                    }
                }

                use math::add as sum;
                sum(20, 21) + math::add(0, 1)
            "#,
    );

    assert_matches_interpreter(
        r#"
                mod outer {
                    pub mod inner {
                        pub fn answer() -> i32 { 42 }
                    }
                }
                outer::inner::answer()
            "#,
    );

    assert_matches_interpreter(
        r#"
                mod first {
                    fn answer() -> i32 { 20 }
                    pub fn value() -> i32 { answer() }
                }
                mod second {
                    fn answer() -> i32 { 22 }
                    pub fn value() -> i32 { answer() }
                }
                first::value() + second::value()
            "#,
    );
}

#[test]
fn compiles_module_qualified_nominal_types() {
    assert_matches_interpreter(
        r#"
                mod model {
                    pub struct Point { value: i32 }
                    pub enum Message { Value { value: i32 } }
                }

                let point = model::Point { value: 20 };
                let message = model::Message::Value { value: 22 };
                match message {
                    model::Message::Value { value } => point.value + value,
                    _ => 0,
                }
            "#,
    );
}

#[test]
fn compiles_impls_declared_inside_modules() {
    assert_matches_interpreter(
        r#"
                mod model {
                    pub struct Number { value: i32 }
                    pub trait Read { fn read(&self) -> i32; }

                    impl Number {
                        fn new(value: i32) -> Self { Number { value: value } }
                        fn add(&mut self, amount: i32) {
                            let next = self.value + amount;
                            *self = Number { value: next };
                        }
                    }

                    impl Read for Number {
                        fn read(&self) -> i32 { self.value }
                    }

                    pub fn read_again(number: &Number) -> i32 {
                        <Number as Read>::read(number)
                    }
                }

                let mut number = model::Number::new(20);
                number.add(1);
                number.read() + model::read_again(&number)
            "#,
    );
}

#[test]
fn compile_file_loads_external_modules() {
    let directory =
        std::env::temp_dir().join(format!("rils-bytecode-module-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let root = directory.join("main.rils");
    let module = directory.join("math.rils");
    std::fs::write(&root, "mod math; use math::answer; answer()").unwrap();
    std::fs::write(&module, "pub fn answer() -> i32 { 42 }").unwrap();

    let compiled = crate::compile_file(&root).expect("file module should compile");
    assert_eq!(compiled.execute().unwrap(), Value::I32(42));

    std::fs::remove_file(root).unwrap();
    std::fs::remove_file(module).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn rejects_static_errors_and_unsupported_features() {
    let static_error = compile("let value = missing;")
        .err()
        .expect("unknown names must be rejected");
    assert!(
        static_error.message.contains("missing"),
        "unexpected diagnostic: {}",
        static_error.message
    );

    let ownership_error = compile(
        "fn outer(value: &i32) { fn inner() { *value } inner } let value = 1; outer(&value)",
    )
    .err()
    .expect("closures cannot capture local references");
    assert!(ownership_error.message.contains("capture local references"));

    let borrowed_method = compile(
        "struct Value { inner: i32 } impl Value { fn read(&self) -> i32 { self.inner } } let value = Value { inner: 1 }; let read = value.read; read()",
    )
    .err()
    .expect("bound method values cannot retain a local reference");
    assert!(borrowed_method.message.contains("bound method values"));

    let owned_method = compile(
        "struct Message { text: string } impl Message { fn consume(self) -> string { self.text } } let message = Message { text: \"owned\" }; let consume = message.consume; consume()",
    )
    .err()
    .expect("a Copy function value cannot hide a non-Copy receiver");
    assert!(owned_method.message.contains("owned receiver require Copy"));
}
