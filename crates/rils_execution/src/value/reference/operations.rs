//! Operations on the original referent, without an intermediate value snapshot.

use super::{ReferenceTarget, ReferenceValue, Value};

impl ReferenceValue {
    fn with_nested_reference<R>(
        &self,
        operation: impl FnOnce(&ReferenceValue) -> Result<R, String>,
    ) -> Result<Option<R>, String> {
        use super::super::borrowed::Read;
        use std::rc::Rc;
        self.with_borrowed_target(|value| match value {
            Read::View(view) if view.layout()?.is_rust_type::<Rc<ReferenceValue>>() => {
                view.with_rust::<Rc<ReferenceValue>, _>(|reference| operation(reference).map(Some))?
            }
            Read::Leaf(leaf) if leaf.is_rust_type::<Rc<ReferenceValue>>() => {
                leaf.with_rust::<Rc<ReferenceValue>, _>(|reference| operation(reference).map(Some))?
            }
            _ => Ok(None),
        })?
    }

    pub(crate) fn has_native_method(&self, symbol: &str) -> Result<bool, String> {
        if let ReferenceTarget::Storage(target) = &self.target {
            return target
                .try_borrow()
                .map_err(|_| "reference target is already mutably accessed".to_owned())?
                .with_value(|value| super::super::native_ops::has_method(value, symbol));
        }
        if let Some(result) =
            self.with_nested_reference(|reference| reference.has_native_method(symbol))?
        {
            return Ok(result);
        }
        let Some(codec) = self.native_codec()? else {
            return Ok(false);
        };
        let Some(layout) = self.native_layout()? else {
            return Ok(false);
        };
        Ok(codec
            .registered_type(&layout)?
            .is_some_and(|descriptor| descriptor.has_method(symbol)))
    }

    pub(crate) fn call_native(&self, symbol: &str, arguments: &[Value]) -> Result<Value, String> {
        if let ReferenceTarget::Storage(target) = &self.target {
            let result = target
                .try_borrow()
                .map_err(|_| "reference target is already mutably accessed".to_owned())?
                .with_value(|value| {
                    if let Some(reference) = value.as_reference() {
                        return reference.call_native(symbol, arguments).map(Some);
                    }
                    let Value::Native(object) = value else {
                        return Ok(None);
                    };
                    object
                        .call(symbol, arguments)
                        .ok_or_else(|| format!("native receiver does not implement {symbol}"))?
                        .map(Some)
                })?;
            if let Some(result) = result {
                return Ok(result);
            }
        }
        if let Some(result) =
            self.with_nested_reference(|reference| reference.call_native(symbol, arguments))?
        {
            return Ok(result);
        }
        let codec = self.native_codec()?.unwrap_or_default();
        let descriptor = self
            .with_native_view(|view| codec.registered_type(view.layout()?.as_ref()))??
            .ok_or("native receiver has no registered method table")?;
        let result = if self.mutable {
            self.with_native_mut(|view| descriptor.call_view_mut(view, symbol, arguments))?
        } else {
            self.with_native_view(|view| descriptor.call_view(view, symbol, arguments))?
        };
        result.ok_or_else(|| format!("native receiver does not implement {symbol}"))?
    }

    pub(crate) fn clone_referent(&self) -> Result<Value, String> {
        if let ReferenceTarget::Storage(target) = &self.target {
            return target
                .try_borrow()
                .map_err(|_| "reference target is already mutably accessed".to_owned())?
                .with_value(Value::clone_owned);
        }
        let codec = self.native_codec()?.unwrap_or_default();
        let cloned = self.with_native_view(|view| codec.clone_view(view))??;
        codec.from_native(cloned)
    }
}
