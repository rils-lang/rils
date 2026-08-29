use super::*;

fn integer(source: &str) -> i32 {
    match eval(source).unwrap() {
        Value::I32(value) => value,
        value => panic!("expected integer, found {value:?}"),
    }
}

#[test]
fn derives_default_from_field_defaults() {
    let source = r#"
        #[derive(Default)]
        struct Settings {
            enabled: bool,
            retries: i32,
            name: string,
            position: (f32, f32),
            tags: Vec<string>,
            selected: Option<i32>,
        }

        let settings = <Settings as Default>::default();
        assert!(!settings.enabled);
        assert!(settings.retries == 0);
        assert!(settings.name == "");
        assert!(settings.position.0 == 0f32);
        assert!(settings.selected == None);
        <i64 as Default>::default()
    "#;
    assert_eq!(eval(source).unwrap(), Value::I64(0));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I64(0));
}

#[test]
fn derives_default_for_unit_structs() {
    let source = r#"
        #[derive(Default)]
        struct Marker;
        let marker = <Marker as Default>::default();
        type_of(marker)
    "#;
    assert_eq!(eval(source).unwrap(), Value::String("Marker".into()));
    assert_eq!(
        compile(source).unwrap().execute().unwrap(),
        Value::String("Marker".into())
    );
}

#[test]
fn derives_debug_for_structs_and_enums() {
    let source = r#"
        #[derive(Debug)]
        struct Point { x: i32, y: i32 }
        #[derive(Debug)]
        enum Shape { Empty, Point(Point) }
        let point = Point { x: 1, y: 2 };
        println!("point = {:#?}", point);
        point.x
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(1));
    let module = crate::compile(source).expect("Debug derives should compile to bytecode");
    let mut host = crate::BytecodeHost::standard();
    host.enable_standard_io().unwrap();
    assert_eq!(module.execute_with_host(&host).unwrap(), Value::I32(1));
}

#[test]
fn bytecode_formatting_calls_custom_traits_and_nested_debug() {
    let source = r#"
        struct Label { value: i32 }
        impl core::fmt::Display for Label {
            fn fmt(&self, formatter: &mut core::fmt::Formatter) -> Result<(), core::fmt::FormatError> {
                formatter.write_str("custom label")
            }
        }
        impl core::fmt::Debug for Label {
            fn fmt(&self, formatter: &mut core::fmt::Formatter) -> Result<(), core::fmt::FormatError> {
                formatter.write_str("debug label")
            }
        }
        #[derive(Debug)]
        struct Wrapper { label: Label }
        let label = Label { value: 1 };
        println!("{}", label);
        let wrapper = Wrapper { label: Label { value: 2 } };
        println!("{:?}", wrapper);
    "#;
    assert_eq!(eval(source).unwrap(), Value::Unit);
    let module = compile(source).unwrap();
    let captured = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let output = captured.clone();
    let mut host = BytecodeHost::standard();
    host.allow_capability("std::io");
    host.register_function(
        "std::io::println",
        FunctionSignature::variadic(Type::Unit),
        "std::io",
        move |arguments| {
            let Value::String(value) = &arguments[1] else {
                return Err("expected formatted output".into());
            };
            output.borrow_mut().push(value.to_string());
            Ok(Value::Unit)
        },
    )
    .unwrap();
    module.execute_with_host(&host).unwrap();
    assert_eq!(
        captured.borrow().as_slice(),
        ["custom label", "Wrapper { label: debug label }"]
    );
}

#[test]
fn self_paths_resolve_to_the_current_impl_type() {
    let source = r#"
        struct Counter { value: i32 }
        impl Counter {
            fn new(value: i32) -> Self { Self { value: value } }
            fn answer() -> Self { Self::new(42) }
        }
        let counter = Counter::answer();
        counter.value
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn default_is_available_for_builtin_composite_types() {
    let source = r#"
        let pair = <(bool, i16) as Default>::default();
        let values = <[u8; 2] as Default>::default();
        let optional = <Option<string> as Default>::default();
        let items = <Vec<i32> as Default>::default();
        assert!(!pair.0 && pair.1 == 0i16);
        assert!(values[0usize] == 0u8 && values[1usize] == 0u8);
        assert!(optional == None);
        let _items = items;
        pair.0
    "#;
    assert_eq!(eval(source).unwrap(), Value::Bool(false));
    assert_eq!(
        compile(source).unwrap().execute().unwrap(),
        Value::Bool(false)
    );
}

#[test]
fn supports_explicit_default_impls_in_derived_fields() {
    let source = r#"
        struct Port { value: i32 }
        impl Default for Port {
            fn default() -> Self { Port { value: 8080 } }
        }
        #[derive(Default)]
        struct Server { port: Port }
        let server = <Server as Default>::default();
        server.port.value
    "#;
    assert_eq!(integer(source), 8080);
    assert_eq!(
        compile(source).unwrap().execute().unwrap(),
        Value::I32(8080)
    );
}

#[test]
fn trait_supertraits_are_required_by_interpreter_and_compiler() {
    let valid = r#"
        trait Behaviour: Default {}
        #[derive(Default)]
        struct State;
        impl Behaviour for State {}
        <State as Default>::default();
    "#;
    assert_eq!(eval(valid).unwrap(), Value::Unit);
    assert_eq!(compile(valid).unwrap().execute().unwrap(), Value::Unit);

    let missing = "trait Behaviour: Default {} struct State; impl Behaviour for State {}";
    let interpreted = eval(missing).unwrap_err().to_string();
    assert!(interpreted.contains("must implement supertrait `Default`"));
    let compiled = match compile(missing) {
        Ok(_) => panic!("missing supertrait unexpectedly compiled"),
        Err(error) => error.to_string(),
    };
    assert!(compiled.contains("must implement supertrait `Default`"));
}

#[test]
fn supports_concrete_numeric_types_char_and_contextual_usize_inference() {
    let source = r#"
        assert!(type_of(1i8) == "i8");
        assert!(type_of(1i16) == "i16");
        assert!(type_of(1i32) == "i32");
        assert!(type_of(1i64) == "i64");
        assert!(type_of(1i128) == "i128");
        assert!(type_of(1isize) == "isize");
        assert!(type_of(1u8) == "u8");
        assert!(type_of(1u16) == "u16");
        assert!(type_of(1u32) == "u32");
        assert!(type_of(1u64) == "u64");
        assert!(type_of(1u128) == "u128");
        assert!(type_of(1usize) == "usize");
        assert!(type_of(1.5f32) == "f32");
        assert!(type_of(1.5f64) == "f64");
        assert!(type_of('你') == "char");

        let values = [20, 22];
        let index = 1;
        assert!(type_of(index) == "usize");
        values[index]
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(22));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(22));
}

#[test]
fn casts_integers_without_silent_information_loss() {
    let source = r#"
        let values = [20, 22];
        let index = 1_i32;
        values[index as usize]
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(22));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(22));

    let narrowing = match compile("let value = 1usize; value as i32") {
        Ok(_) => panic!("lossy cast unexpectedly compiled"),
        Err(error) => error,
    };
    assert!(
        narrowing
            .to_string()
            .contains("cannot losslessly cast `usize` to `i32`"),
        "{narrowing}"
    );

    let negative = eval("let value = -1i32; value as usize").unwrap_err();
    assert!(negative.to_string().contains("without losing information"));
    let negative = compile("let value = -1i32; value as usize")
        .unwrap()
        .execute()
        .unwrap_err();
    assert!(negative.to_string().contains("without losing information"));
}

#[test]
fn integer_intrinsics_cover_fallible_conversion_and_overflow_modes() {
    let source = r#"
        let narrowed = i16::try_from(123usize);
        assert!(is_ok(narrowed));
        assert!(is_err(i16::try_from(100000usize)));
        assert!(255u8.checked_add(1u8) == None);
        assert!(255u8.wrapping_add(1u8) == 0u8);
        assert!(255u8.saturating_add(1u8) == 255u8);
        let overflowed = 255u8.overflowing_add(1u8);
        assert!(overflowed.0 == 0u8);
        assert!(overflowed.1);
        assert!(type_of(42i32.to_f32()) == "f32");
        42i32.to_f64()
    "#;
    assert_eq!(eval(source).unwrap(), Value::F64(42.0));
    assert_eq!(
        compile(source).unwrap().execute().unwrap(),
        Value::F64(42.0)
    );
}

#[test]
fn integer_intrinsics_cover_bits_powers_euclidean_and_unary_overflow() {
    assert_eq!(
        integer(
            r#"
                let min = -127i8 - 1i8;
                assert!(min.checked_abs().is_none());
                assert!(min.saturating_abs() == 127i8);
                let absolute = min.overflowing_abs();
                assert!(absolute.0 == min && absolute.1);
                assert!(0u8.checked_neg().unwrap() == 0u8);
                assert!(1u8.checked_neg().is_none());

                assert!(15u8.count_ones() == 4u32);
                assert!(15u8.count_zeros() == 4u32);
                assert!(1u8.leading_zeros() == 7u32);
                assert!(8u8.trailing_zeros() == 3u32);
                assert!(129u8.rotate_left(1u32) == 3u8);
                assert!(3u8.rotate_right(1u32) == 129u8);
                assert!(1u8.checked_shl(7u32).unwrap() == 128u8);
                assert!(1u8.checked_shl(u8::BITS).is_none());
                assert!(128u8.checked_shr(7u32).unwrap() == 1u8);
                assert!(1u8.wrapping_shl(9u32) == 2u8);
                assert!(128u8.wrapping_shr(9u32) == 64u8);
                let shifted = 1u8.overflowing_shl(8u32);
                assert!(shifted.0 == 1u8 && shifted.1);
                assert!(1u16.swap_bytes() == 256u16);
                assert!(1u8.reverse_bits() == 128u8);

                assert!(3i32.pow(4u32) == 81);
                assert!(20i8.checked_pow(2u32).is_none());
                assert!(20i8.wrapping_pow(2u32) == -112i8);
                assert!(20i8.saturating_pow(2u32) == 127i8);
                let powered = 20i8.overflowing_pow(2u32);
                assert!(powered.0 == -112i8 && powered.1);

                assert!((-5i32).div_euclid(2) == -3);
                assert!((-5i32).rem_euclid(2) == 1);
                42
            "#,
        ),
        42
    );

    for source in ["20i8.pow(2u32)", "1i32.div_euclid(0)", "1i32.rem_euclid(0)"] {
        let error = eval(source).unwrap_err();
        assert!(
            error.to_string().contains("overflow")
                || error.to_string().contains("division by zero"),
            "{error}"
        );
    }
}

