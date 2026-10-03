//! Field and indexed element declarations outlive their owned payloads.

use std::rc::Rc;

use rils_value::DynamicType;

use super::{Value, storage};
use crate::types::Type;

#[derive(Clone)]
pub struct FieldSlot {
    pub value: Option<Value>,
    pub type_annotation: Type,
    pub references: usize,
    native_declaration: Option<Rc<DynamicType<Value>>>,
}

impl FieldSlot {
    /// Initializes a slot and retains the payload's registered declaration.
    pub fn new(type_annotation: Type, value: Value) -> Self {
        Self {
            native_declaration: storage::native_declaration(&value),
            value: Some(value),
            type_annotation,
            references: 0,
        }
    }

    /// Consumes a replacement without discarding a declaration after a move.
    /// Place mutability and reference permissions are checked by the caller.
    /// A failed conversion leaves this slot unchanged.
    pub fn assign(&mut self, value: Value) -> Result<(), String> {
        let value = storage::constrain_assignment(
            value,
            &self.type_annotation,
            self.native_declaration.clone(),
        )?;
        if let Some(declaration) = storage::native_declaration(&value) {
            self.native_declaration = Some(declaration);
        }
        self.value = Some(value);
        Ok(())
    }
}
