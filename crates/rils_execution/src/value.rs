use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    rc::{Rc, Weak as StdWeak},
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
pub use range::RangeValue;

#[path = "value/reference.rs"]
mod reference;
pub use reference::ReferenceValue;

#[path = "value/iterator.rs"]
mod iterator;
pub use iterator::{
    BorrowedMapIteratorValue, BorrowedSequenceIterValue, BorrowedSetIteratorValue,
    OwnedIteratorValue,
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
    pub function: fn(&[Value]) -> Result<Value, String>,
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
    pub generic_parameters: Vec<GenericParameter>,
    pub fields: Vec<NamedField>,
    pub methods: RefCell<HashMap<String, Rc<UserFunction>>>,
    pub trait_methods: RefCell<HashMap<String, HashMap<String, Rc<UserFunction>>>>,
    pub implemented_traits: RefCell<HashSet<String>>,
    pub associated_types: RefCell<HashMap<String, HashMap<String, TypeAliasType>>>,
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
    pub fields: RefCell<HashMap<String, FieldSlot>>,
    pub type_arguments: Vec<Type>,
}

#[derive(Clone)]
pub struct FieldSlot {
    pub value: Option<Value>,
    pub type_annotation: Type,
    pub references: usize,
}

#[derive(Clone)]
pub struct SequenceValue {
    pub elements: RefCell<Vec<FieldSlot>>,
    pub element_type: RefCell<Option<Type>>,
    pub active_iterators: std::cell::Cell<usize>,
}

#[derive(Clone)]
pub struct RcValue {
    pub value: Value,
    pub type_argument: Type,
}

#[derive(Clone)]
pub struct WeakValue {
    pub value: StdWeak<RcValue>,
    pub type_argument: Type,
}

#[derive(Clone)]
pub struct CellValue {
    pub value: RefCell<Value>,
    pub type_argument: Type,
}

