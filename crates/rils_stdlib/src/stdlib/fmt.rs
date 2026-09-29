//! Formatting contracts and the temporary formatting destination.

use rils_stdlib_macros::decl_rils;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Default)]
pub struct FormatterBuffer {
    output: RefCell<std::string::String>,
    alternate: Cell<bool>,
    depth: Cell<usize>,
}

impl FormatterBuffer {
    pub fn new(alternate: bool) -> Self {
        Self {
            output: RefCell::new(std::string::String::new()),
            alternate: Cell::new(alternate),
            depth: Cell::new(0),
        }
    }

    pub fn write_str(&self, value: &str) {
        self.output.borrow_mut().push_str(value);
    }

    pub fn finish(&self) -> std::string::String {
        self.output.borrow().clone()
    }

    pub fn alternate(&self) -> bool {
        self.alternate.get()
    }

    pub fn depth(&self) -> usize {
        self.depth.get()
    }

    pub fn set_depth(&self, depth: usize) {
        self.depth.set(depth);
    }
}

#[decl_rils(core::fmt)]
mod native {
    use super::super::{basic::FormatError, result::Result, string::String};

    /// Diagnostic textual formatting.
    #[rils_trait]
    pub trait Debug: ::std::fmt::Debug {
        /// Writes the diagnostic representation into a formatter.
        fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError>;
    }

    /// User-facing textual formatting.
    #[rils_trait]
    pub trait Display: ::std::fmt::Display {
        /// Writes the user-facing representation into a formatter.
        fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError>;
    }

    /// A transient formatting destination supplied by format macros.
    #[rils_struct]
    #[derive(Default)]
    pub struct Formatter(std::rc::Rc<super::FormatterBuffer>);

    impl Formatter {
        pub fn from_buffer(buffer: std::rc::Rc<super::FormatterBuffer>) -> Self {
            Self(buffer)
        }

        pub fn buffer(&self) -> std::rc::Rc<super::FormatterBuffer> {
            self.0.clone()
        }

        /// Appends text to this formatting destination.
        #[export_rils]
        #[rils_legacy_id(core::fmt::write_str)]
        pub fn write_str(&mut self, value: String) -> Result<(), FormatError> {
            self.0.write_str(&std::string::String::from(value));
            Result::Ok(())
        }

        /// Writes the structural Debug representation used by derived implementations.
        #[export_rils]
        #[rils_legacy_id(core::fmt::write_derived_debug)]
        pub fn write_derived_debug<T: std::fmt::Debug>(
            &mut self,
            value: &T,
        ) -> Result<(), FormatError> {
            self.0.write_str(&format!("{value:?}"));
            Result::Ok(())
        }
    }
}

pub use native::{Debug, Display, Formatter};

fn formatter_matches(ty: &rils_syntax::Type) -> bool {
    matches!(ty, rils_syntax::Type::Named { name, arguments } if name == "Formatter" && arguments.is_empty())
}

fn formatter_layout(
    ty: &rils_syntax::Type,
    _resolve: &mut rils_native::LayoutResolver<'_>,
) -> std::option::Option<Result<Rc<rils_value::DynamicLayout>, std::string::String>> {
    formatter_matches(ty).then(|| Ok(rils_value::DynamicLayout::of::<Formatter>(ty.clone())))
}

pub const NATIVE_LAYOUT_FORMATTER: rils_native::LayoutRegistration =
    rils_native::LayoutRegistration {
        matches: formatter_matches,
        layout: formatter_layout,
    };
