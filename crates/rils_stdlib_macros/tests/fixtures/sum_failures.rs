use rils_stdlib_macros::decl_rils;

#[decl_rils(core::fixture)]
mod native {
    #[rils_enum]
    pub enum Option<T> {
        Some(T),
        None,
    }

    impl<T> Option<T> {
        #[export_rils]
        pub fn transform<U, F>(self, callback: F) -> Option<U>
        where
            F: FnOnce(T) -> U,
        {
            match self {
                Self::Some(value) => Option::Some(callback(value)),
                Self::None => Option::None,
            }
        }

        #[export_rils]
        pub fn require(self, message: String) -> T {
            match self {
                Self::Some(value) => value,
                Self::None => panic!("missing: {message}"),
            }
        }

        #[export_rils]
        pub fn require_return(self) -> T {
            if let Self::Some(value) = self {
                return value;
            }
            panic!("missing");
        }

        #[export_rils]
        pub fn require_empty(self) -> T {
            match self {
                Self::Some(value) => value,
                Self::None => panic!(),
            }
        }
    }
}

pub use native::Option;