#[derive(Clone)]
pub struct RefCellValue {
    pub storage: StorageRef,
    pub type_argument: Type,
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
    Runtime(rils_builtins::BuiltinId),
    IntegerIntrinsic(rils_builtins::BuiltinId),
    FloatIntrinsic(rils_builtins::BuiltinId),
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
    VecNew,
    VecFrom,
    HashMapNew,
    HashSetNew,
    RcNew,
    CellNew,
    RefCellNew,
    VecDequeNew,
    BinaryHeapNew,
    BTreeMapNew,
    BTreeSetNew,
    IntegerIntrinsic {
        id: rils_builtins::BuiltinId,
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
    I8(i8),
    I16(i16),
    I32(i32),
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
    String(Rc<str>),
    Tuple(Rc<SequenceValue>),
    Array(Rc<SequenceValue>),
    Vec(Rc<SequenceValue>),
    HashMap(Rc<HashMapValue>),
    BTreeMap(Rc<BTreeMapValue>),
    BTreeSet(Rc<BTreeSetValue>),
    HashSet(Rc<HashSetValue>),
    Rc(Rc<RcValue>),
    Weak(Rc<WeakValue>),
    Cell(Rc<CellValue>),
    RefCell(Rc<RefCellValue>),
    VecDeque(Rc<VecDequeValue>),
    BinaryHeap(Rc<BinaryHeapValue>),
    OwnedIterator(Rc<OwnedIteratorValue>),
    BorrowedSequenceIter(Rc<BorrowedSequenceIterValue>),
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
    Range(RangeValue),
    VariantConstructor(Rc<VariantConstructor>),
    BoundMethod(Rc<BoundMethod>),
    BuiltinBoundMethod(Rc<BuiltinBoundMethod>),
    TraitMethodSelector(Rc<TraitMethodSelector>),
}

impl Value {
    pub fn is_copy(&self) -> bool {
        match self {
            Self::Unit | Self::Bool(_) | Self::Char(_) => true,
            Self::I8(_) => rils_builtins::native_implements("i8", "Copy"),
            Self::I16(_) => rils_builtins::native_implements("i16", "Copy"),
            Self::I32(_) => rils_builtins::native_implements("i32", "Copy"),
            Self::I64(_) => rils_builtins::native_implements("i64", "Copy"),
            Self::I128(_) => rils_builtins::native_implements("i128", "Copy"),
            Self::Isize(_) => rils_builtins::native_implements("isize", "Copy"),
            Self::U8(_) => rils_builtins::native_implements("u8", "Copy"),
            Self::U16(_) => rils_builtins::native_implements("u16", "Copy"),
            Self::U32(_) => rils_builtins::native_implements("u32", "Copy"),
            Self::U64(_) => rils_builtins::native_implements("u64", "Copy"),
            Self::U128(_) => rils_builtins::native_implements("u128", "Copy"),
            Self::Usize(_) => rils_builtins::native_implements("usize", "Copy"),
            Self::F32(_) => rils_builtins::native_implements("f32", "Copy"),
            Self::F64(_) => rils_builtins::native_implements("f64", "Copy"),
            Self::Reference(_) => true,
            Self::Option { value: None, .. } => true,
            Self::Option {
                value: Some(value), ..
            } => value.is_copy(),
            Self::Result { value, .. } => match value {
                Ok(value) | Err(value) => value.is_copy(),
            },
            Self::Tuple(sequence) | Self::Array(sequence) => sequence
                .elements
                .borrow()
                .iter()
                .all(|slot| slot.value.as_ref().is_some_and(Value::is_copy)),
            Self::Struct(instance) => instance
                .fields
                .borrow()
                .values()
                .all(|field| field.value.as_ref().is_some_and(Value::is_copy)),
            Self::Enum(instance) => match &instance.payload {
                EnumPayload::Unit => true,
                EnumPayload::Tuple(values) => values.iter().all(Value::is_copy),
                EnumPayload::Record(values) => values.values().all(Value::is_copy),
            },
            Self::Function(_)
            | Self::BytecodeFunction(_)
            | Self::NativeFunction(_)
            | Self::HostFunction(_)
            | Self::HostType(_)
            | Self::HostBoundMethod(_)
            | Self::BuiltinType(_)
            | Self::BuiltinFunction(_)
            | Self::Module(_)
            | Self::StructType(_)
            | Self::EnumType(_)
            | Self::TraitType(_)
            | Self::TypeAlias(_)
            | Self::VariantConstructor(_)
            | Self::BoundMethod(_)
            | Self::BuiltinBoundMethod(_)
            | Self::TraitMethodSelector(_) => true,
            // Portable host handles are opaque, copyable identity tokens. The payload is
            // reference-counted internally, but copying the token must not copy
            // or transfer ownership of the host object itself.
            Self::HostObject(object) => object.type_definition.copy,
            Self::String(_) => rils_builtins::native_implements("string", "Copy"),
            Self::Range(_)
            | Self::Rc(_)
            | Self::Weak(_)
            | Self::Cell(_)
            | Self::RefCell(_)
            | Self::VecDeque(_)
            | Self::BinaryHeap(_)
            | Self::Vec(_)
            | Self::HashMap(_)
            | Self::BTreeMap(_)
            | Self::BTreeSet(_)
            | Self::HashSet(_)
            | Self::OwnedIterator(_)
            | Self::BorrowedSequenceIter(_)
            | Self::BorrowedMapIterator(_)
            | Self::BorrowedSetIterator(_)
            | Self::BytecodeIterator(_) => false,
        }
    }

    pub fn contains_reference(&self) -> bool {
        match self {
            Self::BinaryHeap(heap) => heap.elements.borrow().iter().any(Value::contains_reference),
            Self::VecDeque(queue) => queue
                .elements
                .borrow()
                .iter()
                .any(|value| value.contains_reference()),
            Self::Reference(_) => true,
            Self::BytecodeFunction(function) => {
                function.captures.iter().any(|slot| {
                    slot.borrow()
                        .read()
                        .ok()
                        .is_some_and(|value| value.contains_reference())
                }) || function
                    .bound_arguments
                    .iter()
                    .any(Value::contains_reference)
            }
            Self::BytecodeIterator(iterator) => iterator
                .storage
                .borrow()
                .read()
                .ok()
                .is_some_and(|value| value.contains_reference()),
            Self::Option {
                value: Some(value), ..
            } => value.contains_reference(),
            Self::Result { value, .. } => match value {
                Ok(value) | Err(value) => value.contains_reference(),
            },
            Self::Tuple(sequence) | Self::Array(sequence) | Self::Vec(sequence) => sequence
                .elements
                .borrow()
                .iter()
                .filter_map(|slot| slot.value.as_ref())
                .any(Value::contains_reference),
            Self::HashMap(map) => map
                .entries
                .borrow()
                .values()
                .filter_map(|slot| slot.value.as_ref())
                .any(Value::contains_reference),
            Self::BTreeMap(map) => map
                .entries
                .borrow()
                .values()
                .filter_map(|slot| slot.value.as_ref())
                .any(Value::contains_reference),
            Self::Struct(instance) => instance
                .fields
                .borrow()
                .values()
                .filter_map(|field| field.value.as_ref())
                .any(Value::contains_reference),
            Self::Enum(instance) => match &instance.payload {
                EnumPayload::Unit => false,
                EnumPayload::Tuple(values) => values.iter().any(Value::contains_reference),
                EnumPayload::Record(values) => values.values().any(Value::contains_reference),
            },
            Self::Range(_) => false,
            Self::OwnedIterator(iterator) => iterator.contains_reference(),
            Self::BorrowedSequenceIter(_) => true,
            Self::BorrowedMapIterator(_) | Self::BorrowedSetIterator(_) => true,
            _ => false,
        }
    }

