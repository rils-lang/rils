//! Hold collection storage guards for the complete projected read.
use super::{HashKey, MapCollection, SetCollection};
use crate::value::FieldSlot;

impl MapCollection {
    pub(crate) fn with_entry<R>(
        &self,
        key: &HashKey,
        callback: impl for<'a> FnOnce(&'a HashKey, &'a FieldSlot) -> R,
    ) -> Result<R, String> {
        macro_rules! inspect {
            ($map:expr) => {{
                let entries = $map
                    .entries
                    .try_borrow()
                    .map_err(|_| "referenced map is already mutably accessed")?;
                let (key, slot) = entries
                    .get_key_value(key)
                    .ok_or("iterator map entry no longer exists")?;
                Ok(callback(key, slot))
            }};
        }
        match self {
            Self::Hash(map) => inspect!(map),
            Self::BTree(map) => inspect!(map),
        }
    }
}

impl SetCollection {
    pub(crate) fn with_item<R>(
        &self,
        key: &HashKey,
        callback: impl for<'a> FnOnce(&'a HashKey) -> R,
    ) -> Result<R, String> {
        macro_rules! inspect {
            ($set:expr) => {{
                let entries = $set
                    .entries
                    .try_borrow()
                    .map_err(|_| "referenced set is already mutably accessed")?;
                let key = entries
                    .get(key)
                    .ok_or("iterator set item no longer exists")?;
                Ok(callback(key))
            }};
        }
        match self {
            Self::Hash(set) => inspect!(set),
            Self::BTree(set) => inspect!(set),
        }
    }
}
