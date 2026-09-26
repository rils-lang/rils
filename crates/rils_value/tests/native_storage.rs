use std::{cell::Cell, rc::Rc};

use rils_stdlib::stdlib::{option::Option as NativeOption, string::String as NativeString};
use rils_syntax::Type;
use rils_value::{NativeObject, NativeType};

#[test]
fn small_copy_values_use_inline_layout_without_a_value_allocation() {
    let descriptor = Rc::new(NativeType::<i32>::new::<i32>(Type::I32).with_copy::<i32>());
    let object = NativeObject::new(descriptor.clone(), 42_i32).unwrap();
    assert!(object.is_inline());
    assert_eq!(object.with::<i32, _>(|value| *value), Ok(42));
    let copy = object.copy_owned().unwrap();
    assert!(copy.is_inline());
    assert_eq!(copy.with::<i32, _>(|value| *value), Ok(42));
    assert!(object.with_mut::<i32, _>(|value| *value += 1).is_err());
    assert!(NativeObject::new(descriptor, 42_u8).is_err());
}

#[test]
fn representative_stdlib_layouts_are_supported_before_runtime_migration() {
    let usize_descriptor =
        Rc::new(NativeType::<()>::new::<usize>(Type::USIZE).with_copy::<usize>());
    let usize_value = NativeObject::new(usize_descriptor, usize::MAX).unwrap();
    assert!(usize_value.is_inline());
    assert_eq!(usize_value.with::<usize, _>(|value| *value), Ok(usize::MAX));

    let option_i32_descriptor = Rc::new(
        NativeType::<()>::new::<NativeOption<i32>>(Type::Option(Box::new(Type::I32)))
            .with_copy::<NativeOption<i32>>(),
    );
    let option_i32 = NativeObject::new(option_i32_descriptor, NativeOption::Some(7_i32)).unwrap();
    assert!(option_i32.is_inline());
    assert_eq!(
        option_i32.with::<NativeOption<i32>, _>(|value| match value {
            NativeOption::Some(value) => Some(*value),
            NativeOption::None => None,
        }),
        Ok(Some(7))
    );

    let string_descriptor = Rc::new(NativeType::<()>::new::<NativeString>(Type::String));
    let string =
        NativeObject::new(string_descriptor, NativeString::from("hello".to_owned())).unwrap();
    assert!(!string.is_inline());
    assert_eq!(
        string.with::<NativeString, _>(|value| std::string::String::from(value.clone())),
        Ok("hello".to_owned())
    );

    let option_string_descriptor = Rc::new(NativeType::<()>::new::<NativeOption<NativeString>>(
        Type::Option(Box::new(Type::String)),
    ));
    let option_string = NativeObject::new(
        option_string_descriptor,
        NativeOption::Some(NativeString::from("world".to_owned())),
    )
    .unwrap();
    assert!(!option_string.is_inline());
    assert_eq!(
        option_string.with::<NativeOption<NativeString>, _>(|value| match value {
            NativeOption::Some(value) => Some(std::string::String::from(value.clone())),
            NativeOption::None => None,
        }),
        Ok(Some("world".to_owned()))
    );
}

#[test]
fn large_or_high_alignment_copy_values_use_independent_heap_storage() {
    #[derive(Clone, Copy)]
    #[repr(align(64))]
    struct Aligned([u8; 64]);

    let descriptor =
        Rc::new(NativeType::<u8>::new::<Aligned>(Type::named("Aligned")).with_copy::<Aligned>());
    let value = NativeObject::new(descriptor, Aligned([7; 64])).unwrap();
    assert!(!value.is_inline());
    let copy = value.copy_owned().unwrap();
    assert!(!copy.is_inline());
    assert_eq!(copy.with::<Aligned, _>(|value| value.0[0]), Ok(7));
    value
        .with_mut::<Aligned, _>(|value| value.0[0] = 9)
        .unwrap();
    assert_eq!(copy.with::<Aligned, _>(|value| value.0[0]), Ok(7));
}

#[test]
fn arbitrary_registered_operations_receive_typed_context_and_arguments() {
    struct Counter(u8);

    let descriptor = Rc::new(
        NativeType::<u8>::new::<Counter>(Type::named("Counter")).register_method(
            "advance_by",
            |context| {
                let [step] = context.arguments() else {
                    return Err("expected one step".into());
                };
                context.receiver_mut::<Counter, _>(|counter| {
                    counter.0 += step;
                    counter.0
                })
            },
        ),
    );
    let value = NativeObject::new(descriptor, Counter(1)).unwrap();
    let alias = value.clone();
    assert_eq!(alias.call("advance_by", &[2]).unwrap(), Ok(3));
    assert_eq!(value.with::<Counter, _>(|counter| counter.0), Ok(3));
    assert!(value.call("missing", &[]).is_none());
    assert!(value.call("advance_by", &[]).unwrap().is_err());
    assert!(value.copy_owned().is_err());
    assert!(
        value
            .with_mut::<Counter, _>(|_| value.with::<Counter, _>(|counter| counter.0))
            .unwrap()
            .is_err()
    );
}

#[test]
fn shared_native_payload_drops_exactly_once() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let drops = Rc::new(Cell::new(0));
    let descriptor = Rc::new(NativeType::<()>::new::<Probe>(Type::named("Probe")));
    let value = NativeObject::new(descriptor, Probe(drops.clone())).unwrap();
    let alias = value.clone();
    assert!(!value.is_inline());
    drop(value);
    assert_eq!(drops.get(), 0);
    drop(alias);
    assert_eq!(drops.get(), 1);
}
