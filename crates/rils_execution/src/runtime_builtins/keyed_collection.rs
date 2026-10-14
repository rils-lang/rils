//! Storage policy supplied by a generated standard-library declaration bridge.

use crate::{Value, value::native_receiver::NativeReceiver};

#[derive(Clone, Copy)]
pub(crate) struct KeyedCollection {
    pub name: &'static str,
    pub ordered: bool,
    pub parameters: usize,
}

impl KeyedCollection {
    pub fn matches(self, ty: &crate::Type) -> bool {
        matches!(ty, crate::Type::Named { name, arguments }
            if name == self.name && arguments.len() == self.parameters)
    }

    pub fn receiver(self, value: &Value) -> Result<NativeReceiver, String> {
        let receiver = NativeReceiver::from_value(value)?
            .ok_or_else(|| format!("expected native {} receiver", self.name))?;
        if !self.matches(receiver.descriptor().layout().rils_type()) {
            return Err(format!("wrong {} receiver", self.name));
        }
        Ok(receiver)
    }

    pub fn checked_receiver(self, value: &Value, mutable: bool) -> Result<NativeReceiver, String> {
        if mutable
            && !value
                .as_reference()
                .is_some_and(|reference| reference.mutable)
        {
            return Err(format!(
                "{} mutation requires a mutable reference",
                self.name
            ));
        }
        self.receiver(value)
    }

    pub fn item_layout(
        self,
        object: &NativeReceiver,
        field: Option<usize>,
    ) -> Result<std::rc::Rc<rils_value::DynamicLayout>, String> {
        let item = object
            .descriptor()
            .layout()
            .sequence_item()
            .ok_or("native collection has no item layout")?;
        match field {
            None => Ok(item.clone()),
            Some(index) => item
                .record_fields()
                .and_then(|fields| fields.get(index))
                .map(|field| field.layout_handle())
                .ok_or_else(|| "native entry has no requested field".into()),
        }
    }

    pub fn mutation_error(self, message: String) -> String {
        if message.contains("referenced") || message.contains("iterator") {
            format!(
                "cannot mutate {} while it is borrowed: {message}",
                self.name
            )
        } else {
            message
        }
    }

    pub fn into_iter(
        self,
        receiver: Value,
        context: &super::NativeOwnedContext,
    ) -> Result<Value, String> {
        match receiver {
            Value::Dynamic(object) if self.matches(object.descriptor().layout().rils_type()) => {
                crate::iteration::native_sequence_into_iterator(object, context)
            }
            _ => Err(format!(
                "{}::into_iter received the wrong collection",
                self.name
            )),
        }
    }
}
