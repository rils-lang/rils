//! Standard-library collection key requirements over generic native services.
use super::NativeServices;
use rils_native::NativeKey;
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue, DynamicValueRef, NativeLeafRef};

#[derive(Clone, Copy, Debug)]
pub enum KeyPolicy {
    Hash,
    Ordered,
}

impl KeyPolicy {
    pub fn identity(
        self,
        value: DynamicValueRef<'_>,
        services: &dyn NativeServices,
    ) -> Result<NativeKey, String> {
        let registry = crate::native::registry();
        match self {
            Self::Ordered => registry.ordered_key(value),
            Self::Hash => {
                validate(&value, services)?;
                registry.key(value)
            }
        }
    }

    pub fn leaf_identity(self, leaf: &NativeLeafRef<'_>) -> Result<NativeKey, String> {
        let registry = crate::native::registry();
        match self {
            Self::Hash => registry.key_leaf(leaf),
            Self::Ordered => registry.ordered_key_leaf(leaf),
        }
    }
}

/// A borrowed query keeps its own declaration context and never clones its payload.
pub trait KeyQuery {
    fn identity(&self, expected: &DynamicLayout, policy: KeyPolicy) -> Result<NativeKey, String>;
}

fn validate(view: &DynamicValueRef<'_>, services: &dyn NativeServices) -> Result<(), String> {
    let layout = view.layout()?;
    if matches!(layout.rils_type(), Type::Named { .. })
        && !(services.implements(layout.rils_type(), "Eq")
            && services.implements(layout.rils_type(), "Hash"))
    {
        return Err(format!(
            "{} cannot be used as a hash collection key: requires Eq + Hash",
            layout.rils_type()
        ));
    }
    if layout.option_item().is_some() {
        if view.option_is_some()? {
            validate(&view.option_item()?, services)?;
        }
    } else if layout.variant_alternatives().is_some() {
        let payload = view.variant_payload()?;
        // A named enum's payload record belongs to that declaration, and has no
        // independent nominal trait table.
        if matches!(layout.rils_type(), Type::Named { .. }) {
            validate_fields(&payload, services)?;
        } else {
            validate(&payload, services)?;
        }
    } else {
        validate_fields(view, services)?;
    }
    Ok(())
}

fn validate_fields(
    view: &DynamicValueRef<'_>,
    services: &dyn NativeServices,
) -> Result<(), String> {
    if let Some(fields) = view.layout()?.record_fields() {
        for index in 0..fields.len() {
            validate(&view.field(index)?, services)?;
        }
    }
    Ok(())
}

/// A query composed entirely of built-in leaves needs no nominal declaration table.
struct BuiltinServices;
impl NativeServices for BuiltinServices {
    fn implements(&self, _: &Type, _: &str) -> bool {
        false
    }
    fn clone_value(&self, _: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
        Err("key identity extraction does not clone values".into())
    }
}

impl KeyQuery for DynamicValueRef<'_> {
    fn identity(&self, expected: &DynamicLayout, policy: KeyPolicy) -> Result<NativeKey, String> {
        let actual = self.layout()?;
        if !expected.compatible_with(&actual) {
            return Err(format!(
                "collection key expects {}, found {}",
                expected.rils_type(),
                actual.rils_type()
            ));
        }
        policy.identity(self.clone(), &BuiltinServices)
    }
}
impl KeyQuery for DynamicValue {
    fn identity(&self, expected: &DynamicLayout, policy: KeyPolicy) -> Result<NativeKey, String> {
        self.view().identity(expected, policy)
    }
}
