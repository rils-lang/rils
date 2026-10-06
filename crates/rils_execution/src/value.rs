use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
};

use crate::{
    ast::{
        AssociatedType, Block, EnumVariant, GenericParameter, NamedField, Parameter, TraitMethod,
    },
    environment::{EnvironmentRef, StorageRef},
    types::{FunctionSignature, Type},
};

#[path = "value/display.rs"]
mod display;

#[path = "value/hash.rs"]
mod hash;
pub use hash::{
    BTreeMapValue, BTreeSetValue, HashKey, HashMapValue, HashSetValue, MapCollection, SetCollection,
};
use hash::{btree_maps_equal, clone_hash_map, hash_maps_equal};

#[path = "value/range.rs"]
mod range;
pub use range::native_range;

pub use rils_value::NativeChildren;
pub type NativeObject = rils_value::NativeObject<Value>;
pub type NativeType = rils_value::NativeType<Value>;
pub type DynamicObject = rils_value::DynamicObject<Value>;
#[path = "value/dynamic_option.rs"]
pub mod dynamic_option;
pub mod dynamic_result;
pub mod dynamic_sequence;
pub mod owned_sum;
#[path = "value/record.rs"]
mod record;
pub use record::StructFields;
mod slot;
pub use slot::FieldSlot;
#[path = "value/native_layouts.rs"]
pub mod native_layouts;
pub mod native_ops;
#[path = "value/record_codec.rs"]
pub mod record_codec;
#[path = "value/record_layout.rs"]
pub mod record_layout;
#[path = "value/string.rs"]
mod string;
pub use string::{native_string, string_payload};
#[path = "value/character.rs"]
mod character;
pub use character::{char_payload, native_char};
mod ownership;
#[path = "value/scalar.rs"]
mod scalar;
pub mod storage;

#[path = "value/reference.rs"]
mod reference;
pub use reference::ReferenceValue;

#[path = "value/iterator.rs"]
mod iterator;
pub use iterator::{
    BorrowedIndexedIteratorValue, BorrowedMapIteratorValue, BorrowedSetIteratorValue,
    IndexedIteratorStorage, OwnedIteratorValue,
};

pub type HostFunctionHandler = dyn Fn(&[Value]) -> Result<Value, String>;

#[derive(Clone)]
pub struct UserFunction {
    pub name: String,
    pub generic_parameters: Vec<GenericParameter>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Block,
    pub closure: EnvironmentRef,
    pub semantic_expression_ids: Option<rils_frontend::semantic::ExpressionIdentityMap>,
}

#[derive(Clone)]
pub struct BytecodeFunctionValue {
    pub function: usize,
    pub name: String,
    pub parameter_count: usize,
    pub captures: Vec<StorageRef>,
    pub bound_arguments: Vec<Value>,
}

#[derive(Clone)]
pub struct BytecodeIteratorValue {
    pub storage: StorageRef,
    pub next_function: usize,
}

#[derive(Clone)]
pub struct NativeFunction {
    pub binding_name: &'static str,
    pub name: &'static str,
    pub min_arity: usize,
    pub max_arity: usize,
    pub signature: Option<FunctionSignature>,
    pub body: NativeFunctionBody,
}

