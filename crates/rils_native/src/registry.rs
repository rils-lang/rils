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

/// A native leaf's immutable identity for hash and ordered collections.
pub struct KeyRegistration {
    pub matches: fn(&Type) -> bool,
    pub key: fn(DynamicValueRef<'_>) -> Result<crate::NativeKey, String>,
}

/// Immutable registration table. It stores function pointers, not runtime values.
pub struct NativeRegistry {
    layouts: &'static [LayoutRegistration],
    elements: &'static [ElementRegistration],
    pub(crate) keys: &'static [KeyRegistration],
    pub(crate) formats: &'static [crate::FormatRegistration],
}

impl NativeRegistry {
    pub const fn new(
        layouts: &'static [LayoutRegistration],
        elements: &'static [ElementRegistration],
    ) -> Self {
        Self {
            layouts,
            elements,
            keys: &[],
            formats: &[],
        }
    }

    pub const fn with_keys(
        layouts: &'static [LayoutRegistration],
        elements: &'static [ElementRegistration],
        keys: &'static [KeyRegistration],
    ) -> Self {
        Self {
            layouts,
            elements,
            keys,
            formats: &[],
        }
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

    pub const fn with_formats(mut self, formats: &'static [crate::FormatRegistration]) -> Self {
        self.formats = formats;
        self
    }

    pub fn clone_borrowed_element(&self, item: &DynamicValue) -> Result<DynamicValue, String> {
        self.clone_borrowed_view(item.view())
    }

    pub fn clone_borrowed_view(&self, view: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
        self.clone_borrowed_view_with(view, &mut |_| None)
    }

    /// Allow an execution context to supply additional registered leaf reads.
    /// The callback is consulted recursively without introducing runtime
    /// dependencies into the native registry.
    pub fn clone_borrowed_view_with(
        &self,
        view: DynamicValueRef<'_>,
        leaf: &mut dyn FnMut(&DynamicValueRef<'_>) -> Option<Result<DynamicValue, String>>,
    ) -> Result<DynamicValue, String> {
        let layout = view.layout()?;
        if layout.is_copy() {
            return view.copy_owned();
        }
        if layout.option_item().is_some() {
            return if view.option_is_some()? {
                DynamicValue::some(
                    layout,
                    self.clone_borrowed_view_with(view.option_item()?, leaf)?,
                )
            } else {
                DynamicValue::none(layout)
            };
        }
        if layout.variant_alternatives().is_some() {
            let index = view.variant_index()?;
            let item = self.clone_borrowed_view_with(view.variant_payload()?, leaf)?;
            return DynamicValue::variant(layout, index, item);
        }
        if let Some(fields) = layout.record_fields() {
            let values = (0..fields.len())
                .map(|index| self.clone_borrowed_view_with(view.field(index)?, leaf))
                .collect::<Result<Vec<_>, _>>()?;
            return DynamicValue::record(layout, values);
        }
        if layout.sequence_item().is_some() {
            let values = (0..view.sequence_len()?)
                .map(|index| self.clone_borrowed_view_with(view.sequence_item(index)?, leaf))
                .collect::<Result<Vec<_>, _>>()?;
            return DynamicValue::sequence(layout, values);
        }
        if let Some(value) = leaf(&view) {
            return value;
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
