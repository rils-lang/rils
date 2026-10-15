//! Clone projected elements without exposing execution values to the standard library.
use crate::stdlib::option::Option as RilsOption;
use rils_value::{DynamicValue, DynamicValueRef, IndexedView};

pub trait SequenceClone {
    type CloneOutput;
    fn front_cloned(&self) -> Self::CloneOutput;
    fn back_cloned(&self) -> Self::CloneOutput;
}
impl<T: Clone> SequenceClone for std::collections::VecDeque<T> {
    type CloneOutput = RilsOption<T>;
    fn front_cloned(&self) -> Self::CloneOutput {
        self.front()
            .cloned()
            .map_or(RilsOption::None, RilsOption::Some)
    }
    fn back_cloned(&self) -> Self::CloneOutput {
        self.back()
            .cloned()
            .map_or(RilsOption::None, RilsOption::Some)
    }
}
pub struct CloningView<'a> {
    view: IndexedView<'a>,
    clone: fn(DynamicValueRef<'_>) -> Result<DynamicValue, String>,
}
impl<'a> CloningView<'a> {
    pub fn new(
        view: IndexedView<'a>,
        clone: fn(DynamicValueRef<'_>) -> Result<DynamicValue, String>,
    ) -> Self {
        Self { view, clone }
    }
    fn boundary(&self, back: bool) -> Result<Option<DynamicValue>, String> {
        if self.view.is_empty() {
            return Ok(None);
        }
        (self.clone)(self.view.item(if back { self.view.len() - 1 } else { 0 })?).map(Some)
    }
}
impl SequenceClone for CloningView<'_> {
    type CloneOutput = Result<Option<DynamicValue>, String>;
    fn front_cloned(&self) -> Self::CloneOutput {
        self.boundary(false)
    }
    fn back_cloned(&self) -> Self::CloneOutput {
        self.boundary(true)
    }
}
