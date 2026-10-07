//! Transfer an active native branch into a declaration's destination layout.

use rils_value::{DynamicLayout, DynamicValue};
use std::rc::Rc;

use super::{NativeRecordCodec, Type, Value};

impl NativeRecordCodec {
    pub(super) fn recompose_sum(
        &mut self,
        value: Value,
        layout: Rc<DynamicLayout>,
    ) -> Result<DynamicValue, String> {
        let Value::Dynamic(object) = value else {
            return Err("sum transfer requires native storage".into());
        };
        if let Some(source) = object.descriptor().metadata::<Self>() {
            self.retain_layout_declarations(&source, object.descriptor().layout())?;
        }
        let payload = object.into_value().map_err(|failure| failure.1)?;
        match layout.rils_type() {
            Type::Option(_) => {
                let Some(item) = payload.take_option()? else {
                    return DynamicValue::none(layout);
                };
                let item_layout = layout
                    .option_item()
                    .ok_or("Option has no item layout")?
                    .clone();
                let item = self.encode(self.decode(item)?, item_layout)?;
                DynamicValue::some(layout, item)
            }
            Type::Result(_, _) => {
                let (index, item) = payload.take_variant()?;
                let item_layout = layout
                    .variant_alternatives()
                    .and_then(|alternatives| alternatives.get(index))
                    .ok_or("Result has an invalid variant tag")?
                    .clone();
                let item = self.encode(self.decode(item)?, item_layout)?;
                DynamicValue::variant(layout, index, item)
            }
            _ => Err("sum transfer requires Option or Result layout".into()),
        }
    }
}