    pub fn contains_local_reference(&self, environment: &EnvironmentRef) -> bool {
        match self {
            Self::BTreeMap(map) => map
                .entries
                .borrow()
                .values()
                .filter_map(|slot| slot.value.as_ref())
                .any(|value| value.contains_local_reference(environment)),
            Self::BinaryHeap(heap) => heap
                .elements
                .borrow()
                .iter()
                .any(|value| value.contains_local_reference(environment)),
            Self::VecDeque(queue) => queue
                .elements
                .borrow()
                .iter()
                .any(|value| value.contains_local_reference(environment)),
            Self::Reference(reference) => reference.is_local_to(environment),
            Self::BorrowedSequenceIter(iterator) => iterator.source.is_local_to(environment),
            Self::BorrowedMapIterator(iterator) => iterator.source.is_local_to(environment),
            Self::BorrowedSetIterator(iterator) => iterator.source.is_local_to(environment),
            Self::Option {
                value: Some(value), ..
            } => value.contains_local_reference(environment),
            Self::Result { value, .. } => match value {
                Ok(value) | Err(value) => value.contains_local_reference(environment),
            },
            Self::Tuple(sequence) | Self::Array(sequence) | Self::Vec(sequence) => sequence
                .elements
                .borrow()
                .iter()
                .filter_map(|slot| slot.value.as_ref())
                .any(|value| value.contains_local_reference(environment)),
            Self::Struct(instance) => instance
                .fields
                .borrow()
                .values()
                .filter_map(|field| field.value.as_ref())
                .any(|value| value.contains_local_reference(environment)),
            Self::Enum(instance) => match &instance.payload {
                EnumPayload::Unit => false,
                EnumPayload::Tuple(values) => values
                    .iter()
                    .any(|value| value.contains_local_reference(environment)),
                EnumPayload::Record(values) => values
                    .values()
                    .any(|value| value.contains_local_reference(environment)),
            },
            _ => false,
        }
    }

    pub fn has_active_references(&self) -> bool {
        match self {
            Self::BinaryHeap(heap) => heap
                .elements
                .borrow()
                .iter()
                .any(Value::has_active_references),
            Self::VecDeque(queue) => queue
                .elements
                .borrow()
                .iter()
                .any(|value| value.has_active_references()),
            Self::Struct(instance) => instance.fields.borrow().values().any(|field| {
                field.references > 0
                    || field
                        .value
                        .as_ref()
                        .is_some_and(Value::has_active_references)
            }),
            Self::Tuple(sequence) | Self::Array(sequence) | Self::Vec(sequence) => {
                sequence.active_iterators.get() > 0
                    || sequence.elements.borrow().iter().any(|slot| {
                        slot.references > 0
                            || slot
                                .value
                                .as_ref()
                                .is_some_and(Value::has_active_references)
                    })
            }
            Self::HashMap(map) => {
                map.borrowed.get() > 0
                    || map.entries.borrow().values().any(|slot| {
                        slot.references > 0
                            || slot
                                .value
                                .as_ref()
                                .is_some_and(Value::has_active_references)
                    })
            }
            Self::BTreeMap(map) => {
                map.borrowed.get() > 0
                    || map.entries.borrow().values().any(|slot| {
                        slot.references > 0
                            || slot
                                .value
                                .as_ref()
                                .is_some_and(Value::has_active_references)
                    })
            }
            Self::HashSet(set) => set.borrowed.get() > 0,
            Self::BTreeSet(set) => set.borrowed.get() > 0,
            _ => false,
        }
    }

