//! Runtime-composed layouts for generic native payloads.

use std::{alloc::Layout, any::TypeId, ptr, rc::Rc};

use rils_syntax::Type;

use crate::storage::RawStorage;

mod object;
mod operations;
mod path;
mod record;
mod sequence;
mod variant;
pub use object::DynamicObject;
pub use operations::{DynamicCallContext, DynamicType};
pub use path::DynamicPathStep;
pub use record::DynamicField;
pub use sequence::{SequenceBorrowLedger, SequenceItemLease, SequenceIteratorLease};

use record::RecordLayout;
use variant::VariantLayout;

enum DropKind {
    Rust {
        type_id: TypeId,
        drop_value: unsafe fn(*mut u8),
    },
    Option {
        item: Rc<DynamicLayout>,
        item_offset: usize,
    },
    Record(RecordLayout),
    Variant(VariantLayout),
    Sequence {
        item: Rc<DynamicLayout>,
    },
}

/// A concrete Rust layout or a composite layout assembled from runtime types.
/// Optional values use an explicit byte tag followed by an aligned payload;
/// this is Rils' layout and does not assume Rust's `Option<T>` ABI.
pub struct DynamicLayout {
    rils_type: Type,
    layout: Layout,
    copy: bool,
    drop_kind: DropKind,
}

impl DynamicLayout {
    pub fn of<T: 'static>(rils_type: Type) -> Rc<Self> {
        Rc::new(Self {
            rils_type,
            layout: Layout::new::<T>(),
            copy: false,
            drop_kind: DropKind::Rust {
                type_id: TypeId::of::<T>(),
                drop_value: drop_rust::<T>,
            },
        })
    }

    pub fn copy_of<T: Copy + 'static>(rils_type: Type) -> Rc<Self> {
        let mut layout = Self::of::<T>(rils_type);
        Rc::get_mut(&mut layout)
            .expect("new layout has one owner")
            .copy = true;
        layout
    }

    pub fn option(item: Rc<Self>) -> Result<Rc<Self>, String> {
        let (layout, item_offset) = Layout::new::<u8>()
            .extend(item.layout)
            .map_err(|_| "optional native layout exceeds the address space".to_owned())?;
        Ok(Rc::new(Self {
            rils_type: Type::Option(Box::new(item.rils_type.clone())),
            layout: layout.pad_to_align(),
            copy: item.copy,
            drop_kind: DropKind::Option { item, item_offset },
        }))
    }

    pub fn rils_type(&self) -> &Type {
        &self.rils_type
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn is_copy(&self) -> bool {
        self.copy
    }

    pub fn option_item(&self) -> Option<&Rc<Self>> {
        match &self.drop_kind {
            DropKind::Option { item, .. } => Some(item),
            _ => None,
        }
    }

    /// Whether two independently constructed descriptors describe the same
    /// initialized bytes and destructor behavior. Runtime layout caches may
    /// have different owners, so pointer identity alone is insufficient.
    pub fn compatible_with(&self, other: &Self) -> bool {
        if self.rils_type != other.rils_type
            || self.layout != other.layout
            || self.copy != other.copy
        {
            return false;
        }
        match (&self.drop_kind, &other.drop_kind) {
            (DropKind::Rust { type_id: left, .. }, DropKind::Rust { type_id: right, .. }) => {
                left == right
            }
            (
                DropKind::Option {
                    item: left,
                    item_offset: left_offset,
                },
                DropKind::Option {
                    item: right,
                    item_offset: right_offset,
                },
            ) => left_offset == right_offset && left.compatible_with(right),
            (DropKind::Record(left), DropKind::Record(right)) => {
                left.fields.len() == right.fields.len()
                    && left.fields.iter().zip(&right.fields).all(|(left, right)| {
                        left.name() == right.name()
                            && left.offset() == right.offset()
                            && left.layout().compatible_with(right.layout())
                    })
            }
            (DropKind::Variant(left), DropKind::Variant(right)) => {
                left.payload_offset == right.payload_offset
                    && left.alternatives.len() == right.alternatives.len()
                    && left
                        .alternatives
                        .iter()
                        .zip(&right.alternatives)
                        .all(|(left, right)| left.compatible_with(right))
            }
            (DropKind::Sequence { item: left }, DropKind::Sequence { item: right }) => {
                left.compatible_with(right)
            }
            _ => false,
        }
    }

    unsafe fn drop_value(&self, pointer: *mut u8) {
        match &self.drop_kind {
            DropKind::Rust { drop_value, .. } => {
                // SAFETY: the owning DynamicValue initialized this exact layout.
                unsafe { drop_value(pointer) };
            }
            DropKind::Option { item, item_offset } => {
                // SAFETY: the option constructor writes a valid tag before the
                // value becomes initialized. Tag 1 means the child is live.
                if unsafe { ptr::read(pointer) } == 1 {
                    // SAFETY: extend calculated this aligned child offset.
                    unsafe { item.drop_value(pointer.add(*item_offset)) };
                }
            }
            DropKind::Record(record) => {
                // SAFETY: an initialized record has one live tag per field.
                unsafe { record.drop_value(pointer) };
            }
            DropKind::Variant(variant) => {
                // SAFETY: the constructor wrote a valid tag and payload.
                unsafe { variant.drop_value(pointer) };
            }
            DropKind::Sequence { .. } => {
                // SAFETY: a sequence always initializes one SequenceStorage.
                unsafe { ptr::drop_in_place(pointer.cast::<sequence::SequenceStorage>()) };
            }
        }
    }
}

