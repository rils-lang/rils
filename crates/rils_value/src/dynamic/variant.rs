//! Tagged native layouts for result and user enum payloads.

use std::{alloc::Layout, any::TypeId, ptr, rc::Rc};

use rils_syntax::Type;

use super::{DropKind, DynamicLayout, DynamicValue};

pub(super) struct VariantLayout {
    pub(super) alternatives: Vec<Rc<DynamicLayout>>,
    pub(super) payload_offset: usize,
}

impl VariantLayout {
    pub(super) unsafe fn drop_value(&self, pointer: *mut u8) {
        // SAFETY: a constructed variant always has a valid u32 discriminant.
        let index = unsafe { ptr::read(pointer.cast::<u32>()) } as usize;
        let child = &self.alternatives[index];
        // SAFETY: the payload was initialized with exactly this descriptor.
        unsafe { child.drop_value(pointer.add(self.payload_offset)) };
    }
}

impl DynamicLayout {
    /// Build a tagged union of concrete child layouts. The tag is a u32 and
    /// the payload starts at the maximum required alignment of all children.
    pub fn variant(ty: Type, alternatives: Vec<Rc<Self>>) -> Result<Rc<Self>, String> {
        if alternatives.is_empty() || alternatives.len() > u32::MAX as usize {
            return Err("variant layout requires between 1 and u32::MAX alternatives".into());
        }
        if let Type::Result(ok, error) = &ty {
            if alternatives.len() != 2
                || alternatives[0].rils_type() != ok.as_ref()
                || alternatives[1].rils_type() != error.as_ref()
            {
                return Err("result variant alternatives do not match their types".into());
            }
        } else if !matches!(ty, Type::Named { .. }) {
            return Err("variant layout requires a result or named enum type".into());
        }
        let payload_size = alternatives
            .iter()
            .map(|child| child.layout.size())
            .max()
            .expect("nonempty alternatives");
        let payload_align = alternatives
            .iter()
            .map(|child| child.layout.align())
            .max()
            .expect("nonempty alternatives");
        let payload = Layout::from_size_align(payload_size, payload_align)
            .map_err(|_| "variant payload exceeds the address space".to_owned())?;
        let (layout, payload_offset) = Layout::new::<u32>()
            .extend(payload)
            .map_err(|_| "variant layout exceeds the address space".to_owned())?;
        let copy = alternatives.iter().all(|child| child.copy);
        Ok(Rc::new(Self {
            rils_type: ty,
            layout: layout.pad_to_align(),
            copy,
            drop_kind: DropKind::Variant(VariantLayout {
                alternatives,
                payload_offset,
            }),
        }))
    }

    pub fn variant_alternatives(&self) -> Option<&[Rc<Self>]> {
        match &self.drop_kind {
            DropKind::Variant(variant) => Some(&variant.alternatives),
            _ => None,
        }
    }
}

impl DynamicValue {
    /// Move a payload into one alternative of a tagged native layout.
    pub fn variant(
        descriptor: Rc<DynamicLayout>,
        index: usize,
        mut payload: Self,
    ) -> Result<Self, String> {
        let DropKind::Variant(variant) = &descriptor.drop_kind else {
            return Err("variant constructor requires a variant layout".into());
        };
        let expected = variant
            .alternatives
            .get(index)
            .ok_or_else(|| format!("variant index {index} is out of bounds"))?;
        if !expected.compatible_with(&payload.descriptor) {
            return Err("variant payload has a different layout".into());
        }
        let mut result = Self::uninitialized(descriptor.clone());
        // SAFETY: the checked descriptor owns enough aligned payload space.
        // Clearing the source's live bit transfers its sole destructor.
        unsafe {
            ptr::copy_nonoverlapping(
                payload.storage.pointer(),
                result.storage.pointer_mut().add(variant.payload_offset),
                payload.storage.size(),
            );
            ptr::write(result.storage.pointer_mut().cast::<u32>(), index as u32);
        }
        payload.initialized = false;
        result.initialized = true;
        Ok(result)
    }

    pub fn variant_index(&self) -> Result<usize, String> {
        if !matches!(self.descriptor.drop_kind, DropKind::Variant(_)) {
            return Err("value is not a variant".into());
        }
        // SAFETY: every initialized variant has a valid u32 tag.
        Ok(unsafe { ptr::read(self.storage.pointer().cast::<u32>()) as usize })
    }

    /// Borrow the active variant when its payload is a registered Rust leaf.
    pub fn with_variant<T: 'static, R>(
        &self,
        index: usize,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        let DropKind::Variant(variant) = &self.descriptor.drop_kind else {
            return Err("value is not a variant".into());
        };
        if self.variant_index()? != index {
            return Err(format!("variant {index} is not active"));
        }
        let child = &variant.alternatives[index];
        if !matches!(child.drop_kind, DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>())
        {
            return Err(format!(
                "variant payload is not {}",
                std::any::type_name::<T>()
            ));
        }
        // SAFETY: the active tag selects an initialized T at the aligned offset.
        Ok(f(unsafe {
            &*self
                .storage
                .pointer()
                .add(variant.payload_offset)
                .cast::<T>()
        }))
    }

    /// Mutably borrow the active Rust payload through an exclusive parent.
    pub fn with_variant_mut<T: 'static, R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, String> {
        let DropKind::Variant(variant) = &self.descriptor.drop_kind else {
            return Err("value is not a variant".into());
        };
        if self.variant_index()? != index {
            return Err(format!("variant {index} is not active"));
        }
        let child = &variant.alternatives[index];
        if !matches!(child.drop_kind, DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>())
        {
            return Err(format!(
                "variant payload is not {}",
                std::any::type_name::<T>()
            ));
        }
        // SAFETY: the active tag selects T and &mut self covers the callback.
        Ok(f(unsafe {
            &mut *self
                .storage
                .pointer_mut()
                .add(variant.payload_offset)
                .cast::<T>()
        }))
    }

    /// Consume the variant and transfer ownership of its active payload.
    pub fn take_variant(mut self) -> Result<(usize, Self), String> {
        let DropKind::Variant(variant) = &self.descriptor.drop_kind else {
            return Err("value is not a variant".into());
        };
        let index = self.variant_index()?;
        let child = &variant.alternatives[index];
        let mut payload = Self::uninitialized(child.clone());
        // SAFETY: the tag selects the initialized child at the aligned offset.
        unsafe {
            ptr::copy_nonoverlapping(
                self.storage.pointer().add(variant.payload_offset),
                payload.storage.pointer_mut(),
                child.layout.size(),
            );
        }
        self.initialized = false;
        payload.initialized = true;
        Ok((index, payload))
    }
}
