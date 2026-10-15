use rils_native::NativeKey;
use rils_stdlib::storage::{KeyPolicy, NativeServices};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue, DynamicValueRef};

struct Declarations<'a>(&'a [(&'a str, &'a [&'a str])]);
impl NativeServices for Declarations<'_> {
    fn implements(&self, ty: &Type, required: &str) -> bool {
        let Type::Named { name, .. } = ty else {
            return false;
        };
        self.0
            .iter()
            .any(|(known, traits)| name == known && traits.contains(&required))
    }
    fn clone_value(&self, _: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
        panic!("key identity must not clone its source")
    }
}

fn integer() -> DynamicValue {
    DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), 42_i32).unwrap()
}

fn record(name: &str, field: DynamicValue) -> DynamicValue {
    let layout = DynamicLayout::record(
        Type::named(name),
        vec![("value".into(), field.layout_handle())],
    )
    .unwrap();
    DynamicValue::record(layout, vec![field]).unwrap()
}

#[test]
fn hash_keys_require_both_nominal_traits_recursively() {
    for traits in [&[][..], &["Eq"][..], &["Hash"][..], &["Eq", "Hash"][..]] {
        let inner = record("Inner", integer());
        let optional =
            DynamicValue::some(DynamicLayout::option(inner.layout_handle()).unwrap(), inner)
                .unwrap();
        let outer = record("Outer", optional);
        let entries = [("Outer", &["Eq", "Hash"][..]), ("Inner", traits)];
        let result = KeyPolicy::Hash.identity(outer.view(), &Declarations(&entries));
        if traits.len() == 2 {
            assert_eq!(
                result.unwrap(),
                NativeKey::Fields(vec![NativeKey::Some(Box::new(NativeKey::Fields(vec![
                    NativeKey::Signed(42)
                ])))])
            );
        } else {
            assert!(
                result
                    .unwrap_err()
                    .contains("Inner cannot be used as a hash collection key: requires Eq + Hash")
            );
        }
    }
}

#[test]
fn named_enum_payload_needs_only_its_enums_nominal_traits() {
    let payload = record("Choice::Value", record("Inner", integer()));
    let layout = DynamicLayout::named_variant(
        Type::named("Choice"),
        vec![("Value".into(), payload.layout_handle())],
    )
    .unwrap();
    let value = DynamicValue::variant(layout, 0, payload).unwrap();
    let approved = Declarations(&[("Choice", &["Eq", "Hash"]), ("Inner", &["Eq", "Hash"])]);
    assert_eq!(
        KeyPolicy::Hash.identity(value.view(), &approved).unwrap(),
        NativeKey::Variant(
            0,
            Box::new(NativeKey::Fields(vec![NativeKey::Fields(vec![
                NativeKey::Signed(42)
            ])]))
        )
    );
    let missing_inner = Declarations(&[("Choice", &["Eq", "Hash"])]);
    assert!(
        KeyPolicy::Hash
            .identity(value.view(), &missing_inner)
            .unwrap_err()
            .contains("Inner")
    );
    let missing_enum = Declarations(&[("Inner", &["Eq", "Hash"])]);
    assert!(
        KeyPolicy::Hash
            .identity(value.view(), &missing_enum)
            .unwrap_err()
            .contains("Choice")
    );
}

#[test]
fn ordered_keys_keep_registry_leaf_restrictions() {
    let value = integer();
    assert_eq!(
        KeyPolicy::Ordered
            .identity(value.view(), &Declarations(&[]))
            .unwrap(),
        NativeKey::Signed(42)
    );
    let composite = record("Key", integer());
    assert!(
        KeyPolicy::Ordered
            .identity(
                composite.view(),
                &Declarations(&[("Key", &["Eq", "Hash", "Ord"])])
            )
            .is_err()
    );
}