/// One owned value in a runtime-composed layout. Moving a child into an option
/// transfers its initialization state, so every Rust destructor runs once.
pub struct DynamicValue {
    descriptor: Rc<DynamicLayout>,
    storage: RawStorage,
    initialized: bool,
}

impl DynamicValue {
    pub fn from_rust<T: 'static>(descriptor: Rc<DynamicLayout>, value: T) -> Result<Self, String> {
        if !matches!(
            descriptor.drop_kind,
            DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>()
        ) {
            return Err(format!(
                "native value does not match {}",
                descriptor.rils_type
            ));
        }
        let mut result = Self::uninitialized(descriptor);
        // SAFETY: the descriptor was checked against T and storage has T's layout.
        unsafe { ptr::write(result.storage.pointer_mut().cast::<T>(), value) };
        result.initialized = true;
        Ok(result)
    }

    pub fn none(descriptor: Rc<DynamicLayout>) -> Result<Self, String> {
        if !matches!(descriptor.drop_kind, DropKind::Option { .. }) {
            return Err("None requires an optional layout".into());
        }
        // An absent value has no live child and needs only the tag byte.
        let mut result = Self {
            descriptor,
            storage: RawStorage::new(Layout::new::<u8>(), true),
            initialized: false,
        };
        // SAFETY: the first byte of every optional layout is its tag.
        unsafe { ptr::write(result.storage.pointer_mut(), 0) };
        result.initialized = true;
        Ok(result)
    }

    pub fn some(descriptor: Rc<DynamicLayout>, mut item: Self) -> Result<Self, String> {
        let DropKind::Option {
            item: expected,
            item_offset,
        } = &descriptor.drop_kind
        else {
            return Err("Some requires an optional layout".into());
        };
        if !expected.compatible_with(&item.descriptor) {
            return Err("optional item layout does not match its descriptor".into());
        }
        let mut result = Self::uninitialized(descriptor.clone());
        // SAFETY: the child has the exact descriptor used by Layout::extend.
        // Copying these bytes transfers ownership; clear the source's live bit
        // before its RawStorage is released.
        unsafe {
            ptr::copy_nonoverlapping(
                item.storage.pointer(),
                result.storage.pointer_mut().add(*item_offset),
                item.storage.size(),
            );
            ptr::write(result.storage.pointer_mut(), 1);
        }
        item.initialized = false;
        result.initialized = true;
        Ok(result)
    }

    pub fn is_some(&self) -> Result<bool, String> {
        if !matches!(self.descriptor.drop_kind, DropKind::Option { .. }) {
            return Err("value is not optional".into());
        }
        // SAFETY: optional values are initialized with tag 0 or 1.
        Ok(unsafe { ptr::read(self.storage.pointer()) == 1 })
    }

    /// Borrow the live item of an optional layout after checking its Rust type.
    pub fn with_option<T: 'static, R>(&self, f: impl FnOnce(Option<&T>) -> R) -> Result<R, String> {
        let DropKind::Option { item, item_offset } = &self.descriptor.drop_kind else {
            return Err("value is not optional".into());
        };
        if !matches!(item.drop_kind, DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>())
        {
            return Err(format!(
                "optional item is not {}",
                std::any::type_name::<T>()
            ));
        }
        if !self.is_some()? {
            return Ok(f(None));
        }
        // SAFETY: the option tag is 1, the child layout was checked against T,
        // and Layout::extend established the aligned child offset.
        Ok(f(Some(unsafe {
            &*self.storage.pointer().add(*item_offset).cast::<T>()
        })))
    }

    pub fn take_option(mut self) -> Result<Option<Self>, String> {
        let DropKind::Option { item, item_offset } = &self.descriptor.drop_kind else {
            return Err("value is not optional".into());
        };
        if !self.is_some()? {
            return Ok(None);
        }
        let mut child = Self::uninitialized(item.clone());
        // SAFETY: tag 1 guarantees an initialized child at the aligned offset.
        // Set tag 0 after transferring bytes to avoid a second drop.
        unsafe {
            ptr::copy_nonoverlapping(
                self.storage.pointer().add(*item_offset),
                child.storage.pointer_mut(),
                item.layout.size(),
            );
            ptr::write(self.storage.pointer_mut(), 0);
        }
        child.initialized = true;
        Ok(Some(child))
    }

    pub fn with<T: 'static, R>(&self, f: impl FnOnce(&T) -> R) -> Result<R, String> {
        if !matches!(
            self.descriptor.drop_kind,
            DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>()
        ) {
            return Err(format!(
                "native value is not {}",
                std::any::type_name::<T>()
            ));
        }
        // SAFETY: descriptor identity was checked and the value is initialized.
        Ok(unsafe { f(&*self.storage.pointer().cast::<T>()) })
    }

    pub fn with_mut<T: 'static, R>(&mut self, f: impl FnOnce(&mut T) -> R) -> Result<R, String> {
        if !matches!(
            self.descriptor.drop_kind,
            DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>()
        ) {
            return Err(format!(
                "native value is not {}",
                std::any::type_name::<T>()
            ));
        }
        // SAFETY: descriptor identity was checked and &mut self gives
        // exclusive access to the initialized Rust value.
        Ok(unsafe { f(&mut *self.storage.pointer_mut().cast::<T>()) })
    }

    /// Consume one concrete Rust leaf without cloning its payload.
    pub fn into_rust<T: 'static>(mut self) -> Result<T, (Self, String)> {
        if !matches!(
            self.descriptor.drop_kind,
            DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>()
        ) {
            return Err((
                self,
                format!("native value is not {}", std::any::type_name::<T>()),
            ));
        }
        // SAFETY: the descriptor checked T and this value owns the initialized
        // payload. Clearing the live bit transfers its sole destructor to T.
        let value = unsafe { ptr::read(self.storage.pointer().cast::<T>()) };
        self.initialized = false;
        Ok(value)
    }

    pub fn copy_owned(&self) -> Result<Self, String> {
        if !self.descriptor.copy {
            return Err(format!("{} is not Copy", self.descriptor.rils_type));
        }
        Ok(Self {
            descriptor: self.descriptor.clone(),
            // SAFETY: DynamicLayout::copy is set only for T: Copy or for an
            // optional layout whose child has the same guarantee.
            storage: unsafe { self.storage.copy_bytes(true) },
            initialized: true,
        })
    }

    pub fn descriptor(&self) -> &DynamicLayout {
        &self.descriptor
    }

    pub fn layout_handle(&self) -> Rc<DynamicLayout> {
        self.descriptor.clone()
    }

    pub fn is_inline(&self) -> bool {
        self.storage.is_inline()
    }

    pub(crate) fn has_layout(&self, layout: &Rc<DynamicLayout>) -> bool {
        self.descriptor.compatible_with(layout)
    }

    fn uninitialized(descriptor: Rc<DynamicLayout>) -> Self {
        Self {
            storage: RawStorage::new(descriptor.layout, descriptor.copy),
            descriptor,
            initialized: false,
        }
    }
}

impl Drop for DynamicValue {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: initialized is set only after writing this exact layout.
            unsafe { self.descriptor.drop_value(self.storage.pointer_mut()) };
        }
    }
}

unsafe fn drop_rust<T>(pointer: *mut u8) {
    // SAFETY: the caller guarantees one initialized T at this address.
    unsafe { ptr::drop_in_place(pointer.cast::<T>()) };
}
