use crate::{Type, Value, runtime_builtins::native_output::NativeOutput, value::native_ops};
use rils_stdlib::stdlib::{iterator::Iter, option::Option as NOption, result::Result as NResult};

#[test]
fn composite_outputs_preserve_types_across_empty_and_error_branches() {
    type Output = NOption<NResult<(i32, bool), String>>;
    for input in [
        Output::None,
        NOption::Some(NResult::Ok((i32::MIN, false))),
        NOption::Some(NResult::Err("error".into())),
    ] {
        let result = input.into_value().unwrap();
        assert_eq!(Type::of_value(&result), Some(Output::output_type()));
        assert!(!result.contains_reference());
    }
}

#[test]
fn generated_iterators_keep_declared_type_and_composite_items() {
    type Item = NOption<(i32, bool)>;
    let iterator = Iter::from_generator([NOption::Some((42, true)), NOption::None].into_iter())
        .into_value()
        .unwrap();
    assert_eq!(Type::of_value(&iterator), Some(Iter::<Item>::output_type()));
    let Value::Native(object) = iterator else {
        panic!("native iterator")
    };
    for present in [true, true, false] {
        let output = object.call(native_ops::NEXT, &[]).unwrap().unwrap();
        assert_eq!(
            Type::of_value(&output),
            Some(Type::Option(Box::new(Item::output_type())))
        );
        assert_eq!(
            crate::value::dynamic_option::take_owned(output)
                .unwrap()
                .is_some(),
            present
        );
    }
}

#[test]
fn generated_iterator_conversion_errors_are_propagated_lazily() {
    struct Failing;
    impl NativeOutput for Failing {
        fn output_type() -> Type {
            Type::I32
        }
        fn into_value(self) -> Result<Value, String> {
            Err("conversion failed".into())
        }
    }
    let value = Iter::from_generator(std::iter::once(Failing))
        .into_value()
        .unwrap();
    let Value::Native(object) = value else {
        panic!("native iterator")
    };
    assert_eq!(
        object.call(native_ops::NEXT, &[]).unwrap().unwrap_err(),
        "conversion failed"
    );
    let exhausted = object.call(native_ops::NEXT, &[]).unwrap().unwrap();
    assert!(
        crate::value::dynamic_option::take_owned(exhausted)
            .unwrap()
            .is_none()
    );
}
