use rils::{Engine, compile, eval};

#[test]
fn interpreter_and_bytecode_return_typed_handles() {
    assert_eq!(eval("40 + 2").unwrap().get_cloned::<i32>().unwrap(), 42);
    let compiled = compile("40 + 2").unwrap();
    assert_eq!(
        compiled.execute().unwrap().into_owned::<i32>().ok(),
        Some(42)
    );
}

#[test]
fn result_handle_borrows_or_moves_string_explicitly() {
    let result = eval("\"hello\"").unwrap();
    assert_eq!(result.with_ref::<String, _>(|text| text.len()).unwrap(), 5);
    assert_eq!(result.get_cloned::<String>().unwrap(), "hello");
    assert_eq!(result.into_owned::<String>().ok().as_deref(), Some("hello"));
}

#[test]
fn returned_script_reference_cannot_be_moved() {
    let mut engine = Engine::new();
    engine.eval("let value = 7;").unwrap();
    let result = engine.eval("&value").unwrap();
    assert_eq!(result.with_ref::<i32, _>(|value| *value).unwrap(), 7);
    assert_eq!(result.get_cloned::<i32>().unwrap(), 7);
    assert!(result.into_owned::<i32>().is_err());
}

#[test]
fn native_vec_element_reference_borrows_original_string() {
    let mut engine = Engine::new();
    engine
        .eval("let mut values: Vec<string> = Vec::new(); values.push(\"hello\");")
        .unwrap();
    let result = engine.eval("&values[0]").unwrap();
    assert!(
        result
            .with_ref::<String, _>(|text| text == "hello")
            .unwrap()
    );
    assert!(result.into_owned::<String>().is_err());
}

#[test]
fn native_view_reads_noncopy_generic_element_without_value_conversion() {
    let mut engine = Engine::new();
    engine
        .eval("let mut values: Vec<Option<string>> = Vec::new(); values.push(Some(\"hello\"));")
        .unwrap();
    let result = engine.eval("&values[0]").unwrap();
    let text = result
        .with_native_view(|option| {
            assert_eq!(option.option_is_some(), Ok(true));
            let item = option.option_item()?;
            assert_eq!(item.layout()?.rils_type().to_string(), "string");
            Ok::<_, String>(item.copy_owned().is_err())
        })
        .unwrap()
        .unwrap();
    assert!(text);
    assert!(result.into_owned::<String>().is_err());
}

#[test]
fn script_struct_fields_have_borrowed_result_handles() {
    let result =
        eval("struct Point { x: i32, label: string } Point { x: 7, label: \"ok\" }").unwrap();
    assert_eq!(result.struct_name().as_deref(), Some("Point"));
    assert_eq!(result.field_name(0).as_deref(), Some("x"));
    assert_eq!(result.field_name(1).as_deref(), Some("label"));
    let x = result.field(0).unwrap();
    let label = result.field(1).unwrap();
    drop(result);
    assert_eq!(x.get_cloned::<i32>().unwrap(), 7);
    assert_eq!(label.get_cloned::<String>().unwrap(), "ok");
    assert!(x.into_owned::<i32>().is_err());
}
