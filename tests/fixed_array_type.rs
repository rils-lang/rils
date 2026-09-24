use rils::{BytecodeModule, Value, compile, eval};

#[test]
fn fixed_arrays_keep_sequence_methods_without_an_array_type() {
    let source = r#"
        let values: [i32; 3] = [1, 2, 3];
        values.len() + values.into_iter().count()
    "#;
    assert_eq!(eval(source).unwrap(), Value::Usize(6));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::Usize(6));
}

#[test]
fn removed_array_type_and_vec_mutation_are_rejected() {
    let obsolete = "let values: Array<i32> = [1, 2]; values.len()";
    assert!(eval(obsolete).is_err());
    assert!(compile(obsolete).is_err());

    let mutation = "let mut values: [i32; 2] = [1, 2]; values.push(3)";
    assert!(eval(mutation).is_err());
    assert!(compile(mutation).is_err());
}

#[test]
fn borrowed_slices_read_fixed_arrays_without_moving_them() {
    let source = r#"
        fn first(values: &[i32]) -> i32 { values[0] }
        let values: [i32; 3] = [7, 8, 9];
        first(&values) + values[1]
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(15));
    let compiled = compile(source).unwrap();
    assert_eq!(compiled.execute().unwrap(), Value::I32(15));
    let restored = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(restored.execute().unwrap(), Value::I32(15));
}

#[test]
fn borrowed_slices_accept_non_copy_arrays_and_vectors() {
    let source = r#"
        fn count(values: &[string]) -> usize { values.len() }
        let values: [string; 2] = ["left", "right"];
        let mut vector: Vec<string> = Vec::new();
        vector.push("third");
        count(&values) + count(&vector) + values.len()
    "#;
    assert_eq!(eval(source).unwrap(), Value::Usize(5));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::Usize(5));
}

#[test]
fn bare_slice_types_are_rejected() {
    let source = "fn invalid(values: [i32]) {}";
    assert!(eval(source).is_err());
    assert!(compile(source).is_err());
}

#[test]
fn slices_cannot_return_references_to_local_arrays() {
    let source = r#"
        fn invalid() -> &[i32] {
            let values: [i32; 1] = [1];
            &values
        }
        invalid()
    "#;
    assert!(eval(source).is_err());
    assert!(compile(source).is_err());
}
