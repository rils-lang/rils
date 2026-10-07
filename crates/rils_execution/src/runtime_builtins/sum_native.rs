//! Signature-driven conversion of native sums, independent of method semantics.

use std::rc::Rc;

use rils_stdlib::stdlib::{
    option::Option as NativeOption, result::Result as NativeResult, string::String as NativeString,
};
use rils_value::{DynamicLayout, DynamicValue};

use super::NativeOwnedContext;
use crate::{
    Type, Value,
    value::{borrowed_sum::Branch, record_codec::NativeRecordCodec},
};

pub(super) fn branch(value: &Value) -> Result<Branch, String> {
    if let Some(branch) = crate::value::sum::branch(value)? {
        return Ok(branch);
    }
    match value {
        Value::Reference(reference) => branch(&reference.read()?),
        Value::Option { value: Some(_), .. } => Ok(Branch::Some),
        Value::Option { value: None, .. } => Ok(Branch::None),
        Value::Result { value: Ok(_), .. } => Ok(Branch::Ok),
        Value::Result { value: Err(_), .. } => Ok(Branch::Err),
        _ => Err(format!(
            "native query expects a sum, found {}",
            value.type_name()
        )),
    }
}

pub(super) fn string_argument(value: Value) -> Result<String, String> {
    let Value::Native(object) = value else {
        return Err("native argument expects string".into());
    };
    object
        .into_rust::<NativeString>()
        .map(String::from)
        .map_err(|failure| failure.1)
}

pub(super) struct SumBridge {
    layout: Rc<DynamicLayout>,
    codec: NativeRecordCodec,
}

impl SumBridge {
    pub(super) fn new(
        value: &Value,
        owner: &str,
        context: &NativeOwnedContext,
    ) -> Result<Self, String> {
        let mut codec = NativeRecordCodec::with_definitions(&context.structs, &context.enums)
            .with_owned_conversion();
        let layout = match value {
            Value::Dynamic(object) => {
                codec = crate::value::sum::codec(object, &context.structs, &context.enums)?
                    .with_owned_conversion();
                object.descriptor().layout_handle()
            }
            Value::Reference(reference) => {
                if !reference.mutable {
                    return Err("native receiver requires `&mut self`".into());
                }
                let layout = reference
                    .native_layout()?
                    .ok_or("mutable sum requires native storage")?;
                if let Some(source) = reference.native_codec()? {
                    codec.retain_layout_declarations(&source, &layout)?;
                }
                layout
            }
            _ => context.layout(&Type::of_value(value).ok_or("sum has no concrete type")?)?,
        };
        if !layout.rils_type().is_concrete_type() {
            return Err(format!(
                "cannot infer the complete native sum type: {}",
                layout.rils_type()
            ));
        }
        if !matches!(
            (owner, layout.rils_type()),
            ("Option", Type::Option(_)) | ("Result", Type::Result(_, _))
        ) {
            return Err(format!(
                "native method expects {owner}, found {}",
                value.type_name()
            ));
        }
        Ok(Self { layout, codec })
    }

    fn item_layout(&self, index: usize) -> Result<Rc<DynamicLayout>, String> {
        self.layout
            .option_item()
            .filter(|_| index == 0)
            .or_else(|| {
                self.layout
                    .variant_alternatives()
                    .and_then(|items| items.get(index))
            })
            .cloned()
            .ok_or("sum has no generic item layout".into())
    }

    pub(super) fn item_argument(
        &mut self,
        value: Value,
        index: usize,
        name: &str,
    ) -> Result<Value, String> {
        let layout = self.item_layout(index)?;
        if !layout.rils_type().accepts(&value) {
            return Err(format!(
                "type mismatch: {name} must be {}, found {}",
                layout.rils_type(),
                value.type_name()
            ));
        }
        let value = self.codec.into_native(value, layout)?;
        self.codec.from_native(value)
    }

    pub(super) fn import_option(&mut self, value: Value) -> Result<NativeOption<Value>, String> {
        let value = self.codec.into_native(value, self.layout.clone())?;
        self.decode_option(value)
    }

    fn decode_option(&self, value: DynamicValue) -> Result<NativeOption<Value>, String> {
        match value.take_option()? {
            Some(value) => self.codec.from_native(value).map(NativeOption::Some),
            None => Ok(NativeOption::None),
        }
    }

    pub(super) fn import_result(
        &mut self,
        value: Value,
    ) -> Result<NativeResult<Value, Value>, String> {
        let value = self.codec.into_native(value, self.layout.clone())?;
        let (index, value) = value.take_variant()?;
        let value = self.codec.from_native(value)?;
        match index {
            0 => Ok(NativeResult::Ok(value)),
            1 => Ok(NativeResult::Err(value)),
            _ => Err("Result has an invalid variant tag".into()),
        }
    }

    fn encode_option(
        &mut self,
        value: NativeOption<Value>,
        layout: Rc<DynamicLayout>,
    ) -> Result<DynamicValue, String> {
        match value {
            NativeOption::None => DynamicValue::none(layout),
            NativeOption::Some(value) => {
                let item = layout
                    .option_item()
                    .ok_or("Option has no item layout")?
                    .clone();
                let value = self.codec.into_native(value, item)?;
                DynamicValue::some(layout, value)
            }
        }
    }

    pub(super) fn export_option(
        &mut self,
        value: NativeOption<Value>,
        item: Option<usize>,
    ) -> Result<Value, String> {
        let layout = match item {
            Some(index) => DynamicLayout::option(self.item_layout(index)?)?,
            None => self.layout.clone(),
        };
        let value = self.encode_option(value, layout)?;
        crate::value::native_instance::from_native(value, Rc::new(self.codec.clone()))
    }

    pub(super) fn with_mut_option(
        &mut self,
        receiver: Value,
        call: impl FnOnce(&mut NativeOption<Value>) -> Result<NativeOption<Value>, String>,
    ) -> Result<Value, String> {
        let Value::Reference(reference) = receiver else {
            return Err("native method requires a mutable binding".into());
        };
        let layout = self.layout.clone();
        reference.with_native_mut(|mut view| {
            if view.view().is_partially_moved()? {
                return Err("cannot move a partially moved value into native storage".into());
            }
            let previous = view.replace_value(DynamicValue::none(layout.clone())?)?;
            let mut native_self = self.decode_option(previous)?;
            let result = call(&mut native_self);
            let next = self.encode_option(native_self, layout)?;
            view.replace_value(next)?;
            self.export_option(result?, None)
        })?
    }
}
