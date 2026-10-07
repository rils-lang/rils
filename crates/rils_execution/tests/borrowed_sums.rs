use std::{cell::Cell, rc::Rc};

use rils_execution::{
    RilsValue, Type, Value,
    environment::{Environment, StorageRef},
    value::{
        ReferenceValue,
        borrowed_sum::{self, Branch},
        record_codec,
        record_layout::RecordLayoutResolver,
    },
};
use rils_value::{DynamicLayout, DynamicObject, DynamicPathStep, DynamicType, DynamicValue};

fn source(
    value: Value,
) -> (
    rils_execution::environment::EnvironmentRef,
    StorageRef,
    Rc<ReferenceValue>,
) {
    let environment = Environment::global();
    environment.borrow_mut().define("sum", value, true, None);
    let storage = environment.borrow().slot("sum").unwrap();
    let root = Rc::new(ReferenceValue::new_storage(storage.clone(), true));
    (environment, storage, root)
}

fn number_sum(branch: Branch) -> Value {
    let number = RecordLayoutResolver::new(&[]).resolve(&Type::I32).unwrap();
    let item = || record_codec::into_native(Value::from_i32(1), number.clone()).unwrap();
    let payload = match branch {
        Branch::Some => {
            DynamicValue::some(DynamicLayout::option(number.clone()).unwrap(), item()).unwrap()
        }
        Branch::None => DynamicValue::none(DynamicLayout::option(number.clone()).unwrap()).unwrap(),
        Branch::Ok | Branch::Err => DynamicValue::variant(
            DynamicLayout::variant(
                Type::Result(Box::new(Type::I32), Box::new(Type::I32)),
                vec![number.clone(), number.clone()],
            )
            .unwrap(),
            usize::from(branch == Branch::Err),
            item(),
        )
        .unwrap(),
    };
    Value::Dynamic(
        DynamicObject::new(Rc::new(DynamicType::new(payload.layout_handle())), payload).unwrap(),
    )
}

#[test]
fn every_sum_branch_uses_original_storage_and_lexical_guards() {
    for branch in [Branch::Some, Branch::None, Branch::Ok, Branch::Err] {
        let value = number_sum(branch);
        assert!(matches!(&value, Value::Dynamic(object) if object.is_inline()));
        let (environment, storage, root) = source(value);
        assert_eq!(
            borrowed_sum::branch(&Value::Reference(root.clone())).unwrap(),
            Some(branch)
        );
        assert!(
            matches!(storage.borrow().read().unwrap(), Value::Dynamic(object) if object.is_inline())
        );
        if branch == Branch::None {
            assert!(borrowed_sum::payload(&Value::Reference(root.clone()), branch).is_err());
            assert!(root.project_native_step(DynamicPathStep::Some).is_err());
            continue;
        }
        let bound = borrowed_sum::payload(&Value::Reference(root.clone()), branch).unwrap();
        assert!(
            matches!(storage.borrow().read().unwrap(), Value::Dynamic(object) if !object.is_inline())
        );
        let Value::Reference(child) = &bound else {
            panic!("payload reference")
        };
        assert!(!child.mutable);
        assert!(child.is_local_to(&environment));
        let step = match branch {
            Branch::Some => DynamicPathStep::Some,
            Branch::Ok => DynamicPathStep::Variant(0),
            Branch::Err => DynamicPathStep::Variant(1),
            Branch::None => unreachable!(),
        };
        let writer = root.project_native_step(step).unwrap().unwrap();
        writer.write(Value::from_i32(42)).unwrap();
        assert_eq!(child.copy_native().unwrap(), Some(Value::from_i32(42)));
        assert!(storage.borrow_mut().assign(number_sum(branch)).is_err());
        let independent = storage.borrow_mut().take().unwrap();
        writer.write(Value::from_i32(7)).unwrap();
        assert_eq!(child.copy_native().unwrap(), Some(Value::from_i32(7)));
        assert_eq!(
            independent
                .as_option()
                .and_then(|v| v.0)
                .or_else(|| independent.as_result().map(|v| v.0.unwrap_or_else(|e| e))),
            Some(Value::from_i32(42))
        );
        drop(writer);
        drop(root);
        assert!(child.is_local_to(&environment));
        assert!(storage.borrow_mut().assign(number_sum(branch)).is_err());
        drop(bound);
        storage.borrow_mut().assign(number_sum(branch)).unwrap();
    }
}

