//! Owned conversion between execution handles and composed native bytes.
//!
//! The codec moves structural values into native fields and restores runtime
//! wrappers at the execution boundary. Nominal declarations are retained in
//! the codec so moved structs and enums keep their native storage and identity.

use std::{collections::HashMap, rc::Rc};

use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue};

use crate::Type;

use super::{EnumType, StructType, Value, native_layouts, native_string};

#[path = "record_codec/cloning.rs"]
mod cloning;
#[path = "record_codec/declarations.rs"]
mod declarations;
#[path = "record_codec/native.rs"]
pub(crate) mod native;
#[path = "record_codec/sum.rs"]
mod sum;

#[derive(Clone, Default)]
pub struct NativeRecordCodec {
    native_types: HashMap<Type, Rc<super::NativeType>>,
    structs: HashMap<String, Rc<StructType>>,
    enums: HashMap<String, Rc<EnumType>>,
    require_owned: bool,
}

impl NativeRecordCodec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_definitions(structs: &[Rc<StructType>], enums: &[Rc<EnumType>]) -> Self {
        Self {
            native_types: HashMap::new(),
            structs: structs
                .iter()
                .map(|definition| (definition.name.clone(), definition.clone()))
                .collect(),
            enums: enums
                .iter()
                .map(|definition| (definition.name.clone(), definition.clone()))
                .collect(),
            require_owned: false,
        }
    }

    pub(crate) fn with_owned_conversion(mut self) -> Self {
        self.require_owned = true;
        self
    }

    pub(crate) fn nominal_definition(&self, ty: &Type) -> Option<crate::value::Declaration> {
        let Type::Named { name, .. } = ty else {
            return None;
        };
        self.structs
            .get(name)
            .cloned()
            .map(crate::value::Declaration::StructType)
            .or_else(|| {
                self.enums
                    .get(name)
                    .cloned()
                    .map(crate::value::Declaration::EnumType)
            })
    }

    pub fn into_native(
        &mut self,
        value: Value,
        layout: Rc<DynamicLayout>,
    ) -> Result<DynamicValue, String> {
        self.encode(value, layout)
    }

    pub fn from_native(&self, value: DynamicValue) -> Result<Value, String> {
        self.decode(value)
    }

    /// Encode an assignment against the field's declared concrete layout.
    /// The old native payload is returned to its owner without first turning
    /// the complete record back into `Value` slots.
    pub fn replace_path_field(
        &mut self,
        record: &mut DynamicValue,
        path: &[DynamicPathStep],
        value: Value,
    ) -> Result<Option<DynamicValue>, String> {
        let layout = record.path_field_layout(path)?;
        let value = self.encode(value, layout)?;
        record.replace_path_field(path, value)
    }
}

pub fn into_native(value: Value, layout: Rc<DynamicLayout>) -> Result<DynamicValue, String> {
    NativeRecordCodec::new().into_native(value, layout)
}

pub fn from_native(value: DynamicValue) -> Result<Value, String> {
    NativeRecordCodec::new().from_native(value)
}

impl NativeRecordCodec {
    fn encode(&mut self, value: Value, layout: Rc<DynamicLayout>) -> Result<DynamicValue, String> {
        if self.require_owned && value.is_partially_moved() {
            return Err("cannot move a partially moved value into native storage".into());
        }
        let ty = layout.rils_type().clone();
        if let Value::Native(object) = &value
            && object.descriptor().layout_handle().compatible_with(&layout)
            && (self.require_owned || self.native_types.contains_key(&ty))
        {
            self.register_native_type(object.descriptor_handle())?;
            let Value::Native(object) = value else {
                unreachable!()
            };
            return object.into_dynamic().map_err(|failure| failure.1);
        }
        if let Value::Dynamic(object) = &value
            && matches!(ty, Type::Option(_) | Type::Result(_, _))
            && object.descriptor().layout().rils_type() != &ty
            && ty.accepts(&value)
        {
            return self.recompose_sum(value, layout);
        }
        match (ty, value) {
            (Type::Unit, value) if value.is_unit() => DynamicValue::from_rust(layout, ()),
            (Type::Bool, value) => DynamicValue::from_rust(
                layout,
                value.get_cloned::<bool>().ok().ok_or("expected bool")?,
            ),
            (Type::Reference { .. }, value) if value.as_reference().is_some() => {
                super::runtime_layouts::encode_reference(value.as_reference().unwrap(), layout)
            }
            (Type::Function { .. }, value) => {
                super::runtime_layouts::encode_callable(value, layout)
            }
            (Type::Named { .. }, Value::HostObject(value)) => {
                super::runtime_layouts::encode_host(value, layout)
            }
            (Type::Char, value) if super::char_payload(&value).is_some() => {
                DynamicValue::from_rust(layout, super::char_payload(&value).expect("checked char"))
            }
            (Type::String, Value::Native(object))
                if object.descriptor().rils_type() == &Type::String =>
            {
                // Builtin calls still pass borrowed argument slices and may
                // retain a shallow handle until the call returns. Keep the
                // native sequence's item independently owned in that case.
                let text = match object.into_rust::<NativeString>() {
                    Ok(text) => text,
                    Err(failure) if self.require_owned => return Err(failure.1),
                    Err(failure) => failure.0.with::<NativeString, _>(Clone::clone)?,
                };
                DynamicValue::from_rust(layout, text)
            }
            (ty, Value::Dynamic(object)) if object.descriptor().layout().rils_type() == &ty => {
                if !object.descriptor().layout().compatible_with(&layout) {
                    return Err("value has a different native layout".into());
                }
                if let Some(source) = object.descriptor().metadata::<NativeRecordCodec>() {
                    self.retain_layout_declarations(&source, &layout)?;
                }
                let value = match object.into_value() {
                    Ok(value) => value,
                    Err(failure) if self.require_owned => {
                        if !failure.0.descriptor().layout().is_copy() {
                            return Err(failure.1);
                        }
                        failure.0.with(|value| value.copy_owned())??
                    }
                    Err(failure) => {
                        let cloned = Value::Dynamic(failure.0).clone_owned()?;
                        let Value::Dynamic(cloned) = cloned else {
                            unreachable!("dynamic Clone preserves the storage kind")
                        };
                        cloned.into_value().map_err(|failure| failure.1)?
                    }
                };
                if !value.descriptor().compatible_with(&layout) {
                    return Err("value has a different native layout".into());
                }
                Ok(value)
            }
            (ty, value) if native_layouts::integer::layout(&ty).is_some() => {
                native_layouts::integer::option_item(&value, &ty, layout)
                    .ok_or_else(|| format!("no integer codec for {ty}"))?
            }
            (ty, value) if native_layouts::float::layout(&ty).is_some() => {
                native_layouts::float::option_item(&value, &ty, layout)
                    .ok_or_else(|| format!("no float codec for {ty}"))?
            }
            (ty, value) => Err(format!(
                "no owned native field converter for {ty} from {}",
                value.type_name()
            )),
        }
    }

