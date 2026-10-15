use rils_stdlib_macros::decl_rils;
#[decl_rils(core::fixture)]
mod native {
    #[rils_struct]
    pub struct Buffer<T, #[rils_storage(sequence, owning_fixture::Buffer)] S = Vec<T>>(
        S,
        std::marker::PhantomData<T>,
    );
    impl<T, S> Buffer<T, S> {
        pub fn with_storage(storage: S) -> Self {
            Self(storage, std::marker::PhantomData)
        }
    }
    #[rils_impl]
    impl<T, S: IntoIterator> IntoIterator for Buffer<T, S> {
        #[rils_associated(T)]
        type Item = S::Item;
        #[rils_associated(Iterator<T>)]
        type IntoIter = S::IntoIter;
        fn into_iter(self) -> Self::IntoIter {
            let mut cursor = self.0.into_iter();
            cursor.next();
            cursor
        }
    }
}
pub use native::Buffer;
