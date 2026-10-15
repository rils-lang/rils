use rils_stdlib_macros::decl_rils;

#[decl_rils(core::fixture)]
mod native {
    #[rils_struct]
    pub struct Buffer<T, #[rils_storage(sequence, parameter_fixture::fixture::Buffer)] S = Vec<T>>(
        S,
        std::marker::PhantomData<T>,
    );

    impl<T, S: super::ArrayStorage<T>> Buffer<T, S> {
        #[export_rils]
        pub fn from<const N: usize>(#[rils_param([T; N])] values: S::Array<N>) -> Self {
            Self(S::from_array::<N>(values), std::marker::PhantomData)
        }
    }

    impl<T, S> Buffer<T, S> {
        pub fn into_storage(self) -> S {
            self.0
        }
    }
}

pub use native::Buffer;

pub trait ArrayStorage<T> {
    type Array<const N: usize>;
    fn from_array<const N: usize>(values: Self::Array<N>) -> Self;
}

impl<T> ArrayStorage<T> for Vec<T> {
    type Array<const N: usize> = [T; N];
    fn from_array<const N: usize>(values: [T; N]) -> Self {
        Vec::from(values)
    }
}

pub struct RuntimeArray<T>(pub Vec<T>);
impl<T> ArrayStorage<T> for RuntimeArray<T> {
    type Array<const N: usize> = Vec<T>;
    fn from_array<const N: usize>(values: Vec<T>) -> Self {
        Self(values)
    }
}
