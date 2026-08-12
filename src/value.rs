use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    rc::Rc,
};

use crate::{
    ast::{
        AssociatedType, Block, EnumVariant, GenericParameter, NamedField, Parameter, TraitMethod,
    },
    environment::{AssignError, EnvironmentRef, StorageRef},
    types::{FunctionSignature, Type},
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
}

#[derive(Clone)]
pub struct BytecodeFunctionValue {
    pub(crate) function: usize,
    pub(crate) name: String,
    pub(crate) parameter_count: usize,
    pub(crate) captures: Vec<StorageRef>,
    pub(crate) bound_arguments: Vec<Value>,
}

#[derive(Clone)]
pub struct BytecodeIteratorValue {
    pub(crate) storage: StorageRef,
    pub(crate) next_function: usize,
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

#[derive(Clone, Debug, PartialEq)]
pub struct RangeValue {
    current: Box<Value>,
    end: Box<Value>,
    element_type: Type,
}

impl RangeValue {
    pub fn new(current: Value, end: Value) -> Result<Self, String> {
        let element_type = match (&current, &end) {
            (Value::I8(_), Value::I8(_)) => Type::Integer(crate::IntegerType::I8),
            (Value::I16(_), Value::I16(_)) => Type::Integer(crate::IntegerType::I16),
            (Value::I32(_), Value::I32(_)) => Type::I32,
            (Value::I64(_), Value::I64(_)) => Type::Integer(crate::IntegerType::I64),
            (Value::I128(_), Value::I128(_)) => Type::Integer(crate::IntegerType::I128),
            (Value::Isize(_), Value::Isize(_)) => Type::Integer(crate::IntegerType::Isize),
            (Value::U8(_), Value::U8(_)) => Type::Integer(crate::IntegerType::U8),
            (Value::U16(_), Value::U16(_)) => Type::Integer(crate::IntegerType::U16),
            (Value::U32(_), Value::U32(_)) => Type::Integer(crate::IntegerType::U32),
            (Value::U64(_), Value::U64(_)) => Type::Integer(crate::IntegerType::U64),
            (Value::U128(_), Value::U128(_)) => Type::Integer(crate::IntegerType::U128),
            (Value::Usize(_), Value::Usize(_)) => Type::USIZE,
            _ => return Err("range bounds must have the same integer type".into()),
        };
        Ok(Self {
            current: Box::new(current),
            end: Box::new(end),
            element_type,
        })
    }

    pub fn element_type(&self) -> Type {
        self.element_type.clone()
    }

    pub fn next(&mut self) -> Result<Option<Value>, String> {
        fn advance<T: Copy + Ord>(
            current: &mut T,
            end: &T,
            add_one: impl FnOnce(T) -> Option<T>,
        ) -> Result<Option<T>, String> {
            if *current >= *end {
                Ok(None)
            } else {
                let value = *current;
                *current =
                    add_one(value).ok_or_else(|| "range iteration overflowed".to_string())?;
                Ok(Some(value))
            }
        }
        match (self.current.as_mut(), self.end.as_ref()) {
            (Value::I8(a), Value::I8(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::I8))
            }
            (Value::I16(a), Value::I16(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::I16))
            }
            (Value::I32(a), Value::I32(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::I32))
            }
            (Value::I64(a), Value::I64(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::I64))
            }
            (Value::I128(a), Value::I128(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::I128))
            }
            (Value::Isize(a), Value::Isize(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::Isize))
            }
            (Value::U8(a), Value::U8(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::U8))
            }
            (Value::U16(a), Value::U16(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::U16))
            }
            (Value::U32(a), Value::U32(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::U32))
            }
            (Value::U64(a), Value::U64(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::U64))
            }
            (Value::U128(a), Value::U128(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::U128))
            }
            (Value::Usize(a), Value::Usize(b)) => {
                advance(a, b, |v| v.checked_add(1)).map(|v| v.map(Value::Usize))
            }
            _ => Err("range bounds have incompatible types".into()),
        }
    }
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
}

#[derive(Clone)]
pub struct SequenceIteratorValue {
    pub items: RefCell<VecDeque<Value>>,
    pub element_type: Type,
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
    RangeNext,
    RangeIntoIter,
    Clone,
    SequenceLen,
    VecPush,
    VecPop,
    SequenceIntoIter,
    SequenceNext,
    ResultIsOk,
    ResultIsErr,
    ResultUnwrap,
    ResultUnwrapOr,
    IntegerIntrinsic(rils_builtins::IntrinsicId),
}

