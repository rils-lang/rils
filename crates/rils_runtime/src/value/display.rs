use std::fmt;

use crate::ast::EnumVariant;

use super::hash::{display_hash_map, display_hash_set};
use super::{BuiltinType, EnumPayload, SequenceValue, Value, enum_variant_name};

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unit => write!(f, "()"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::I8(value) => write!(f, "{value}"),
            Self::I16(value) => write!(f, "{value}"),
            Self::I32(value) => write!(f, "{value}"),
            Self::I64(value) => write!(f, "{value}"),
            Self::I128(value) => write!(f, "{value}"),
            Self::Isize(value) => write!(f, "{value}"),
            Self::U8(value) => write!(f, "{value}"),
            Self::U16(value) => write!(f, "{value}"),
            Self::U32(value) => write!(f, "{value}"),
            Self::U64(value) => write!(f, "{value}"),
            Self::U128(value) => write!(f, "{value}"),
            Self::Usize(value) => write!(f, "{value}"),
            Self::F32(value) => write!(f, "{value}"),
            Self::F64(value) => write!(f, "{value}"),
            Self::Char(value) => write!(f, "{value}"),
            Self::String(value) => write!(f, "{value}"),
            Self::Tuple(sequence) => display_sequence(f, sequence, "(", ")", true),
            Self::Array(sequence) | Self::Vec(sequence) => {
                display_sequence(f, sequence, "[", "]", false)
            }
            Self::HashMap(map) => display_hash_map(f, map),
            Self::HashSet(set) => display_hash_set(f, set),
            Self::SequenceIterator(_) => write!(f, "<sequence iterator>"),
            Self::BytecodeIterator(_) => write!(f, "<bytecode iterator>"),
            Self::Reference(reference) => match reference.read() {
                Ok(value) => write!(f, "{value}"),
                Err(_) => write!(f, "<invalid reference>"),
            },
            Self::Option { value: None, .. } => write!(f, "None"),
            Self::Option {
                value: Some(value), ..
            } => write!(f, "Some({value})"),
            Self::Result { value, .. } => match value {
                Ok(value) => write!(f, "Ok({value})"),
                Err(value) => write!(f, "Err({value})"),
            },
            Self::Function(function) => write!(f, "<fn {}>", function.name),
            Self::BytecodeFunction(function) => write!(f, "<fn {}>", function.name),
            Self::NativeFunction(function) => write!(f, "<native fn {}>", function.name),
            Self::HostFunction(function) => write!(f, "<host fn {}>", function.name),
            Self::HostType(definition) => write!(f, "<host type {}>", definition.name),
            Self::HostObject(object) => write!(f, "<{}>", object.type_definition.name),
            Self::HostBoundMethod(method) => write!(f, "<bound host fn {}>", method.function.name),
            Self::BuiltinType(BuiltinType::Vec) => write!(f, "<type Vec>"),
            Self::BuiltinType(BuiltinType::HashMap) => write!(f, "<type HashMap>"),
            Self::BuiltinType(BuiltinType::HashSet) => write!(f, "<type HashSet>"),
            Self::BuiltinType(BuiltinType::Integer(kind)) => write!(f, "<type {kind}>"),
            Self::BuiltinType(BuiltinType::Float(kind)) => write!(f, "<type {kind}>"),
            Self::BuiltinFunction(_) => write!(f, "<builtin function>"),
            Self::Module(module) => write!(f, "<module {}>", module.name),
            Self::StructType(definition) => write!(f, "<struct {}>", definition.name),
            Self::EnumType(definition) => write!(f, "<enum {}>", definition.name),
            Self::TraitType(definition) => write!(f, "<trait {}>", definition.name),
            Self::TypeAlias(definition) => write!(f, "<type alias {}>", definition.name),
            Self::Struct(instance) => {
                write!(f, "{} {{ ", instance.type_definition.name)?;
                let fields = instance.fields.borrow();
                for (index, field) in instance.type_definition.fields.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    let value = &fields[&field.name].value;
                    if let Some(value) = value {
                        write!(f, "{}: {value}", field.name)?;
                    } else {
                        write!(f, "{}: <moved>", field.name)?;
                    }
                }
                write!(f, " }}")
            }
            Self::Enum(instance) => {
                write!(f, "{}::{}", instance.type_definition.name, instance.variant)?;
                match &instance.payload {
                    EnumPayload::Unit => Ok(()),
                    EnumPayload::Tuple(values) => {
                        write!(f, "(")?;
                        for (index, value) in values.iter().enumerate() {
                            if index > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{value}")?;
                        }
                        write!(f, ")")
                    }
                    EnumPayload::Record(values) => {
                        write!(f, " {{ ")?;
                        let variant = instance
                            .type_definition
                            .variants
                            .iter()
                            .find(|variant| enum_variant_name(variant) == instance.variant)
                            .expect("enum instance refers to a declared variant");
                        let fields = match variant {
                            EnumVariant::Record { fields, .. } => fields,
                            _ => unreachable!(),
                        };
                        for (index, field) in fields.iter().enumerate() {
                            if index > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{}: {}", field.name, values[&field.name])?;
                        }
                        write!(f, " }}")
                    }
                }
            }
            Self::Range(range) => write!(f, "{}..{}", range.current, range.end),
            Self::VariantConstructor(constructor) => write!(
                f,
                "<constructor {}::{}>",
                constructor.type_definition.name, constructor.variant
            ),
            Self::BoundMethod(method) => write!(f, "<bound fn {}>", method.function.name),
            Self::BuiltinBoundMethod(_) => write!(f, "<bound builtin method>"),
            Self::TraitMethodSelector(selector) => write!(
                f,
                "<trait method {}::{}>",
                selector.trait_name, selector.method_name
            ),
        }
    }
}

