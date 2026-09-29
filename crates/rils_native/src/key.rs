//! Owned key identities extracted without materializing interpreter values.

use rils_syntax::{IntegerType, Type};
use rils_value::DynamicValueRef;

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
        let key = match layout.rils_type() {
            Type::Unit => view.with_rust::<(), _>(|_| NativeKey::Unit)?,
            Type::Bool => view.with_rust::<bool, _>(|value| NativeKey::Bool(*value))?,
            Type::Char => view.with_rust::<char, _>(|value| NativeKey::Char(*value))?,
            Type::Integer(kind) => integer_key(&view, *kind)?,
            ty => {
                let registration = self
                    .keys
                    .iter()
                    .find(|registration| (registration.matches)(ty))
                    .ok_or_else(|| format!("{ty} has no registered native key identity"))?;
                (registration.key)(view)?
            }
        };
        Ok(key)
    }
}

fn integer_key(view: &DynamicValueRef<'_>, kind: IntegerType) -> Result<NativeKey, String> {
    macro_rules! signed {
        ($ty:ty) => {
            view.with_rust::<$ty, _>(|value| NativeKey::Signed(*value as i128))
        };
    }
    macro_rules! unsigned {
        ($ty:ty) => {
            view.with_rust::<$ty, _>(|value| NativeKey::Unsigned(*value as u128))
        };
    }
    match kind {
        IntegerType::I8 => signed!(i8),
        IntegerType::I16 => signed!(i16),
        IntegerType::I32 => signed!(i32),
        IntegerType::I64 => signed!(i64),
        IntegerType::I128 => signed!(i128),
        IntegerType::Isize => signed!(isize),
        IntegerType::U8 => unsigned!(u8),
        IntegerType::U16 => unsigned!(u16),
        IntegerType::U32 => unsigned!(u32),
        IntegerType::U64 => unsigned!(u64),
        IntegerType::U128 => unsigned!(u128),
        IntegerType::Usize => unsigned!(usize),
    }
}
