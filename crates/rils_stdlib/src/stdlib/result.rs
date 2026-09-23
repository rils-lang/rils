use rils_builtins_macros::decl_rils;

#[decl_rils(core::result)]
mod native {
    use super::super::prelude::*;

    /// A successful value or a structured error.
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
        pub fn map<U>(self, transform: fn(T) -> U) -> Result<U, E> {
            self.try_map(|value| Ok::<_, std::convert::Infallible>(transform(value)))
                .unwrap_or_else(|never| match never {})
        }

        /// Maps an Err value while preserving Ok.
        #[export_rils]
        pub fn map_err<F>(self, transform: fn(E) -> F) -> Result<T, F> {
            self.try_map_err(|error| Ok::<_, std::convert::Infallible>(transform(error)))
                .unwrap_or_else(|never| match never {})
        }

        /// Calls the supplied function for Ok and flattens its Result.
        #[export_rils]
        pub fn and_then<U>(self, transform: fn(T) -> Result<U, E>) -> Result<U, E> {
            self.try_and_then(|value| Ok::<_, std::convert::Infallible>(transform(value)))
                .unwrap_or_else(|never| match never {})
        }

        /// Calls the supplied fallback for Err and flattens its Result.
        #[export_rils]
        pub fn or_else<F>(self, fallback: fn(E) -> Result<T, F>) -> Result<T, F> {
            self.try_or_else(|error| Ok::<_, std::convert::Infallible>(fallback(error)))
                .unwrap_or_else(|never| match never {})
        }

        pub fn try_map<U, Fail, F>(self, transform: F) -> std::result::Result<Result<U, E>, Fail>
        where
            F: FnOnce(T) -> std::result::Result<U, Fail>,
        {
            match self {
                Self::Ok(value) => transform(value).map(Result::Ok),
                Self::Err(error) => Ok(Result::Err(error)),
            }
        }

        pub fn try_map_err<F, Fail, Callback>(
            self,
            transform: Callback,
        ) -> std::result::Result<Result<T, F>, Fail>
        where
            Callback: FnOnce(E) -> std::result::Result<F, Fail>,
        {
            match self {
                Self::Ok(value) => Ok(Result::Ok(value)),
                Self::Err(error) => transform(error).map(Result::Err),
            }
        }

        pub fn try_and_then<U, Fail, F>(
            self,
            transform: F,
        ) -> std::result::Result<Result<U, E>, Fail>
        where
            F: FnOnce(T) -> std::result::Result<Result<U, E>, Fail>,
        {
            match self {
                Self::Ok(value) => transform(value),
                Self::Err(error) => Ok(Result::Err(error)),
            }
        }

        pub fn try_or_else<F, Fail, Callback>(
            self,
            fallback: Callback,
        ) -> std::result::Result<Result<T, F>, Fail>
        where
            Callback: FnOnce(E) -> std::result::Result<Result<T, F>, Fail>,
        {
            match self {
                Self::Ok(value) => Ok(Result::Ok(value)),
                Self::Err(error) => fallback(error),
            }
        }
    }
}

pub use native::Result;
