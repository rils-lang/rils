//! Interior-mutable value cell.

use rils_stdlib_macros::decl_rils;

#[decl_rils(core::cell)]
mod native {
    /// A single-threaded interior-mutable value cell.
    #[rils_struct]
    pub struct Cell<T>(std::cell::Cell<T>);

    impl<T> std::ops::Deref for Cell<T> {
        type Target = std::cell::Cell<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for Cell<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> Cell<T> {
        /// Creates a cell containing a value.
        #[export_rils]
        #[rils_native_bridge]
        pub fn new(value: T) -> Self {
            Self(std::cell::Cell::new(value))
        }

        /// Copies the current value.
        #[export_rils]
        #[rils_native_bridge]
        pub fn get(&self) -> T
        where
            T: Copy,
        {
            self.0.get()
        }

        /// Replaces the current value.
        #[export_rils]
        #[rils_native_bridge]
        pub fn set(&self, value: T) {
            self.0.set(value);
        }

        /// Replaces and returns the previous value.
        #[export_rils]
        #[rils_native_bridge]
        pub fn replace(&self, value: T) -> T {
            self.0.replace(value)
        }
    }

    /// A dynamically borrow-checked interior-mutable value.
    #[rils_struct]
    pub struct RefCell<T>(std::cell::RefCell<T>);

    impl<T> std::ops::Deref for RefCell<T> {
        type Target = std::cell::RefCell<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for RefCell<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> RefCell<T> {
        /// Creates a dynamically checked cell.
        #[export_rils]
        #[rils_native_bridge]
        pub fn new(value: T) -> Self {
            Self(std::cell::RefCell::new(value))
        }

        /// Borrows the contained value for reading.
        #[export_rils]
        #[rils_native_bridge]
        #[rils_return(&T)]
        pub fn borrow(&self) -> std::cell::Ref<'_, T> {
            self.0.borrow()
        }

        /// Borrows the contained value for writing.
        #[export_rils]
        #[rils_native_bridge]
        #[rils_return(&mut T)]
        pub fn borrow_mut(&self) -> std::cell::RefMut<'_, T> {
            self.0.borrow_mut()
        }

        /// Replaces and returns the previous value.
        #[export_rils]
        #[rils_native_bridge]
        pub fn replace(&self, value: T) -> T {
            self.0.replace(value)
        }

        pub fn get(&mut self) -> &mut T {
            self.0.get_mut()
        }
    }
}

pub use native::{Cell, RefCell};

#[doc(hidden)]
pub struct ErasedCell(pub std::cell::RefCell<rils_value::DynamicValue>);

#[doc(hidden)]
pub struct ErasedRefCell {
    pub value: rils_value::DynamicObject<()>,
    pub references: std::cell::Cell<usize>,
}

fn cell_matches(ty: &rils_syntax::Type) -> bool {
    matches!(ty, rils_syntax::Type::Named { name, arguments } if name == "Cell" && arguments.len() == 1)
}

fn cell_layout(
    ty: &rils_syntax::Type,
    resolve: &mut rils_native::LayoutResolver<'_>,
) -> std::option::Option<Result<std::rc::Rc<rils_value::DynamicLayout>, std::string::String>> {
    let rils_syntax::Type::Named { arguments, .. } = ty else {
        return std::option::Option::None;
    };
    cell_matches(ty).then(|| {
        resolve(&arguments[0]).map(|_| rils_value::DynamicLayout::of::<ErasedCell>(ty.clone()))
    })
}

pub const NATIVE_LAYOUT_CELL: rils_native::LayoutRegistration = rils_native::LayoutRegistration {
    matches: cell_matches,
    layout: cell_layout,
};

fn ref_cell_matches(ty: &rils_syntax::Type) -> bool {
    matches!(ty, rils_syntax::Type::Named { name, arguments } if name == "RefCell" && arguments.len() == 1)
}

fn ref_cell_layout(
    ty: &rils_syntax::Type,
    resolve: &mut rils_native::LayoutResolver<'_>,
) -> std::option::Option<Result<std::rc::Rc<rils_value::DynamicLayout>, std::string::String>> {
    let rils_syntax::Type::Named { arguments, .. } = ty else {
        return std::option::Option::None;
    };
    ref_cell_matches(ty).then(|| {
        resolve(&arguments[0]).map(|_| rils_value::DynamicLayout::of::<ErasedRefCell>(ty.clone()))
    })
}

pub const NATIVE_LAYOUT_REFCELL: rils_native::LayoutRegistration =
    rils_native::LayoutRegistration {
        matches: ref_cell_matches,
        layout: ref_cell_layout,
    };