    fn decode(&self, value: DynamicValue) -> Result<Value, String> {
        if let Some(descriptor) = self.native_types.get(value.descriptor().rils_type()) {
            return descriptor
                .from_dynamic(value)
                .map(Value::Native)
                .map_err(|failure| failure.1);
        }
        if super::runtime_layouts::is_execution_leaf(value.descriptor()) {
            return super::runtime_layouts::decode(value);
        }
        let ty = value.descriptor().rils_type().clone();
        match ty {
            Type::Unit => value
                .into_rust::<()>()
                .map(|_| Value::from(crate::value::native_unit()))
                .map_err(|error| error.1),
            Type::Bool => value
                .into_rust::<bool>()
                .map(|value| Value::from(crate::value::native_bool(value)))
                .map_err(|error| error.1),
            Type::Char => value
                .into_rust::<char>()
                .map(|value| Value::from(super::native_char(value)))
                .map_err(|error| error.1),
            Type::String => value
                .into_rust::<NativeString>()
                .map(|text| Value::from(native_string(std::string::String::from(text))))
                .map_err(|error| error.1),
            Type::Named { .. } if value.descriptor().is_rust_value() => {
                let layout = value.layout_handle();
                let descriptor = Rc::new(rils_value::DynamicType::new(layout));
                Ok(Value::Dynamic(super::DynamicObject::new(
                    descriptor, value,
                )?))
            }
            Type::Option(_) | Type::Result(_, _) | Type::Tuple(_) | Type::Array { .. } => {
                super::native_instance::from_native(value, Rc::new(self.clone()))
            }
            _ if value.descriptor().sequence_item().is_some() => {
                super::native_instance::from_native(value, Rc::new(self.clone()))
            }
            Type::Named { name, arguments } if self.structs.contains_key(&name) => {
                let definition = self.structs[&name].clone();
                let fields = value
                    .descriptor()
                    .record_fields()
                    .ok_or_else(|| format!("expected a record native layout for `{name}`"))?;
                let field_names = fields
                    .iter()
                    .map(|field| field.name().to_owned())
                    .collect::<Vec<_>>();
                if field_names.len() != definition.fields.len() {
                    return Err(format!(
                        "struct `{name}` field count does not match its declaration"
                    ));
                }
                for (index, field) in definition.fields.iter().enumerate() {
                    if field_names[index] != field.name {
                        return Err(format!(
                            "struct `{name}` field order differs from its declaration"
                        ));
                    }
                }
                if definition.generic_parameters.len() != arguments.len() {
                    return Err(format!("struct `{name}` has wrong type argument count"));
                }
                super::native_instance::from_native(value, Rc::new(self.clone()))
            }
            Type::Named { name, arguments } if self.enums.contains_key(&name) => {
                let definition = self.enums[&name].clone();
                if definition.generic_parameters.len() != arguments.len() {
                    return Err(format!("enum `{name}` has wrong type argument count"));
                }
                super::native_instance::validate_enum_layout(value.descriptor(), &definition)?;
                super::native_instance::from_native(value, Rc::new(self.clone()))
            }
            ty if native_layouts::integer::layout(&ty).is_some() => {
                native_layouts::integer::field_value(value, &ty)
                    .ok_or_else(|| format!("no integer codec for {ty}"))?
            }
            ty if native_layouts::float::layout(&ty).is_some() => {
                native_layouts::float::field_value(value, &ty)
                    .ok_or_else(|| format!("no float codec for {ty}"))?
            }
            Type::Named { .. } => {
                let layout = value.layout_handle();
                Ok(Value::Dynamic(super::DynamicObject::new(
                    Rc::new(rils_value::DynamicType::new(layout)),
                    value,
                )?))
            }
            _ if value.descriptor().is_rust_value()
                || value.descriptor().record_fields().is_some() =>
            {
                let layout = value.layout_handle();
                let descriptor = Rc::new(rils_value::DynamicType::new(layout));
                Ok(Value::Dynamic(super::DynamicObject::new(
                    descriptor, value,
                )?))
            }
            ty => Err(format!("no owned native field decoder for {ty}")),
        }
    }
}