#[derive(Clone, Copy)]
pub enum BuiltinType {
    Vec,
    Integer(crate::IntegerType),
}

#[derive(Clone, Copy)]
pub enum BuiltinFunction {
    VecNew,
    VecFrom,
    IntegerIntrinsic {
        id: rils_builtins::IntrinsicId,
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

pub struct ReferenceValue {
    pub mutable: bool,
    target: ReferenceTarget,
    _guard: Option<Rc<ReferenceValue>>,
}

enum ReferenceTarget {
    Storage(StorageRef),
    StructField {
        instance: Rc<StructInstance>,
        name: String,
    },
    SequenceElement {
        sequence: Rc<SequenceValue>,
        index: usize,
    },
}

impl ReferenceValue {
    pub fn new_storage(target: StorageRef, mutable: bool) -> Self {
        target.borrow_mut().add_reference();
        Self {
            mutable,
            target: ReferenceTarget::Storage(target),
            _guard: None,
        }
    }

    pub fn new_struct_field(
        instance: Rc<StructInstance>,
        name: String,
        mutable: bool,
    ) -> Result<Self, String> {
        Self::new_guarded_struct_field(instance, name, mutable, None)
    }

    pub fn new_guarded_struct_field(
        instance: Rc<StructInstance>,
        name: String,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        let mut fields = instance.fields.borrow_mut();
        let field = fields
            .get_mut(&name)
            .ok_or_else(|| format!("unknown field `{name}`"))?;
        if field.value.is_none() {
            return Err(format!("cannot reference moved field `{name}`"));
        }
        field.references += 1;
        drop(fields);
        Ok(Self {
            mutable,
            target: ReferenceTarget::StructField { instance, name },
            _guard: guard,
        })
    }

    pub fn new_sequence_element(
        sequence: Rc<SequenceValue>,
        index: usize,
        mutable: bool,
    ) -> Result<Self, String> {
        Self::new_guarded_sequence_element(sequence, index, mutable, None)
    }

    pub fn new_guarded_sequence_element(
        sequence: Rc<SequenceValue>,
        index: usize,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<Self, String> {
        let mut elements = sequence.elements.borrow_mut();
        let slot = elements
            .get_mut(index)
            .ok_or_else(|| format!("index {index} is out of bounds"))?;
        if slot.value.is_none() {
            return Err(format!("cannot reference moved element at index {index}"));
        }
        slot.references += 1;
        drop(elements);
        Ok(Self {
            mutable,
            target: ReferenceTarget::SequenceElement { sequence, index },
            _guard: guard,
        })
    }

    pub fn reborrow(&self, mutable: bool) -> Result<Self, String> {
        if mutable && !self.mutable {
            return Err("cannot mutably borrow through an immutable reference".into());
        }
        match &self.target {
            ReferenceTarget::Storage(target) => Ok(Self::new_storage(target.clone(), mutable)),
            ReferenceTarget::StructField { instance, name } => Self::new_guarded_struct_field(
                instance.clone(),
                name.clone(),
                mutable,
                self._guard.clone(),
            ),
            ReferenceTarget::SequenceElement { sequence, index } => {
                Self::new_guarded_sequence_element(
                    sequence.clone(),
                    *index,
                    mutable,
                    self._guard.clone(),
                )
            }
        }
    }

    pub fn read(&self) -> Result<Value, String> {
        match &self.target {
            ReferenceTarget::Storage(target) => target
                .borrow()
                .read()
                .map_err(|_| "reference target has been moved".into()),
            ReferenceTarget::StructField { instance, name } => instance
                .fields
                .borrow()
                .get(name)
                .and_then(|field| field.value.clone())
                .ok_or_else(|| format!("reference target field `{name}` has been moved")),
            ReferenceTarget::SequenceElement { sequence, index } => sequence
                .elements
                .borrow()
                .get(*index)
                .and_then(|slot| slot.value.clone())
                .ok_or_else(|| format!("reference target element {index} has been moved")),
        }
    }

    pub fn write(&self, value: Value) -> Result<(), AssignError> {
        if !self.mutable {
            return Err(AssignError::Immutable);
        }
        match &self.target {
            ReferenceTarget::Storage(target) => target.borrow_mut().assign_through_reference(value),
            ReferenceTarget::StructField { instance, name } => {
                let mut fields = instance.fields.borrow_mut();
                let field = fields.get_mut(name).ok_or(AssignError::Undefined)?;
                field.value = Some(
                    field
                        .type_annotation
                        .constrain(&value)
                        .ok_or_else(|| AssignError::TypeMismatch(field.type_annotation.clone()))?,
                );
                Ok(())
            }
            ReferenceTarget::SequenceElement { sequence, index } => {
                let mut elements = sequence.elements.borrow_mut();
                let slot = elements.get_mut(*index).ok_or(AssignError::Undefined)?;
                slot.value = Some(
                    slot.type_annotation
                        .constrain(&value)
                        .ok_or_else(|| AssignError::TypeMismatch(slot.type_annotation.clone()))?,
                );
                Ok(())
            }
        }
    }
}

impl Drop for ReferenceValue {
    fn drop(&mut self) {
        match &self.target {
            ReferenceTarget::Storage(target) => target.borrow_mut().remove_reference(),
            ReferenceTarget::StructField { instance, name } => {
                if let Some(field) = instance.fields.borrow_mut().get_mut(name) {
                    field.references = field.references.saturating_sub(1);
                }
            }
            ReferenceTarget::SequenceElement { sequence, index } => {
                if let Some(slot) = sequence.elements.borrow_mut().get_mut(*index) {
                    slot.references = slot.references.saturating_sub(1);
                }
            }
        }
    }
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
    SequenceIterator(Rc<SequenceIteratorValue>),
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
            Self::Unit
            | Self::Bool(_)
            | Self::I8(_)
            | Self::I16(_)
            | Self::I32(_)
            | Self::I64(_)
            | Self::I128(_)
            | Self::Isize(_)
            | Self::U8(_)
            | Self::U16(_)
            | Self::U32(_)
            | Self::U64(_)
            | Self::U128(_)
            | Self::Usize(_)
            | Self::F32(_)
            | Self::F64(_)
            | Self::Char(_) => true,
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
            Self::String(_)
            | Self::Range(_)
            | Self::Vec(_)
            | Self::SequenceIterator(_)
            | Self::BytecodeIterator(_)
            | Self::HostObject(_) => false,
        }
    }

    pub fn contains_reference(&self) -> bool {
        match self {
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
            Self::SequenceIterator(iterator) => iterator
                .items
                .borrow()
                .iter()
                .any(Value::contains_reference),
            _ => false,
        }
    }

    pub fn has_active_references(&self) -> bool {
        match self {
            Self::Struct(instance) => instance.fields.borrow().values().any(|field| {
                field.references > 0
                    || field
                        .value
                        .as_ref()
                        .is_some_and(Value::has_active_references)
            }),
            Self::Tuple(sequence) | Self::Array(sequence) | Self::Vec(sequence) => {
                sequence.elements.borrow().iter().any(|slot| {
                    slot.references > 0
                        || slot
                            .value
                            .as_ref()
                            .is_some_and(Value::has_active_references)
                })
            }
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
            _ => false,
        }
    }

    pub fn clone_owned(&self) -> Result<Self, String> {
        Ok(match self {
            Self::Reference(_) => return Err("references cannot be cloned as owned values".into()),
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
            Self::Vec(sequence) => Self::Vec(Rc::new(clone_sequence(sequence)?)),
            Self::SequenceIterator(_) => return Err("iterators cannot be cloned".into()),
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
            Self::SequenceIterator(_) => {
                Type::of_value(self).map_or_else(|| "SequenceIterator".into(), |ty| ty.to_string())
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
            Self::BuiltinType(BuiltinType::Integer(kind)) => format!("type {kind}"),
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
            Self::BuiltinType(BuiltinType::Integer(kind)) => write!(f, "<type {kind}>"),
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

fn enum_payload_equal(left: &EnumPayload, right: &EnumPayload) -> bool {
    match (left, right) {
        (EnumPayload::Unit, EnumPayload::Unit) => true,
        (EnumPayload::Tuple(left), EnumPayload::Tuple(right)) => left == right,
        (EnumPayload::Record(left), EnumPayload::Record(right)) => left == right,
        _ => false,
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(value) => write!(f, "{value:?}"),
            _ => write!(f, "{self}"),
        }
    }
}
