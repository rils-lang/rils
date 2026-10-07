//! Scoped, checked Rust access shared by standalone and composed native storage.

use std::{any::TypeId, marker::PhantomData, ptr::NonNull};

use rils_syntax::Type;

/// A read-only leaf whose owner remains borrowed for the entire access.
/// This view neither owns nor copies the payload and does not expose its pointer.
pub struct NativeLeafRef<'a> {
    pointer: NonNull<u8>,
    rust_type: TypeId,
    rils_type: Type,
    _borrow: PhantomData<&'a ()>,
}

impl<'a> NativeLeafRef<'a> {
    pub fn from_rust<T: 'static>(value: &'a T, rils_type: Type) -> Self {
        Self {
            pointer: NonNull::from(value).cast(),
            rust_type: TypeId::of::<T>(),
            rils_type,
            _borrow: PhantomData,
        }
    }

    /// # Safety
    /// The pointer must address an initialized value of `rust_type`, aligned
    /// for that type and kept alive and immutably borrowed for `'a`.
    pub(crate) unsafe fn from_raw(pointer: *const u8, rust_type: TypeId, rils_type: Type) -> Self {
        Self {
            pointer: NonNull::new(pointer.cast_mut()).expect("borrowed native storage is non-null"),
            rust_type,
            rils_type,
            _borrow: PhantomData,
        }
    }

    pub fn rils_type(&self) -> &Type {
        &self.rils_type
    }

    pub fn is_rust_type<T: 'static>(&self) -> bool {
        self.rust_type == TypeId::of::<T>()
    }

    pub fn with_rust<T: 'static, R>(&self, callback: impl FnOnce(&T) -> R) -> Result<R, String> {
        if !self.is_rust_type::<T>() {
            return Err(format!("native leaf is not {}", std::any::type_name::<T>()));
        }
        // SAFETY: constructors retain the owner's borrow; TypeId checks the
        // exact initialized Rust representation before forming this reference.
        Ok(unsafe { callback(self.pointer.cast::<T>().as_ref()) })
    }
}