    pub fn is_partially_moved(&self) -> bool {
        match self {
            Self::Struct(instance) => instance
                .fields
                .borrow()
                .values()
                .any(|field| field.value.is_none()),
            Self::Tuple(sequence) | Self::Array(sequence) | Self::Vec(sequence) => sequence
                .elements
                .borrow()
                .iter()
                .any(|slot| slot.value.is_none()),
            Self::HashMap(map) => map
                .entries
                .borrow()
                .values()
                .any(|slot| slot.value.is_none()),
            Self::BTreeMap(map) => map
                .entries
                .borrow()
                .values()
                .any(|slot| slot.value.is_none()),
            _ => false,
        }
    }

    pub fn clone_owned(&self) -> Result<Self, String> {
        Ok(match self {
            Self::Reference(reference) => return Ok(Self::Reference(reference.clone())),
            Self::Option {
                value,
                element_type,
            } => Self::Option {
                value: value
                    .as_ref()
                    .map(|value| value.clone_owned().map(Rc::new))
                    .transpose()?,
                element_type: element_type.clone(),
            },
            Self::Result {
                value,
                ok_type,
                error_type,
            } => Self::Result {
                value: match value {
                    Ok(value) => Ok(Rc::new(value.clone_owned()?)),
                    Err(value) => Err(Rc::new(value.clone_owned()?)),
                },
                ok_type: ok_type.clone(),
                error_type: error_type.clone(),
            },
            Self::Tuple(sequence) => Self::Tuple(Rc::new(clone_sequence(sequence)?)),
            Self::Array(sequence) => Self::Array(Rc::new(clone_sequence(sequence)?)),
            Self::VecDeque(queue) => Self::VecDeque(Rc::new(VecDequeValue {
                elements: RefCell::new(
                    queue
                        .elements
                        .borrow()
                        .iter()
                        .map(Value::clone_owned)
                        .collect::<Result<VecDeque<_>, _>>()?,
                ),
                element_type: RefCell::new(queue.element_type.borrow().clone()),
            })),
            Self::BinaryHeap(heap) => Self::BinaryHeap(Rc::new(BinaryHeapValue {
                elements: RefCell::new(
                    heap.elements
                        .borrow()
                        .iter()
                        .map(Value::clone_owned)
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                element_type: RefCell::new(heap.element_type.borrow().clone()),
            })),
            Self::Vec(sequence) => Self::Vec(Rc::new(clone_sequence(sequence)?)),
            Self::HashMap(map) => Self::HashMap(Rc::new(clone_hash_map(map)?)),
            Self::BTreeMap(map) => Self::BTreeMap(Rc::new(hash::clone_btree_map(map)?)),
            Self::BTreeSet(set) => Self::BTreeSet(Rc::new(BTreeSetValue {
                borrowed: std::cell::Cell::new(0),
                entries: RefCell::new(set.entries.borrow().clone()),
                element_type: RefCell::new(set.element_type.borrow().clone()),
            })),
            Self::HashSet(set) => Self::HashSet(Rc::new(HashSetValue {
                borrowed: std::cell::Cell::new(0),
                entries: RefCell::new(set.entries.borrow().clone()),
                element_type: RefCell::new(set.element_type.borrow().clone()),
            })),
            Self::OwnedIterator(_) => return Err("iterators cannot be cloned".into()),
            Self::BorrowedSequenceIter(_) => return Err("iterators cannot be cloned".into()),
            Self::BorrowedMapIterator(_) | Self::BorrowedSetIterator(_) => {
                return Err("iterators cannot be cloned".into());
            }
            Self::BytecodeIterator(_) => return Err("iterators cannot be cloned".into()),
            Self::Struct(instance) => {
                let source = instance.fields.borrow();
                let mut fields = HashMap::new();
                for (name, field) in source.iter() {
                    let value = field.value.as_ref().ok_or_else(|| {
                        format!(
                            "cannot clone partially moved struct `{}`",
                            instance.type_definition.name
                        )
                    })?;
                    fields.insert(
                        name.clone(),
                        FieldSlot {
                            value: Some(value.clone_owned()?),
                            type_annotation: field.type_annotation.clone(),
                            references: 0,
                        },
                    );
                }
                Self::Struct(Rc::new(StructInstance {
                    type_definition: instance.type_definition.clone(),
                    fields: RefCell::new(fields),
                    type_arguments: instance.type_arguments.clone(),
                }))
            }
            Self::Enum(instance) => {
                let payload = match &instance.payload {
                    EnumPayload::Unit => EnumPayload::Unit,
                    EnumPayload::Tuple(values) => {
                        EnumPayload::Tuple(values.iter().map(Value::clone_owned).collect::<Result<
                            Vec<_>,
                            _,
                        >>(
                        )?)
                    }
                    EnumPayload::Record(values) => EnumPayload::Record(
                        values
                            .iter()
                            .map(|(name, value)| Ok((name.clone(), value.clone_owned()?)))
                            .collect::<Result<HashMap<_, _>, String>>()?,
                    ),
                };
                Self::Enum(Rc::new(EnumInstance {
                    type_definition: instance.type_definition.clone(),
                    variant: instance.variant.clone(),
                    payload,
                    type_arguments: instance.type_arguments.clone(),
                }))
            }
            Self::Range(range) => Self::Range(range.clone()),
            value => value.clone(),
        })
    }

    pub fn type_name(&self) -> String {
        match self {
            Self::Unit => "()".into(),
            Self::Bool(_) => "bool".into(),
            Self::I8(_) => "i8".into(),
            Self::I16(_) => "i16".into(),
            Self::I32(_) => "i32".into(),
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
            Self::String(_) => "string".into(),
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
            Self::Rc(_) => Type::of_value(self).map_or_else(|| "Rc".into(), |ty| ty.to_string()),
            Self::Weak(_) => {
                Type::of_value(self).map_or_else(|| "Weak".into(), |ty| ty.to_string())
            }
            Self::Cell(_) => {
                Type::of_value(self).map_or_else(|| "Cell".into(), |ty| ty.to_string())
            }
            Self::RefCell(_) => {
                Type::of_value(self).map_or_else(|| "RefCell".into(), |ty| ty.to_string())
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
            Self::BorrowedSequenceIter(_) => {
                Type::of_value(self).map_or_else(|| "Iter".into(), |ty| ty.to_string())
            }
            Self::BorrowedMapIterator(_) | Self::BorrowedSetIterator(_) => {
                Type::of_value(self).map_or_else(|| "Iter".into(), |ty| ty.to_string())
            }
            Self::BytecodeIterator(_) => "iterator".into(),
            Self::Reference(reference) => reference.read().map_or_else(
                |_| "invalid reference".into(),
                |value| {
                    if reference.mutable {
                        format!("&mut {}", value.type_name())
                    } else {
                        format!("&{}", value.type_name())
                    }
                },
            ),
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
            Self::Module(module) => format!("module {}", module.name),
            Self::EnumType(definition) => format!("type {}", definition.name),
            Self::TraitType(definition) => format!("trait {}", definition.name),
            Self::TypeAlias(definition) => format!("type alias {}", definition.name),
            Self::Struct(instance) => instance.type_definition.name.clone(),
            Self::Enum(instance) => instance.type_definition.name.clone(),
            Self::Range(_) => "Range".into(),
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
        match (self, other) {
            (Self::Unit, Self::Unit) => true,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::I8(left), Self::I8(right)) => left == right,
            (Self::I16(left), Self::I16(right)) => left == right,
            (Self::I32(left), Self::I32(right)) => left == right,
            (Self::I64(left), Self::I64(right)) => left == right,
            (Self::I128(left), Self::I128(right)) => left == right,
            (Self::Isize(left), Self::Isize(right)) => left == right,
            (Self::U8(left), Self::U8(right)) => left == right,
            (Self::U16(left), Self::U16(right)) => left == right,
            (Self::U32(left), Self::U32(right)) => left == right,
            (Self::U64(left), Self::U64(right)) => left == right,
            (Self::U128(left), Self::U128(right)) => left == right,
            (Self::Usize(left), Self::Usize(right)) => left == right,
            (Self::F32(left), Self::F32(right)) => left == right,
            (Self::F64(left), Self::F64(right)) => left == right,
            (Self::Char(left), Self::Char(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
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

fn clone_sequence(sequence: &SequenceValue) -> Result<SequenceValue, String> {
    let elements = sequence
        .elements
        .borrow()
        .iter()
        .map(|slot| {
            let value = slot
                .value
                .as_ref()
                .ok_or_else(|| "cannot clone a partially moved collection".to_string())?;
            Ok(FieldSlot {
                value: Some(value.clone_owned()?),
                type_annotation: slot.type_annotation.clone(),
                references: 0,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SequenceValue {
        active_iterators: std::cell::Cell::new(0),
        elements: RefCell::new(elements),
        element_type: RefCell::new(sequence.element_type.borrow().clone()),
    })
}

fn sequence_equal(left: &SequenceValue, right: &SequenceValue) -> bool {
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
