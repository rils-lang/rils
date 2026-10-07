//! Runtime bridge between static [`Type`] descriptions and dynamically stored [`Value`]s.

use std::rc::Rc;

use crate::{
    ast::EnumVariant,
    types::{FunctionSignature, RuntimeValue, Type, merge_types},
    value::{FieldSlot, Value, enum_variant_name},
};

impl RuntimeValue for Value {
    fn is_accepted_by(&self, expected: &Type) -> bool {
        accepts(expected, self)
    }

    fn constrain_to(&self, expected: &Type) -> Option<Self> {
        constrain(expected, self)
    }

    fn runtime_type(&self) -> Option<Type> {
        type_of_value(self)
    }
}

impl Value {
    /// Apply declaration information needed by erased runtime storage after
    /// the shared frontend has checked the binding. Only values whose
    /// representation still needs a concrete type witness are changed.
    pub fn apply_declared_storage(self, expected: &Type) -> Result<Self, String> {
        crate::value::storage::TypedStorageContext::new(&[], &[]).apply_declared(self, expected)
    }

    /// Preserve unique ownership for an already concrete native or nominal
    /// value. Other cases still use the existing constraint logic, which may
    /// infer generic arguments or materialize a different representation.
    pub fn constrain_owned(self, expected: &Type) -> Option<Self> {
        if let Self::Dynamic(object) = &self
            && matches!(expected, Type::Option(_) | Type::Result(_, _))
            && merge_types(expected, object.descriptor().layout().rils_type()).is_none()
            && expected.accepts(&self)
        {
            // Preserve the owner here. The execution site's typed storage
            // context supplies nominal and host layouts for the conversion.
            return Some(self);
        }
        if matches!((&self, expected), (Self::Result { .. }, Type::Result(_, _))) {
            expected.constrain(&self)?;
            return self.apply_declared_storage(expected).ok();
        }
        if matches!(
            self,
            Self::Vec(_)
                | Self::VecDeque(_)
                | Self::BinaryHeap(_)
                | Self::BTreeSet(_)
                | Self::HashSet(_)
                | Self::BTreeMap(_)
                | Self::HashMap(_)
        ) {
            // The legacy constructor has no type argument until the binding
            // supplies one. Keep the unique owner so its empty storage can be
            // replaced by the declaration-derived native sequence layout.
            let checked = expected.constrain(&self)?;
            drop(checked);
            return Some(crate::value::dynamic_sequence::promote_empty(
                self, expected,
            ));
        }
        if Type::of_value(&self).as_ref() == Some(expected)
            && matches!(self, Self::Native(_) | Self::Dynamic(_))
            && expected.accepts(&self)
        {
            return Some(self);
        }
        expected.constrain(&self)
    }
}

