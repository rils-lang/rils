//! Registration protocol shared by standard-library definitions and runtimes.

mod key;
mod registry;

pub use key::NativeKey;
pub use registry::{
    ElementRegistration, KeyRegistration, LayoutRegistration, LayoutResolver, NativeRegistry,
};
