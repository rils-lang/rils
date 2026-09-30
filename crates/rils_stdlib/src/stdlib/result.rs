use rils_stdlib_macros::decl_rils;

#[decl_rils(core::result)]
mod native {
    use super::super::prelude::*;

    /// A successful value or a structured error.
    #[rils_enum]
    pub enum Result<T, E> {
        /// A successful result.
        Ok(T),
        /// A failed result.
        Err(E),
    }

    impl<T, E> Result<T, E> {
        /// Returns true for Ok.
        #[export_rils]
        pub fn is_ok(&self) -> bool {
            matches!(self, Self::Ok(_))
        }

        /// Returns true for Err.
        #[export_rils]
        pub fn is_err(&self) -> bool {
            matches!(self, Self::Err(_))
        }

        /// Converts Result<T, E> to Option<T>.
        #[export_rils]
        pub fn ok(self) -> Option<T> {
            match self {
                Self::Ok(value) => Option::Some(value),
                Self::Err(_) => Option::None,
            }
        }

        /// Converts Result<T, E> to Option<E>.
        #[export_rils]
        pub fn err(self) -> Option<E> {
            match self {
                Self::Ok(_) => Option::None,
                Self::Err(value) => Option::Some(value),
            }
        }

        /// Returns the Ok value or fails.
        #[export_rils]
        pub fn unwrap(self) -> T
        where
            E: std::fmt::Display,
        {
            match self {
                Self::Ok(value) => value,
                Self::Err(error) => panic!("called `unwrap` on Err({error})"),
            }
        }

        /// Returns the Ok value or the supplied default.
        #[export_rils]
        pub fn unwrap_or(self, default: T) -> T {
            match self {
                Self::Ok(value) => value,
                Self::Err(_) => default,
            }
        }

        /// Returns the Ok value or fails with the supplied message.
        #[export_rils]
        pub fn expect(self, message: String) -> T
        where
            E: std::fmt::Display,
        {
            match self {
                Self::Ok(value) => value,
                Self::Err(error) => panic!("{message}: {error}"),
            }
        }

        /// Returns the Err value or fails when the Result is Ok.
        #[export_rils]
        pub fn unwrap_err(self) -> E
        where
            T: std::fmt::Display,
        {
            match self {
                Self::Err(error) => error,
                Self::Ok(value) => panic!("called `unwrap_err` on Ok({value})"),
            }
        }

        /// Returns the Err value or fails with the supplied message when the Result is Ok.
        #[export_rils]
        pub fn expect_err(self, message: String) -> E
        where
            T: std::fmt::Display,
        {
            match self {
                Self::Err(error) => error,
                Self::Ok(value) => panic!("{message}: {value}"),
            }
        }

        /// Maps an Ok value while preserving Err.
        #[export_rils]
        pub fn map<U, F>(self, transform: F) -> Result<U, E>
        where
            F: FnOnce(T) -> U,
        {
            match self {
                Self::Ok(value) => Result::Ok(transform(value)),
                Self::Err(error) => Result::Err(error),
            }
        }

        /// Maps an Err value while preserving Ok.
        #[export_rils]
        pub fn map_err<F, Callback>(self, transform: Callback) -> Result<T, F>
        where
            Callback: FnOnce(E) -> F,
        {
            match self {
                Self::Ok(value) => Result::Ok(value),
                Self::Err(error) => Result::Err(transform(error)),
            }
        }

        /// Calls the supplied function for Ok and flattens its Result.
        #[export_rils]
        pub fn and_then<U, F>(self, transform: F) -> Result<U, E>
        where
            F: FnOnce(T) -> Result<U, E>,
        {
            match self {
                Self::Ok(value) => transform(value),
                Self::Err(error) => Result::Err(error),
            }
        }

        /// Calls the supplied fallback for Err and flattens its Result.
        #[export_rils]
        pub fn or_else<F, Callback>(self, fallback: Callback) -> Result<T, F>
        where
            Callback: FnOnce(E) -> Result<T, F>,
        {
            match self {
                Self::Ok(value) => Result::Ok(value),
                Self::Err(error) => fallback(error),
            }
        }
    }

    #[rils_impl]
    impl<T: Clone, E: Clone> Clone for Result<T, E> {
        fn clone(&self) -> Self {
            match self {
                Self::Ok(value) => Self::Ok(value.clone()),
                Self::Err(error) => Self::Err(error.clone()),
            }
        }
    }

    #[rils_impl]
    impl<T: Copy, E: Copy> Copy for Result<T, E> {}
}

pub use native::Result;