fn accepts(expected: &Type, value: &Value) -> bool {
    match (expected, value) {
        (Type::Unknown | Type::Variable(_) | Type::BoundVariable { .. }, _) => true,
        (Type::Unit, Value::Unit)
        | (Type::Bool, Value::Bool(_))
        | (Type::Integer(crate::IntegerType::I16), Value::I16(_))
        | (Type::Integer(crate::IntegerType::I64), Value::I64(_))
        | (Type::Integer(crate::IntegerType::I128), Value::I128(_))
        | (Type::Integer(crate::IntegerType::Isize), Value::Isize(_))
        | (Type::Integer(crate::IntegerType::U8), Value::U8(_))
        | (Type::Integer(crate::IntegerType::U16), Value::U16(_))
        | (Type::Integer(crate::IntegerType::U32), Value::U32(_))
        | (Type::Integer(crate::IntegerType::U64), Value::U64(_))
        | (Type::Integer(crate::IntegerType::U128), Value::U128(_))
        | (Type::Integer(crate::IntegerType::Usize), Value::Usize(_))
        | (Type::Float(crate::FloatType::F32), Value::F32(_))
        | (Type::Float(crate::FloatType::F64), Value::F64(_))
        | (Type::Char, Value::Char(_)) => true,
        (Type::Tuple(expected), Value::Tuple(sequence)) => {
            let elements = sequence.elements.borrow();
            expected.len() == elements.len()
                && expected
                    .iter()
                    .zip(elements.iter())
                    .all(|(expected, slot)| {
                        slot.value
                            .as_ref()
                            .is_some_and(|value| expected.accepts(value))
                    })
        }
        (Type::Array { element, length }, Value::Array(sequence)) => {
            let elements = sequence.elements.borrow();
            *length == elements.len()
                && elements.iter().all(|slot| {
                    slot.value
                        .as_ref()
                        .is_some_and(|value| element.accepts(value))
                })
        }
        (Type::ArrayParameter { element, .. }, Value::Array(sequence))
        | (Type::Slice(element), Value::Array(sequence) | Value::Vec(sequence)) => {
            sequence.elements.borrow().iter().all(|slot| {
                slot.value
                    .as_ref()
                    .is_some_and(|value| element.accepts(value))
            })
        }
        (Type::Named { name, arguments }, Value::Vec(sequence)) if name == "Vec" => {
            arguments.len() == 1
                && sequence.elements.borrow().iter().all(|slot| {
                    slot.value
                        .as_ref()
                        .is_some_and(|value| arguments[0].accepts(value))
                })
        }
        (Type::Named { name, arguments }, Value::HashMap(map)) if name == "HashMap" => {
            arguments.len() == 2
                && merge_types(&arguments[0], &map.key_type.borrow()).is_some()
                && merge_types(&arguments[1], &map.value_type.borrow()).is_some()
        }
        (Type::Named { name, arguments }, Value::BTreeMap(map)) if name == "BTreeMap" => {
            arguments.len() == 2
                && merge_types(&arguments[0], &map.key_type.borrow()).is_some()
                && merge_types(&arguments[1], &map.value_type.borrow()).is_some()
        }
        (Type::Named { name, arguments }, Value::VecDeque(value)) if name == "VecDeque" => {
            arguments.len() == 1
                && merge_types(
                    &arguments[0],
                    &value.element_type.borrow().clone().unwrap_or(Type::Unknown),
                )
                .is_some()
        }
        (Type::Named { name, arguments }, Value::BinaryHeap(value)) if name == "BinaryHeap" => {
            arguments.len() == 1
                && merge_types(
                    &arguments[0],
                    &value.element_type.borrow().clone().unwrap_or(Type::Unknown),
                )
                .is_some()
        }
        (Type::Named { name, arguments }, Value::HashSet(set)) if name == "HashSet" => {
            arguments.len() == 1 && merge_types(&arguments[0], &set.element_type.borrow()).is_some()
        }
        (Type::Named { name, arguments }, Value::BTreeSet(set)) if name == "BTreeSet" => {
            arguments.len() == 1 && merge_types(&arguments[0], &set.element_type.borrow()).is_some()
        }
        (Type::Named { name, arguments }, Value::OwnedIterator(iterator))
            if name == "OwnedIterator" =>
        {
            arguments.len() == 1 && merge_types(&arguments[0], &iterator.element_type).is_some()
        }
        (Type::Named { name, arguments }, Value::OwnedIterator(iterator)) => {
            matches!(
                &iterator.iterator_type,
                Some(Type::Named {
                    name: actual_name,
                    arguments: actual_arguments,
                }) if name.rsplit("::").next() == actual_name.rsplit("::").next()
                    && arguments.len() == actual_arguments.len()
                    && arguments.iter().zip(actual_arguments).all(|(expected, actual)| {
                        merge_types(expected, actual).is_some()
                    })
            )
        }
        (Type::Named { name, arguments }, Value::BorrowedIndexedIterator(iterator))
            if name == "Iter" =>
        {
            arguments.len() == 1
                && merge_types(
                    &arguments[0],
                    &Type::Reference {
                        mutable: false,
                        inner: Box::new(iterator.element_type.clone()),
                    },
                )
                .is_some()
        }
        (Type::Named { name, arguments }, Value::BorrowedMapIterator(iterator))
            if name == "Iter" =>
        {
            arguments.len() == 1 && merge_types(&arguments[0], &iterator.item_type()).is_some()
        }
        (Type::Named { name, arguments }, Value::BorrowedSetIterator(iterator))
            if name == "Iter" =>
        {
            arguments.len() == 1 && merge_types(&arguments[0], &iterator.item_type()).is_some()
        }
        (
            Type::Reference {
                mutable: expected_mutable,
                inner: expected_inner,
            },
            Value::Reference(reference),
        ) => {
            (!*expected_mutable || reference.mutable)
                && reference
                    .read()
                    .ok()
                    .is_some_and(|value| expected_inner.accepts(&value))
        }
        (
            expected @ Type::Function { .. },
            value @ (Value::Function(_)
            | Value::BytecodeFunction(_)
            | Value::NativeFunction(_)
            | Value::HostFunction(_)
            | Value::HostBoundMethod(_)
            | Value::VariantConstructor(_)
            | Value::BoundMethod(_)
            | Value::BuiltinBoundMethod(_)
            | Value::TraitMethodSelector(_)),
        ) => Type::of_value(value).is_some_and(|actual| merge_types(expected, &actual).is_some()),
        (
            Type::Option(expected),
            Value::Option {
                value: None,
                element_type,
            },
        ) => element_type
            .as_ref()
            .is_none_or(|actual| merge_types(expected, actual).is_some()),
        (
            Type::Option(inner_type),
            Value::Option {
                value: Some(value), ..
            },
        ) => inner_type.accepts(value.as_ref()),
        (
            Type::Result(expected_ok, expected_error),
            Value::Result {
                value,
                ok_type,
                error_type,
            },
        ) => match value {
            Ok(value) => {
                expected_ok.accepts(value.as_ref())
                    && error_type
                        .as_ref()
                        .is_none_or(|actual| merge_types(expected_error, actual).is_some())
            }
            Err(value) => {
                expected_error.accepts(value.as_ref())
                    && ok_type
                        .as_ref()
                        .is_none_or(|actual| merge_types(expected_ok, actual).is_some())
            }
        },

        (Type::Named { name, arguments }, Value::HostObject(object)) => {
            arguments.is_empty()
                && (object.type_definition.name == *name
                    || object.type_definition.base_types.contains(name))
        }
        (expected, Value::Native(object)) => {
            merge_types(expected, object.descriptor().rils_type()).is_some()
        }
        (expected, Value::Dynamic(object)) => {
            merge_types(expected, object.descriptor().layout().rils_type()).is_some()
                || object
                    .with(|value| {
                        crate::value::runtime_layouts::accepts_view(value.view(), expected)
                    })
                    .is_ok_and(|accepted| accepted.unwrap_or(false))
        }
        _ => false,
    }
}

