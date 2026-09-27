//! Allocation and pointer access for erased Rust layouts.

use std::{
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
    mem::MaybeUninit,
    ptr::{self, NonNull},
};

const INLINE_CAPACITY: usize = 32;
const INLINE_ALIGNMENT: usize = 16;

#[repr(C, align(16))]
struct InlineBytes([MaybeUninit<u8>; INLINE_CAPACITY]);

enum Backing {
    Inline(InlineBytes),
    Heap {
        pointer: NonNull<u8>,
        allocation_layout: Layout,
    },
}

/// Raw bytes with the requested Rust layout. Only owners with a matching drop
/// operation may place initialized values in this storage.
pub(crate) struct RawStorage {
    backing: Backing,
    layout: Layout,
}

impl RawStorage {
    pub(crate) fn new(layout: Layout, inline: bool) -> Self {
        let backing =
            if inline && layout.size() <= INLINE_CAPACITY && layout.align() <= INLINE_ALIGNMENT {
                Backing::Inline(InlineBytes([MaybeUninit::uninit(); INLINE_CAPACITY]))
            } else {
                // A nonzero allocation also supplies a valid address for aligned ZSTs.
                let allocation_layout = if layout.size() == 0 {
                    Layout::from_size_align(1, layout.align()).expect("valid type alignment")
                } else {
                    layout
                };
                // SAFETY: allocation_layout is valid and has nonzero size.
                let pointer = unsafe { alloc(allocation_layout) };
                let pointer =
                    NonNull::new(pointer).unwrap_or_else(|| handle_alloc_error(allocation_layout));
                Backing::Heap {
                    pointer,
                    allocation_layout,
                }
            };
        Self { backing, layout }
    }

    /// # Safety
    /// The initialized value must permit a bytewise copy, and the destination
    /// owner must use the same drop operation as this source.
    pub(crate) unsafe fn copy_bytes(&self, inline: bool) -> Self {
        let mut copy = Self::new(self.layout, inline);
        // SAFETY: callers must establish that a byte copy is valid for the
        // initialized value. The regions have equal layouts and do not overlap.
        unsafe { ptr::copy_nonoverlapping(self.pointer(), copy.pointer_mut(), self.layout.size()) };
        copy
    }

    pub(crate) fn is_inline(&self) -> bool {
        matches!(self.backing, Backing::Inline(_))
    }

    pub(crate) fn size(&self) -> usize {
        self.layout.size()
    }

    pub(crate) fn pointer(&self) -> *const u8 {
        match &self.backing {
            Backing::Inline(bytes) => bytes.0.as_ptr().cast::<u8>(),
            Backing::Heap { pointer, .. } => pointer.as_ptr(),
        }
    }

    pub(crate) fn pointer_mut(&mut self) -> *mut u8 {
        match &mut self.backing {
            Backing::Inline(bytes) => bytes.0.as_mut_ptr().cast::<u8>(),
            Backing::Heap { pointer, .. } => pointer.as_ptr(),
        }
    }
}

impl Drop for RawStorage {
    fn drop(&mut self) {
        if let Backing::Heap {
            pointer,
            allocation_layout,
        } = &self.backing
        {
            // SAFETY: this pointer came from alloc(allocation_layout) above.
            unsafe { dealloc(pointer.as_ptr(), *allocation_layout) };
        }
    }
}

pub(crate) struct Payload {
    storage: RawStorage,
    drop_value: unsafe fn(*mut u8),
    initialized: bool,
}

impl Payload {
    pub(crate) fn new<T>(value: T, inline: bool) -> Self {
        let mut payload = Self {
            storage: RawStorage::new(Layout::new::<T>(), inline),
            drop_value: drop_t::<T>,
            initialized: false,
        };
        // SAFETY: storage is aligned and sized for T and currently uninitialized.
        unsafe { ptr::write(payload.storage.pointer_mut().cast::<T>(), value) };
        payload.initialized = true;
        payload
    }

    pub(crate) fn copy_value(&self, inline: bool) -> Self {
        assert!(self.initialized);
        Self {
            // SAFETY: NativeType can set Copy only through a T: Copy bound.
            storage: unsafe { self.storage.copy_bytes(inline) },
            drop_value: self.drop_value,
            initialized: true,
        }
    }

    pub(crate) fn is_inline(&self) -> bool {
        self.storage.is_inline()
    }

    pub(crate) fn with<T, R>(&self, f: impl FnOnce(&T) -> R) -> R {
        // SAFETY: NativeObject checked TypeId and this payload is initialized.
        unsafe { f(&*self.storage.pointer().cast::<T>()) }
    }

    pub(crate) fn with_mut<T, R>(&mut self, f: impl FnOnce(&mut T) -> R) -> R {
        // SAFETY: NativeObject checked TypeId and holds an exclusive borrow.
        unsafe { f(&mut *self.storage.pointer_mut().cast::<T>()) }
    }

    pub(crate) fn into_value<T>(mut self) -> T {
        assert!(self.initialized);
        // SAFETY: NativeObject checked the stored TypeId before consuming this
        // payload. Clearing the live bit transfers its sole destructor to T.
        let value = unsafe { ptr::read(self.storage.pointer().cast::<T>()) };
        self.initialized = false;
        value
    }
}

impl Drop for Payload {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: initialized is set only after writing a T or copying a
            // registered Copy value into matching storage.
            unsafe { (self.drop_value)(self.storage.pointer_mut()) };
        }
    }
}

unsafe fn drop_t<T>(pointer: *mut u8) {
    // SAFETY: the caller guarantees an initialized T at this address.
    unsafe { ptr::drop_in_place(pointer.cast::<T>()) };
}
