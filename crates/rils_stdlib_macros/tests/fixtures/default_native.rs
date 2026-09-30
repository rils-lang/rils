use rils_stdlib_macros::decl_rils;

#[decl_rils(core::fixture)]
mod native {
    #[rils_struct]
    pub struct Vec<T>(std::vec::Vec<T>);

    impl<T> From<std::vec::Vec<T>> for Vec<T> {
        fn from(values: std::vec::Vec<T>) -> Self {
            Self(values)
        }
    }

    impl<T> Vec<T> {
        #[export_rils]
        pub fn occupancy(&self) -> usize {
            self.0.len() + 7
        }

        #[export_rils]
        pub fn has_items(&self) -> bool {
            !self.0.is_empty()
        }
    }

    #[rils_struct]
    pub struct Legacy;

    impl Legacy {
        #[export_rils]
        #[rils_import(core::fixture::old)]
        pub fn old(&self) -> usize {
            1
        }

        #[export_rils]
        #[rils_import(core::fixture::new)]
        pub fn new() -> Self {
            Self
        }
    }
}

pub use native::{Legacy, Vec};
