use rils_stdlib_macros::decl_rils;

#[decl_rils(core::fixture)]
mod native {
    #[rils_struct]
    pub struct Vec<T, #[rils_storage(sequence, storage_fixture::fixture::Vec)] S = std::vec::Vec<T>>(
        S,
        std::marker::PhantomData<T>,
    );

    impl<T, S> Vec<T, S> {
        pub fn with_storage(storage: S) -> Self {
            Self(storage, std::marker::PhantomData)
        }
    }

    impl<T, S: super::SequenceTruncate> Vec<T, S> {
        #[export_rils]
        #[rils_return(())]
        pub fn keep_prefix(&mut self, count: usize) -> S::TruncateOutput {
            self.0.truncate(count.saturating_sub(1))
        }
        #[export_rils]
        #[rils_return(())]
        pub fn keep_difference(&mut self, maximum: usize, reserved: usize) -> S::TruncateOutput {
            self.keep_prefix(maximum.saturating_sub(reserved))
        }
        #[export_rils]
        #[rils_return(())]
        pub fn reset(&mut self) -> S::TruncateOutput {
            self.keep_prefix(1)
        }
    }

    impl<T, S: super::Search<T>> Vec<T, S> {
        #[export_rils]
        #[rils_return(bool)]
        pub fn lacks(&self, value: &T) -> Result<bool, String> {
            self.0.matches(value).map(|found| !found)
        }
    }

    impl<T, S: super::SequenceIter> Vec<T, S> {
        #[export_rils]
        #[rils_return(Iter<&T>)]
        pub fn scan(&self) -> Result<usize, String> {
            self.0.iter().map(|count| count + 3)
        }
    }

    impl<T> From<std::vec::Vec<T>> for Vec<T> {
        fn from(values: std::vec::Vec<T>) -> Self {
            Self::with_storage(values)
        }
    }

    impl<T, S: super::SequenceRead> Vec<T, S> {
        #[export_rils]
        pub fn occupancy(&self) -> usize {
            self.0.len() + 7
        }

        #[export_rils]
        pub fn has_items(&self) -> bool {
            !self.0.is_empty()
        }
    }
}

pub use native::Vec;

// A backend with no Rust Vec or element allocation.
pub struct Length(pub usize);
pub trait SequenceRead {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl SequenceRead for Length {
    fn len(&self) -> usize {
        self.0
    }
}
impl<T> SequenceRead for std::vec::Vec<T> {
    fn len(&self) -> usize {
        std::vec::Vec::len(self)
    }
}

pub trait SequenceTruncate {
    type TruncateOutput;
    fn truncate(&mut self, length: usize) -> Self::TruncateOutput;
}
pub struct Mutable<'a>(pub &'a mut std::vec::Vec<i32>);
impl SequenceTruncate for Mutable<'_> {
    type TruncateOutput = Result<(), String>;
    fn truncate(&mut self, length: usize) -> Self::TruncateOutput {
        if length == 13 {
            return Err("fixture backend refusal".into());
        }
        self.0.truncate(length);
        Ok(())
    }
}

pub trait Search<T> {
    fn matches(&self, value: &T) -> Result<bool, String>;
}
impl Search<usize> for Length {
    fn matches(&self, value: &usize) -> Result<bool, String> {
        if *value == 13 {
            return Err("query refused".into());
        }
        Ok(self.0 == *value)
    }
}

pub trait SequenceIter {
    fn iter(&self) -> Result<usize, String>;
}
impl SequenceIter for Length {
    fn iter(&self) -> Result<usize, String> {
        if self.0 == 13 {
            Err("iteration refused".into())
        } else {
            Ok(self.0)
        }
    }
}
