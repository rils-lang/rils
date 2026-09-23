use rils_builtins_macros::decl_rils;

decl_rils! {
    pub mod core::option;

    /// An optional value.
    pub enum Option<T> {
        /// A present value.
        Some(T),
        /// An absent value.
        None,
    }

    impl<T> Option<T> {
        /// Returns true when a value is present.
        pub fn is_some(&self) -> bool {
            #rils { matches!(#self, #Option::Some(_)) }
        }
    }
}
