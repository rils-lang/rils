//! Native receiver context that retains the original owner and projection.

use super::{DynamicObject, ReferenceValue, Value};
use rils_value::{DynamicType, DynamicValueMut, DynamicValueRef};
use std::rc::Rc;

enum Storage {
    Object(DynamicObject),
    Reference(Rc<ReferenceValue>),
}

pub(crate) struct NativeReceiver {
    descriptor: Rc<DynamicType<Value>>,
    storage: Storage,
}

impl NativeReceiver {
    pub(crate) fn from_value(value: &Value) -> Result<Option<Self>, String> {
        match value {
            __reference_value if __reference_value.as_reference().is_some() => {
                let reference = __reference_value
                    .as_reference()
                    .expect("checked reference handle");
                {
                    let Some(layout) = reference.native_layout()? else {
                        return Ok(None);
                    };
                    Ok(Some(Self {
                        descriptor: Rc::new(match super::borrowed::native_codec(value)? {
                            Some(codec) => DynamicType::new(layout).register_metadata(codec),
                            None => return Err("native receiver has no declaration context".into()),
                        }),
                        storage: Storage::Reference(reference.clone()),
                    }))
                }
            }
            Value::Dynamic(object) => {
                object.descriptor().metadata_or_insert_with(|| {
                    Rc::new(super::record_codec::NativeRecordCodec::default())
                });
                Ok(Some(Self {
                    descriptor: object.descriptor_handle(),
                    storage: Storage::Object(object.clone()),
                }))
            }

            _ => Ok(None),
        }
    }

    pub(crate) fn descriptor(&self) -> &DynamicType<Value> {
        &self.descriptor
    }

    pub(crate) fn codec(&self) -> super::record_codec::NativeRecordCodec {
        self.codec_handle().as_ref().clone()
    }

    pub(crate) fn codec_handle(&self) -> Rc<super::record_codec::NativeRecordCodec> {
        self.descriptor
            .metadata::<super::record_codec::NativeRecordCodec>()
            .expect("native receiver retains its owner's declaration context")
    }

    pub(crate) fn commit_codec(
        &self,
        codec: &super::record_codec::NativeRecordCodec,
    ) -> Result<(), String> {
        self.codec_handle()
            .commit_layout_declarations(codec, self.descriptor.layout())
    }

    pub(crate) fn with<R>(
        &self,
        callback: impl FnOnce(DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        match &self.storage {
            Storage::Object(object) => object.with(|value| callback(value.view())),
            Storage::Reference(reference) => reference.with_native_view(callback),
        }
    }

    pub(crate) fn with_mut<R>(
        &self,
        callback: impl FnOnce(DynamicValueMut<'_>) -> R,
    ) -> Result<R, String> {
        match &self.storage {
            Storage::Object(object) => object.with_mut(|value| callback(value.view_mut())),
            Storage::Reference(reference) => reference.with_native_mut(callback),
        }
    }

    pub(crate) fn same_storage(&self, other: &Self) -> bool {
        match (
            self.with(|view| view.sequence_borrows()),
            other.with(|view| view.sequence_borrows()),
        ) {
            (Ok(Ok(left)), Ok(Ok(right))) => Rc::ptr_eq(&left, &right),
            _ => false,
        }
    }
}
