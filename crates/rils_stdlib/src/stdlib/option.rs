use rils_builtins_macros::decl_rils;

#[decl_rils(core::option)]
mod native {
    /// An optional value.
    #[rils_enum]
    pub enum Option<T> {
        /// An absent optional value.
        None,
        /// A present optional value.
        Some(T),
    }

    impl<T> Option<T> {
        /// Returns true when a value is present.
        #[export_rils]
        pub fn is_some(&self) -> bool {
            matches!(self, Self::Some(_))
        }

        /// Returns true when no value is present.
        #[export_rils]
        pub fn is_none(&self) -> bool {
            matches!(self, Self::None)
        }

        /// Returns the present value or fails.
        #[export_rils]
        pub fn unwrap(self) -> T {
            match self {
                Self::Some(value) => value,
                Self::None => panic!("called `unwrap` on `None`"),
            }
        }

        /// Returns the present value or the supplied default.
        #[export_rils]
        pub fn unwrap_or(self, default: T) -> T {
            match self {
                Self::Some(value) => value,
                Self::None => default,
            }
        }

        /// Returns the present value or fails with the supplied message.
        #[export_rils]
        pub fn expect(self, message: String) -> T {
            match self {
                Self::Some(value) => value,
                Self::None => panic!("{message}"),
            }
        }

        /// Moves the value out, leaving None.
        #[export_rils]
        pub fn take(&mut self) -> Self {
            std::mem::replace(self, Self::None)
        }

        /// Returns this Option when present, otherwise the supplied Option.
        #[export_rils]
        pub fn or(self, other: Self) -> Self {
            match self {
                Self::Some(_) => self,
                Self::None => other,
            }
        }

        /// Returns the present Option only when exactly one operand is present.
        #[export_rils]
        pub fn xor(self, other: Self) -> Self {
            match (self, other) {
                (Self::Some(value), Self::None) | (Self::None, Self::Some(value)) => {
                    Self::Some(value)
                }
                _ => Self::None,
            }
        }

        /// Replaces the contained value and returns the previous Option.
        #[export_rils]
        pub fn replace(&mut self, value: T) -> Self {
            std::mem::replace(self, Self::Some(value))
        }

        /// Maps a present value with the supplied function.
        #[export_rils]
        pub fn map<U>(self, transform: fn(T) -> U) -> Option<U> {
            self.try_map(|value| Ok::<_, std::convert::Infallible>(transform(value)))
                .unwrap_or_else(|never| match never {})
        }

        /// Calls the supplied function for a present value and flattens its Option result.
        #[export_rils]
        pub fn and_then<U>(self, transform: fn(T) -> Option<U>) -> Option<U> {
            self.try_and_then(|value| Ok::<_, std::convert::Infallible>(transform(value)))
                .unwrap_or_else(|never| match never {})
        }

        /// Calls the supplied fallback only when the Option is None.
        #[export_rils]
        pub fn or_else(self, fallback: fn() -> Option<T>) -> Self {
            self.try_or_else(|| Ok::<_, std::convert::Infallible>(fallback()))
                .unwrap_or_else(|never| match never {})
        }

        pub fn try_map<U, E, F>(self, transform: F) -> std::result::Result<Option<U>, E>
        where
            F: FnOnce(T) -> std::result::Result<U, E>,
        {
            match self {
                Self::Some(value) => transform(value).map(Option::Some),
                Self::None => Ok(Option::None),
            }
        }

        pub fn try_and_then<U, E, F>(self, transform: F) -> std::result::Result<Option<U>, E>
        where
            F: FnOnce(T) -> std::result::Result<Option<U>, E>,
        {
            match self {
                Self::Some(value) => transform(value),
                Self::None => Ok(Option::None),
            }
        }

        pub fn try_or_else<E, F>(self, fallback: F) -> std::result::Result<Self, E>
        where
            F: FnOnce() -> std::result::Result<Self, E>,
        {
            match self {
                Self::Some(_) => Ok(self),
                Self::None => fallback(),
            }
        }
    }

    #[rils_impl]
    impl<T: Clone> Clone for Option<T> {
        fn clone(&self) -> Self {
            match self {
                Self::Some(value) => Self::Some(value.clone()),
                Self::None => Self::None,
            }
        }
    }

    #[rils_impl]
    impl<T: Copy> Copy for Option<T> {}
}

pub use native::Option;
