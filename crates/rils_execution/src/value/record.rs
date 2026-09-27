//! Declaration-order field slots shared by the interpreter and bytecode VM.

use std::{collections::HashMap, ops::Index, rc::Rc};

use super::{FieldSlot, StructType};

#[derive(Clone)]
pub struct StructFields {
    definition: Rc<StructType>,
    slots: Vec<FieldSlot>,
}

impl StructFields {
    pub fn from_map(
        definition: Rc<StructType>,
        mut fields: HashMap<String, FieldSlot>,
    ) -> Result<Self, String> {
        if fields.len() != definition.fields.len() {
            return Err(format!(
                "struct `{}` requires {} fields, found {}",
                definition.name,
                definition.fields.len(),
                fields.len()
            ));
        }
        let slots = definition
            .fields
            .iter()
            .map(|field| {
                fields.remove(&field.name).ok_or_else(|| {
                    format!(
                        "struct `{}` is missing field `{}`",
                        definition.name, field.name
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { definition, slots })
    }

    pub fn get(&self, name: &str) -> Option<&FieldSlot> {
        self.definition
            .field_index(name)
            .and_then(|index| self.slots.get(index))
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut FieldSlot> {
        let index = self.definition.field_index(name)?;
        self.slots.get_mut(index)
    }

    pub fn get_index(&self, index: usize) -> Option<&FieldSlot> {
        self.slots.get(index)
    }

    pub fn get_index_mut(&mut self, index: usize) -> Option<&mut FieldSlot> {
        self.slots.get_mut(index)
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.definition.field_index(name).is_some()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &FieldSlot> {
        self.slots.iter()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &FieldSlot)> {
        self.definition
            .fields
            .iter()
            .zip(&self.slots)
            .map(|(field, slot)| (&field.name, slot))
    }
}

impl Index<&String> for StructFields {
    type Output = FieldSlot;

    fn index(&self, name: &String) -> &Self::Output {
        self.get(name)
            .unwrap_or_else(|| panic!("record has no field `{name}`"))
    }
}
