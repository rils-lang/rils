//! An owning cursor whose consumers do not depend on the backing allocation.
use super::{DynamicLayout, DynamicValue, DynamicValueRef};
use std::rc::Rc;

enum Storage {
    Sequence(std::vec::IntoIter<DynamicValue>),
    Array {
        value: DynamicValue,
        next: usize,
        length: usize,
    },
}

pub struct OwnedElements {
    storage: Storage,
    item: Rc<DynamicLayout>,
}

impl OwnedElements {
    /// Input values are transferred once; remaining storage is private to the cursor.
    pub fn from_values(
        item: Rc<DynamicLayout>,
        values: impl IntoIterator<Item = DynamicValue>,
    ) -> Result<Self, String> {
        let values: Vec<_> = values.into_iter().collect();
        Self::validate(&item, &values)?;
        Ok(Self::from_validated(item, values))
    }

    pub(super) fn validate(item: &DynamicLayout, values: &[DynamicValue]) -> Result<(), String> {
        for value in values {
            if !item.compatible_with(value.descriptor()) {
                return Err("cursor element has a different layout".into());
            }
            if value.view().is_partially_moved()? {
                return Err("cannot iterate partially moved native elements".into());
            }
            if value.has_path_references() {
                return Err("cannot transfer referenced cursor elements".into());
            }
        }
        Ok(())
    }

    pub(super) fn from_validated(item: Rc<DynamicLayout>, values: Vec<DynamicValue>) -> Self {
        Self {
            item,
            storage: Storage::Sequence(values.into_iter()),
        }
    }

    pub fn from_array(value: DynamicValue, item: Rc<DynamicLayout>) -> Result<Self, String> {
        let rils_syntax::Type::Array { length, element } = value.descriptor().rils_type() else {
            return Err("cursor requires array storage".into());
        };
        let length = *length;
        if element.as_ref() != item.rils_type() {
            return Err("array element type differs from cursor layout".into());
        }
        for index in 0..length {
            if !value.view().field(index)?.layout()?.compatible_with(&item) {
                return Err("array element has a different layout".into());
            }
        }
        if value.has_path_references() {
            return Err("cannot iterate a referenced array".into());
        }
        if value.view().is_partially_moved()? {
            return Err("cannot iterate a partially moved array".into());
        }
        Ok(Self {
            item,
            storage: Storage::Array {
                value,
                next: 0,
                length,
            },
        })
    }

    pub fn item_layout(&self) -> &Rc<DynamicLayout> {
        &self.item
    }

    pub fn any_remaining(
        &self,
        mut predicate: impl FnMut(DynamicValueRef<'_>) -> Result<bool, String>,
    ) -> Result<bool, String> {
        match &self.storage {
            Storage::Sequence(items) => {
                for item in items.as_slice() {
                    if predicate(item.view())? {
                        return Ok(true);
                    }
                }
            }
            Storage::Array {
                value,
                next,
                length,
            } => {
                for index in *next..*length {
                    if predicate(value.view().field(index)?)? {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    pub fn has_active_references(&self) -> bool {
        match &self.storage {
            Storage::Sequence(items) => items
                .as_slice()
                .iter()
                .any(DynamicValue::has_path_references),
            Storage::Array { value, .. } => value.has_path_references(),
        }
    }
}

impl Iterator for OwnedElements {
    type Item = Result<DynamicValue, String>;
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.storage {
            Storage::Sequence(items) => items.next().map(Ok),
            Storage::Array {
                value,
                next,
                length,
            } => {
                if *next == *length {
                    return None;
                }
                let result = value.take_field(*next);
                *next += 1;
                Some(result)
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let length = match &self.storage {
            Storage::Sequence(items) => items.len(),
            Storage::Array { next, length, .. } => length - next,
        };
        (length, Some(length))
    }
}
impl ExactSizeIterator for OwnedElements {}
impl std::iter::FusedIterator for OwnedElements {}