fn display_sequence(
    f: &mut fmt::Formatter<'_>,
    sequence: &SequenceValue,
    open: &str,
    close: &str,
    tuple: bool,
) -> fmt::Result {
    write!(f, "{open}")?;
    let elements = sequence.elements.borrow();
    for (index, slot) in elements.iter().enumerate() {
        if index > 0 {
            write!(f, ", ")?;
        }
        match &slot.value {
            Some(value) => write!(f, "{value}")?,
            None => write!(f, "<moved>")?,
        }
    }
    if tuple && elements.len() == 1 {
        write!(f, ",")?;
    }
    write!(f, "{close}")
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            return match self {
                Self::Tuple(sequence) => {
                    let elements = sequence.elements.borrow();
                    let mut tuple = f.debug_tuple("");
                    for slot in elements.iter() {
                        match &slot.value {
                            Some(value) => {
                                tuple.field(value);
                            }
                            None => {
                                tuple.field(&"<moved>");
                            }
                        }
                    }
                    tuple.finish()
                }
                Self::Array(sequence) | Self::Vec(sequence) => {
                    let elements = sequence.elements.borrow();
                    let mut list = f.debug_list();
                    for slot in elements.iter() {
                        match &slot.value {
                            Some(value) => {
                                list.entry(value);
                            }
                            None => {
                                list.entry(&"<moved>");
                            }
                        }
                    }
                    list.finish()
                }
                Self::Option { value: None, .. } => f.write_str("None"),
                Self::Option {
                    value: Some(value), ..
                } => f.debug_tuple("Some").field(value).finish(),
                Self::Result {
                    value: Ok(value), ..
                } => f.debug_tuple("Ok").field(value).finish(),
                Self::Result {
                    value: Err(value), ..
                } => f.debug_tuple("Err").field(value).finish(),
                Self::Struct(instance) => {
                    let fields = instance.fields.borrow();
                    let mut structure = f.debug_struct(&instance.type_definition.name);
                    for field in &instance.type_definition.fields {
                        match &fields[&field.name].value {
                            Some(value) => {
                                structure.field(&field.name, value);
                            }
                            None => {
                                structure.field(&field.name, &"<moved>");
                            }
                        }
                    }
                    structure.finish()
                }
                Self::Enum(instance) => {
                    let name = format!("{}::{}", instance.type_definition.name, instance.variant);
                    match &instance.payload {
                        EnumPayload::Unit => f.write_str(&name),
                        EnumPayload::Tuple(values) => {
                            let mut tuple = f.debug_tuple(&name);
                            for value in values {
                                tuple.field(value);
                            }
                            tuple.finish()
                        }
                        EnumPayload::Record(values) => {
                            let variant = instance
                                .type_definition
                                .variants
                                .iter()
                                .find(|variant| enum_variant_name(variant) == instance.variant)
                                .expect("enum instance refers to a declared variant");
                            let EnumVariant::Record { fields, .. } = variant else {
                                unreachable!()
                            };
                            let mut structure = f.debug_struct(&name);
                            for field in fields {
                                structure.field(&field.name, &values[&field.name]);
                            }
                            structure.finish()
                        }
                    }
                }
                Self::Reference(reference) => match reference.read() {
                    Ok(value) => write!(f, "{value:#?}"),
                    Err(_) => f.write_str("<invalid reference>"),
                },
                Self::String(value) => write!(f, "{value:#?}"),
                _ => write!(f, "{self}"),
            };
        }
        match self {
            Self::String(value) => write!(f, "{value:?}"),
            _ => write!(f, "{self}"),
        }
    }
}