fn constrain(expected: &Type, value: &Value) -> Option<Value> {
    if !expected.accepts(value) {
        return None;
    }
    match (expected, value) {
        (Type::Named { name, arguments }, Value::VecDeque(queue))
            if name == "VecDeque" && arguments.len() == 1 =>
        {
            let ty = merge_types(
                &arguments[0],
                &queue.element_type.borrow().clone().unwrap_or(Type::Unknown),
            )?;
            if !queue
                .elements
                .borrow()
                .iter()
                .all(|value| ty.accepts(value))
            {
                return None;
            }
            *queue.element_type.borrow_mut() = Some(ty);
            Some(value.clone())
        }
        (Type::Named { name, arguments }, Value::BinaryHeap(heap))
            if name == "BinaryHeap" && arguments.len() == 1 =>
        {
            let ty = merge_types(
                &arguments[0],
                &heap.element_type.borrow().clone().unwrap_or(Type::Unknown),
            )?;
            if !heap.elements.borrow().iter().all(|value| ty.accepts(value)) {
                return None;
            }
            *heap.element_type.borrow_mut() = Some(ty);
            Some(value.clone())
        }

        (Type::Tuple(expected), Value::Tuple(sequence)) => {
            let source = sequence.elements.borrow();
            let elements = expected
                .iter()
                .zip(source.iter())
                .map(|(expected, slot)| {
                    Some(FieldSlot::new(
                        expected.clone(),
                        expected.constrain(slot.value.as_ref()?)?,
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Value::Tuple(Rc::new(crate::value::IndexedStorage {
                active_iterators: std::cell::Cell::new(0),
                elements: std::cell::RefCell::new(elements),
                element_type: std::cell::RefCell::new(None),
            })))
        }
        (
            Type::Array { element, .. } | Type::ArrayParameter { element, .. },
            Value::Array(sequence),
        ) => {
            let source = sequence.elements.borrow();
            let elements = source
                .iter()
                .map(|slot| {
                    Some(FieldSlot::new(
                        (**element).clone(),
                        element.constrain(slot.value.as_ref()?)?,
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Value::Array(Rc::new(crate::value::IndexedStorage {
                active_iterators: std::cell::Cell::new(0),
                elements: std::cell::RefCell::new(elements),
                element_type: std::cell::RefCell::new(Some((**element).clone())),
            })))
        }
        (Type::Named { name, arguments }, Value::Vec(sequence))
            if name == "Vec" && arguments.len() == 1 =>
        {
            let expected = &arguments[0];
            let source = sequence.elements.borrow();
            let elements = source
                .iter()
                .map(|slot| {
                    Some(FieldSlot::new(
                        expected.clone(),
                        expected.constrain(slot.value.as_ref()?)?,
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Value::Vec(Rc::new(crate::value::IndexedStorage {
                active_iterators: std::cell::Cell::new(0),
                elements: std::cell::RefCell::new(elements),
                element_type: std::cell::RefCell::new(Some(expected.clone())),
            })))
        }
        (
            Type::Option(inner_type),
            Value::Option {
                value,
                element_type,
            },
        ) => {
            let resolved_type = if matches!(inner_type.as_ref(), Type::Unknown) {
                element_type
                    .clone()
                    .or_else(|| {
                        value
                            .as_ref()
                            .and_then(|value| Type::of_value(value.as_ref()))
                    })
                    .unwrap_or(Type::Unknown)
            } else {
                (**inner_type).clone()
            };
            let value = match value {
                Some(value) => {
                    let constrained = resolved_type.constrain(value.as_ref())?;
                    // This legacy constraint API only borrows the source Option.
                    // A non-Copy native item needs its own payload before the
                    // consuming constructor can move it into a new layout.
                    if matches!(constrained, Value::Native(_))
                        && !constrained.is_copy()
                        && crate::value::dynamic_option::supports(&resolved_type)
                    {
                        Some(constrained.clone_owned().ok()?)
                    } else {
                        Some(constrained)
                    }
                }
                None => None,
            };
            match crate::value::dynamic_option::construct(value, &resolved_type).ok()? {
                crate::value::dynamic_option::Construction::Native(value) => Some(value),
                crate::value::dynamic_option::Construction::Unsupported(value) => {
                    Some(Value::Option {
                        value: value.map(Rc::new),
                        element_type: Some(resolved_type),
                    })
                }
            }
        }
        (Type::Result(ok_type, error_type), Value::Result { value, .. }) => Some(Value::Result {
            value: match value {
                Ok(value) => Ok(Rc::new(ok_type.constrain(value.as_ref())?)),
                Err(value) => Err(Rc::new(error_type.constrain(value.as_ref())?)),
            },
            ok_type: Some((**ok_type).clone()),
            error_type: Some((**error_type).clone()),
        }),

        _ => Some(value.clone()),
    }
}

fn type_of_value(value: &Value) -> Option<Type> {
    match value {
        Value::Unit => Some(Type::Unit),
        Value::Bool(_) => Some(Type::Bool),
        Value::I16(_) => Some(Type::Integer(crate::IntegerType::I16)),
        Value::I64(_) => Some(Type::Integer(crate::IntegerType::I64)),
        Value::I128(_) => Some(Type::Integer(crate::IntegerType::I128)),
        Value::Isize(_) => Some(Type::Integer(crate::IntegerType::Isize)),
        Value::U8(_) => Some(Type::Integer(crate::IntegerType::U8)),
        Value::U16(_) => Some(Type::Integer(crate::IntegerType::U16)),
        Value::U32(_) => Some(Type::Integer(crate::IntegerType::U32)),
        Value::U64(_) => Some(Type::Integer(crate::IntegerType::U64)),
        Value::U128(_) => Some(Type::Integer(crate::IntegerType::U128)),
        Value::Usize(_) => Some(Type::USIZE),
        Value::F32(_) => Some(Type::Float(crate::FloatType::F32)),
        Value::F64(_) => Some(Type::F64),
        Value::Char(_) => Some(Type::Char),
        Value::Tuple(sequence) => Some(Type::Tuple(
            sequence
                .elements
                .borrow()
                .iter()
                .map(|slot| Type::of_value(slot.value.as_ref()?))
                .collect::<Option<Vec<_>>>()?,
        )),
        Value::Array(sequence) => Some(Type::Array {
            element: Box::new(
                sequence
                    .element_type
                    .borrow()
                    .clone()
                    .unwrap_or(Type::Unknown),
            ),
            length: sequence.elements.borrow().len(),
        }),
        Value::Vec(sequence) => Some(Type::Named {
            name: "Vec".into(),
            arguments: vec![
                sequence
                    .element_type
                    .borrow()
                    .clone()
                    .unwrap_or(Type::Unknown),
            ],
        }),
        Value::VecDeque(value) => Some(Type::Named {
            name: "VecDeque".into(),
            arguments: vec![value.element_type.borrow().clone().unwrap_or(Type::Unknown)],
        }),
        Value::BinaryHeap(value) => Some(Type::Named {
            name: "BinaryHeap".into(),
            arguments: vec![value.element_type.borrow().clone().unwrap_or(Type::Unknown)],
        }),
        Value::HashMap(map) => Some(Type::Named {
            name: "HashMap".into(),
            arguments: vec![
                map.key_type.borrow().clone(),
                map.value_type.borrow().clone(),
            ],
        }),
        Value::BTreeMap(map) => Some(Type::Named {
            name: "BTreeMap".into(),
            arguments: vec![
                map.key_type.borrow().clone(),
                map.value_type.borrow().clone(),
            ],
        }),
        Value::HashSet(set) => Some(Type::Named {
            name: "HashSet".into(),
            arguments: vec![set.element_type.borrow().clone()],
        }),
        Value::BTreeSet(set) => Some(Type::Named {
            name: "BTreeSet".into(),
            arguments: vec![set.element_type.borrow().clone()],
        }),
        Value::OwnedIterator(iterator) => Some(iterator.iterator_type.clone().unwrap_or_else(
            || Type::Named {
                name: "OwnedIterator".into(),
                arguments: vec![iterator.element_type.clone()],
            },
        )),
        Value::BorrowedIndexedIterator(iterator) => Some(Type::Named {
            name: "Iter".into(),
            arguments: vec![Type::Reference {
                mutable: false,
                inner: Box::new(iterator.element_type.clone()),
            }],
        }),
        Value::BorrowedMapIterator(iterator) => Some(Type::Named {
            name: "Iter".into(),
            arguments: vec![iterator.item_type()],
        }),
        Value::BorrowedSetIterator(iterator) => Some(Type::Named {
            name: "Iter".into(),
            arguments: vec![iterator.item_type()],
        }),
        Value::BytecodeIterator(_) => Some(Type::Named {
            name: "Iterator".into(),
            arguments: vec![Type::Unknown],
        }),
        Value::Reference(reference) => Some(Type::Reference {
            mutable: reference.mutable,
            inner: Box::new(match reference.native_layout().ok()? {
                Some(layout) => layout.rils_type().clone(),
                None => Type::of_value(&reference.read().ok()?)?,
            }),
        }),
        Value::Function(function) => Some(function_type(function)),
        Value::BytecodeFunction(_) => Some(Type::opaque_function()),
        Value::NativeFunction(function) => Some(
            function
                .signature
                .as_ref()
                .map_or_else(Type::opaque_function, FunctionSignature::as_type),
        ),
        Value::HostFunction(function) => Some(
            function
                .signature
                .as_ref()
                .map_or_else(Type::opaque_function, FunctionSignature::as_type),
        ),
        Value::HostBoundMethod(method) => Some(
            method
                .function
                .signature
                .as_ref()
                .map_or_else(Type::opaque_function, FunctionSignature::as_type),
        ),
        Value::HostObject(object) => Some(Type::named(object.type_definition.name.clone())),
        Value::Native(object) => Some(object.descriptor().rils_type().clone()),
        Value::Dynamic(object) => Some(object.descriptor().layout().rils_type().clone()),
        Value::VariantConstructor(constructor) => {
            let variant = constructor
                .type_definition
                .variants
                .iter()
                .find(|variant| enum_variant_name(variant) == constructor.variant)?;
            let EnumVariant::Tuple { fields, .. } = variant else {
                return Some(Type::opaque_function());
            };
            Some(Type::function(
                fields.clone(),
                Type::Named {
                    name: constructor.type_definition.name.clone(),
                    arguments: constructor
                        .type_definition
                        .generic_parameters
                        .iter()
                        .map(|parameter| Type::Variable(parameter.name.clone()))
                        .collect(),
                },
            ))
        }
        Value::BoundMethod(method) => {
            let mut signature = function_type(&method.function);
            if let Type::Function {
                parameters: Some(parameters),
                ..
            } = &mut signature
                && !parameters.is_empty()
            {
                parameters.remove(0);
            }
            Some(signature)
        }
        Value::BuiltinBoundMethod(method) => {
            let receiver = Type::of_value(method.receiver.as_ref()).unwrap_or(Type::Unknown);
            let receiver = match receiver {
                Type::Reference { inner, .. } => *inner,
                receiver => receiver,
            };
            let signature = match method.method {
                crate::value::BuiltinMethod::Native(symbol) => {
                    if let Some((intrinsic, _)) = rils_builtins::intrinsic_by_symbol(symbol) {
                        match receiver {
                            Type::Integer(integer) => {
                                rils_frontend::standard_library::integer_intrinsic_type(
                                    intrinsic, integer,
                                )
                            }
                            Type::Float(float) => {
                                rils_frontend::standard_library::float_intrinsic_type(
                                    intrinsic, float,
                                )
                            }
                            _ => Type::opaque_function(),
                        }
                    } else {
                        rils_frontend::standard_library::builtin_member_type(
                            &receiver,
                            rils_builtins::native_member(symbol)?.name,
                        )
                        .unwrap_or_else(Type::opaque_function)
                    }
                }
                crate::value::BuiltinMethod::IteratorIdentity => {
                    Type::function(Vec::new(), receiver)
                }
            };
            Some(signature)
        }
        Value::TraitMethodSelector(_) => Some(Type::opaque_function()),
        Value::Option { element_type, .. } => Some(Type::Option(Box::new(
            element_type.clone().unwrap_or(Type::Unknown),
        ))),
        Value::Result {
            ok_type,
            error_type,
            ..
        } => Some(Type::Result(
            Box::new(ok_type.clone().unwrap_or(Type::Unknown)),
            Box::new(error_type.clone().unwrap_or(Type::Unknown)),
        )),

        Value::BuiltinFunction(_) => Some(Type::opaque_function()),
        Value::BuiltinType(_)
        | Value::Module(_)
        | Value::HostType(_)
        | Value::StructType(_)
        | Value::EnumType(_)
        | Value::TraitType(_)
        | Value::TypeAlias(_) => None,
    }
}

fn function_type(function: &crate::value::UserFunction) -> Type {
    Type::function(
        function
            .parameters
            .iter()
            .map(|parameter| parameter.type_annotation.clone().unwrap_or(Type::Unknown))
            .collect(),
        function.return_type.clone().unwrap_or(Type::Unknown),
    )
}
