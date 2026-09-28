use std::rc::Rc;

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue};

pub type LayoutResolver<'a> = dyn FnMut(&Type) -> Result<Rc<DynamicLayout>, String> + 'a;
pub type LayoutFactory =
    for<'a> fn(&Type, &'a mut LayoutResolver<'a>) -> Option<Result<Rc<DynamicLayout>, String>>;

/// A layout factory emitted beside the standard-library type definition.
pub struct LayoutRegistration {
    pub matches: fn(&Type) -> bool,
    pub layout: LayoutFactory,
}

/// How one non-Copy native element can be read through a Rils reference.
pub struct ElementRegistration {
    pub matches: fn(&Type) -> bool,
    pub clone_borrowed: fn(&DynamicValue) -> Result<DynamicValue, String>,
}

/// Immutable registration table. It stores function pointers, not runtime values.
pub struct NativeRegistry {
    layouts: &'static [LayoutRegistration],
    elements: &'static [ElementRegistration],
}

impl NativeRegistry {
    pub const fn new(
        layouts: &'static [LayoutRegistration],
        elements: &'static [ElementRegistration],
    ) -> Self {
        Self { layouts, elements }
    }

    pub fn layout(
        &self,
        ty: &Type,
        resolve: &mut LayoutResolver<'_>,
    ) -> Option<Result<Rc<DynamicLayout>, String>> {
        self.layouts
            .iter()
            .find(|registration| (registration.matches)(ty))
            .and_then(|registration| (registration.layout)(ty, resolve))
    }

    pub fn can_read_element(&self, layout: &DynamicLayout) -> bool {
        layout.is_copy()
            || self
                .elements
                .iter()
                .any(|registration| (registration.matches)(layout.rils_type()))
    }

    pub fn clone_borrowed_element(&self, item: &DynamicValue) -> Result<DynamicValue, String> {
        if item.descriptor().is_copy() {
            return item.copy_owned();
        }
        let ty = item.descriptor().rils_type();
        let registration = self
            .elements
            .iter()
            .find(|registration| (registration.matches)(ty))
            .ok_or_else(|| format!("native element {ty} cannot be read through a reference"))?;
        (registration.clone_borrowed)(item)
    }
}