#[derive(Clone, Copy)]
pub enum NativeFunctionBody {
    Rust(fn(&[Value]) -> Result<Value, String>),
    RustOwned(fn(Vec<Value>) -> Result<Value, String>),
    Symbol(&'static str),
}

#[derive(Clone)]
pub struct HostFunction {
    pub name: String,
    pub min_arity: usize,
    pub max_arity: usize,
    pub signature: Option<FunctionSignature>,
    pub function: Rc<HostFunctionHandler>,
}

pub struct HostType {
    pub name: String,
    pub base_types: HashSet<String>,
    pub copy: bool,
    pub methods: RefCell<HashMap<String, Rc<HostFunction>>>,
}

#[derive(Clone)]
pub struct HostObject {
    pub type_definition: Rc<HostType>,
    pub payload: Rc<dyn Any>,
}

#[derive(Clone)]
pub struct HostBoundMethod {
    pub receiver: Rc<Value>,
    pub function: Rc<HostFunction>,
}

pub struct StructType {
    pub name: String,
    pub opaque_native: bool,
    pub generic_parameters: Vec<GenericParameter>,
    pub fields: Vec<NamedField>,
    pub field_indices: std::cell::OnceCell<HashMap<String, usize>>,
    pub methods: RefCell<HashMap<String, Rc<UserFunction>>>,
    pub trait_methods: RefCell<HashMap<String, HashMap<String, Rc<UserFunction>>>>,
    pub implemented_traits: RefCell<HashSet<String>>,
    pub associated_types: RefCell<HashMap<String, HashMap<String, TypeAliasType>>>,
}

impl StructType {
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.field_indices
            .get_or_init(|| {
                self.fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| (field.name.clone(), index))
                    .collect()
            })
            .get(name)
            .copied()
    }
}

pub struct EnumType {
    pub name: String,
    pub generic_parameters: Vec<GenericParameter>,
    pub variants: Vec<EnumVariant>,
    pub methods: RefCell<HashMap<String, Rc<UserFunction>>>,
    pub trait_methods: RefCell<HashMap<String, HashMap<String, Rc<UserFunction>>>>,
    pub implemented_traits: RefCell<HashSet<String>>,
    pub associated_types: RefCell<HashMap<String, HashMap<String, TypeAliasType>>>,
}

pub struct TraitType {
    pub name: String,
    pub generic_parameters: Vec<crate::ast::GenericParameter>,
    pub bounds: Vec<String>,
    pub associated_types: Vec<AssociatedType>,
    pub methods: Vec<TraitMethod>,
}

pub struct ModuleValue {
    pub name: String,
    pub members: EnvironmentRef,
    pub public: RefCell<HashSet<String>>,
}

#[derive(Clone)]
pub struct TypeAliasType {
    pub name: String,
    pub generic_parameters: Vec<GenericParameter>,
    pub target: Type,
}

#[derive(Clone)]
pub struct StructInstance {
    pub type_definition: Rc<StructType>,
    pub fields: RefCell<StructFields>,
    pub type_arguments: Vec<Type>,
}

#[derive(Clone)]
pub struct IndexedStorage {
    pub elements: RefCell<Vec<FieldSlot>>,
    pub element_type: RefCell<Option<Type>>,
    pub active_iterators: std::cell::Cell<usize>,
}

#[derive(Clone)]
pub struct VecDequeValue {
    pub elements: RefCell<VecDeque<Value>>,
    pub element_type: RefCell<Option<Type>>,
}

#[derive(Clone)]
pub struct BinaryHeapValue {
    pub elements: RefCell<Vec<Value>>,
    pub element_type: RefCell<Option<Type>>,
}

#[derive(Clone)]
pub enum EnumPayload {
    Unit,
    Tuple(Vec<Value>),
    Record(HashMap<String, Value>),
}

#[derive(Clone)]
pub struct EnumInstance {
    pub type_definition: Rc<EnumType>,
    pub variant: String,
    pub payload: EnumPayload,
    pub type_arguments: Vec<Type>,
}

#[derive(Clone)]
pub struct VariantConstructor {
    pub type_definition: Rc<EnumType>,
    pub variant: String,
    pub environment: EnvironmentRef,
}

#[derive(Clone)]
pub struct BoundMethod {
    pub receiver: Rc<Value>,
    pub function: Rc<UserFunction>,
}

