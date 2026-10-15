//! Indexed access independent of the sequence's backing allocation.
use super::{DropKind, DynamicValueRef};

pub struct IndexedView<'a> {
    view: DynamicValueRef<'a>,
    length: usize,
    array: bool,
}

impl<'a> IndexedView<'a> {
    pub(super) fn new(view: DynamicValueRef<'a>) -> Result<Self, String> {
        let layout = view.layout()?;
        let (length, array) = match layout.rils_type() {
            rils_syntax::Type::Array { length, .. } => (*length, true),
            _ if matches!(layout.drop_kind, DropKind::Sequence { .. }) => {
                (view.sequence_len()?, false)
            }
            _ => return Err("value has no indexed storage".into()),
        };
        Ok(Self {
            view,
            length,
            array,
        })
    }

    pub fn len(&self) -> usize {
        self.length
    }
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn item(&self, index: usize) -> Result<DynamicValueRef<'_>, String> {
        if index >= self.length {
            return Err(format!("indexed item {index} is out of bounds"));
        }
        let item = if self.array {
            self.view.field(index)?
        } else {
            self.view.sequence_item(index)?
        };
        item.layout()?;
        Ok(item)
    }
}
