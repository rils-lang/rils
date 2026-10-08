//! Owned key identities extracted without materializing interpreter values.

use rils_value::{DynamicValueRef, NativeLeafRef};

use crate::NativeRegistry;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NativeKey {
    Unit,
    Bool(bool),
    Char(char),
    Signed(i128),
    Unsigned(u128),
    String(String),
    Fields(Vec<Self>),
    None,
    Some(Box<Self>),
    Variant(usize, Box<Self>),
}

impl NativeRegistry {
    /// Capture a key's immutable identity while the source remains borrowed.
    pub fn key(&self, view: DynamicValueRef<'_>) -> Result<NativeKey, String> {
        let layout = view.layout()?;
        if layout.option_item().is_some() {
            return if view.option_is_some()? {
                Ok(NativeKey::Some(Box::new(self.key(view.option_item()?)?)))
            } else {
                Ok(NativeKey::None)
            };
        }
        if layout.variant_alternatives().is_some() {
            let index = view.variant_index()?;
            return Ok(NativeKey::Variant(
                index,
                Box::new(self.key(view.variant_payload()?)?),
            ));
        }
        if let Some(fields) = layout.record_fields() {
            return (0..fields.len())
                .map(|index| self.key(view.field(index)?))
                .collect::<Result<Vec<_>, _>>()
                .map(NativeKey::Fields);
        }
        self.key_leaf(&view.leaf()?)
    }

    pub fn key_leaf(&self, leaf: &NativeLeafRef<'_>) -> Result<NativeKey, String> {
        self.leaf_key(leaf, false)
    }

    pub fn ordered_key(&self, view: DynamicValueRef<'_>) -> Result<NativeKey, String> {
        self.ordered_key_leaf(&view.leaf()?)
    }

    pub fn ordered_key_leaf(&self, leaf: &NativeLeafRef<'_>) -> Result<NativeKey, String> {
        self.leaf_key(leaf, true)
    }

    fn leaf_key(&self, leaf: &NativeLeafRef<'_>, ordered: bool) -> Result<NativeKey, String> {
        let ty = leaf.rils_type();
        let registration = self
            .keys
            .iter()
            .find(|registration| (registration.matches)(ty))
            .ok_or_else(|| format!("{ty} has no registered native key identity"))?;
        if ordered && !registration.ordered {
            return Err(format!("{ty} has no registered native key ordering"));
        }
        (registration.key)(leaf)
    }
}
