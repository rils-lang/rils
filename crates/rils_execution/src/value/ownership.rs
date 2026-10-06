//! Ownership, lexical references, and explicit cloning of execution values.

use super::*;

impl Value {
    pub fn is_copy(&self) -> bool {
        match self {
            Self::Unit | Self::Bool(_) | Self::Char(_) => true,
            Self::I16(_) => rils_builtins::native_implements("i16", "Copy"),
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
            Self::Native(object) => object.descriptor().is_copy(),
            Self::Dynamic(object) => object.descriptor().layout().is_copy(),
            Self::VecDeque(_)
            | Self::BinaryHeap(_)
            | Self::Vec(_)
            | Self::HashMap(_)
            | Self::BTreeMap(_)
            | Self::BTreeSet(_)
            | Self::HashSet(_)
            | Self::OwnedIterator(_)
            | Self::BorrowedIndexedIterator(_)
            | Self::BorrowedMapIterator(_)
            | Self::BorrowedSetIterator(_)
            | Self::BytecodeIterator(_) => false,
        }
    }

    pub fn contains_reference(&self) -> bool {
        match self {
            Self::Dynamic(object) => object
                .with(|value| runtime_layouts::contains_reference(value.view()))
                .and_then(|result| result)
                .unwrap_or(true),
            Self::BinaryHeap(heap) => heap.elements.borrow().iter().any(Value::contains_reference),
            Self::VecDeque(queue) => queue
                .elements
                .borrow()
                .iter()
                .any(|value| value.contains_reference()),
            Self::Reference(_) => true,
            Self::Native(object) => object.any_child(Value::contains_reference),
            Self::BoundMethod(method) => method.receiver.contains_reference(),
            Self::BuiltinBoundMethod(method) => method.receiver.contains_reference(),
            Self::HostBoundMethod(method) => method.receiver.contains_reference(),
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
            Self::OwnedIterator(iterator) => iterator.contains_reference(),
            Self::BorrowedIndexedIterator(_) => true,
            Self::BorrowedMapIterator(_) | Self::BorrowedSetIterator(_) => true,
            _ => false,
        }
    }

    pub fn contains_local_reference(&self, environment: &EnvironmentRef) -> bool {
        match self {
            Self::Dynamic(object) => object
                .with(|value| runtime_layouts::contains_local_reference(value.view(), environment))
                .and_then(|result| result)
                .unwrap_or(true),
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
            Self::Native(object) => {
                object.any_child(|value| value.contains_local_reference(environment))
            }
            Self::BoundMethod(method) => method.receiver.contains_local_reference(environment),
            Self::BuiltinBoundMethod(method) => {
                method.receiver.contains_local_reference(environment)
            }
            Self::HostBoundMethod(method) => method.receiver.contains_local_reference(environment),
            Self::BytecodeFunction(function) => {
                function.captures.iter().any(|slot| {
                    slot.borrow()
                        .read()
                        .ok()
                        .is_some_and(|value| value.contains_local_reference(environment))
                }) || function
                    .bound_arguments
                    .iter()
                    .any(|value| value.contains_local_reference(environment))
            }
            Self::BorrowedIndexedIterator(iterator) => iterator.source.is_local_to(environment),
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
            Self::Dynamic(object) => object
                .with(|payload| {
                    payload.has_path_references()
                        || payload
                            .sequence_borrows()
                            .is_ok_and(|ledger| ledger.has_active())
                })
                .unwrap_or(true),
            Self::Native(object) => {
                object.has_active_references() || object.any_child(Value::has_active_references)
            }
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
            Self::Native(object) => {
                object.is_partially_moved() || object.any_child(Value::is_partially_moved)
            }
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
            Self::BorrowedIndexedIterator(_) => return Err("iterators cannot be cloned".into()),
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
                        FieldSlot::new(field.type_annotation.clone(), value.clone_owned()?),
                    );
                }
                Self::Struct(Rc::new(StructInstance {
                    type_definition: instance.type_definition.clone(),
                    fields: RefCell::new(StructFields::from_map(
                        instance.type_definition.clone(),
                        fields,
                    )?),
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
            Self::Native(object) => Self::Native(native_ops::clone_owned(object)?),
            Self::Dynamic(object) => {
                let payload = object.with(|payload| {
                    rils_stdlib::native::registry().clone_borrowed_element(payload)
                })??;
                Self::Dynamic(DynamicObject::new(object.descriptor_handle(), payload)?)
            }
            value => value.clone(),
        })
    }
}