#[derive(Clone, Copy)]
pub enum BuiltinMethod {
    Native(&'static str),
    IteratorIdentity,
}

#[derive(Clone, Copy)]
pub enum BuiltinType {
    Vec,
    HashMap,
    HashSet,
    Integer(crate::IntegerType),
    Float(crate::FloatType),
}

#[derive(Clone, Copy)]
pub enum BuiltinFunction {
    IntegerIntrinsic {
        symbol: &'static str,
        target: crate::IntegerType,
    },
}

#[derive(Clone)]
pub struct BuiltinBoundMethod {
    pub receiver: Rc<Value>,
    pub method: BuiltinMethod,
}

#[derive(Clone)]
pub struct TraitMethodSelector {
    pub target: Option<Type>,
    pub trait_name: String,
    pub method_name: String,
    pub environment: EnvironmentRef,
}

#[derive(Clone)]
pub enum Value {
    Unit,
    Bool(bool),
    I16(i16),
    I64(i64),
    I128(i128),
    Isize(isize),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),
    F32(f32),
    F64(f64),
    Char(char),
    Tuple(Rc<IndexedStorage>),
    Array(Rc<IndexedStorage>),
    Vec(Rc<IndexedStorage>),
    HashMap(Rc<HashMapValue>),
    BTreeMap(Rc<BTreeMapValue>),
    BTreeSet(Rc<BTreeSetValue>),
    HashSet(Rc<HashSetValue>),
    VecDeque(Rc<VecDequeValue>),
    BinaryHeap(Rc<BinaryHeapValue>),
    OwnedIterator(Rc<OwnedIteratorValue>),
    BorrowedIndexedIterator(Rc<BorrowedIndexedIteratorValue>),
    BorrowedMapIterator(Rc<BorrowedMapIteratorValue>),
    BorrowedSetIterator(Rc<BorrowedSetIteratorValue>),
    BytecodeIterator(Rc<BytecodeIteratorValue>),
    Reference(Rc<ReferenceValue>),
    Option {
        value: Option<Rc<Value>>,
        element_type: Option<Type>,
    },
    Result {
        value: Result<Rc<Value>, Rc<Value>>,
        ok_type: Option<Type>,
        error_type: Option<Type>,
    },
    Function(Rc<UserFunction>),
    BytecodeFunction(Rc<BytecodeFunctionValue>),
    NativeFunction(NativeFunction),
    HostFunction(Rc<HostFunction>),
    HostType(Rc<HostType>),
    HostObject(Rc<HostObject>),
    Native(NativeObject),
    Dynamic(DynamicObject),
    HostBoundMethod(Rc<HostBoundMethod>),
    BuiltinType(BuiltinType),
    BuiltinFunction(BuiltinFunction),
    Module(Rc<ModuleValue>),
    StructType(Rc<StructType>),
    EnumType(Rc<EnumType>),
    TraitType(Rc<TraitType>),
    TypeAlias(Rc<TypeAliasType>),
    Struct(Rc<StructInstance>),
    Enum(Rc<EnumInstance>),
    VariantConstructor(Rc<VariantConstructor>),
    BoundMethod(Rc<BoundMethod>),
    BuiltinBoundMethod(Rc<BuiltinBoundMethod>),
    TraitMethodSelector(Rc<TraitMethodSelector>),
}

impl Value {
    /// Construct a character in native storage.
    pub fn from_char(value: char) -> Self {
        native_char(value)
    }

    /// Read a character from native or legacy storage.
    pub fn as_char(&self) -> Option<char> {
        char_payload(self)
    }

    /// Construct a Rils string in native storage.
    pub fn from_string(value: impl Into<std::string::String>) -> Self {
        native_string(value)
    }

    /// Read owned string text from native or legacy storage.
    pub fn as_string(&self) -> Option<std::string::String> {
        string_payload(self)
    }

    /// Read a Vec snapshot regardless of whether it uses dynamic or legacy storage.
    pub fn as_vec(&self) -> Option<Vec<Value>> {
        dynamic_sequence::view_vec(self)?.ok()
    }

    /// Read an option regardless of whether it uses dynamic or legacy storage.
    pub fn as_option(&self) -> Option<(Option<Value>, Type)> {
        dynamic_option::view_any(self)?.ok()
    }

    /// Read a result regardless of whether it uses dynamic or legacy storage.
    pub fn as_result(&self) -> Option<(Result<Value, Value>, Type, Type)> {
        match self {
            Self::Result {
                value,
                ok_type,
                error_type,
            } => Some((
                value
                    .clone()
                    .map(|item| item.as_ref().clone())
                    .map_err(|item| item.as_ref().clone()),
                ok_type.clone().unwrap_or(Type::Unknown),
                error_type.clone().unwrap_or(Type::Unknown),
            )),
            Self::Dynamic(_) => dynamic_result::view(self)?.ok(),
            _ => None,
        }
    }

