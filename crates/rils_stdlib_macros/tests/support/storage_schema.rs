//! Minimal descriptor recorder used by generated-call fixtures.
#[derive(Clone, Copy, Debug)]
pub enum EntryShape {
    Parameter(usize),
    Tuple(&'static [EntryShape]),
}
#[derive(Clone, Copy, Debug)]
pub struct EntrySchema {
    pub owner: &'static str,
    pub parameters: usize,
    pub entry: EntryShape,
}
