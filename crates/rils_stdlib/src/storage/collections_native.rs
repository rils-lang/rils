//! Dynamic implementations of collection capabilities used by the method bodies.
mod map;
mod set;

use super::{EntrySchema, KeyPolicy, NativeServices};
use rils_native::NativeKey;
use rils_value::DynamicValueRef;

fn identity(
    entry: DynamicValueRef<'_>,
    schema: &EntrySchema,
    policy: KeyPolicy,
    services: &dyn NativeServices,
) -> Result<NativeKey, String> {
    policy.identity(schema.project(entry, 0)?, services)
}

fn find(
    view: &DynamicValueRef<'_>,
    schema: &EntrySchema,
    key: &NativeKey,
    policy: KeyPolicy,
    services: &dyn NativeServices,
) -> Result<Option<usize>, String> {
    for index in 0..view.sequence_len()? {
        if identity(view.sequence_item(index)?, schema, policy, services)? == *key {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn insertion_index(
    view: &DynamicValueRef<'_>,
    schema: &EntrySchema,
    key: &NativeKey,
    policy: KeyPolicy,
    services: &dyn NativeServices,
) -> Result<usize, String> {
    if matches!(policy, KeyPolicy::Ordered) {
        for index in 0..view.sequence_len()? {
            if identity(view.sequence_item(index)?, schema, policy, services)? > *key {
                return Ok(index);
            }
        }
    }
    view.sequence_len()
}
