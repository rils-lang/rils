//! Collection algorithms over native layouts, independent of execution values.

pub mod map;
pub mod queue;
pub mod set;
pub mod vector;

use rils_value::{DynamicValue, DynamicValueRef};

/// Execution supplies leaf cloning so user-defined and host values keep their metadata.
pub type CloneValue<'a> = dyn FnMut(DynamicValueRef<'_>) -> Result<DynamicValue, String> + 'a;
