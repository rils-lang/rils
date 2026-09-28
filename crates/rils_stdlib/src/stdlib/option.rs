use rils_stdlib_macros::decl_rils;

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
        #[rils_legacy_id(core::option::option::unwrap)]
        pub fn unwrap(self) -> T {
            match self {
                Self::Some(value) => value,
                Self::None => panic!("called `unwrap` on `None`"),
            }
        }

        /// Returns the present value or the supplied default.
        #[export_rils]
        #[rils_legacy_id(core::option::option::unwrap_or)]
        pub fn unwrap_or(self, default: T) -> T {
            match self {
                Self::Some(value) => value,
                Self::None => default,
            }
        }

        /// Returns the present value or fails with the supplied message.
        #[export_rils]
        #[rils_legacy_id(core::option::option::expect)]
        pub fn expect(self, message: String) -> T {
            match self {
                Self::Some(value) => value,
                Self::None => panic!("{message}"),
            }
        }

        /// Moves the value out, leaving None.
        #[export_rils]
        #[rils_legacy_id(core::option::option::take)]
        pub fn take(&mut self) -> Self {
            std::mem::replace(self, Self::None)
        }

        /// Returns this Option when present, otherwise the supplied Option.
        #[export_rils]
        #[rils_legacy_id(core::option::option::or)]
        pub fn or(self, other: Self) -> Self {
            match self {
                Self::Some(_) => self,
                Self::None => other,
            }
        }

        /// Returns the present Option only when exactly one operand is present.
        #[export_rils]
        #[rils_legacy_id(core::option::option::xor)]
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
        #[rils_legacy_id(core::option::option::replace)]
        pub fn replace(&mut self, value: T) -> Self {
            std::mem::replace(self, Self::Some(value))
        }

        /// Maps a present value with the supplied function.
        #[export_rils]
        pub fn map<U, F>(self, transform: F) -> Option<U>
        where
            F: FnOnce(T) -> U,
        {
            match self {
                Self::Some(value) => Option::Some(transform(value)),
                Self::None => Option::None,
            }
        }

        /// Calls the supplied function for a present value and flattens its Option result.
        #[export_rils]
        pub fn and_then<U, F>(self, transform: F) -> Option<U>
        where
            F: FnOnce(T) -> Option<U>,
        {
            match self {
                Self::Some(value) => transform(value),
                Self::None => Option::None,
            }
        }

        /// Calls the supplied fallback only when the Option is None.
        #[export_rils]
        pub fn or_else<F>(self, fallback: F) -> Self
        where
            F: FnOnce() -> Self,
        {
            match self {
                Self::Some(_) => self,
                Self::None => fallback(),
            }
        }

        /// Keeps a present value only when the predicate accepts a shared borrow.
        #[export_rils]
        pub fn filter<F>(self, predicate: F) -> Self
        where
            F: FnOnce(&T) -> bool,
        {
            match self {
                Self::Some(value) if predicate(&value) => Self::Some(value),
                _ => Self::None,
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