#[test]
fn integer_associated_constants_preserve_type_and_width() {
    assert_eq!(
        integer(
            r#"
                assert!(i8::MIN == -127i8 - 1i8);
                assert!(i8::MAX == 127i8);
                assert!(u8::MIN == 0u8);
                assert!(u8::MAX == 255u8);
                assert!(i128::BITS == 128u32);
                assert!(usize::BITS == isize::BITS);
                42
            "#,
        ),
        42
    );
}

#[test]
fn float_intrinsics_cover_classification_rounding_and_bounds() {
    let source = r#"
        let value = -3.5f64;
        assert!(value.abs() == 3.5f64);
        assert!(value.floor() == -4f64);
        assert!(value.ceil() == -3f64);
        assert!(value.round() == -4f64);
        assert!(value.trunc() == -3f64);
        assert!(value.fract() == -0.5f64);
        assert!(4f64.sqrt() == 2f64);
        assert!(4f64.recip() == 0.25f64);
        assert!(2f64.mul_add(3f64, 4f64) == 10f64);
        assert!(5f64.clamp(0f64, 4f64) == 4f64);
        assert!(5f64.min(2f64) == 2f64);
        assert!(5f64.max(8f64) == 8f64);
        assert!((-0f64).is_sign_negative());
        assert!(0f64.is_sign_positive());

        let nan = (-1f64).sqrt();
        assert!(nan.is_nan());
        assert!(!nan.is_finite());
        let infinity = 0f64.recip();
        assert!(infinity.is_infinite());
        assert!(!infinity.is_normal());
        assert!(type_of(2f32.sqrt()) == "f32");
        assert!(f32::NAN.is_nan());
        assert!(f64::INFINITY.is_infinite());
        assert!(f64::NEG_INFINITY.is_sign_negative());
        assert!(f32::MIN < 0f32);
        assert!(f32::MAX > 0f32);
        assert!(f32::EPSILON > 0f32);
        assert!(f32::MIN_POSITIVE.is_normal());
        42
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));

    let error = eval("1f64.clamp(2f64, 0f64)").unwrap_err();
    assert!(error.to_string().contains("min <= max"), "{error}");
}

#[test]
fn integer_ranges_preserve_their_concrete_type() {
    let source = r#"
        let mut total: u16 = 0u16;
        for value in 1u16..4u16 {
            total = total + value;
        }
        total
    "#;
    assert_eq!(eval(source).unwrap(), Value::U16(6));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::U16(6));
}

#[test]
fn builtin_result_constructs_matches_and_unwraps_values() {
    assert_eq!(
        integer(
            r#"
                fn answer(success: bool) -> Result<i32, string> {
                    if success { Ok(42) } else { Err("failed") }
                }

                let success = answer(true);
                assert!(is_ok(success));
                let failure = answer(false);
                assert!(is_err(failure));

                match answer(true) {
                    Ok(value) => value,
                    Err(_) => 0,
                }
            "#,
        ),
        42
    );

    assert_eq!(integer("unwrap(Ok(42))"), 42);
    assert_eq!(integer("unwrap_or(Err(\"failed\"), 42)"), 42);
    assert_eq!(
        integer(
            r#"
                let value: Result<i32, string> = Ok(42);
                assert!(value.is_ok());
                value.unwrap()
            "#,
        ),
        42
    );
    assert_eq!(
        integer("let value: Result<i32, string> = Err(\"failed\"); value.unwrap_or(42)"),
        42
    );
    assert_eq!(integer("core::result::unwrap(core::result::Ok(42))"), 42);
}

#[test]
fn result_supports_error_side_extraction() {
    assert_eq!(
        eval(
            r#"
                let first: Result<i32, string> = Err("missing");
                assert!(first.unwrap_err() == "missing");
                let second: Result<i32, string> = Err("invalid");
                second.expect_err("expected failure")
            "#
        )
        .unwrap(),
        Value::String("invalid".into())
    );
    let error = eval("let value: Result<i32, string> = Ok(42); value.unwrap_err();")
        .expect_err("unwrap_err on Ok must fail");
    assert!(error.to_string().contains("Ok(42)"));
}