    /// Produce the legacy view used by transitional interpreter and VM paths.
    pub fn materialize_native_sum(&self) -> Option<Result<Value, String>> {
        dynamic_option::materialize(self).or_else(|| dynamic_result::materialize(self))
    }

    pub fn type_name(&self) -> String {
        match self {
            Self::Unit => "()".into(),
            Self::Bool(_) => "bool".into(),
            Self::I16(_) => "i16".into(),
            Self::I64(_) => "i64".into(),
            Self::I128(_) => "i128".into(),
            Self::Isize(_) => "isize".into(),
            Self::U8(_) => "u8".into(),
            Self::U16(_) => "u16".into(),
            Self::U32(_) => "u32".into(),
            Self::U64(_) => "u64".into(),
            Self::U128(_) => "u128".into(),
            Self::Usize(_) => "usize".into(),
            Self::F32(_) => "f32".into(),
            Self::F64(_) => "f64".into(),
            Self::Char(_) => "char".into(),
            Self::Tuple(_) => {
                Type::of_value(self).map_or_else(|| "tuple".into(), |ty| ty.to_string())
            }
            Self::Array(_) => {
                Type::of_value(self).map_or_else(|| "array".into(), |ty| ty.to_string())
            }
            Self::Vec(_) => Type::of_value(self).map_or_else(|| "Vec".into(), |ty| ty.to_string()),
            Self::HashMap(_) => {
                Type::of_value(self).map_or_else(|| "HashMap".into(), |ty| ty.to_string())
            }
            Self::BTreeMap(_) => {
                Type::of_value(self).map_or_else(|| "BTreeMap".into(), |ty| ty.to_string())
            }
            Self::BTreeSet(_) => {
                Type::of_value(self).map_or_else(|| "BTreeSet".into(), |ty| ty.to_string())
            }
            Self::HashSet(_) => {
                Type::of_value(self).map_or_else(|| "HashSet".into(), |ty| ty.to_string())
            }
            Self::VecDeque(_) => {
                Type::of_value(self).map_or_else(|| "VecDeque".into(), |ty| ty.to_string())
            }
            Self::BinaryHeap(_) => {
                Type::of_value(self).map_or_else(|| "BinaryHeap".into(), |ty| ty.to_string())
            }
            Self::OwnedIterator(_) => {
                Type::of_value(self).map_or_else(|| "OwnedIterator".into(), |ty| ty.to_string())
            }
            Self::BorrowedIndexedIterator(_) => {
                Type::of_value(self).map_or_else(|| "Iter".into(), |ty| ty.to_string())
            }
            Self::BorrowedMapIterator(_) | Self::BorrowedSetIterator(_) => {
                Type::of_value(self).map_or_else(|| "Iter".into(), |ty| ty.to_string())
            }
            Self::BytecodeIterator(_) => "iterator".into(),
            Self::Reference(reference) => {
                let name = match reference.native_layout() {
                    Ok(Some(layout)) => layout.rils_type().to_string(),
                    _ => match reference.read() {
                        Ok(value) => value.type_name(),
                        Err(_) => return "invalid reference".into(),
                    },
                };
                if reference.mutable {
                    format!("&mut {name}")
                } else {
                    format!("&{name}")
                }
            }
            Self::Option { .. } => "option".into(),
            Self::Result { .. } => {
                Type::of_value(self).map_or_else(|| "Result".into(), |ty| ty.to_string())
            }
            value @ (Self::Function(_)
            | Self::BytecodeFunction(_)
            | Self::NativeFunction(_)
            | Self::HostFunction(_)
            | Self::HostBoundMethod(_)
            | Self::BuiltinFunction(_)
            | Self::VariantConstructor(_)
            | Self::BoundMethod(_)
            | Self::BuiltinBoundMethod(_)
            | Self::TraitMethodSelector(_)) => {
                Type::of_value(value).map_or_else(|| "function".into(), |ty| ty.to_string())
            }
            Self::StructType(definition) => format!("type {}", definition.name),
            Self::BuiltinType(BuiltinType::Vec) => "type Vec".into(),
            Self::BuiltinType(BuiltinType::HashMap) => "type HashMap".into(),
            Self::BuiltinType(BuiltinType::HashSet) => "type HashSet".into(),
            Self::BuiltinType(BuiltinType::Integer(kind)) => format!("type {kind}"),
            Self::BuiltinType(BuiltinType::Float(kind)) => format!("type {kind}"),
            Self::HostType(definition) => format!("type {}", definition.name),
            Self::HostObject(object) => object.type_definition.name.clone(),
            Self::Native(object) => object.descriptor().rils_type().to_string(),
            Self::Dynamic(object) => object.descriptor().layout().rils_type().to_string(),
            Self::Module(module) => format!("module {}", module.name),
            Self::EnumType(definition) => format!("type {}", definition.name),
            Self::TraitType(definition) => format!("trait {}", definition.name),
            Self::TypeAlias(definition) => format!("type alias {}", definition.name),
            Self::Struct(instance) => instance.type_definition.name.clone(),
            Self::Enum(instance) => instance.type_definition.name.clone(),
        }
    }

