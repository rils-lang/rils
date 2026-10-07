use std::{collections::BTreeMap, rc::Rc};

use rils_execution::{
    IntegerType, Type, Value,
    value::{
        host_declarations::{HOST_FLAGS_RAW_VARIANT, enum_definition, link_enum_declaration},
        native_instance::definition,
        record_codec::NativeRecordCodec,
        storage::TypedStorageContext,
    },
};
use rils_frontend::ast::EnumVariant;
use rils_host::HostEnumDefinition;

fn signature(flags: bool) -> HostEnumDefinition {
    HostEnumDefinition {
        underlying_type: IntegerType::U32,
        flags,
        variants: BTreeMap::from([("Ready".into(), 1)]),
    }
}

#[test]
fn equivalent_host_enum_contracts_keep_the_installed_declaration_and_local_traits() {
    for (flags, variant) in [
        (false, "Ready"),
        (true, "Ready"),
        (true, HOST_FLAGS_RAW_VARIANT),
    ] {
        let signature = signature(flags);
        let source = enum_definition("host::State".into(), &signature);
        let installed = enum_definition("host::State".into(), &signature);
        assert!(!Rc::ptr_eq(&source, &installed));
        installed
            .implemented_traits
            .borrow_mut()
            .insert("Eq".into());
        let source_declarations = [source.clone()];
        let installed_declarations = [installed.clone()];
        let storage = TypedStorageContext::new(&[], &source_declarations);
        let ty = Type::named("host::State");
        let value = if variant == HOST_FLAGS_RAW_VARIANT {
            storage.construct_tuple_variant(&ty, variant, vec![Value::from_u128(7)])
        } else {
            storage.construct_unit_variant(&ty, variant)
        }
        .unwrap();
        let mut codec = NativeRecordCodec::with_definitions(&[], &installed_declarations);
        let bytes = codec
            .into_native(value, storage.layout(&ty).unwrap())
            .unwrap();
        let Value::Dynamic(object) = codec.from_native(bytes).unwrap() else {
            panic!("native host enum");
        };
        let Some(Value::EnumType(restored)) = definition(&object) else {
            panic!("host declaration");
        };
        assert!(Rc::ptr_eq(&installed, &restored));
        assert!(restored.implemented_traits.borrow().contains("Eq"));
        assert!(!source.implemented_traits.borrow().contains("Eq"));
        assert!(Value::Dynamic(object).is_copy());
    }
}

#[test]
fn host_enum_contract_identity_checks_width_discriminants_flags_and_variant_names() {
    let original = signature(false);
    for change in ["width", "discriminant", "flags", "name"] {
        let source = enum_definition("host::State".into(), &original);
        let source_declarations = [source];
        let storage = TypedStorageContext::new(&[], &source_declarations);
        let ty = Type::named("host::State");
        let value = storage.construct_unit_variant(&ty, "Ready").unwrap();
        let mut changed = original.clone();
        match change {
            "width" => changed.underlying_type = IntegerType::U16,
            "discriminant" => {
                changed.variants.insert("Ready".into(), 2);
            }
            "flags" => changed.flags = true,
            _ => changed.variants = BTreeMap::from([("Other".into(), 1)]),
        }
        let installed = [enum_definition("host::State".into(), &changed)];
        let mut codec = NativeRecordCodec::with_definitions(&[], &installed);
        assert!(
            codec
                .into_native(value, storage.layout(&ty).unwrap())
                .is_err(),
            "{change}"
        );
    }
}

#[test]
fn portable_enum_schema_is_checked_and_linking_keeps_trait_tables_independent() {
    let signature = signature(true);
    let installed = enum_definition("host::State".into(), &signature);
    let mut portable = installed.as_ref().clone();
    portable.host_definition = None;
    portable.implemented_traits.get_mut().insert("Eq".into());
    let linked = link_enum_declaration(&portable, &installed).unwrap();
    assert_eq!(linked.host_definition, Some(signature));
    assert!(linked.implemented_traits.borrow().contains("Eq"));
    assert!(linked.implemented_traits.borrow().contains("Copy"));
    linked.implemented_traits.borrow_mut().insert("Hash".into());
    assert!(!portable.implemented_traits.borrow().contains("Hash"));
    assert!(!installed.implemented_traits.borrow().contains("Eq"));
    for change in ["name", "tag", "payload", "count"] {
        let mut invalid = portable.clone();
        match change {
            "name" => invalid.name = "host::Other".into(),
            "tag" => {
                let EnumVariant::Unit { name, .. } = &mut invalid.variants[0] else {
                    unreachable!()
                };
                *name = "Other".into();
            }
            "payload" => {
                let EnumVariant::Tuple { fields, .. } = &mut invalid.variants[1] else {
                    unreachable!()
                };
                fields[0] = Type::Bool;
            }
            _ => {
                invalid.variants.pop();
            }
        }
        assert!(
            link_enum_declaration(&invalid, &installed).is_err(),
            "{change}"
        );
    }
}
