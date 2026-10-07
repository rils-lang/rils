//! Registered formatting of borrowed native views without copying their payloads.

use std::fmt;

use rils_value::{DynamicLayout, DynamicValueRef};

use crate::NativeRegistry;

pub type FormatChild = fn(DynamicValueRef<'_>, &mut fmt::Formatter<'_>, bool) -> fmt::Result;
pub type NativeFormat = fn(
    &DynamicValueRef<'_>,
    &mut fmt::Formatter<'_>,
    bool,
    FormatChild,
) -> Result<fmt::Result, String>;

/// A type chooses how its borrowed storage is displayed and debugged.
pub struct FormatRegistration {
    pub matches: fn(&DynamicLayout) -> bool,
    pub format: NativeFormat,
}

impl FormatRegistration {
    /// Use Display for both modes when the language's Debug spelling is identical.
    pub const fn display<T: fmt::Display + 'static>() -> Self {
        fn matches<T: 'static>(layout: &DynamicLayout) -> bool {
            layout.is_rust_type::<T>()
        }
        fn format<T: fmt::Display + 'static>(
            view: &DynamicValueRef<'_>,
            formatter: &mut fmt::Formatter<'_>,
            _debug: bool,
            _child: FormatChild,
        ) -> Result<fmt::Result, String> {
            view.with_rust::<T, _>(|value| fmt::Display::fmt(value, formatter))
        }
        Self {
            matches: matches::<T>,
            format: format::<T>,
        }
    }

    /// Register the Rust implementations for an exact leaf layout.
    pub const fn of<T: fmt::Display + fmt::Debug + 'static>() -> Self {
        fn matches<T: 'static>(layout: &DynamicLayout) -> bool {
            layout.is_rust_type::<T>()
        }
        fn format<T: fmt::Display + fmt::Debug + 'static>(
            view: &DynamicValueRef<'_>,
            formatter: &mut fmt::Formatter<'_>,
            debug: bool,
            _child: FormatChild,
        ) -> Result<fmt::Result, String> {
            view.with_rust::<T, _>(|value| {
                if debug {
                    fmt::Debug::fmt(value, formatter)
                } else {
                    fmt::Display::fmt(value, formatter)
                }
            })
        }
        Self {
            matches: matches::<T>,
            format: format::<T>,
        }
    }
}

impl NativeRegistry {
    pub fn format_view(
        &self,
        view: &DynamicValueRef<'_>,
        formatter: &mut fmt::Formatter<'_>,
        debug: bool,
        child: FormatChild,
    ) -> Option<Result<fmt::Result, String>> {
        let layout = match view.layout() {
            Ok(layout) => layout,
            Err(error) => return Some(Err(error)),
        };
        self.formats
            .iter()
            .find(|registration| (registration.matches)(&layout))
            .map(|registration| (registration.format)(view, formatter, debug, child))
    }
}
