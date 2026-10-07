use std::fmt;

use super::hash::{display_hash_map, display_hash_set};
use super::{BuiltinType, IndexedStorage, Value};

#[path = "display/native_view.rs"]
mod native_view;

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unit => write!(f, "()"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::I16(value) => write!(f, "{value}"),
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
            Self::Tuple(sequence) => display_sequence(f, sequence, "(", ")", true),
            Self::Array(sequence) | Self::Vec(sequence) => {
                display_sequence(f, sequence, "[", "]", false)
            }
            Self::HashMap(map) => display_hash_map(f, map),
            Self::BTreeMap(map) => super::hash::display_btree_map(f, map),
            Self::BTreeSet(set) => super::hash::display_btree_set(f, set),
            Self::HashSet(set) => display_hash_set(f, set),
            Self::VecDeque(_) => write!(f, "<VecDeque>"),
            Self::BinaryHeap(_) => write!(f, "<BinaryHeap>"),
            Self::OwnedIterator(_) => write!(f, "<iterator>"),
            Self::BorrowedIndexedIterator(_) => write!(f, "<borrowed indexed iterator>"),
            Self::BorrowedMapIterator(_) => write!(f, "<borrowed map iterator>"),
            Self::BorrowedSetIterator(_) => write!(f, "<borrowed set iterator>"),
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
            Self::Native(object) => write!(f, "{}", super::native_ops::display(object)),
            Self::Dynamic(object)
                if super::native_layouts::vec::matches(
                    object.descriptor().layout().rils_type(),
                ) =>
            {
                match super::dynamic_sequence::copy_items(object) {
                    Ok(items) => {
                        write!(f, "[")?;
                        for (index, item) in items.iter().enumerate() {
                            if index > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{item}")?;
                        }
                        write!(f, "]")
                    }
                    Err(_) => write!(f, "<{}>", self.type_name()),
                }
            }
            Self::Dynamic(object) => match self.materialize_native_sum() {
                Some(Ok(value)) => write!(f, "{value}"),
                Some(Err(_)) | None => native_view::display(object, f),
            },
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
    sequence: &IndexedStorage,
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
                Self::Dynamic(object)
                    if super::native_layouts::vec::matches(
                        object.descriptor().layout().rils_type(),
                    ) =>
                {
                    match super::dynamic_sequence::copy_items(object) {
                        Ok(items) => f.debug_list().entries(items).finish(),
                        Err(_) => write!(f, "<{}>", self.type_name()),
                    }
                }
                Self::Dynamic(object) => match self.materialize_native_sum() {
                    Some(Ok(value)) => write!(f, "{value:?}"),
                    _ => native_view::debug(object, f),
                },
                Self::Result {
                    value: Ok(value), ..
                } => f.debug_tuple("Ok").field(value).finish(),
                Self::Result {
                    value: Err(value), ..
                } => f.debug_tuple("Err").field(value).finish(),

                Self::Reference(reference) => match reference.read() {
                    Ok(value) => write!(f, "{value:#?}"),
                    Err(_) => f.write_str("<invalid reference>"),
                },
                Self::Native(object) if object.descriptor().rils_type() == &crate::Type::String => {
                    write!(f, "{:#?}", self.as_string().unwrap_or_default())
                }
                Self::Native(object) if object.descriptor().rils_type() == &crate::Type::Char => {
                    write!(f, "{:#?}", self.as_char().expect("native char payload"))
                }
                _ => write!(f, "{self}"),
            };
        }
        match self {
            Self::Reference(reference) => native_view::debug_reference(reference, f),
            Self::Native(object) if object.descriptor().rils_type() == &crate::Type::String => {
                write!(f, "{:?}", self.as_string().unwrap_or_default())
            }
            Self::Native(object) if object.descriptor().rils_type() == &crate::Type::Char => {
                write!(f, "{:?}", self.as_char().expect("native char payload"))
            }
            Self::Dynamic(object)
                if super::native_layouts::vec::matches(
                    object.descriptor().layout().rils_type(),
                ) =>
            {
                match super::dynamic_sequence::copy_items(object) {
                    Ok(items) => f.debug_list().entries(items).finish(),
                    Err(_) => write!(f, "<{}>", self.type_name()),
                }
            }
            Self::Dynamic(object) => match super::dynamic_option::view(self) {
                Some(Ok((Some(value), _))) => f.debug_tuple("Some").field(&value).finish(),
                Some(Ok((None, _))) => f.write_str("None"),
                _ => match super::dynamic_result::view(self) {
                    Some(Ok((Ok(value), _, _))) => f.debug_tuple("Ok").field(&value).finish(),
                    Some(Ok((Err(value), _, _))) => f.debug_tuple("Err").field(&value).finish(),
                    _ => native_view::debug(object, f),
                },
            },
            _ => write!(f, "{self}"),
        }
    }
}
