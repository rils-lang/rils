use rils_stdlib_macros::decl_rils;

#[decl_rils(core::fixture)]
mod native {
    #[rils_struct]
    pub struct Reordered<
        A,
        B,
        #[rils_storage(sequence((B, A)), sequence_fixture::Reordered)] S = Vec<(B, A)>,
    >(S, std::marker::PhantomData<(A, B)>);

    impl<A, B> From<Vec<(B, A)>> for Reordered<A, B> {
        fn from(values: Vec<(B, A)>) -> Self {
            Self(values, std::marker::PhantomData)
        }
    }

    #[rils_impl]
    impl<A, B, S: IntoIterator<Item = (B, A)>> IntoIterator for Reordered<A, B, S> {
        type Item = (B, A);
        #[rils_associated(Entries<A, B>)]
        type IntoIter = S::IntoIter;
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }
}

pub use native::Reordered;
