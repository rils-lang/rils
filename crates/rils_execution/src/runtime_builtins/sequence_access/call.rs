//! Receiver access, ownership and parameter conversion for any declared sequence.
use super::{
    output::{ResultShape, StorageOutput},
    services::Services,
};
use crate::{
    Value,
    runtime_builtins::NativeOwnedContext,
    value::{native_receiver::NativeReceiver, record_codec::NativeRecordCodec},
};
use rils_stdlib::storage::{EntrySchema, MutableAccess, SharedAccess};
use rils_value::DynamicValue;
pub(crate) struct Call<'a> {
    pub(super) receiver: NativeReceiver,
    pub(super) schema: &'a EntrySchema,
    pub(super) services: Services,
}
impl<'a> Call<'a> {
    pub(crate) fn prepare(
        value: &Value,
        schema: &'a EntrySchema,
        mutable: bool,
    ) -> Result<Self, String> {
        if mutable
            && !value
                .as_reference()
                .is_some_and(|reference| reference.mutable)
        {
            return Err(format!(
                "{} mutation requires a mutable reference",
                schema.owner
            ));
        }
        let receiver =
            NativeReceiver::from_value(value)?.ok_or("call requires native sequence storage")?;
        schema.validate(receiver.descriptor().layout())?;
        let services = Services(receiver.codec());
        Ok(Self {
            receiver,
            schema,
            services,
        })
    }
    pub(super) fn parameter_layout(
        &self,
        parameter: usize,
    ) -> Result<std::rc::Rc<rils_value::DynamicLayout>, String> {
        if let crate::Type::Array { element, .. } = self.receiver.descriptor().layout().rils_type()
        {
            if parameter != 0 {
                return Err("array has only one element parameter".into());
            }
            return self.services.0.resolve_layout(element, &[]);
        }
        self.schema
            .parameter_layout(self.receiver.descriptor().layout(), parameter)
    }
    pub(crate) fn encode(
        &mut self,
        value: Value,
        parameter: usize,
        context: &NativeOwnedContext,
    ) -> Result<DynamicValue, String> {
        let layout = self.parameter_layout(parameter)?;
        let mut codec = self.services.0.clone().with_owned_conversion();
        codec.retain_layout_declarations(
            &NativeRecordCodec::with_definitions(&context.structs, &context.enums),
            &layout,
        )?;
        let value = codec.into_native(value, layout)?;
        self.services = Services(codec);
        Ok(value)
    }
    pub(crate) fn consume(&mut self, value: Value) -> Result<OwnedSource, String> {
        let source = NativeReceiver::from_value(&value)?
            .ok_or("sequence argument requires native storage")?;
        if self.receiver.same_storage(&source) {
            return Err("sequence cannot consume itself".into());
        }
        if value.as_reference().is_some() {
            return Err("sequence argument requires an owned value".into());
        }
        if !self
            .receiver
            .descriptor()
            .layout()
            .compatible_with(source.descriptor().layout())
        {
            return Err("sequence argument has a different layout".into());
        }
        let services = Services(source.codec());
        self.services
            .0
            .retain_layout_declarations(&services.0, source.descriptor().layout())?;
        drop(source);
        let Value::Dynamic(value) = value else {
            return Err("sequence argument requires an owned value".into());
        };
        let payload = value.into_value().map_err(|error| error.1)?;
        Ok(OwnedSource { payload, services })
    }
    pub(crate) fn shared<R: StorageOutput>(
        self,
        shape: ResultShape,
        call: impl FnOnce(SharedAccess<'_>) -> Result<R, String>,
    ) -> Result<Value, String> {
        let output = self
            .receiver
            .with(|view| call(SharedAccess::new(view, self.schema, &self.services)?))??;
        output.into_value(shape, &self)
    }
    pub(crate) fn mutable<R: StorageOutput>(
        self,
        shape: ResultShape,
        call: impl FnOnce(MutableAccess<'_>) -> Result<R, String>,
    ) -> Result<Value, String> {
        self.receiver.commit_codec(&self.services.0)?;
        let output = self
            .receiver
            .with_mut(|view| call(MutableAccess::new(view, self.schema, &self.services)?))??;
        output.into_value(shape, &self)
    }
    pub(crate) fn pair<R: StorageOutput>(
        mut self,
        other: &Value,
        shape: ResultShape,
        call: impl FnOnce(SharedAccess<'_>, SharedAccess<'_>) -> Result<R, String>,
    ) -> Result<Value, String> {
        let other = Self::prepare(other, self.schema, false)?;
        if !self
            .receiver
            .descriptor()
            .layout()
            .compatible_with(other.receiver.descriptor().layout())
        {
            return Err("sequence argument has a different layout".into());
        }
        let mut output_codec = self.services.0.clone();
        output_codec
            .retain_layout_declarations(&other.services.0, other.receiver.descriptor().layout())?;
        let output = self.receiver.with(|left| {
            other.receiver.with(|right| {
                call(
                    SharedAccess::new(left, self.schema, &self.services)?,
                    SharedAccess::new(right, self.schema, &other.services)?,
                )
            })?
        })??;
        self.services = Services(output_codec);
        output.into_value(shape, &self)
    }
}
pub(crate) struct OwnedSource {
    payload: DynamicValue,
    services: Services,
}
impl OwnedSource {
    pub(crate) fn access<'a>(
        &'a mut self,
        schema: &'a EntrySchema,
    ) -> Result<MutableAccess<'a>, String> {
        MutableAccess::new(self.payload.view_mut(), schema, &self.services)
    }
}
