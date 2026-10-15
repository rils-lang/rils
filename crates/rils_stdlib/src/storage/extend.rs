//! Consuming transfers between sequence storage views.
use rils_value::DynamicValueMut;

pub trait SequenceExtend: Sized {
    type ExtendOutput;
    fn extend(&mut self, source: Self) -> Self::ExtendOutput;
}
impl<T> SequenceExtend for Vec<T> {
    type ExtendOutput = ();
    fn extend(&mut self, source: Self) {
        std::iter::Extend::extend(self, source);
    }
}
impl SequenceExtend for DynamicValueMut<'_> {
    type ExtendOutput = Result<(), String>;
    fn extend(&mut self, mut source: Self) -> Self::ExtendOutput {
        if !self
            .view()
            .layout()?
            .compatible_with(source.view().layout()?.as_ref())
        {
            return Err("sequence element types do not match".into());
        }
        self.sequence_borrows()?.check_structural_mutation()?;
        let items = source.drain_sequence()?;
        for item in items {
            self.push_sequence_item(item?)?;
        }
        Ok(())
    }
}
