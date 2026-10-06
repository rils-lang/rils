use rils_execution::{
    environment::Environment,
    value::{StructType, Value},
};
use std::rc::Rc;

fn declaration(name: String) -> Rc<StructType> {
    Rc::new(StructType {
        name,
        opaque_native: false,
        generic_parameters: Vec::new(),
        fields: Vec::new(),
        field_indices: Default::default(),
        methods: Default::default(),
        trait_methods: Default::default(),
        implemented_traits: Default::default(),
        associated_types: Default::default(),
    })
}

#[test]
fn declarations_have_module_identity_without_introducing_lexical_bindings() {
    let root = Environment::global();
    let left = Environment::named_module_child(root.clone(), "left");
    let right = Environment::named_module_child(root.clone(), "right");
    for module in [&left, &right] {
        let name = module.borrow().qualified_type_name("Item");
        module
            .borrow_mut()
            .define("Item", Value::StructType(declaration(name)), false, None);
    }
    assert_eq!(root.borrow().visible_type_definitions().0.len(), 2);
    assert!(root.borrow().slot("Item").is_none());
    for name in ["left::Item", "right::Item"] {
        let Some(Value::StructType(value)) = root.borrow().get(name) else {
            panic!("missing {name}")
        };
        assert_eq!(value.name, name);
    }
    let scope = Environment::child(left);
    assert_eq!(scope.borrow().qualified_type_name("Local"), "left::Local");
}

#[test]
fn aliases_deduplicate_declarations_and_registry_does_not_extend_ownership() {
    let root = Environment::global();
    let module = Environment::named_module_child(root.clone(), "model");
    let value = declaration("model::Item".into());
    let weak = Rc::downgrade(&value);
    module
        .borrow_mut()
        .define("Item", Value::StructType(value.clone()), false, None);
    module
        .borrow_mut()
        .define("ImportedItem", Value::StructType(value), false, None);
    assert_eq!(root.borrow().visible_type_definitions().0.len(), 1);
    assert_eq!(
        weak.strong_count(),
        2,
        "only the two bindings should retain the declaration"
    );
    drop(module);
    assert!(weak.upgrade().is_none());
    assert!(root.borrow().visible_type_definitions().0.is_empty());
    assert!(root.borrow().get("model::Item").is_none());
}
