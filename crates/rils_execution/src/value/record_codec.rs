//! Owned conversion between legacy execution values and composed native bytes.
//!
//! The codec moves structural values into native fields and restores runtime
//! wrappers at the execution boundary. Nominal declarations are retained in
//! the codec so nested structs and enums can be reconstructed after a move.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue};

use crate::{Type, ast::EnumVariant};

use super::{
    EnumInstance, EnumPayload, EnumType, FieldSlot, HashKey, IndexedStorage, StructFields,
    StructInstance, StructType, Value, native_layouts, native_string,
};

#[derive(Clone, Default)]
pub struct NativeRecordCodec {
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

    pub(crate) fn nominal_definition(&self, ty: &Type) -> Option<Value> {
        let Type::Named { name, .. } = ty else {
            return None;
        };
        self.structs
            .get(name)
            .cloned()
            .map(Value::StructType)
            .or_else(|| self.enums.get(name).cloned().map(Value::EnumType))
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

/// Restore a user nominal value that was exposed through a legacy sum wrapper.
/// Registered standard-library payloads retain their native handle.
pub fn restore_owned_nominal(
    value: Value,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Result<Value, String> {
    let Value::Dynamic(object) = value else {
        return Ok(value);
    };
    let Type::Named { name, .. } = object.descriptor().layout().rils_type() else {
        return Ok(Value::Dynamic(object));
    };
    let candidates = structs
        .iter()
        .filter(|definition| definition.name == *name)
        .count()
        + enums
            .iter()
            .filter(|definition| definition.name == *name)
            .count();
    match candidates {
        0 => return Ok(Value::Dynamic(object)),
        1 => {}
        _ => return Err(format!("ambiguous nominal declaration for {name}")),
    }
    let payload = object.into_value().map_err(|failure| failure.1)?;
    NativeRecordCodec::with_definitions(structs, enums).from_native(payload)
}

impl NativeRecordCodec {
    fn encode(&mut self, value: Value, layout: Rc<DynamicLayout>) -> Result<DynamicValue, String> {
        if self.require_owned && value.is_partially_moved() {
            return Err("cannot move a partially moved value into native storage".into());
        }
        let ty = layout.rils_type().clone();
        if let Value::Dynamic(object) = &value
            && matches!(ty, Type::Option(_) | Type::Result(_, _))
            && object.descriptor().layout().rils_type() != &ty
            && ty.accepts(&value)
        {
            let Value::Dynamic(object) = value else {
                unreachable!()
            };
            let payload = object.into_value().map_err(|error| error.1)?;
            let value = self.decode(payload)?;
            return self.encode(value, layout);
        }
        match (ty, value) {
            (Type::Reference { .. }, Value::Reference(value)) => {
                super::runtime_layouts::encode_reference(value, layout)
            }
            (Type::Function { .. }, value) => {
                super::runtime_layouts::encode_callable(value, layout)
            }
            (Type::Named { .. }, Value::HostObject(value)) => {
                super::runtime_layouts::encode_host(value, layout)
            }
            (Type::Unit, Value::Unit) => DynamicValue::from_rust(layout, ()),
            (Type::Bool, Value::Bool(value)) => DynamicValue::from_rust(layout, value),
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
            (ty, Value::VecDeque(queue)) if native_layouts::vec_deque::matches(&ty) => {
                let queue = Rc::try_unwrap(queue)
                    .map_err(|_| "cannot move a shared VecDeque".to_owned())?;
                let item = layout
                    .sequence_item()
                    .ok_or_else(|| "expected a native sequence layout for VecDeque".to_owned())?;
                let values = queue
                    .elements
                    .into_inner()
                    .into_iter()
                    .map(|value| self.encode(value, item.clone()))
                    .collect::<Result<Vec<_>, _>>()?;
                DynamicValue::sequence(layout, values)
            }
            (ty, Value::BinaryHeap(heap)) if native_layouts::binary_heap::matches(&ty) => {
                let heap = Rc::try_unwrap(heap)
                    .map_err(|_| "cannot move a shared BinaryHeap".to_owned())?;
                let item = layout
                    .sequence_item()
                    .ok_or_else(|| "expected a native sequence layout for BinaryHeap".to_owned())?;
                let values = heap
                    .elements
                    .into_inner()
                    .into_iter()
                    .map(|value| self.encode(value, item.clone()))
                    .collect::<Result<Vec<_>, _>>()?;
                DynamicValue::sequence(layout, values)
            }
            (ty, Value::HashSet(set)) if native_layouts::hash_set::matches(&ty) => {
                let set =
                    Rc::try_unwrap(set).map_err(|_| "cannot move a shared HashSet".to_owned())?;
                if set.borrowed.get() != 0 {
                    return Err("cannot move a borrowed HashSet".into());
                }
                self.encode_set(layout, set.entries.into_inner())
            }
            (ty, Value::BTreeSet(set)) if native_layouts::btree_set::matches(&ty) => {
                let set =
                    Rc::try_unwrap(set).map_err(|_| "cannot move a shared BTreeSet".to_owned())?;
                if set.borrowed.get() != 0 {
                    return Err("cannot move a borrowed BTreeSet".into());
                }
                self.encode_set(layout, set.entries.into_inner())
            }
            (ty, Value::HashMap(map)) if native_layouts::hash_map::matches(&ty) => {
                let map =
                    Rc::try_unwrap(map).map_err(|_| "cannot move a shared HashMap".to_owned())?;
                if map.borrowed.get() != 0 {
                    return Err("cannot move a borrowed HashMap".into());
                }
                self.encode_map(layout, map.entries.into_inner())
            }
            (ty, Value::BTreeMap(map)) if native_layouts::btree_map::matches(&ty) => {
                let map =
                    Rc::try_unwrap(map).map_err(|_| "cannot move a shared BTreeMap".to_owned())?;
                if map.borrowed.get() != 0 {
                    return Err("cannot move a borrowed BTreeMap".into());
                }
                self.encode_map(layout, map.entries.into_inner())
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

    fn encode_set(
        &mut self,
        layout: Rc<DynamicLayout>,
        entries: impl IntoIterator<Item = HashKey>,
    ) -> Result<DynamicValue, String> {
        let item = layout
            .sequence_item()
            .ok_or_else(|| "expected a native set sequence layout".to_owned())?;
        let values = entries
            .into_iter()
            .map(|key| self.encode(key.into_value(), item.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        DynamicValue::sequence(layout, values)
    }

    fn encode_map(
        &mut self,
        layout: Rc<DynamicLayout>,
        entries: impl IntoIterator<Item = (HashKey, FieldSlot)>,
    ) -> Result<DynamicValue, String> {
        let pair = layout
            .sequence_item()
            .ok_or_else(|| "expected a native map sequence layout".to_owned())?;
        let fields = pair
            .record_fields()
            .ok_or_else(|| "expected a native key-value aggregate layout".to_owned())?;
        if fields.len() != 2 {
            return Err("native map entry must have two fields".into());
        }
        let key_layout = fields[0].layout_handle();
        let value_layout = fields[1].layout_handle();
        let values = entries
            .into_iter()
            .map(|(key, slot)| {
                if slot.references != 0 {
                    return Err("cannot move a referenced map value".into());
                }
                let value = slot
                    .value
                    .ok_or_else(|| "cannot move a partially moved map value".to_owned())?;
                DynamicValue::record(
                    pair.clone(),
                    vec![
                        self.encode(key.into_value(), key_layout.clone())?,
                        self.encode(value, value_layout.clone())?,
                    ],
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        DynamicValue::sequence(layout, values)
    }

    fn decode(&self, mut value: DynamicValue) -> Result<Value, String> {
        if super::runtime_layouts::is_execution_leaf(value.descriptor()) {
            return super::runtime_layouts::decode(value);
        }
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
                .map(super::native_char)
                .map_err(|error| error.1),
            Type::String => value
                .into_rust::<NativeString>()
                .map(|text| native_string(std::string::String::from(text)))
                .map_err(|error| error.1),
            Type::Named { .. } if value.descriptor().is_rust_value() => {
                let layout = value.layout_handle();
                let descriptor = Rc::new(rils_value::DynamicType::new(layout));
                Ok(Value::Dynamic(super::DynamicObject::new(
                    descriptor, value,
                )?))
            }
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
            _ if value.descriptor().sequence_item().is_some() => {
                let layout = value.layout_handle();
                Ok(Value::Dynamic(super::DynamicObject::new(
                    Rc::new(rils_value::DynamicType::new(layout)),
                    value,
                )?))
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
                        FieldSlot::new(field.type_annotation.substitute(&substitutions), item),
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
                Ok(FieldSlot::new(ty.clone(), item))
            })
            .collect()
    }
}
