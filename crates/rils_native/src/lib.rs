//! Registration protocol shared by standard-library definitions and runtimes.

mod equality;
mod format;
mod key;
mod registry;

pub use equality::{EqualChildren, EqualityRegistration, ViewEquality};
pub use format::{FormatChild, FormatRegistration, NativeFormat};
pub use key::NativeKey;
pub use registry::{
    ElementRegistration, KeyRegistration, LayoutRegistration, LayoutResolver, NativeRegistry,
};