    pub fn is_truthy(&self) -> bool {
        !matches!(self, Self::Unit | Self::Bool(false))
    }

    pub fn host_payload<T: 'static>(&self) -> Option<&T> {
        let Self::HostObject(object) = self else {
            return None;
        };
        object.payload.downcast_ref::<T>()
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        if matches!(self, Self::Dynamic(_)) || matches!(other, Self::Dynamic(_)) {
            if let (Some(Ok(left)), Some(Ok(right))) = (
                dynamic_sequence::view_vec(self),
                dynamic_sequence::view_vec(other),
            ) {
                return left == right;
            }
            if let (Some((left, _, _)), Some((right, _, _))) = (self.as_result(), other.as_result())
            {
                return left == right;
            }
            return match (
                dynamic_option::view_any(self),
                dynamic_option::view_any(other),
            ) {
                (Some(Ok((left, _))), Some(Ok((right, _)))) => left == right,
                _ => false,
            };
        }
        match (self, other) {
            (Self::Unit, Self::Unit) => true,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::I16(left), Self::I16(right)) => left == right,
            (Self::I64(left), Self::I64(right)) => left == right,
            (Self::I128(left), Self::I128(right)) => left == right,
            (Self::Isize(left), Self::Isize(right)) => left == right,
            (Self::U8(left), Self::U8(right)) => left == right,
            (Self::U16(left), Self::U16(right)) => left == right,
            (Self::U32(left), Self::U32(right)) => left == right,
            (Self::U64(left), Self::U64(right)) => left == right,
            (Self::U128(left), Self::U128(right)) => left == right,
            (Self::Usize(left), Self::Usize(right)) => left == right,
            (Self::Native(_), Self::Usize(right)) => {
                self.as_usize().is_some_and(|left| left == *right)
            }
            (Self::Usize(left), Self::Native(_)) => {
                other.as_usize().is_some_and(|right| *left == right)
            }
            (Self::F32(left), Self::F32(right)) => left == right,
            (Self::F64(left), Self::F64(right)) => left == right,
            (Self::Native(_), Self::F32(right)) => self.as_f32().is_some_and(|left| left == *right),
            (Self::F32(left), Self::Native(_)) => {
                other.as_f32().is_some_and(|right| *left == right)
            }
            (Self::Native(_), Self::F64(right)) => self.as_f64().is_some_and(|left| left == *right),
            (Self::F64(left), Self::Native(_)) => {
                other.as_f64().is_some_and(|right| *left == right)
            }
            (Self::Char(left), Self::Char(right)) => left == right,
            (Self::Native(_), Self::Char(right)) => {
                char_payload(self).is_some_and(|left| left == *right)
            }
            (Self::Char(left), Self::Native(_)) => {
                char_payload(other).is_some_and(|right| *left == right)
            }
            (Self::Native(left), Self::Native(right)) => native_ops::equal(left, right),
            (Self::Native(_), legacy)
                if matches!(Type::of_value(legacy), Some(Type::Integer(_))) =>
            {
                let lowered = crate::numeric::lower_migrated_integer(self.clone());
                !matches!(&lowered, Self::Native(_)) && &lowered == legacy
            }
            (legacy, Self::Native(_))
                if matches!(Type::of_value(legacy), Some(Type::Integer(_))) =>
            {
                let lowered = crate::numeric::lower_migrated_integer(other.clone());
                !matches!(&lowered, Self::Native(_)) && legacy == &lowered
            }
            (Self::Tuple(left), Self::Tuple(right))
            | (Self::Array(left), Self::Array(right))
            | (Self::Vec(left), Self::Vec(right)) => sequence_equal(left, right),
            (Self::Reference(left), Self::Reference(right)) => Rc::ptr_eq(left, right),
            (Self::Option { value: None, .. }, Self::Option { value: None, .. }) => true,
            (
                Self::Option {
                    value: Some(left), ..
                },
                Self::Option {
                    value: Some(right), ..
                },
            ) => left == right,
            (Self::Result { value: left, .. }, Self::Result { value: right, .. }) => {
                match (left, right) {
                    (Ok(left), Ok(right)) | (Err(left), Err(right)) => left == right,
                    _ => false,
                }
            }
            (Self::HashMap(left), Self::HashMap(right)) => hash_maps_equal(left, right),
            (Self::BTreeMap(left), Self::BTreeMap(right)) => btree_maps_equal(left, right),
            (Self::BTreeSet(left), Self::BTreeSet(right)) => {
                left.entries.borrow().eq(&right.entries.borrow())
            }
            (Self::HashSet(left), Self::HashSet(right)) => {
                *left.entries.borrow() == *right.entries.borrow()
            }
            (Self::Struct(left), Self::Struct(right)) => {
                let left_fields = left.fields.borrow();
                let right_fields = right.fields.borrow();
                left.type_definition.name == right.type_definition.name
                    && left.type_arguments == right.type_arguments
                    && left_fields.len() == right_fields.len()
                    && left_fields.iter().all(|(name, field)| {
                        right_fields
                            .get(name)
                            .is_some_and(|other| field.value == other.value)
                    })
            }
            (Self::Enum(left), Self::Enum(right)) => {
                left.type_definition.name == right.type_definition.name
                    && left.type_arguments == right.type_arguments
                    && left.variant == right.variant
                    && enum_payload_equal(&left.payload, &right.payload)
            }
            _ => false,
        }
    }
}