#[test]
fn nested_sum_projection_does_not_clone_unregistered_non_copy_payloads() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let leaf = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let record =
        DynamicLayout::record(Type::named("Record"), vec![("probe".into(), leaf.clone())]).unwrap();
    let error = RecordLayoutResolver::new(&[])
        .resolve(&Type::String)
        .unwrap();
    let result = DynamicLayout::variant(
        Type::Result(Box::new(Type::named("Record")), Box::new(Type::String)),
        vec![record.clone(), error],
    )
    .unwrap();
    let option = DynamicLayout::option(result.clone()).unwrap();
    let payload = DynamicValue::some(
        option.clone(),
        DynamicValue::variant(
            result,
            0,
            DynamicValue::record(
                record,
                vec![DynamicValue::from_rust(leaf, Probe(drops.clone())).unwrap()],
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let (environment, storage, root) = source(Value::Dynamic(
        DynamicObject::new(Rc::new(DynamicType::new(option)), payload).unwrap(),
    ));
    let outer = Value::Reference(root.clone());
    assert_eq!(borrowed_sum::branch(&outer).unwrap(), Some(Branch::Some));
    let result = borrowed_sum::payload(&outer, Branch::Some).unwrap();
    let Value::Reference(reference) = &result else {
        panic!("result reference")
    };
    assert!(
        reference.read().is_err(),
        "Probe has no borrowed Clone registration"
    );
    assert_eq!(borrowed_sum::branch(&result).unwrap(), Some(Branch::Ok));
    let record = borrowed_sum::payload(&result, Branch::Ok).unwrap();
    let Value::Reference(record) = record else {
        panic!("record reference")
    };
    assert!(record.project_native_variant(1).is_err());
    let field = Rc::new(record.project_native_field("probe").unwrap().unwrap());
    assert!(field.is_local_to(&environment));
    RilsValue::new(Value::Reference(field.clone()))
        .with_native_view(|view| {
            view.with_rust::<Probe, _>(|probe| assert!(Rc::ptr_eq(&probe.0, &drops)))
        })
        .unwrap()
        .unwrap();
    assert_eq!(drops.get(), 0);
    drop(outer);
    drop(result);
    drop(record);
    drop(root);
    assert!(
        storage.borrow_mut().take().is_err(),
        "child retains the root slot lease"
    );
    drop(field);
    drop(storage.borrow_mut().take().unwrap());
    assert_eq!(drops.get(), 1);
}

#[test]
fn non_copy_payload_assignment_requires_ownership_and_preserves_failed_targets() {
    let rils_execution::value::dynamic_option::Construction::Native(value) =
        rils_execution::value::dynamic_option::construct(
            Some(Value::from_string("original")),
            &Type::String,
        )
        .unwrap()
    else {
        panic!("native option")
    };
    let (_, _, root) = source(value);
    let writer = Rc::new(
        root.project_native_step(DynamicPathStep::Some)
            .unwrap()
            .unwrap(),
    );
    let shared = Value::from_string("shared");
    assert!(writer.write(shared.clone()).is_err());
    assert_eq!(
        RilsValue::new(Value::Reference(writer.clone()))
            .get_cloned::<String>()
            .unwrap(),
        "original"
    );
    assert_eq!(shared.as_string().as_deref(), Some("shared"));
    writer.write(Value::from_string("changed")).unwrap();
    assert_eq!(
        RilsValue::new(Value::Reference(writer))
            .get_cloned::<String>()
            .unwrap(),
        "changed"
    );
}
