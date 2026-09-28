//! Native registrations collected from standard-library definitions.

use rils_native::NativeRegistry;

use crate::stdlib::{collections::vector, string};

static LAYOUTS: &[rils_native::LayoutRegistration] = &[vector::NATIVE_LAYOUT];
static ELEMENTS: &[rils_native::ElementRegistration] = &[string::NATIVE_ELEMENT];

static REGISTRY: NativeRegistry = NativeRegistry::new(LAYOUTS, ELEMENTS);

pub fn registry() -> &'static NativeRegistry {
    &REGISTRY
}