pub fn enum_variant_name(variant: &EnumVariant) -> &str {
    match variant {
        EnumVariant::Unit { name, .. }
        | EnumVariant::Tuple { name, .. }
        | EnumVariant::Record { name, .. } => name,
    }
}

fn clone_sequence(sequence: &IndexedStorage) -> Result<IndexedStorage, String> {
    let elements = sequence
        .elements
        .borrow()
        .iter()
        .map(|slot| {
            let value = slot
                .value
                .as_ref()
                .ok_or_else(|| "cannot clone a partially moved collection".to_string())?;
            Ok(FieldSlot::new(
                slot.type_annotation.clone(),
                value.clone_owned()?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(IndexedStorage {
        active_iterators: std::cell::Cell::new(0),
        elements: RefCell::new(elements),
        element_type: RefCell::new(sequence.element_type.borrow().clone()),
    })
}

fn sequence_equal(left: &IndexedStorage, right: &IndexedStorage) -> bool {
    let left = left.elements.borrow();
    let right = right.elements.borrow();
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| left.value == right.value)
}

fn enum_payload_equal(left: &EnumPayload, right: &EnumPayload) -> bool {
    match (left, right) {
        (EnumPayload::Unit, EnumPayload::Unit) => true,
        (EnumPayload::Tuple(left), EnumPayload::Tuple(right)) => left == right,
        (EnumPayload::Record(left), EnumPayload::Record(right)) => left == right,
        _ => false,
    }
}
