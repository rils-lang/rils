//! Construct typed sums and propagate errors directly into native storage.

use std::rc::Rc;

use rils_value::DynamicValue;

use super::{NativeRecordCodec, Type, TypedStorageContext, Value};

impl TypedStorageContext<'_> {
    pub fn construct_option(&self, ty: &Type, item: Option<Value>) -> Result<Value, String> {
        let ty = super::declarations::storage_type(ty);
        let Type::Option(_) = &ty else {
            return Err("Option construction requires an Option type".into());
        };
        if !ty.is_concrete_type() {
            return Err(format!(
                "cannot infer the complete native Option type: {ty}"
            ));
        }
        PreparedOption::new(self, &ty, item.as_ref())?.construct(item)
    }

    pub fn construct_result(
        &self,
        ty: &Type,
        branch: Result<Value, Value>,
    ) -> Result<Value, String> {
        let ty = super::declarations::storage_type(ty);
        let Type::Result(_, _) = &ty else {
            return Err("Result construction requires a Result type".into());
        };
        if !ty.is_concrete_type() {
            return Err(format!(
                "cannot infer the complete native Result type: {ty}"
            ));
        }
        let layout = self.layout(&ty)?;
        let (index, item) = match branch {
            Ok(item) => (0, item),
            Err(item) => (1, item),
        };
        let child = layout
            .variant_alternatives()
            .and_then(|alternatives| alternatives.get(index))
            .ok_or("Result has no branch layout")?
            .clone();
        let mut codec =
            NativeRecordCodec::with_definitions(self.structs, self.enums).with_owned_conversion();
        let item = codec.into_native(item, child)?;
        let payload = DynamicValue::variant(layout, index, item)?;
        super::super::native_instance::from_native(payload, Rc::new(codec))
    }

    /// Only `?` may replace the inactive Ok witness. Ordinary assignments must
    /// still match both generic arguments, including the inactive branch.
    pub fn propagate_result_error(&self, value: Value, expected: &Type) -> Result<Value, String> {
        let expected = super::declarations::storage_type(expected);
        let Type::Result(_, error_type) = &expected else {
            return Err("the `?` operator requires a Result return type".into());
        };
        let branch = super::super::dynamic_result::take_owned_with_definitions(
            value,
            self.structs,
            self.enums,
        )?;
        let Err(error) = branch else {
            return Err("error propagation requires Err".into());
        };
        if !error_type.accepts(&error) {
            return Err(format!(
                "type mismatch: expected {error_type}, found {}",
                error.type_name()
            ));
        }
        self.construct_result(&expected, Err(error))
    }
}

/// A complete Option layout and its declarations, prepared before moving a source.
#[derive(Clone)]
pub(crate) struct PreparedOption {
    layout: Rc<rils_value::DynamicLayout>,
    codec: NativeRecordCodec,
    hosts: Vec<Rc<super::super::HostType>>,
}

impl PreparedOption {
    pub(crate) fn check_move(value: &Value) -> Result<(), String> {
        let unique = match value {
            Value::Native(object) => object.is_uniquely_owned(),
            Value::Dynamic(object) => object.is_uniquely_owned(),
            _ => true,
        };
        if !unique && !value.is_copy() {
            return Err("cannot move a shared non-Copy Option payload".into());
        }
        if value.has_active_references() || value.is_partially_moved() {
            return Err("cannot move a referenced or partially moved Option payload".into());
        }
        Ok(())
    }

    pub(crate) fn new(
        context: &TypedStorageContext<'_>,
        ty: &Type,
        sample: Option<&Value>,
    ) -> Result<Self, String> {
        let Type::Option(item) = ty else {
            return Err("expected Option type".into());
        };
        if !ty.is_concrete_type() {
            return Err(format!(
                "cannot infer the complete native Option type: {ty}"
            ));
        }
        let mut codec = NativeRecordCodec::with_definitions(context.structs, context.enums)
            .with_owned_conversion();
        let child = match sample {
            Some(value) => codec.layout_for_value(value, item, context.hosts)?,
            None => codec.resolve_layout(item, context.hosts)?,
        };
        Ok(Self {
            layout: rils_value::DynamicLayout::option(child)?,
            hosts: context.hosts.to_vec(),
            codec,
        })
    }

    pub(crate) fn retain_sample(&mut self, value: &Value) -> Result<(), String> {
        let child = self
            .layout
            .option_item()
            .ok_or("Option has no item layout")?;
        let candidate = self
            .codec
            .layout_for_value(value, child.rils_type(), &self.hosts)?;
        if !child.compatible_with(&candidate) {
            return Err(format!(
                "conflicting native Option layout for {}",
                child.rils_type()
            ));
        }
        Ok(())
    }

    pub(crate) fn merge(&mut self, other: &Self) -> Result<(), String> {
        if !self.layout.compatible_with(&other.layout) {
            return Err("conflicting native Option layouts".into());
        }
        self.codec
            .retain_layout_declarations(&other.codec, &other.layout)
    }

    pub(crate) fn construct(mut self, item: Option<Value>) -> Result<Value, String> {
        let payload = match item {
            Some(item) => {
                let child = self
                    .layout
                    .option_item()
                    .ok_or("Option has no item layout")?
                    .clone();
                DynamicValue::some(self.layout, self.codec.into_native(item, child)?)?
            }
            None => DynamicValue::none(self.layout)?,
        };
        super::super::native_instance::from_native(payload, Rc::new(self.codec))
    }
}
