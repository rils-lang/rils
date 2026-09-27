//! Owned conversion between legacy execution values and composed native bytes.
//!
//! The codec moves structural values into native fields and restores runtime
//! wrappers at the execution boundary. Nominal declarations are retained in
//! the codec so nested structs and enums can be reconstructed after a move.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicLayout, DynamicValue};

use crate::{Type, ast::EnumVariant};

use super::{
    EnumInstance, EnumPayload, EnumType, FieldSlot, IndexedStorage, StructFields, StructInstance,
    StructType, Value, native_layouts, native_string,
};

#[derive(Default)]
pub struct NativeRecordCodec {
    structs: HashMap<String, Rc<StructType>>,
    enums: HashMap<String, Rc<EnumType>>,
}

impl NativeRecordCodec {
    pub fn new() -> Self {
        Self::default()
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
}

pub fn into_native(value: Value, layout: Rc<DynamicLayout>) -> Result<DynamicValue, String> {
    NativeRecordCodec::new().into_native(value, layout)
}

pub fn from_native(value: DynamicValue) -> Result<Value, String> {
    NativeRecordCodec::new().from_native(value)
}

impl NativeRecordCodec {
    fn encode(&mut self, value: Value, layout: Rc<DynamicLayout>) -> Result<DynamicValue, String> {
        let ty = layout.rils_type().clone();
        match (ty, value) {
            (Type::Unit, Value::Unit) => DynamicValue::from_rust(layout, ()),
            (Type::Bool, Value::Bool(value)) => DynamicValue::from_rust(layout, value),
            (Type::Char, Value::Char(value)) => DynamicValue::from_rust(layout, value),
            (Type::String, Value::String(value)) => {
                DynamicValue::from_rust(layout, NativeString::from(value.to_string()))
            }
            (Type::String, Value::Native(object))
                if object.descriptor().rils_type() == &Type::String =>
            {
                let text = object
                    .into_rust::<NativeString>()
                    .map_err(|failure| failure.1)?;
                DynamicValue::from_rust(layout, text)
            }
            (Type::Option(_), Value::Dynamic(object)) => {
                let value = object.into_value().map_err(|failure| failure.1)?;
                if !value.descriptor().compatible_with(&layout) {
                    return Err("optional value has a different native layout".into());
                }
                Ok(value)
            }
            (Type::Option(_), Value::Option { value: None, .. }) => DynamicValue::none(layout),
            (
                Type::Option(_),
                Value::Option {
                    value: Some(value), ..
                },
            ) => {
                let item = layout
                    .option_item()
                    .ok_or_else(|| "expected an optional native layout".to_owned())?;
                let value = Rc::try_unwrap(value)
                    .map_err(|_| "cannot move a shared optional item".to_owned())?;
                let item = self.encode(value, item.clone())?;
                DynamicValue::some(layout, item)
            }
            (Type::Result(_, _), Value::Result { value, .. }) => {
                let alternatives = layout
                    .variant_alternatives()
                    .ok_or_else(|| "expected a result native layout".to_owned())?;
                let (index, item) = match value {
                    Ok(value) => (0, value),
                    Err(value) => (1, value),
                };
                let item = Rc::try_unwrap(item)
                    .map_err(|_| "cannot move a shared result item".to_owned())?;
                let item = self.encode(item, alternatives[index].clone())?;
                DynamicValue::variant(layout, index, item)
            }
            (Type::Tuple(_), Value::Tuple(sequence))
            | (Type::Array { .. }, Value::Array(sequence)) => {
                let sequence = Rc::try_unwrap(sequence)
                    .map_err(|_| "cannot move a shared tuple or array".to_owned())?;
                if sequence.active_iterators.get() != 0 {
                    return Err("cannot move an indexed value with active iterators".into());
                }
                let fields = layout
                    .record_fields()
                    .ok_or_else(|| "expected a tuple or array native layout".to_owned())?;
                let slots = sequence.elements.into_inner();
                if slots.len() != fields.len() {
                    return Err("tuple or array length does not match its layout".into());
                }
                let values = slots
                    .into_iter()
                    .zip(fields)
                    .map(|(slot, field)| {
                        if slot.references != 0 {
                            return Err("cannot move a referenced tuple or array element".into());
                        }
                        let value = slot.value.ok_or_else(|| {
                            "cannot move a partially moved tuple or array".to_owned()
                        })?;
                        self.encode(value, field.layout_handle())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                DynamicValue::record(layout, values)
            }
            (ty, Value::Vec(sequence)) if native_layouts::vec::matches(&ty) => {
                let sequence =
                    Rc::try_unwrap(sequence).map_err(|_| "cannot move a shared Vec".to_owned())?;
                if sequence.active_iterators.get() != 0 {
                    return Err("cannot move Vec with active iterators".into());
                }
                let item = layout
                    .sequence_item()
                    .ok_or_else(|| "expected a native sequence layout for Vec".to_owned())?;
                let values = sequence
                    .elements
                    .into_inner()
                    .into_iter()
                    .map(|slot| {
                        if slot.references != 0 {
                            return Err("cannot move a referenced Vec element".into());
                        }
                        let value = slot
                            .value
                            .ok_or_else(|| "cannot move a partially moved Vec".to_owned())?;
                        self.encode(value, item.clone())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                DynamicValue::sequence(layout, values)
            }
            (Type::Named { name, arguments }, Value::Struct(instance))
                if instance.type_definition.name == name
                    && instance.type_arguments == arguments =>
            {
                let instance = Rc::try_unwrap(instance)
                    .map_err(|_| format!("cannot move a shared struct `{name}`"))?;
                let definition = instance.type_definition;
                self.structs.insert(name.clone(), definition.clone());
                let fields = layout
                    .record_fields()
                    .ok_or_else(|| format!("expected a record native layout for `{name}`"))?;
                let slots = instance.fields.into_inner().into_slots();
                if slots.len() != fields.len() || slots.len() != definition.fields.len() {
                    return Err(format!(
                        "struct `{name}` field count does not match its layout"
                    ));
                }
                let values = slots
                    .into_iter()
                    .zip(fields)
                    .zip(&definition.fields)
                    .map(|((slot, field), declaration)| {
                        if field.name() != declaration.name {
                            return Err(format!(
                                "struct `{name}` field order differs from its declaration"
                            ));
                        }
                        if slot.references != 0 {
                            return Err(format!("cannot move referenced field `{}`", field.name()));
                        }
                        let value = slot
                            .value
                            .ok_or_else(|| format!("cannot move field `{}` twice", field.name()))?;
                        self.encode(value, field.layout_handle())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                DynamicValue::record(layout, values)
            }
            (Type::Named { name, arguments }, Value::Enum(instance))
                if instance.type_definition.name == name
                    && instance.type_arguments == arguments =>
            {
                let instance = Rc::try_unwrap(instance)
                    .map_err(|_| format!("cannot move a shared enum `{name}`"))?;
                let definition = instance.type_definition;
                let (index, declaration) = definition
                    .variants
                    .iter()
                    .enumerate()
                    .find(|(_, variant)| super::enum_variant_name(variant) == instance.variant)
                    .ok_or_else(|| format!("unknown variant `{}::{}`", name, instance.variant))?;
                let payload_layout = layout
                    .variant_alternatives()
                    .and_then(|variants| variants.get(index))
                    .ok_or_else(|| format!("missing native payload layout for `{name}`"))?
                    .clone();
                let payload = match (declaration, instance.payload) {
                    (EnumVariant::Unit { .. }, EnumPayload::Unit) => {
                        if payload_layout.rils_type() != &Type::Unit {
                            return Err(format!(
                                "unit variant `{}::{}` has a payload layout",
                                name, instance.variant
                            ));
                        }
                        DynamicValue::from_rust(payload_layout, ())?
                    }
                    (EnumVariant::Tuple { fields, .. }, EnumPayload::Tuple(values)) => {
                        let layouts = payload_layout
                            .record_fields()
                            .ok_or_else(|| "tuple variant has no aggregate layout".to_owned())?;
                        if values.len() != fields.len() || values.len() != layouts.len() {
                            return Err(format!(
                                "variant `{}::{}` has wrong arity",
                                name, instance.variant
                            ));
                        }
                        if layouts
                            .iter()
                            .zip(fields)
                            .enumerate()
                            .any(|(index, (layout, _))| layout.name() != index.to_string())
                        {
                            return Err(format!(
                                "variant `{}::{}` tuple field order differs from its declaration",
                                name, instance.variant
                            ));
                        }
                        let values = values
                            .into_iter()
                            .zip(layouts)
                            .map(|(value, field)| self.encode(value, field.layout_handle()))
                            .collect::<Result<Vec<_>, _>>()?;
                        DynamicValue::record(payload_layout, values)?
                    }
                    (EnumVariant::Record { fields, .. }, EnumPayload::Record(mut values)) => {
                        let layouts = payload_layout
                            .record_fields()
                            .ok_or_else(|| "record variant has no aggregate layout".to_owned())?;
                        if values.len() != fields.len() || values.len() != layouts.len() {
                            return Err(format!(
                                "variant `{}::{}` has wrong field count",
                                name, instance.variant
                            ));
                        }
                        if layouts
                            .iter()
                            .zip(fields)
                            .any(|(layout, field)| layout.name() != field.name)
                        {
                            return Err(format!(
                                "variant `{}::{}` record field order differs from its declaration",
                                name, instance.variant
                            ));
                        }
                        let values = layouts
                            .iter()
                            .map(|field| {
                                let value = values.remove(field.name()).ok_or_else(|| {
                                    format!(
                                        "variant `{}::{}` lacks field `{}`",
                                        name,
                                        instance.variant,
                                        field.name()
                                    )
                                })?;
                                self.encode(value, field.layout_handle())
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        DynamicValue::record(payload_layout, values)?
                    }
                    _ => {
                        return Err(format!(
                            "variant `{}::{}` has wrong payload shape",
                            name, instance.variant
                        ));
                    }
                };
                self.enums.insert(name.clone(), definition);
                DynamicValue::variant(layout, index, payload)
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

    fn decode(&self, mut value: DynamicValue) -> Result<Value, String> {
        let ty = value.descriptor().rils_type().clone();
        match ty {
            Type::Unit => value
                .into_rust::<()>()
                .map(|_| Value::Unit)
                .map_err(|error| error.1),
            Type::Bool => value
                .into_rust::<bool>()
                .map(Value::Bool)
                .map_err(|error| error.1),
            Type::Char => value
                .into_rust::<char>()
                .map(Value::Char)
                .map_err(|error| error.1),
            Type::String => value
                .into_rust::<NativeString>()
                .map(|text| native_string(std::string::String::from(text)))
                .map_err(|error| error.1),
            Type::Option(item_type) => {
                let item = value.take_option()?;
                let item = item.map(|item| self.decode(item)).transpose()?;
                Ok(Value::Option {
                    value: item.map(Rc::new),
                    element_type: Some(*item_type),
                })
            }
            Type::Result(ok_type, error_type) => {
                let (index, item) = value.take_variant()?;
                let item = Rc::new(self.decode(item)?);
                Ok(Value::Result {
                    value: if index == 0 { Ok(item) } else { Err(item) },
                    ok_type: Some(*ok_type),
                    error_type: Some(*error_type),
                })
            }
            Type::Tuple(elements) => {
                let slots = self.take_indexed_fields(&mut value, &elements)?;
                Ok(Value::Tuple(Rc::new(IndexedStorage {
                    elements: RefCell::new(slots),
                    element_type: RefCell::new(None),
                    active_iterators: Default::default(),
                })))
            }
            Type::Array { element, length } => {
                let types = vec![(*element).clone(); length];
                let slots = self.take_indexed_fields(&mut value, &types)?;
                Ok(Value::Array(Rc::new(IndexedStorage {
                    elements: RefCell::new(slots),
                    element_type: RefCell::new(Some(*element)),
                    active_iterators: Default::default(),
                })))
            }
            ty if native_layouts::vec::matches(&ty) => {
                let Type::Named { arguments, .. } = ty else {
                    unreachable!("generated Vec matcher checks a named type")
                };
                let item_type = arguments[0].clone();
                let length = value.sequence_len()?;
                let mut slots = Vec::with_capacity(length);
                for _ in 0..length {
                    let item = self.decode(value.take_sequence_item(0)?)?;
                    slots.push(FieldSlot {
                        value: Some(item),
                        type_annotation: item_type.clone(),
                        references: 0,
                    });
                }
                Ok(Value::Vec(Rc::new(IndexedStorage {
                    elements: RefCell::new(slots),
                    element_type: RefCell::new(Some(item_type)),
                    active_iterators: Default::default(),
                })))
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
                let substitutions = definition
                    .generic_parameters
                    .iter()
                    .zip(&arguments)
                    .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
                    .collect::<HashMap<_, _>>();
                let mut slots = HashMap::new();
                for (index, field) in definition.fields.iter().enumerate() {
                    if field_names[index] != field.name {
                        return Err(format!(
                            "struct `{name}` field order differs from its declaration"
                        ));
                    }
                    let item = self.decode(value.take_field(index)?)?;
                    slots.insert(
                        field.name.clone(),
                        FieldSlot {
                            value: Some(item),
                            type_annotation: field.type_annotation.substitute(&substitutions),
                            references: 0,
                        },
                    );
                }
                let fields = StructFields::from_map(definition.clone(), slots)?;
                Ok(Value::Struct(Rc::new(StructInstance {
                    type_definition: definition,
                    fields: RefCell::new(fields),
                    type_arguments: arguments,
                })))
            }
            Type::Named { name, arguments } if self.enums.contains_key(&name) => {
                let definition = self.enums[&name].clone();
                let (index, mut payload) = value.take_variant()?;
                let declaration = definition
                    .variants
                    .get(index)
                    .ok_or_else(|| format!("enum `{name}` has no variant at index {index}"))?;
                let (variant, payload) = match declaration {
                    EnumVariant::Unit { name, .. } => {
                        if payload.descriptor().rils_type() != &Type::Unit {
                            return Err(format!("unit variant `{name}` has a payload layout"));
                        }
                        (name.clone(), EnumPayload::Unit)
                    }
                    EnumVariant::Tuple { name, fields, .. } => {
                        let layouts = payload.descriptor().record_fields().ok_or_else(|| {
                            format!("tuple variant `{name}` has no aggregate layout")
                        })?;
                        if layouts.len() != fields.len()
                            || layouts
                                .iter()
                                .enumerate()
                                .any(|(index, field)| field.name() != index.to_string())
                        {
                            return Err(format!("tuple variant `{name}` has wrong field order"));
                        }
                        let mut values = Vec::with_capacity(fields.len());
                        for index in 0..fields.len() {
                            values.push(self.decode(payload.take_field(index)?)?);
                        }
                        (name.clone(), EnumPayload::Tuple(values))
                    }
                    EnumVariant::Record { name, fields, .. } => {
                        let layouts = payload.descriptor().record_fields().ok_or_else(|| {
                            format!("record variant `{name}` has no aggregate layout")
                        })?;
                        if layouts.len() != fields.len()
                            || layouts
                                .iter()
                                .zip(fields)
                                .any(|(layout, field)| layout.name() != field.name)
                        {
                            return Err(format!("record variant `{name}` has wrong field order"));
                        }
                        let mut values = HashMap::with_capacity(fields.len());
                        for (index, field) in fields.iter().enumerate() {
                            values.insert(
                                field.name.clone(),
                                self.decode(payload.take_field(index)?)?,
                            );
                        }
                        (name.clone(), EnumPayload::Record(values))
                    }
                };
                Ok(Value::Enum(Rc::new(EnumInstance {
                    type_definition: definition,
                    variant,
                    payload,
                    type_arguments: arguments,
                })))
            }
            ty if native_layouts::integer::layout(&ty).is_some() => {
                native_layouts::integer::field_value(value, &ty)
                    .ok_or_else(|| format!("no integer codec for {ty}"))?
            }
            ty if native_layouts::float::layout(&ty).is_some() => {
                native_layouts::float::field_value(value, &ty)
                    .ok_or_else(|| format!("no float codec for {ty}"))?
            }
            ty => Err(format!("no owned native field decoder for {ty}")),
        }
    }

    fn take_indexed_fields(
        &self,
        value: &mut DynamicValue,
        types: &[Type],
    ) -> Result<Vec<FieldSlot>, String> {
        let fields = value
            .descriptor()
            .record_fields()
            .ok_or_else(|| "expected a tuple or array native layout".to_owned())?;
        if fields.len() != types.len() {
            return Err("tuple or array length does not match its layout".into());
        }
        types
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                let item = self.decode(value.take_field(index)?)?;
                Ok(FieldSlot {
                    value: Some(item),
                    type_annotation: ty.clone(),
                    references: 0,
                })
            })
            .collect()
    }
}
