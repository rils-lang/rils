use std::rc::Rc;

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue, DynamicValueRef};

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
    pub clone_borrowed: fn(DynamicValueRef<'_>) -> Result<DynamicValue, String>,
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

    pub fn clone_borrowed_element(&self, item: &DynamicValue) -> Result<DynamicValue, String> {
        self.clone_borrowed_view(item.view())
    }

    pub fn clone_borrowed_view(&self, view: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
        let layout = view.layout()?;
        if layout.is_copy() {
            return view.copy_owned();
        }
        if layout.option_item().is_some() {
            return if view.option_is_some()? {
                DynamicValue::some(layout, self.clone_borrowed_view(view.option_item()?)?)
            } else {
                DynamicValue::none(layout)
            };
        }
        if layout.variant_alternatives().is_some() {
            let index = view.variant_index()?;
            let item = self.clone_borrowed_view(view.variant_payload()?)?;
            return DynamicValue::variant(layout, index, item);
        }
        if let Some(fields) = layout.record_fields() {
            let values = (0..fields.len())
                .map(|index| self.clone_borrowed_view(view.field(index)?))
                .collect::<Result<Vec<_>, _>>()?;
            return DynamicValue::record(layout, values);
        }
        if layout.sequence_item().is_some() {
            let values = (0..view.sequence_len()?)
                .map(|index| self.clone_borrowed_view(view.sequence_item(index)?))
                .collect::<Result<Vec<_>, _>>()?;
            return DynamicValue::sequence(layout, values);
        }
        let ty = layout.rils_type();
        let registration = self
            .elements
            .iter()
            .find(|registration| (registration.matches)(ty))
            .ok_or_else(|| format!("native element {ty} cannot be read through a reference"))?;
        (registration.clone_borrowed)(view)
    }
}
