use std::rc::Rc;

use rils_execution::{
    Type,
    declaration::{Binding, Declaration},
    environment::{AccessError, AssignError, Environment},
    value::{TypeAliasType, Value},
};

fn alias(name: &str) -> Rc<TypeAliasType> {
    Rc::new(TypeAliasType {
        name: name.into(),
        generic_parameters: vec![],
        target: Type::I32,
    })
}

#[test]
fn declarations_never_allocate_runtime_slots() {
    let environment = Environment::global();
    environment.borrow_mut().define(
        "Number",
        Declaration::TypeAlias(alias("Number")),
        false,
        None,
    );
    let mut environment = environment.borrow_mut();
    assert!(environment.get("Number").is_none());
    assert!(environment.slot("Number").is_none());
    assert!(matches!(
        environment.take("Number"),
        Err(AccessError::Undefined)
    ));
    assert!(matches!(
        environment.assign("Number", Value::from(rils_execution::value::native_i32(42))),
        Err(AssignError::Immutable)
    ));
    assert!(environment.binding("Number").unwrap().into_value().is_err());
}

#[test]
fn declarations_and_values_shadow_each_other_without_exposing_parent_slots() {
    let parent = Environment::global();
    parent.borrow_mut().define(
        "item",
        Value::from(rils_execution::value::native_i32(42)),
        true,
        None,
    );
    let child = Environment::child(parent.clone());
    child
        .borrow_mut()
        .define("item", Declaration::TypeAlias(alias("item")), false, None);
    assert!(child.borrow().get("item").is_none());
    assert!(matches!(
        child
            .borrow_mut()
            .assign("item", Value::from(rils_execution::value::native_i32(0))),
        Err(AssignError::Immutable)
    ));
    assert_eq!(
        parent
            .borrow()
            .get("item")
            .unwrap()
            .get_cloned::<i32>()
            .ok(),
        Some(42)
    );
    let nested = Environment::child(child);
    nested.borrow_mut().define(
        "item",
        Value::from(rils_execution::value::native_string(String::from("owned"))),
        false,
        None,
    );
    assert!(nested.borrow().get_declaration("item").is_none());
    nested.borrow().take("item").unwrap();
    assert!(nested.borrow().binding("item").is_none());
    assert!(nested.borrow().get_declaration("item").is_none());
}

#[test]
fn aliases_preserve_identity_and_registry_owns_qualified_metadata() {
    let environment = Environment::global();
    let definition = alias("module::Number");
    let weak = Rc::downgrade(&definition);
    environment.borrow_mut().define(
        "Number",
        Declaration::TypeAlias(definition.clone()),
        false,
        None,
    );
    let imported = environment.borrow().binding("Number").unwrap();
    environment
        .borrow_mut()
        .define("Alias", imported, false, None);
    let Some(Binding::Declaration(Declaration::TypeAlias(imported))) =
        environment.borrow().binding("Alias")
    else {
        panic!("missing declaration")
    };
    assert!(Rc::ptr_eq(&definition, &imported));
    drop(imported);
    drop(definition);
    for name in ["Number", "Alias"] {
        environment.borrow_mut().define(
            name,
            Value::from(rils_execution::value::native_i32(0)),
            false,
            None,
        );
    }
    assert!(weak.upgrade().is_some());
    assert!(matches!(
        environment.borrow().get_declaration("module::Number"),
        Some(Declaration::TypeAlias(_))
    ));
    drop(environment);
    assert!(weak.upgrade().is_none());
}
