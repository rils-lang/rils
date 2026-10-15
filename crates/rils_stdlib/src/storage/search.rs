//! Equality queries over projected sequence elements.
use rils_value::{DynamicValueRef, IndexedView};

/// Bridges a runtime query to a borrowed element without copying it.
pub trait ElementQuery {
    fn equal(&self, element: DynamicValueRef<'_>) -> Result<bool, String>;
}

pub trait SequenceSearch<T> {
    type SearchOutput;
    fn contains(&self, value: &T) -> Self::SearchOutput;
}

impl<T: PartialEq> SequenceSearch<T> for std::vec::Vec<T> {
    type SearchOutput = bool;
    fn contains(&self, value: &T) -> bool {
        self.as_slice().contains(value)
    }
}

impl<T: ElementQuery> SequenceSearch<T> for IndexedView<'_> {
    type SearchOutput = Result<bool, String>;
    fn contains(&self, value: &T) -> Self::SearchOutput {
        for index in 0..self.len() {
            if value.equal(self.item(index)?)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