#[test]
fn standard_fs_uses_result_and_structured_io_errors() {
    let unique = format!(
        "rils-std-fs-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let directory = std::env::temp_dir().join(unique);
    let file = directory.join("message.txt");
    let missing = directory.join("missing.txt");
    let script_path = |path: &std::path::Path| path.to_string_lossy().replace('\\', "/");
    let directory_text = script_path(&directory);
    let file_text = script_path(&file);
    let missing_text = script_path(&missing);

    let source = format!(
        r#"
            use std::io::ErrorKind;

            fn roundtrip() -> Result<i32, std::io::Error> {{
                std::fs::create_dir_all("{directory_text}")?;
                std::fs::write("{file_text}", "hello")?;
                std::fs::append("{file_text}", " world")?;
                let text = std::fs::read_to_string("{file_text}")?;
                let exists = std::fs::try_exists("{file_text}")?;
                let entries = std::fs::read_dir("{directory_text}")?;
                std::fs::remove_file("{file_text}")?;
                Ok(if text == "hello world" && exists && entries.len() == 1 {{ 42 }} else {{ 0 }})
            }}

            fn missing_file() -> Result<(), std::io::Error> {{
                std::fs::read_to_string("{missing_text}")?;
                Ok(())
            }}

            let value = unwrap(roundtrip());
            match missing_file() {{
                Ok(_) => 0,
                Err(error) => match error.kind {{
                    ErrorKind::NotFound => value,
                    _ => 1,
                }},
            }}
        "#
    );
    let result = eval(&source);
    if directory.exists() {
        std::fs::remove_dir_all(&directory).unwrap();
    }
    assert_eq!(result.unwrap(), Value::I32(42));
}

#[test]
fn question_mark_unwraps_ok_and_propagates_err() {
    assert_eq!(
        integer(
            r#"
                fn source(success: bool) -> Result<i32, string> {
                    if success { Ok(40) } else { Err("failed") }
                }

                fn add_two(success: bool) -> Result<i32, string> {
                    let value = source(success)?;
                    Ok(value + 2)
                }

                unwrap(add_two(true))
            "#,
        ),
        42
    );

    assert_eq!(
        integer(
            r#"
                fn fail() -> Result<i32, string> { Err("failed") }
                fn propagate() -> Result<i32, string> {
                    let value = fail()?;
                    assert!(false);
                    Ok(value)
                }
                match propagate() {
                    Ok(_) => 0,
                    Err(message) => if message == "failed" { 42 } else { 1 },
                }
            "#,
        ),
        42
    );
}

#[test]
fn question_mark_reports_invalid_context_and_return_type() {
    let top_level = eval("Ok(1)?").unwrap_err();
    assert!(
        top_level
            .to_string()
            .contains("can only be used inside a function")
    );

    let non_result = eval("fn bad() -> i32 { 1? } bad()").unwrap_err();
    assert!(non_result.to_string().contains("requires Result"));

    let incompatible_error = eval(
        r#"
            fn source() -> Result<i32, string> { Err("failed") }
            fn bad() -> Result<i32, i32> {
                let value = source()?;
                Ok(value)
            }
            bad()
        "#,
    )
    .unwrap_err();
    assert!(
        incompatible_error
            .to_string()
            .contains("type mismatch: expected i32, found string"),
        "{incompatible_error}"
    );
}

#[test]
fn evaluates_arithmetic_with_precedence() {
    assert_eq!(integer("1 + 2 * 3"), 7);
}

#[test]
fn supports_mutable_bindings_and_loops() {
    assert_eq!(
        integer(
            r#"
                let mut total = 0;
                let mut n = 1;
                while n <= 5 {
                    total = total + n;
                    n = n + 1;
                }
                total
                "#
        ),
        15
    );
}

#[test]
fn loops_support_break_values_and_continue() {
    assert_eq!(
        integer(
            r#"
                let answer = {
                    loop {
                        break 42;
                    }
                };
                answer
            "#,
        ),
        42
    );

    assert_eq!(
        integer(
            r#"
                let mut current = 0;
                let mut total = 0;
                while current < 6 {
                    current = current + 1;
                    if current % 2 == 0 {
                        continue;
                    }
                    total = total + current;
                }
                total
            "#,
        ),
        9
    );

    assert_eq!(
        integer(
            r#"
                let found = {
                    for value in 0..10 {
                        if value == 4 {
                            break value;
                        }
                    }
                };
                found
            "#,
        ),
        4
    );
}

#[test]
fn loop_control_is_lexically_scoped() {
    for source in ["break;", "continue;"] {
        let error = eval(source).unwrap_err();
        assert!(error.to_string().contains("inside a loop"), "{error}");
    }

    let nested_function = eval(
        r#"
            loop {
                fn invalid() { break; }
                break;
            }
        "#,
    )
    .unwrap_err();
    assert!(nested_function.to_string().contains("inside a loop"));
}

#[test]
fn struct_fields_are_assignable_places() {
    assert_eq!(
        integer(
            r#"
                struct Inner { value: i32 }
                struct Outer { inner: Inner }

                let mut outer = Outer { inner: Inner { value: 1 } };
                outer.inner.value = 20;

                {
                    let field = &mut outer.inner.value;
                    *field = *field + 22;
                }
                outer.inner.value
            "#
        ),
        42
    );

    assert_eq!(
        integer(
            r#"
                struct Point { x: i32 }
                let mut point = Point { x: 1 };
                {
                    let point_ref = &mut point;
                    (*point_ref).x = 42;
                }
                point.x
            "#
        ),
        42
    );
}

#[test]
fn field_places_enforce_mutability_types_and_active_references() {
    let immutable = eval(
        r#"
            struct Point { x: i32 }
            let point = Point { x: 1 };
            point.x = 2;
        "#,
    )
    .unwrap_err();
    assert!(immutable.to_string().contains("immutable place `point`"));

    let mismatch = eval(
        r#"
            struct Point { x: i32 }
            let mut point = Point { x: 1 };
            point.x = "wrong";
        "#,
    )
    .unwrap_err();
    assert!(mismatch.to_string().contains("field `x` of type i32"));

    let borrowed = eval(
        r#"
            struct Inner { value: i32 }
            struct Outer { inner: Inner }
            let mut outer = Outer { inner: Inner { value: 1 } };
            {
                let field = &mut outer.inner.value;
                outer.inner = Inner { value: 2 };
            }
        "#,
    )
    .unwrap_err();
    assert!(
        borrowed.to_string().contains("while it is referenced"),
        "{borrowed}"
    );
}

#[test]
fn indexing_rejects_non_collection_values() {
    let error = eval(
        r#"
            let mut value = 42;
            value[0] = 1;
        "#,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("type `i32` does not support indexing")
    );
}

#[test]
fn for_loops_consume_custom_iterators() {
    assert_eq!(
        integer(
            r#"
                struct CounterRange {
                    current: i32,
                    end: i32,
                }

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
            "#
        ),
        21
    );
}

#[test]
fn for_loops_use_into_iterator_when_available() {
    assert_eq!(
        integer(
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
            "#
        ),
        10
    );
}

#[test]
fn for_loops_reject_values_without_iterator_traits() {
    let error = eval("for value in 42 {}").unwrap_err();
    assert!(error.to_string().contains("does not implement Iterator"));
}

#[test]
fn integer_ranges_work_with_for_loops() {
    assert_eq!(
        integer(
            r#"
                let mut total = 0;
                for value in 0..5 {
                    total = total + value;
                }
                assert!(type_of(2..4) == "Range");
                let mut range = 2..4;
                assert!(range.next() == Some(2));
                assert!(range.next() == Some(3));
                assert!(range.next() == None);
                let iterator = (0..1).into_iter();
                assert!(type_of(iterator) == "Range");
                total
            "#
        ),
        10
    );

    let error = eval("for value in 0..2.5 {}").unwrap_err();
    assert!(error.to_string().contains("range bounds"));
}

#[test]
fn generic_type_aliases_expand_in_annotations() {
    assert_eq!(
        integer(
            r#"
                struct Box<T> { value: T }
                type ValueBox<T> = Box<T>;
                type IntBox = ValueBox<i32>;

                fn unbox(value: IntBox) -> i32 { value.value }

                let boxed: IntBox = Box { value: 42 };
                unbox(boxed)
            "#
        ),
        42
    );

    let error = eval(
        r#"
            struct Box<T> { value: T }
            type ValueBox<T> = Box<T>;
            let boxed: ValueBox = Box { value: 1 };
        "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("expects 1 type argument"));
}

#[test]
fn associated_types_participate_in_trait_signatures() {
    assert_eq!(
        integer(
            r#"
                trait Source {
                    type Item;
                    fn get(&self) -> Self::Item;
                }

                struct Number { value: i32 }

                impl Source for Number {
                    type Item = i32;
                    fn get(&self) -> i32 { self.value }
                }

                fn read<T: Source>(value: &T) -> T::Item {
                    value.get()
                }

                let number = Number { value: 42 };
                read(&number)
            "#
        ),
        42
    );

    let missing = eval(
        r#"
            trait Source { type Item; }
            struct Number { value: i32 }
            impl Source for Number {}
        "#,
    )
    .unwrap_err();
    assert!(
        missing
            .to_string()
            .contains("missing associated type `Item`")
    );

    let mismatch = eval(
        r#"
            trait Source {
                type Item;
                fn get(&self) -> Self::Item;
            }
            struct Number { value: i32 }
            impl Source for Number {
                type Item = i32;
                fn get(&self) -> string { "wrong" }
            }
        "#,
    )
    .unwrap_err();
    assert!(mismatch.to_string().contains("return type of method `get`"));
}

#[test]
fn trait_associated_types_support_defaults_and_generics() {
    assert_eq!(
        integer(
            r#"
                struct Box<T> { value: T }

                trait Factory {
                    type Item<T> = Box<T>;
                    fn make(self) -> Self::Item<i32>;
                }

                struct IntFactory { value: i32 }

                impl Factory for IntFactory {
                    fn make(self) -> Box<i32> { Box { value: self.value } }
                }

                IntFactory { value: 42 }.make().value
            "#
        ),
        42
    );
}

#[test]
fn trait_methods_keep_their_trait_identity_and_support_ufcs() {
    assert_eq!(
        integer(
            r#"
                trait Left {
                    type Item;
                    fn value(&self) -> i32;
                }

                trait Right {
                    type Item;
                    fn value(&self) -> i32;
                }

                struct Both { inner: i32 }

                impl Left for Both {
                    type Item = i32;
                    fn value(&self) -> i32 { self.inner }
                }

                impl Right for Both {
                    type Item = string;
                    fn value(&self) -> i32 { self.inner + 1 }
                }

                fn read_left<T: Left>(value: &T) -> i32 {
                    <T as Left>::value(value)
                }

                let both = Both { inner: 20 };
                let left_item: <Both as Left>::Item = 1;
                let right_item: <Both as Right>::Item = "ok";
                assert!(left_item == 1);
                assert!(right_item == "ok");
                assert!(Left::value(&both) == 20);
                assert!(<Both as Right>::value(&both) == 21);
                read_left(&both) + 22
            "#
        ),
        42
    );

    let ambiguous = eval(
        r#"
            trait Left { fn value(&self) -> i32; }
            trait Right { fn value(&self) -> i32; }
            struct Both { inner: i32 }
            impl Left for Both { fn value(&self) -> i32 { self.inner } }
            impl Right for Both { fn value(&self) -> i32 { self.inner + 1 } }
            let both = Both { inner: 20 };
            both.value()
        "#,
    )
    .unwrap_err();
    assert!(
        ambiguous
            .to_string()
            .contains("method `value` is ambiguous"),
        "{ambiguous}"
    );
}

#[test]
fn inherent_methods_take_priority_over_trait_methods() {
    assert_eq!(
        integer(
            r#"
                trait Value { fn value(&self) -> i32; }
                struct Number { inner: i32 }

                impl Value for Number {
                    fn value(&self) -> i32 { self.inner }
                }

                impl Number {
                    fn value(&self) -> i32 { self.inner * 2 }
                }

                let number = Number { inner: 21 };
                assert!(Value::value(&number) == 21);
                number.value()
            "#
        ),
        42
    );
}

#[test]
fn builtin_clone_trait_provides_clone_method_for_owned_values() {
    let value = eval(
        r#"
            struct Label { text: string }

            let text = "Rils";
            let text_copy = text.clone();
            let text_ufcs = Clone::clone(&text);

            let label = Label { text: "Rils" };
            let label_copy = label.clone();
            text + text_copy + text_ufcs + label.text + label_copy.text
        "#,
    )
    .unwrap();
    assert_eq!(value, Value::String("RilsRilsRilsRilsRils".into()));

    assert_eq!(
        integer(
            r#"
                struct Number { value: i32 }

                impl Clone for Number {
                    fn clone(&self) -> Self {
                        Number { value: self.value + 1 }
                    }
                }

                let number = Number { value: 20 };
                let cloned = Clone::clone(&number);
                cloned.value
            "#
        ),
        21
    );
}

#[test]
fn functions_are_recursive_and_blocks_return_values() {
    assert_eq!(
        integer(
            r#"
                fn factorial(n) {
                    if n <= 1 {
                        1
                    } else {
                        n * factorial(n - 1)
                    }
                }
                factorial(6)
                "#
        ),
        720
    );
}

#[test]
fn interpreter_recursion_uses_growable_stack_segments() {
    assert_eq!(
        integer(
            r#"
                fn countdown(n: i32) -> i32 {
                    if n == 0 {
                        0
                    } else {
                        countdown(n - 1)
                    }
                }
                countdown(1000)
                "#
        ),
        0
    );
}

#[test]
fn interpreter_reports_the_configured_call_depth_limit() {
    let mut engine = Engine::new();
    engine.set_max_call_depth(8);
    let error = engine
        .eval(
            r#"
                fn countdown(n: i32) -> i32 {
                    if n == 0 { 0 } else { countdown(n - 1) }
                }
                countdown(8)
            "#,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("call stack exceeded the 8 frame limit")
    );
}

#[test]
fn function_types_preserve_higher_order_signatures() {
    let source = r#"
            fn make_value() -> fn() -> i32 {
                fn value() -> i32 {
                    42
                }
                value
            }

            fn apply<T, U>(transform: fn(T) -> U, value: T) -> U {
                transform(value)
            }

            fn double(value: i32) -> i32 {
                value * 2
            }

            let getter: fn() -> i32 = make_value();
            assert!(type_of(getter) == "fn() -> i32");
            assert!(getter() == 42);
            apply(double, 21)
        "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));

    let mismatch = eval(
        r#"
                fn text(value: string) -> string { value }
                let invalid: fn(i32) -> i32 = text;
            "#,
    )
    .unwrap_err();
    assert!(mismatch.to_string().contains("type mismatch"));
}

#[test]
fn nested_functions_capture_mutable_bindings() {
    assert_eq!(
        integer(
            r#"
                fn make_counter() {
                    let mut count = 0;
                    fn next() {
                        count = count + 1;
                        count
                    }
                    next
                }
                let counter = make_counter();
                counter();
                counter()
                "#
        ),
        2
    );
}

#[test]
fn return_works_inside_nested_blocks() {
    assert_eq!(
        integer(
            r#"
                fn first_positive(n) {
                    while n > -5 {
                        if n > 0 {
                            return n;
                        }
                        n
                    }
                    0
                }
                first_positive(3)
                "#
        ),
        3
    );
}

#[test]
fn immutable_bindings_reject_assignment() {
    let error = eval("let answer = 42; answer = 0;").unwrap_err();
    assert!(error.to_string().contains("immutable variable"));
}

#[test]
fn owned_values_move_while_copy_values_remain_available() {
    let moved = eval(r#"let text = "hello"; let owned = text; text"#).unwrap_err();
    assert!(moved.to_string().contains("moved value `text`"));

    assert_eq!(
        integer("let value = 21; let copied = value; value + copied"),
        42
    );
}

#[test]
fn clone_explicitly_duplicates_owned_values() {
    let value = eval(
        r#"
            struct Message { text: string }
            let original = Message { text: "hello" };
            let copied = clone(&original);
            original.text + copied.text
        "#,
    )
    .unwrap();
    assert_eq!(value, Value::String("hellohello".into()));
}

#[test]
fn copy_structs_duplicate_their_storage() {
    assert_eq!(
        integer(
            r#"
                struct Counter { value: i32 }
                let mut first = Counter { value: 1 };
                let second = first;
                {
                    let value = &mut first.value;
                    *value = 41;
                }
                first.value + second.value
            "#
        ),
        42
    );
}

#[test]
fn multiple_mutable_references_share_a_local_storage_slot() {
    assert_eq!(
        integer(
            r#"
                let mut value = 1;
                {
                    let first: &mut i32 = &mut value;
                    let second: &mut i32 = &mut value;
                    *first = 20;
                    *second = *second + 22;
                }
                value
            "#
        ),
        42
    );
}

#[test]
fn multiple_mutable_references_can_target_a_struct_field() {
    assert_eq!(
        integer(
            r#"
                struct Counter { value: i32 }
                let mut counter = Counter { value: 0 };
                {
                    let first = &mut counter.value;
                    let second: &mut i32 = &mut counter.value;
                    *first = 20;
                    *second = *second + 22;
                }
                counter.value
            "#
        ),
        42
    );
}

#[test]
fn field_references_keep_the_owner_storage_stable() {
    let moved = eval(
        r#"
            struct Message { text: string }
            let message = Message { text: "hello" };
            {
                let text = &message.text;
                let moved = message;
            }
        "#,
    )
    .unwrap_err();
    assert!(moved.to_string().contains("while it is referenced"));

    let replaced = eval(
        r#"
            struct Counter { value: i32 }
            let mut counter = Counter { value: 1 };
            {
                let value = &counter.value;
                counter = Counter { value: 2 };
            }
        "#,
    )
    .unwrap_err();
    assert!(replaced.to_string().contains("field") && replaced.to_string().contains("referenced"));
}

#[test]
fn methods_can_receive_local_mutable_references() {
    assert_eq!(
        integer(
            r#"
                struct Counter { value: i32 }
                impl Counter {
                    fn set_answer(&mut self) {
                        *self = Counter { value: 42 };
                    }
                }
                let mut counter = Counter { value: 0 };
                counter.set_answer();
                counter.value
            "#
        ),
        42
    );
}

#[test]
fn methods_support_all_rust_style_self_receivers() {
    assert_eq!(
        integer(
            r#"
                struct Counter { value: i32 }
                impl Counter {
                    fn read(&self) -> i32 {
                        self.value
                    }

                    fn increment(&mut self) {
                        let next = self.value + 1;
                        *self = Counter { value: next };
                    }

                    fn into_answer(mut self) -> Counter {
                        self = Counter { value: 42 };
                        self
                    }

                    fn consume(self) -> i32 {
                        self.value
                    }
                }

                let mut counter = Counter { value: 0 };
                counter.increment();
                assert!(counter.read() == 1);
                counter.into_answer().consume()
            "#
        ),
        42
    );
}

#[test]
fn traits_accept_rust_style_reference_receivers() {
    assert_eq!(
        integer(
            r#"
                trait Read {
                    fn read(&self) -> i32;
                }

                struct Number { value: i32 }

                impl Read for Number {
                    fn read(&self) -> i32 {
                        self.value
                    }
                }

                let number = Number { value: 42 };
                number.read()
            "#
        ),
        42
    );
}

#[test]
fn self_receivers_are_restricted_to_the_first_method_parameter() {
    for source in [
        "fn invalid(&self) {}",
        "struct Value { inner: i32 } impl Value { fn invalid(value: i32, &self) {} }",
    ] {
        let error = eval(source).unwrap_err();
        assert!(error.to_string().contains("self"));
    }
}

#[test]
fn reference_parameters_can_mutate_their_owner() {
    assert_eq!(
        integer(
            r#"
                fn set_answer(target: &mut i32) {
                    *target = 42;
                }
                let mut value = 0;
                set_answer(&mut value);
                value
            "#
        ),
        42
    );
}

#[test]
fn references_prevent_moves_but_not_in_place_assignment() {
    let borrowed = eval(r#"let text = "hello"; { let reference = &text; text; }"#).unwrap_err();
    assert!(borrowed.to_string().contains("while it is referenced"));

    assert_eq!(
        integer("let mut value = 1; { let reference = &mut value; value = 42; *reference }"),
        42
    );
}

#[test]
fn immutable_references_reject_writes() {
    let error = eval("let mut value = 1; { let reference = &value; *reference = 2; }").unwrap_err();
    assert!(error.to_string().contains("immutable reference"));
}

#[test]
fn references_cannot_escape_or_enter_owned_types() {
    for source in [
        "let value = 1; let global = &value;",
        "fn invalid(value: &i32) -> &i32 { value }",
        "struct Invalid { value: &i32 }",
        "let value = 1; let invalid: Option<&i32> = None;",
        "let value = 1; let invalid = Some(&value);",
        "let escaped = { let value = 1; &value };",
        "fn outer() { let value = 1; let reference = &value; fn nested() {} } outer()",
    ] {
        let error = eval(source).unwrap_err();
        assert!(
            error.to_string().contains("reference") || error.to_string().contains("references"),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn engine_keeps_globals_between_evaluations() {
    let mut engine = Engine::new();
    engine.eval("let answer = 42;").unwrap();
    assert_eq!(engine.eval("answer").unwrap(), Value::I32(42));
}

#[test]
fn unit_is_distinct_and_is_the_default_function_result() {
    assert_eq!(eval("()").unwrap(), Value::Unit);
    assert_eq!(
        eval(
            r#"
                fn do_nothing() -> () {}
                do_nothing()
                "#
        )
        .unwrap(),
        Value::Unit
    );
}

#[test]
fn option_represents_present_and_absent_values() {
    assert_eq!(
        integer(
            r#"
                let missing: Option<i32> = None;
                let present: Option<i32> = Some(40);
                fn maybe(value: i32) -> Option<i32> {
                    if value > 0 { Some(value) } else { None }
                }
                unwrap_or(missing, 2) + unwrap(present) + unwrap_or(maybe(0), 0)
                "#
        ),
        42
    );
}

#[test]
fn option_methods_follow_shared_builtin_declarations() {
    assert_eq!(
        integer("let value = Some(42); if value.is_some() { value.unwrap() } else { 0 }"),
        42
    );
    assert_eq!(
        integer(
            "let value: Option<i32> = None; if value.is_none() { value.unwrap_or(7) } else { 0 }"
        ),
        7
    );
    let error = eval("let value: Option<i32> = None; value.unwrap()").unwrap_err();
    assert!(error.to_string().contains("called `unwrap` on `None`"));
}

#[test]
fn option_supports_or_xor_and_replace() {
    assert_eq!(
        integer(
            r#"
                let left = Some(2);
                let missing: Option<i32> = None;
                assert!(left.or(Some(7)).unwrap() == 2);
                assert!(missing.or(Some(7)).unwrap() == 7);
                assert!(Some(3).xor(None).unwrap() == 3);
                assert!(Some(3).xor(Some(4)).is_none());
                let mut value = Some(10);
                assert!(value.replace(20).unwrap() == 10);
                value.unwrap()
            "#
        ),
        20
    );
}

#[test]
fn option_and_result_support_lazy_combinators() {
    assert_eq!(
        integer(
            r#"
                fn double(value: i32) -> i32 { value * 2 }
                fn maybe(value: i32) -> Option<i32> {
                    if value > 0 { Some(value + 1) } else { None }
                }
                fn fallback() -> Option<i32> { Some(9) }
                assert!(Some(20).map(double).unwrap() == 40);
                assert!(Some(4).and_then(maybe).unwrap() == 5);
                let missing: Option<i32> = None;
                assert!(missing.or_else(fallback).unwrap() == 9);

                fn ok_double(value: i32) -> Result<i32, string> { Ok(value * 2) }
                fn error_len(value: string) -> usize { value.len() }
                fn recover(value: string) -> Result<i32, usize> { Err(value.len()) }
                let ok: Result<i32, string> = Ok(10);
                assert!(ok.map(double).unwrap() == 20);
                let failed: Result<i32, string> = Err("bad");
                assert!(failed.map_err(error_len).unwrap_err() == 3usize);
                let chained: Result<i32, string> = Ok(11);
                assert!(chained.and_then(ok_double).unwrap() == 22);
                let recovered: Result<i32, string> = Err("oops");
                assert!(recovered.or_else(recover).unwrap_err() == 4usize);
                4
            "#,
        ),
        4
    );
}

#[test]
fn option_result_combinators_are_lazy_and_type_checked() {
    assert_eq!(
        integer(
            r#"
                fn fail_value(value: i32) -> i32 {
                    let missing: Option<i32> = None;
                    missing.unwrap()
                }
                fn fail_option() -> Option<i32> {
                    let missing: Option<i32> = None;
                    missing.unwrap();
                    None
                }
                fn fail_error(value: string) -> usize {
                    let missing: Option<usize> = None;
                    missing.unwrap()
                }
                let none: Option<i32> = None;
                assert!(none.map(fail_value).is_none());
                assert!(Some(7).or_else(fail_option).unwrap() == 7);
                let ok: Result<i32, string> = Ok(21);
                ok.map_err(fail_error).unwrap() * 2
            "#,
        ),
        42
    );

    for source in [
        "fn wrong(value: i32) -> i32 { value } let value = Some(1); value.and_then(wrong)",
        "fn wrong() -> i32 { 1 } let value: Option<i32> = None; value.or_else(wrong)",
        "fn wrong(value: i32) -> Option<i32> { Some(value) } let value: Result<i32, string> = Ok(1); value.and_then(wrong)",
    ] {
        let error = eval(source).unwrap_err();
        assert!(
            error.to_string().contains("type mismatch")
                || error.to_string().contains("callback must return"),
            "{error}"
        );
    }
}

#[test]
fn annotations_check_initializers_assignments_parameters_and_returns() {
    for source in [
        "let value: i32 = None;",
        "let missing = None;",
        "let mut value = 1; value = None;",
        "let mut value: i32 = 1; value = Some(1);",
        "fn identity(value: i32) -> i32 { value } identity(None)",
        "fn wrong() -> Option<i32> { 42 } wrong()",
        "let missing: Option<i32> = None; unwrap_or(missing, \"wrong\")",
    ] {
        let error = eval(source).unwrap_err();
        assert!(
            error.to_string().contains("type mismatch")
                || error.to_string().contains("cannot assign")
                || error.to_string().contains("cannot infer")
                || error.to_string().contains("default must be"),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn option_cannot_be_used_as_an_implicit_nullable_condition() {
    let error = eval("if None { 1 } else { 2 }").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Option cannot be used as a condition")
    );
}

#[test]
fn nil_reports_an_option_migration_error() {
    let error = eval("nil").unwrap_err();
    assert!(error.to_string().contains("use `None`"));
}

#[test]
fn match_destructures_option_values() {
    assert_eq!(
        integer(
            r#"
                fn flatten(value: Option<Option<i32>>) -> i32 {
                    match value {
                        Some(Some(number)) => number,
                        Some(None) => -1,
                        None => 0,
                    }
                }
                flatten(Some(Some(42)))
                "#
        ),
        42
    );
}

#[test]
fn match_supports_literals_and_wildcards() {
    assert_eq!(
        integer(
            r#"
                match "Rils" {
                    "Rust" => 1,
                    "Rils" => 2,
                    _ => 0,
                }
                "#
        ),
        2
    );
}

#[test]
fn match_bindings_are_scoped_to_the_selected_arm() {
    let error = eval(
        r#"
            match Some(1) {
                Some(value) => value,
                None => 0,
            };
            value
            "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("undefined variable `value`"));
}

#[test]
fn match_reports_non_exhaustive_values() {
    let error = eval("match None { Some(value) => value }").unwrap_err();
    assert!(error.to_string().contains("non-exhaustive match"));
}

#[test]
fn return_propagates_from_match_arm_blocks() {
    assert_eq!(
        integer(
            r#"
                fn read(value: Option<i32>) -> i32 {
                    match value {
                        Some(number) => {
                            return number;
                        }
                        None => 0,
                    }
                }
                read(Some(42))
                "#
        ),
        42
    );
}

#[test]
fn structs_support_construction_fields_and_impl_methods() {
    assert_eq!(
        integer(
            r#"
                struct Point {
                    x: i32,
                    y: i32,
                }

                impl Point {
                    fn new(x: i32, y: i32) -> Point {
                        Point { x: x, y: y }
                    }

                    fn sum(self) -> i32 {
                        self.x + self.y
                    }

                    fn origin() -> Point {
                        Point { x: 0, y: 0 }
                    }
                }

                let point: Point = Point::new(20, 22);
                point.sum() + Point::origin().sum()
                "#
        ),
        42
    );
}

#[test]
fn empty_struct_declarations_work_in_interpreter_and_bytecode() {
    let source = r#"
        struct Unit;
        struct Empty {}

        impl Unit {
            fn answer() -> i32 { 40 }
        }

        impl Empty {
            fn answer() -> i32 { 2 }
        }

        Unit::answer() + Empty::answer()
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn struct_patterns_destructure_fields() {
    assert_eq!(
        integer(
            r#"
                struct Point { x: i32, y: i32 }
                let point = Point { x: 40, y: 2 };
                match point {
                    Point { x, y } => x + y,
                }
                "#
        ),
        42
    );
}

#[test]
fn enums_support_unit_tuple_and_record_variants() {
    assert_eq!(
        integer(
            r#"
                enum Message {
                    Quit,
                    Move(i32, i32),
                    Write { text: string },
                }

                fn score(message: Message) -> i32 {
                    match message {
                        Message::Quit => 0,
                        Message::Move(x, y) => x + y,
                        Message::Write { text } => if text == "Rils" { 42 } else { 1 },
                    }
                }

                score(Message::Quit)
                    + score(Message::Move(20, 22))
                    + score(Message::Write { text: "other" })
                "#
        ),
        43
    );
}

#[test]
fn enums_can_have_impl_methods() {
    assert_eq!(
        integer(
            r#"
                enum Number {
                    Exact(i32),
                    Missing,
                }

                impl Number {
                    fn value_or(self, fallback: i32) -> i32 {
                        match self {
                            Number::Exact(value) => value,
                            Number::Missing => fallback,
                        }
                    }
                }

                Number::Exact(40).value_or(2)
                    + Number::Missing.value_or(0)
                "#
        ),
        40
    );
}

#[test]
fn record_construction_checks_missing_unknown_and_invalid_fields() {
    for source in [
        "struct Point { x: i32, y: i32 } Point { x: 1 };",
        "struct Point { x: i32, y: i32 } Point { x: 1, y: 2, z: 3 };",
        "struct Point { x: i32, y: i32 } Point { x: \"wrong\", y: 2 };",
    ] {
        let error = eval(source).unwrap_err();
        assert!(
            error.to_string().contains("field") || error.to_string().contains("type mismatch"),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn generic_functions_infer_and_reuse_type_parameters() {
    assert_eq!(
        integer(
            r#"
                fn identity<T>(value: T) -> T {
                    value
                }

                fn choose<T>(left: T, right: T) -> T {
                    if true { left } else { right }
                }

                identity(choose(40, 2))
                "#
        ),
        40
    );

    let error = eval(
        r#"
            fn choose<T>(left: T, right: T) -> T { left }
            choose(1, "wrong")
            "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("inferred as both"));
}

#[test]
fn generic_structs_and_impl_methods_preserve_arguments() {
    assert_eq!(
        integer(
            r#"
                struct Pair<T, U> {
                    first: T,
                    second: U,
                }

                impl<T, U> Pair<T, U> {
                    fn swap(self) -> Pair<U, T> {
                        Pair {
                            first: self.second,
                            second: self.first,
                        }
                    }
                }

                let pair: Pair<i32, string> = Pair {
                    first: 42,
                    second: "Rils",
                };
                pair.swap().second
                "#
        ),
        42
    );
}

#[test]
fn methods_can_declare_additional_generic_parameters() {
    assert_eq!(
        integer(
            r#"
                struct Box<T> {
                    value: T,
                }

                impl<T> Box<T> {
                    fn replace<U>(self, value: U) -> Box<U> {
                        Box { value: value }
                    }
                }

                let boxed = Box { value: "old" };
                boxed.replace(42).value
                "#
        ),
        42
    );
}

#[test]
fn generic_enums_support_partial_inference_and_annotations() {
    assert_eq!(
        integer(
            r#"
                enum Outcome<T, E> {
                    Ok(T),
                    Err(E),
                }

                fn value_or<T, E>(result: Outcome<T, E>, fallback: T) -> T {
                    match result {
                        Outcome::Ok(value) => value,
                        Outcome::Err(_) => fallback,
                    }
                }

                let failure: Outcome<i32, string> = Outcome::Err("failed");
                value_or(failure, 42)
                "#
        ),
        42
    );
}

#[test]
fn generic_record_fields_must_infer_consistently() {
    let error = eval(
        r#"
            struct Same<T> {
                left: T,
                right: T,
            }
            Same { left: 1, right: "wrong" }
            "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("inferred as both"));
}

#[test]
fn outer_annotations_fill_unresolved_generic_arguments() {
    assert_eq!(
        integer(
            r#"
                struct Holder<T> {
                    value: Option<T>,
                }
                let holder: Holder<i32> = Holder { value: None };
                unwrap_or(holder.value, 42)
                "#
        ),
        42
    );

    let error = eval(
        r#"
            struct Holder<T> {
                value: Option<T>,
            }
            let holder: Holder<i32> = Holder { value: None };
            unwrap_or(holder.value, "wrong")
            "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("default must be i32"));
}

#[test]
fn traits_define_and_dispatch_required_methods() {
    let value = eval(
        r#"
            trait Describe {
                fn describe(self) -> string;
            }

            struct Point {
                x: i32,
                y: i32,
            }

            impl Describe for Point {
                fn describe(self) -> string {
                    "point"
                }
            }

            Point { x: 1, y: 2 }.describe()
            "#,
    )
    .unwrap();
    assert_eq!(value, Value::String("point".into()));
}

#[test]
fn generic_trait_bounds_are_enforced() {
    let source = r#"
            trait Describe {
                fn describe(self) -> string;
            }

            struct Point { value: i32 }
            struct Hidden { value: i32 }

            impl Describe for Point {
                fn describe(self) -> string { "point" }
            }

            fn describe<T: Describe>(value: T) -> string {
                value.describe()
            }
        "#;

    let mut engine = Engine::new();
    engine.eval(source).unwrap();
    assert_eq!(
        engine.eval("describe(Point { value: 1 })").unwrap(),
        Value::String("point".into())
    );
    let error = engine.eval("describe(Hidden { value: 1 })").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not implement required trait")
    );
}

#[test]
fn trait_self_types_are_checked() {
    assert_eq!(
        integer(
            r#"
                trait Duplicate {
                    fn duplicate(self) -> Self;
                }

                struct Number { value: i32 }

                impl Duplicate for Number {
                    fn duplicate(self) -> Number {
                        Number { value: self.value }
                    }
                }

                Number { value: 42 }.duplicate().value
                "#
        ),
        42
    );
}

#[test]
fn generic_types_can_implement_traits_for_all_arguments() {
    let value = eval(
        r#"
            trait Describe {
                fn describe(self) -> string;
            }

            struct Wrapper<T> {
                value: T,
            }

            impl<T> Describe for Wrapper<T> {
                fn describe(self) -> string {
                    "wrapper"
                }
            }

            fn describe<T: Describe>(value: T) -> string {
                value.describe()
            }

            describe(Wrapper { value: 42 })
            "#,
    )
    .unwrap();
    assert_eq!(value, Value::String("wrapper".into()));
}

#[test]
fn trait_impl_rejects_missing_and_wrong_methods() {
    for source in [
        r#"
            trait Describe { fn describe(self) -> string; }
            struct Point { value: i32 }
            impl Describe for Point {}
            "#,
        r#"
            trait Describe { fn describe(self) -> string; }
            struct Point { value: i32 }
            impl Describe for Point {
                fn describe(self) -> i32 { 1 }
            }
            "#,
        r#"
            trait Describe { fn describe(self) -> string; }
            struct Point { value: i32 }
            impl Describe for Point {
                fn describe(self) -> string { "point" }
                fn extra(self) -> string { "extra" }
            }
            "#,
    ] {
        let error = eval(source).unwrap_err();
        assert!(
            error.to_string().contains("missing method")
                || error.to_string().contains("does not match")
                || error.to_string().contains("not a member"),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn duplicate_trait_impls_are_rejected() {
    let error = eval(
        r#"
            trait Describe { fn describe(self) -> string; }
            struct Point { value: i32 }
            impl Describe for Point {
                fn describe(self) -> string { "first" }
            }
            impl Describe for Point {
                fn describe(self) -> string { "second" }
            }
            "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("already implemented"));
}

#[test]
fn generic_parameters_support_multiple_trait_bounds() {
    assert_eq!(
        integer(
            r#"
                trait Left {
                    fn left(self) -> i32;
                }
                trait Right {
                    fn right(self) -> i32;
                }
                struct Both { value: i32 }
                impl Left for Both {
                    fn left(self) -> i32 { self.value }
                }
                impl Right for Both {
                    fn right(self) -> i32 { self.value }
                }
                fn sum<T: Left + Right>(value: T) -> i32 {
                    value.left() + value.right()
                }
                sum(Both { value: 21 })
                "#
        ),
        42
    );
}

#[test]
fn builtin_copy_and_clone_traits_participate_in_bounds() {
    assert_eq!(
        integer(
            r#"
                fn add_twice<T: Copy>(value: T) -> i32 {
                    value + value
                }

                fn type_name<T: Clone>(value: T) -> string {
                    type_of(value)
                }

                assert!(type_name("hello") == "string");
                add_twice(21)
            "#
        ),
        42
    );

    let error = eval(
        r#"
            fn require_copy<T: Copy>(value: T) -> T { value }
            require_copy("not copy")
        "#,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not implement required trait `Copy`")
    );
}

#[test]
fn nominal_types_can_implement_builtin_clone_and_copy() {
    let cloned = eval(
        r#"
            struct Label { text: string }

            impl Clone for Label {
                fn clone(&self) -> Self {
                    clone(self)
                }
            }

            let label = Label { text: "Rils" };
            let copied = label.clone();
            label.text + copied.text
        "#,
    )
    .unwrap();
    assert_eq!(cloned, Value::String("RilsRils".into()));

    assert_eq!(
        integer(
            r#"
                struct Number { value: i32 }
                impl Copy for Number {}
                let number = Number { value: 21 };
                let copied = number;
                number.value + copied.value
            "#
        ),
        42
    );

    let invalid = eval(
        r#"
            struct Label { text: string }
            impl Copy for Label {}
        "#,
    )
    .unwrap_err();
    assert!(invalid.to_string().contains("non-Copy fields"));
}

#[test]
fn function_like_macros_expand_before_execution() {
    assert_eq!(
        integer(
            r#"
                macro choose_larger {
                    ($left:expr, $right:expr) => {
                        if ($left) > ($right) { ($left) } else { ($right) }
                    }
                }
                macro unless {
                    ($condition:expr, $body:expr) => {
                        if !($condition) $body else { () }
                    }
                }

                let mut answer = choose_larger!(21, 40);
                unless!(answer == 42, { answer = answer + 2; });
                answer
                "#
        ),
        42
    );
}

#[test]
fn macro_branches_and_repetitions_execute() {
    assert_eq!(
        integer(
            r#"
                macro select {
                    ($value:lit) => { $value }
                    ($name:ident) => { $name }
                    ($value:expr) => { $value }
                }
                macro bindings {
                    ($($name:ident = $value:expr),*) => {
                        $(let $name = $value;)*
                    }
                }

                bindings!()
                bindings!(left = select!(20), right = 20 + 2)
                left + right
                "#
        ),
        42
    );
}

#[test]
fn plus_repetition_requires_at_least_one_match() {
    let error = eval(
        r#"
            macro one_or_more {
                ($($value:expr),+) => { $($value),+ }
            }
            one_or_more!()
            "#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("no matching branch"));
}

#[test]
fn rust_helper_forwards_native_functions_as_rils_macros() {
    fn host_sum(arguments: &[Value]) -> Result<Value, String> {
        let mut total = 0_i32;
        for value in arguments {
            let Value::I32(value) = value else {
                return Err("host_sum expects integers".into());
            };
            total += value;
        }
        Ok(Value::I32(total))
    }

    let mut engine = Engine::new();
    rils_forward_macro!(engine, host_sum, 1, usize::MAX, host_sum).unwrap();
    assert_eq!(engine.eval("host_sum!(20, 22)").unwrap(), Value::I32(42));
    let error = engine.eval("host_sum!()").unwrap_err();
    assert!(error.to_string().contains("expects at least 1 argument"));
}

#[test]
fn former_print_functions_require_macro_invocation_syntax() {
    let error = eval("println(42)").unwrap_err();
    assert!(error.to_string().contains("undefined variable `println`"));
}

#[test]
fn engine_output_handler_receives_formatted_print_boundaries() {
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let captured = events.clone();
    let mut engine = Engine::new();
    engine.set_output_handler(move |text, newline| {
        captured.borrow_mut().push((text.to_owned(), newline));
        Ok(())
    });
    engine
        .eval(r#"print!("value={}", 7); println!(" done"); println!();"#)
        .unwrap();
    assert_eq!(
        events.borrow().as_slice(),
        [
            ("value=7".to_string(), false),
            (" done".to_string(), true),
            (String::new(), true),
        ]
    );
}

#[test]
fn standard_native_assert_macro_executes() {
    assert_eq!(eval("assert!(true)").unwrap(), Value::Unit);
    let error = eval("macro println($value) { $value }").unwrap_err();
    assert!(error.to_string().contains("duplicate macro `println`"));
}

#[test]
fn macros_report_invalid_parameters_and_argument_counts() {
    let unknown_parameter = eval("macro bad($value) { $missing } bad!(1)").unwrap_err();
    assert!(
        unknown_parameter
            .to_string()
            .contains("unknown macro parameter")
    );

    let wrong_arity = eval("macro add($left, $right) { $left + $right } add!(1)").unwrap_err();
    assert!(wrong_arity.to_string().contains("expects 2 argument(s)"));
}

#[test]
fn tuples_support_fields_assignment_and_borrowing() {
    assert_eq!(
        integer(
            r#"
                let mut pair: (i32, i32) = (20, 1);
                pair.1 = 2;
                {
                    let value = &mut pair.0;
                    *value = *value + 20;
                }
                pair.0 + pair.1
            "#,
        ),
        42
    );
}

#[test]
fn arrays_support_literals_repeat_and_index_places() {
    assert_eq!(
        integer(
            r#"
                let mut values: [i32; 3] = [10, 20, 0];
                values[2] = 11;
                {
                    let item = &mut values[2];
                    *item = *item + 1;
                }
                let repeated = [2; 3];
                values[0] + values[1] + values[2] + repeated[0]
            "#,
        ),
        44
    );
}

#[test]
fn vec_supports_core_methods_and_owned_iteration() {
    assert_eq!(
        integer(
            r#"
                let mut values: Vec<i32> = Vec::new();
                values.push(10);
                values.push(20);
                values.push(12);
                let last = unwrap(values.pop());
                assert!(values.len() == 2);
                let mut total = last + 2;
                for value in values {
                    total = total + value;
                }
                total
            "#,
        ),
        44
    );
}

#[test]
fn vec_from_array_preserves_element_type_and_iteration() {
    assert_eq!(
        integer(
            r#"
                let values = Vec::from([20, 22]);
                let mut total = 0;
                for value in values {
                    total = total + value;
                }
                total
            "#,
        ),
        42
    );
}

#[test]
fn hash_map_supports_owned_lookup_replacement_and_removal() {
    assert_eq!(
        integer(
            r#"
                let mut scores: HashMap<string, i32> = HashMap::new();
                let alice = "alice";
                assert!(scores.is_empty());
                assert!(scores.insert(alice.clone(), 20).is_none());
                assert!(scores.insert(alice.clone(), 40).unwrap() == 20);
                assert!(scores.contains_key(&alice));
                let copied = scores.get_cloned(&alice).unwrap();
                assert!(scores.len() == 1usize);
                copied + scores.remove(&alice).unwrap()
            "#,
        ),
        80
    );
}

#[test]
fn hash_set_supports_membership_and_set_algebra() {
    assert_eq!(
        integer(
            r#"
                let mut left: HashSet<i32> = HashSet::new();
                let mut right: HashSet<i32> = HashSet::new();
                assert!(left.insert(1));
                assert!(left.insert(2));
                assert!(!left.insert(2));
                right.insert(2);
                right.insert(3);
                let one = 1;
                assert!(left.contains(&one));
                assert!(left.intersection(&right).len() == 1usize);
                assert!(left.union(&right).len() == 3usize);
                assert!(left.difference(&right).len() == 1usize);
                assert!(left.symmetric_difference(&right).len() == 2usize);
                assert!(!left.is_disjoint(&right));
                if left.len() + right.len() == 4usize { 4 } else { 0 }
            "#,
        ),
        4
    );
}

#[test]
fn collections_support_search_and_owned_vec_mutation() {
    assert_eq!(
        eval(
            r#"
                let values = [1, 2, 3];
                let two = 2;
                let seven = 7;
                assert!(values.contains(&two));
                let mut first = Vec::from([1, 3, 4]);
                first.insert(1usize, 2);
                assert!(first.remove(3usize) == 4);
                first.push(5);
                assert!(first.swap_remove(0usize) == 1);
                first.extend(Vec::from([6, 7]));
                assert!(first.contains(&seven));
                first.len()
            "#
        )
        .unwrap(),
        Value::Usize(5)
    );

    let error = eval("let mut values = Vec::from([1, 2]); values.insert(3usize, 4);")
        .expect_err("out-of-bounds insertion must fail");
    assert!(error.to_string().contains("out of bounds"));

    let error = eval(
        "fn mutate() { let mut values = Vec::from([1, 2]); let item = &values[0usize]; values.remove(0usize); } mutate();",
    )
    .expect_err("reordering with an active element reference must fail");
    assert!(
        error.to_string().contains("referenced"),
        "unexpected error: {error}"
    );
}

#[test]
fn collection_iterators_support_trait_qualified_calls() {
    assert_eq!(
        integer(
            r#"
                let values = [20, 22];
                let mut iterator = <[i32; 2] as IntoIterator>::into_iter(values);
                let first = unwrap(Iterator::next(&mut iterator));
                let second = unwrap(Iterator::next(&mut iterator));
                first + second
            "#,
        ),
        42
    );
}

#[test]
fn strings_expose_unicode_and_owned_iterator_workflows() {
    let source = r#"
        let text = "  Rils,世界\r\nsecond  ";
        assert!(text.trim_start().starts_with("Rils"));
        assert!(text.trim_end().ends_with("second"));
        assert!("Straße".to_uppercase() == "STRASSE");
        assert!("RILS".to_lowercase() == "rils");
        assert!("ab".repeat(3usize) == "ababab");
        assert!(unwrap("éaé".rfind("é")) == 3usize);
        assert!(unwrap("prefix".strip_prefix("pre")) == "fix");
        assert!(unwrap("suffix".strip_suffix("fix")) == "suf");
        assert!("value".strip_prefix("x").is_none());

        let mut chars = "R世".chars();
        assert!(unwrap(chars.nth(1usize)) == '世');
        assert!(chars.next().is_none());
        assert!("R世".chars().count() == 2usize);
        assert!("R世".bytes().count() == 4usize);
        assert!(unwrap("abc".chars().last()) == 'c');

        let mut pieces = "a,b,c,d".split(",").skip(1usize).take(2usize).collect_vec();
        assert!(pieces.len() == 2usize);
        assert!(pieces.remove(0usize) == "b");
        assert!(pieces.remove(0usize) == "c");
        let mut reversed = "abc".chars().rev().collect_vec();
        assert!(reversed.remove(0usize) == 'c');
        assert!(reversed.remove(1usize) == 'a');

        let mut lines = 0usize;
        for line in text.lines() {
            lines = lines + 1usize;
        }
        lines
    "#;
    assert_eq!(eval(source).unwrap(), Value::Usize(2));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::Usize(2));
}

#[test]
fn assert_macro_reports_non_copy_string_index_moves_without_overflowing() {
    let error = match compile(
        r#"
            let values = ["first", "second"];
            assert!(values[0usize] == "first");
        "#,
    ) {
        Ok(_) => panic!("moving a string through indexing should be rejected"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("cannot move a non-Copy value out through indexing"),
        "{error}"
    );
}

#[test]
fn iterator_default_methods_cover_transform_query_and_fold_workflows() {
    let source = r#"
        fn double(value: i32) -> i32 { value * 2 }
        fn larger_than_four(value: &i32) -> bool { *value > 4 }
        fn even(value: i32) -> bool { value % 2 == 0 }
        fn positive(value: i32) -> bool { value > 0 }
        fn maybe_even(value: i32) -> Option<i32> {
            if value % 2 == 0 { Some(value * 10) } else { None }
        }
        fn sum(total: i32, value: i32) -> i32 { total + value }

        let mut mapped = [1, 2, 3, 4]
            .into_iter()
            .map(double)
            .filter(larger_than_four)
            .enumerate()
            .collect_vec();
        assert!(mapped.len() == 2usize);
        let first_mapped = mapped.remove(0usize);
        assert!(first_mapped.0 == 0usize);

        let mut selected = [1, 2, 3, 4].into_iter().filter_map(maybe_even).collect_vec();
        assert!(selected.remove(0usize) == 20);
        assert!(selected.remove(0usize) == 40);
        assert!([1, 2, 3, 4].into_iter().fold(0, sum) == 10);
        assert!([1, 3, 4].into_iter().any(even));
        assert!([1, 3, 4].into_iter().all(positive));
        assert!([1, 3, 4].into_iter().find(larger_than_four).is_none());
        assert!([1, 3, 4].into_iter().position(even).unwrap() == 2usize);

        fn validate_positive(value: i32) { assert!(value > 0); }
        [1, 2, 3].into_iter().for_each(validate_positive);
        6
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(6));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(6));
}

#[test]
fn custom_iterators_inherit_iterator_default_methods() {
    let source = r#"
        struct Counter { current: i32, end: i32 }

        impl Iterator for Counter {
            type Item = i32;

            fn next(&mut self) -> Option<i32> {
                if self.current < self.end {
                    let value = self.current;
                    let end = self.end;
                    *self = Counter { current: value + 1, end: end };
                    Some(value)
                } else {
                    None
                }
            }
        }

        fn square(value: i32) -> i32 { value * value }
        fn add(total: i32, value: i32) -> i32 { total + value }
        assert!(Counter { current: 1, end: 5 }.count() == 4usize);
        assert!(Counter { current: 1, end: 5 }.last().unwrap() == 4);
        assert!(Counter { current: 1, end: 5 }.take(2usize).fold(0, add) == 3);
        assert!(Counter { current: 1, end: 5 }.skip(2usize).fold(0, add) == 7);
        assert!(Counter { current: 1, end: 5 }.rev().fold(0, add) == 10);
        let collected = Counter { current: 1, end: 5 }.collect_vec();
        assert!(collected.len() == 4usize);
        Counter { current: 1, end: 5 }.map(square).fold(0, add)
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(30));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(30));
}

#[test]
fn iterator_predicates_short_circuit_and_filter_owned_values_by_reference() {
    let source = r#"
        fn run() -> i32 {
            let mut calls = 0;
            fn is_two(value: i32) -> bool {
                calls = calls + 1;
                value == 2
            }
            assert!([1, 2, 3, 4].into_iter().any(is_two));
            assert!(calls == 2);

            fn non_empty(value: &string) -> bool { !value.is_empty() }
            let mut values = ["first", "", "last"]
                .into_iter()
                .filter(non_empty)
                .collect_vec();
            assert!(values.remove(0usize) == "first");
            assert!(values.remove(0usize) == "last");
            calls
        }
        run()
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(2));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(2));
}

#[test]
fn collection_mutation_respects_active_element_references() {
    let assign = eval(
        r#"
            {
                let mut values = [1, 2];
                let item = &mut values[0];
                values[0] = 3;
            }
        "#,
    )
    .unwrap_err();
    assert!(assign.to_string().contains("while it is referenced"));

    let pop = eval(
        r#"
            {
                let mut values = Vec::from([1]);
                let item = &mut values[0];
                values.pop();
            }
        "#,
    )
    .unwrap_err();
    assert!(
        pop.to_string()
            .contains("cannot pop a referenced Vec element")
    );
}

#[test]
fn inline_modules_enforce_visibility_and_support_use_aliases() {
    assert_eq!(
        integer(
            r#"
                mod math {
                    fn hidden(value: i32) -> i32 { value + 1 }
                    pub fn add(left: i32, right: i32) -> i32 {
                        hidden(left + right - 1)
                    }
                }

                use math::add as sum;
                sum(20, 22)
            "#,
        ),
        42
    );

    let private = eval(
        r#"
            mod math { fn hidden() -> i32 { 42 } }
            math::hidden()
        "#,
    )
    .unwrap_err();
    assert!(private.to_string().contains("no public member `hidden`"));
}

#[test]
fn nested_modules_and_builtin_module_paths_execute() {
    assert_eq!(
        integer(
            r#"
                pub mod outer {
                    pub mod inner {
                        pub fn answer() -> i32 { 42 }
                    }
                }
                let value = outer::inner::answer();
                let optional = core::option::Some(value);
                std::io::println("module answer: {}", value);
                unwrap(optional)
            "#,
        ),
        42
    );
}

#[test]
fn public_nominal_types_construct_through_module_paths() {
    assert_eq!(
        integer(
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
        ),
        42
    );
}

#[test]
fn eval_file_loads_external_modules() {
    let directory = std::env::temp_dir().join(format!("rils-module-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let root = directory.join("main.rils");
    let module = directory.join("math.rils");
    std::fs::write(&root, "mod math; use math::answer; answer()").unwrap();
    std::fs::write(&module, "pub fn answer() -> i32 { 42 }").unwrap();

    let value = Engine::new().eval_file(&root).unwrap();
    assert_eq!(value, Value::I32(42));

    std::fs::remove_file(root).unwrap();
    std::fs::remove_file(module).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn host_modules_accept_stateful_function_closures() {
    let state = std::rc::Rc::new(std::cell::Cell::new(40_i32));
    let captured = state.clone();
    let mut engine = Engine::new();
    engine.register_module("host::counter").unwrap();
    engine
        .register_module_function("host::counter", "next", 0, 0, move |_| {
            let next = captured.get() + 1;
            captured.set(next);
            Ok(Value::I32(next))
        })
        .unwrap();

    assert_eq!(
        engine.eval("host::counter::next()").unwrap(),
        Value::I32(41)
    );
    assert_eq!(
        engine.eval("host::counter::next()").unwrap(),
        Value::I32(42)
    );
}

#[test]
fn typed_host_functions_validate_arguments_and_returns() {
    let mut engine = Engine::new();
    engine
        .register_module_typed_function(
            "host::math",
            "identity",
            vec![Type::I32],
            Type::I32,
            |arguments| Ok(arguments[0].clone()),
        )
        .unwrap();
    assert_eq!(
        engine.eval("host::math::identity(42)").unwrap(),
        Value::I32(42)
    );
    let argument_error = engine.eval("host::math::identity(\"wrong\")").unwrap_err();
    assert!(
        argument_error
            .to_string()
            .contains("expected i32, found string"),
        "{argument_error}"
    );

    let mut engine = Engine::new();
    engine
        .register_module_typed_function("host", "wrong_return", Vec::new(), Type::I32, |_| {
            Ok(Value::String("wrong".into()))
        })
        .unwrap();
    let return_error = engine.eval("host::wrong_return()").unwrap_err();
    assert!(
        return_error
            .to_string()
            .contains("return value of `wrong_return`")
            && return_error
                .to_string()
                .contains("expected i32, found string"),
        "{return_error}"
    );
}

#[test]
fn native_type_handles_create_payloads_and_dispatch_methods() {
    let mut engine = Engine::new();
    let counter_type = engine.register_native_type("host", "Counter").unwrap();
    counter_type
        .register_method("next", 0, 0, |arguments| {
            let counter = arguments[0]
                .host_payload::<std::cell::Cell<i32>>()
                .ok_or_else(|| "invalid Counter receiver".to_string())?;
            let next = counter.get() + 1;
            counter.set(next);
            Ok(Value::I32(next))
        })
        .unwrap();
    let constructor_type = counter_type.clone();
    engine
        .register_module_function("host", "counter", 1, 1, move |arguments| {
            let Value::I32(initial) = arguments[0] else {
                return Err("counter expects i32".into());
            };
            Ok(constructor_type.value(std::cell::Cell::new(initial)))
        })
        .unwrap();

    assert_eq!(
        engine
            .eval(
                r#"
                    use host::Counter;
                    let counter: Counter = host::counter(40);
                    counter.next();
                    counter.next()
                "#,
            )
            .unwrap(),
        Value::I32(42)
    );
}

fn bundled_example_expectations() -> Vec<(&'static str, Value)> {
    vec![
        ("collections_and_closures.rils", Value::I32(42)),
        ("domain_model.rils", Value::I32(42)),
        ("fallible_pipeline.rils", Value::I32(42)),
        ("hello.rils", Value::I32(720)),
        ("iterators.rils", Value::I32(20)),
        ("macros.rils", Value::I32(42)),
        ("references.rils", Value::I32(7)),
        ("task_board/src/main.rils", Value::I32(1222)),
        ("telemetry_pipeline/src/main.rils", Value::I32(7703)),
    ]
}

#[test]
fn bundled_example_catalog_covers_every_deterministic_entry() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let catalog = bundled_example_expectations()
        .into_iter()
        .map(|(path, _)| path.to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let mut discovered = std::collections::BTreeSet::new();

    for entry in std::fs::read_dir(&examples).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "rils")
        {
            if path
                .file_name()
                .is_some_and(|name| name != "standard_fs.rils")
            {
                discovered.insert(path.file_name().unwrap().to_string_lossy().into_owned());
            }
            continue;
        }
        let manifest = path.join("rils.toml");
        if !manifest.is_file() {
            continue;
        }
        let project = Project::from_file(&manifest).unwrap();
        let main = project
            .modules()
            .find(|module| module.module_path == "main")
            .unwrap_or_else(|| panic!("example project `{}` has no main module", path.display()));
        discovered.insert(
            main.path
                .strip_prefix(&examples)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }

    assert_eq!(catalog, discovered);
}

#[test]
fn bundled_examples_compile_to_bytecode() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut paths = bundled_example_expectations()
        .into_iter()
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    paths.push("standard_fs.rils");
    for relative_path in paths {
        let path = examples.join(relative_path);
        compile_file(&path).unwrap_or_else(|error| {
            panic!("example `{}` failed to compile: {error}", path.display())
        });
    }
}

#[test]
fn bundled_examples_match_interpreter_and_vm() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    for (relative_path, expected) in bundled_example_expectations() {
        let path = examples.join(relative_path);
        let interpreted = Engine::new().eval_file(&path).unwrap_or_else(|error| {
            panic!(
                "example `{}` failed in the interpreter: {error}",
                path.display()
            )
        });
        let module = compile_file(&path).unwrap_or_else(|error| {
            panic!("example `{}` failed to compile: {error}", path.display())
        });
        let mut host = BytecodeHost::standard();
        host.enable_standard_io().unwrap();
        let executed = module.execute_with_host(&host).unwrap_or_else(|error| {
            panic!(
                "example `{}` failed in the VM at {:?}: {error}",
                path.display(),
                error.span
            )
        });
        assert_eq!(
            interpreted,
            expected,
            "interpreter returned an unexpected value for example `{}`",
            path.display()
        );
        assert_eq!(
            executed,
            expected,
            "VM returned an unexpected value for example `{}`",
            path.display()
        );
    }
}

#[test]
fn project_files_are_modules_and_entry_main_uses_anchored_paths() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "rils-project-runtime-test-{}-{unique}",
        std::process::id()
    ));
    let scripts = root.join("Assets/Res/rils-script");
    std::fs::create_dir_all(scripts.join("feature")).unwrap();
    std::fs::write(
        root.join("rils.toml"),
        r#"
            [project]
            name = "unity_game"
            src = "Assets/Res/rils-script"
        "#,
    )
    .unwrap();
    std::fs::write(scripts.join("math.rils"), "pub fn answer() -> i32 { 41 }").unwrap();
    std::fs::write(scripts.join("main.rils"), "").unwrap();
    let entry = scripts.join("feature/mod.rils");
    std::fs::write(
        &entry,
        r#"
            fn local() -> i32 { 1 }
            fn main() -> i32 { self::local() + super::math::answer() }
        "#,
    )
    .unwrap();

    let interpreted = Engine::new().eval_file(&entry).unwrap();
    let compiled = compile_file(&entry).unwrap().execute().unwrap();
    assert_eq!(interpreted, Value::I32(42));
    assert_eq!(compiled, interpreted);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn configured_projects_reject_frontend_errors_before_execution() {
    let unique = format!(
        "rils-project-static-error-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let root = std::env::temp_dir().join(unique);
    let scripts = root.join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(
        root.join("rils.toml"),
        "[project]\nname = \"static_error\"\nsrc = \"scripts\"\n",
    )
    .unwrap();
    let entry = scripts.join("main.rils");
    std::fs::write(&entry, "pub fn main() -> i32 { missing() }").unwrap();

    let error = Engine::new().eval_file(&entry).unwrap_err();
    std::fs::remove_dir_all(&root).unwrap();

    assert!(error.to_string().contains("undefined name `missing`"));
}

#[test]
fn project_modules_can_use_standard_native_macros() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "rils-project-native-macro-test-{}-{unique}",
        std::process::id()
    ));
    let scripts = root.join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(
        root.join("rils.toml"),
        "[project]\nname = \"native_macros\"\nsrc = \"scripts\"\n",
    )
    .unwrap();
    std::fs::write(
        scripts.join("checks.rils"),
        r#"
            pub fn verify() {
                assert!(true, "assert should expand outside the entry module");
                print!("print should expand outside the entry module");
                println!("println should expand outside the entry module");
            }
        "#,
    )
    .unwrap();
    let entry = scripts.join("main.rils");
    std::fs::write(&entry, "fn main() { crate::checks::verify(); }").unwrap();

    compile_file(&entry).expect("standard native macros should compile in project modules");

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_entries_support_grouped_and_glob_imports() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "rils-project-use-tree-test-{}-{unique}",
        std::process::id()
    ));
    let scripts = root.join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(
        root.join("rils.toml"),
        "[project]\nname = \"use_tree\"\nsrc = \"scripts\"\n",
    )
    .unwrap();
    std::fs::write(
        scripts.join("api.rils"),
        r#"
            pub fn alpha() -> i32 { 10 }
            pub fn beta() -> i32 { 11 }
            pub mod nested {
                pub fn delta() -> i32 { 9 }
                pub fn epsilon() -> i32 { 12 }
            }
        "#,
    )
    .unwrap();
    let entry = scripts.join("main.rils");
    std::fs::write(
        &entry,
        r#"
            use crate::api::{alpha, beta as b, nested::{delta, epsilon}};
            fn main() -> i32 { alpha() + b() + delta() + epsilon() }
        "#,
    )
    .unwrap();

    let interpreted = Engine::new().eval_file(&entry).unwrap();
    let compiled = compile_file(&entry).unwrap().execute().unwrap();
    assert_eq!(interpreted, Value::I32(42));
    assert_eq!(compiled, interpreted);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_source_ids_survive_bytecode_round_trip_and_locate_runtime_errors() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "rils-source-id-bytecode-test-{}-{unique}",
        std::process::id()
    ));
    let scripts = root.join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(
        root.join("rils.toml"),
        "[project]\nname = \"source_ids\"\nsrc = \"scripts\"\n",
    )
    .unwrap();
    let entry = scripts.join("entry.rils");
    let dependency = scripts.join("math.rils");
    std::fs::write(scripts.join("main.rils"), "").unwrap();
    std::fs::write(&entry, "fn main() -> i32 { crate::math::fail() }").unwrap();
    std::fs::write(&dependency, "pub fn fail() -> i32 { 1 / 0 }").unwrap();

    let module = compile_file(&entry).unwrap();
    assert_eq!(module.sources().len(), 3);
    assert!(
        module
            .sources()
            .iter()
            .all(|source| source.id != SourceId::UNKNOWN)
    );
    let bytes = module.to_bytes().unwrap();
    let loaded = BytecodeModule::from_bytes(&bytes).unwrap();
    assert_eq!(loaded.sources(), module.sources());
    let error = loaded.execute().unwrap_err();
    assert_eq!(
        loaded.source_name(error.span.source),
        Some(dependency.to_string_lossy().as_ref())
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_compile_and_interpreter_errors_retain_dependency_source() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "rils-source-id-diagnostic-test-{}-{unique}",
        std::process::id()
    ));
    let scripts = root.join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(
        root.join("rils.toml"),
        "[project]\nname = \"source_diagnostics\"\nsrc = \"scripts\"\n",
    )
    .unwrap();
    let entry = scripts.join("entry.rils");
    let dependency = scripts.join("broken.rils");
    std::fs::write(&entry, "fn main() -> i32 { 42 }").unwrap();
    std::fs::write(&dependency, "pub fn broken() -> i32 { missing }").unwrap();

    let error = match compile_file(&entry) {
        Ok(_) => panic!("dependency analysis should fail"),
        Err(error) => error,
    };
    assert_eq!(
        error.source_name(),
        Some(dependency.to_string_lossy().as_ref())
    );
    assert_eq!(
        error.source_text(),
        Some("pub fn broken() -> i32 { missing }")
    );

    std::fs::write(&dependency, "pub fn broken() -> i32 { @ }").unwrap();
    let error = Engine::new().eval_file(&entry).unwrap_err();
    let rendered = error.render(entry.to_string_lossy().as_ref(), "");
    assert!(rendered.contains(dependency.to_string_lossy().as_ref()));
    assert!(rendered.contains("pub fn broken() -> i32 { @ }"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn projects_without_main_are_library_projects() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "rils-project-main-test-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::write(
        root.join("rils.toml"),
        "[project]\nname = \"game\"\nsrc = \"scripts\"\n",
    )
    .unwrap();
    let entry = root.join("scripts/no_main.rils");
    std::fs::write(&entry, "pub fn value() -> i32 { 42 }").unwrap();
    assert!(compile_file(&entry).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}
