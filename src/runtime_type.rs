//! Runtime bridge between static [`Type`] descriptions and dynamically stored [`Value`]s.

use std::{collections::HashMap, rc::Rc};

use crate::{
    ast::EnumVariant,
    types::{FunctionSignature, RuntimeValue, Type, merge_type_arguments, merge_types},
    value::{EnumInstance, EnumPayload, FieldSlot, StructInstance, Value, enum_variant_name},
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
fn accepts(expected: &Type, value: &Value) -> bool {
    match (expected, value) {
        (Type::Unknown | Type::Variable(_), _) => true,
        (Type::Unit, Value::Unit)
        | (Type::Bool, Value::Bool(_))
        | (Type::Integer(crate::IntegerType::I8), Value::I8(_))
        | (Type::Integer(crate::IntegerType::I16), Value::I16(_))
        | (Type::Integer(crate::IntegerType::I32), Value::I32(_))
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
        | (Type::Char, Value::Char(_))
        | (Type::String, Value::String(_)) => true,
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
        (Type::Named { name, arguments }, Value::HashSet(set)) if name == "HashSet" => {
            arguments.len() == 1 && merge_types(&arguments[0], &set.element_type.borrow()).is_some()
        }
        (Type::Named { name, arguments }, Value::SequenceIterator(iterator))
            if name == "SequenceIterator" =>
        {
            arguments.len() == 1 && merge_types(&arguments[0], &iterator.element_type).is_some()
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
        (Type::Named { name, arguments }, Value::Struct(instance)) => {
            instance.type_definition.name == *name
                && type_arguments_compatible(arguments, &instance.type_arguments)
        }
        (Type::Named { name, arguments }, Value::Enum(instance)) => {
            instance.type_definition.name == *name
                && type_arguments_compatible(arguments, &instance.type_arguments)
        }
        (Type::Named { name, arguments }, Value::Range(range)) => {
            name == "Range" && (arguments.is_empty() || arguments == &vec![range.element_type()])
        }
        (Type::Named { name, arguments }, Value::HostObject(object)) => {
            arguments.is_empty()
                && (object.type_definition.name == *name
                    || object.type_definition.base_types.contains(name))
        }
        _ => false,
    }
}

fn constrain(expected: &Type, value: &Value) -> Option<Value> {
    if !expected.accepts(value) {
        return None;
    }
    match (expected, value) {
        (Type::Tuple(expected), Value::Tuple(sequence)) => {
            let source = sequence.elements.borrow();
            let elements = expected
                .iter()
                .zip(source.iter())
                .map(|(expected, slot)| {
                    Some(FieldSlot {
                        value: Some(expected.constrain(slot.value.as_ref()?)?),
                        type_annotation: expected.clone(),
                        references: 0,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Value::Tuple(Rc::new(crate::value::SequenceValue {
                elements: std::cell::RefCell::new(elements),
                element_type: std::cell::RefCell::new(None),
            })))
        }
        (Type::Array { element, .. }, Value::Array(sequence)) => {
            let source = sequence.elements.borrow();
            let elements = source
                .iter()
                .map(|slot| {
                    Some(FieldSlot {
                        value: Some(element.constrain(slot.value.as_ref()?)?),
                        type_annotation: (**element).clone(),
                        references: 0,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Value::Array(Rc::new(crate::value::SequenceValue {
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
                    Some(FieldSlot {
                        value: Some(expected.constrain(slot.value.as_ref()?)?),
                        type_annotation: expected.clone(),
                        references: 0,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Value::Vec(Rc::new(crate::value::SequenceValue {
                elements: std::cell::RefCell::new(elements),
                element_type: std::cell::RefCell::new(Some(expected.clone())),
            })))
        }
        (
            Type::Option(inner_type),
            Value::Option {
                value,
                element_type: _,
            },
        ) => {
            let value = match value {
                Some(value) => Some(Rc::new(inner_type.constrain(value.as_ref())?)),
                None => None,
            };
            Some(Value::Option {
                value,
                element_type: Some((**inner_type).clone()),
            })
        }
        (Type::Result(ok_type, error_type), Value::Result { value, .. }) => Some(Value::Result {
            value: match value {
                Ok(value) => Ok(Rc::new(ok_type.constrain(value.as_ref())?)),
                Err(value) => Err(Rc::new(error_type.constrain(value.as_ref())?)),
            },
            ok_type: Some((**ok_type).clone()),
            error_type: Some((**error_type).clone()),
        }),
        (Type::Named { arguments, .. }, Value::Struct(instance)) => {
            let type_arguments = merge_type_arguments(arguments, &instance.type_arguments)?;
            let substitutions = instance
                .type_definition
                .generic_parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .zip(type_arguments.iter().cloned())
                .collect::<HashMap<_, _>>();
            let source_fields = instance.fields.borrow();
            let mut fields = HashMap::new();
            for definition in &instance.type_definition.fields {
                let expected = definition.type_annotation.substitute(&substitutions);
                let value = source_fields.get(&definition.name)?.value.as_ref()?;
                let constrained = expected.constrain(value)?;
                fields.insert(
                    definition.name.clone(),
                    FieldSlot {
                        value: Some(constrained),
                        type_annotation: expected,
                        references: 0,
                    },
                );
            }
            Some(Value::Struct(Rc::new(StructInstance {
                type_definition: instance.type_definition.clone(),
                fields: std::cell::RefCell::new(fields),
                type_arguments,
            })))
        }
        (Type::Named { arguments, .. }, Value::Enum(instance)) => {
            let type_arguments = merge_type_arguments(arguments, &instance.type_arguments)?;
            let substitutions = instance
                .type_definition
                .generic_parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .zip(type_arguments.iter().cloned())
                .collect::<HashMap<_, _>>();
            let variant = instance
                .type_definition
                .variants
                .iter()
                .find(|variant| enum_variant_name(variant) == instance.variant)?;
            let payload = match (variant, &instance.payload) {
                (EnumVariant::Unit { .. }, EnumPayload::Unit) => EnumPayload::Unit,
                (
                    EnumVariant::Tuple {
                        fields: definitions,
                        ..
                    },
                    EnumPayload::Tuple(values),
                ) if definitions.len() == values.len() => EnumPayload::Tuple(
                    definitions
                        .iter()
                        .zip(values)
                        .map(|(definition, value)| {
                            definition.substitute(&substitutions).constrain(value)
                        })
                        .collect::<Option<Vec<_>>>()?,
                ),
                (
                    EnumVariant::Record {
                        fields: definitions,
                        ..
                    },
                    EnumPayload::Record(values),
                ) => {
                    let mut constrained = values.clone();
                    for definition in definitions {
                        let expected = definition.type_annotation.substitute(&substitutions);
                        let value = constrained.get(&definition.name)?;
                        constrained.insert(definition.name.clone(), expected.constrain(value)?);
                    }
                    EnumPayload::Record(constrained)
                }
                _ => return None,
            };
            Some(Value::Enum(Rc::new(EnumInstance {
                type_definition: instance.type_definition.clone(),
                variant: instance.variant.clone(),
                payload,
                type_arguments,
            })))
        }
        _ => Some(value.clone()),
    }
}

fn type_of_value(value: &Value) -> Option<Type> {
    match value {
        Value::Unit => Some(Type::Unit),
        Value::Bool(_) => Some(Type::Bool),
        Value::I8(_) => Some(Type::Integer(crate::IntegerType::I8)),
        Value::I16(_) => Some(Type::Integer(crate::IntegerType::I16)),
        Value::I32(_) => Some(Type::I32),
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
        Value::String(_) => Some(Type::String),
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
        Value::HashMap(map) => Some(Type::Named {
            name: "HashMap".into(),
            arguments: vec![
                map.key_type.borrow().clone(),
                map.value_type.borrow().clone(),
            ],
        }),
        Value::HashSet(set) => Some(Type::Named {
            name: "HashSet".into(),
            arguments: vec![set.element_type.borrow().clone()],
        }),
        Value::SequenceIterator(iterator) => Some(Type::Named {
            name: "SequenceIterator".into(),
            arguments: vec![iterator.element_type.clone()],
        }),
        Value::BytecodeIterator(_) => Some(Type::Named {
            name: "Iterator".into(),
            arguments: vec![Type::Unknown],
        }),
        Value::Reference(reference) => Some(Type::Reference {
            mutable: reference.mutable,
            inner: Box::new(Type::of_value(&reference.read().ok()?)?),
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
        Value::BuiltinBoundMethod(method) => Some(match method.method {
            crate::value::BuiltinMethod::Runtime(id) => {
                let receiver = Type::of_value(method.receiver.as_ref()).unwrap_or(Type::Unknown);
                let receiver = match receiver {
                    Type::Reference { inner, .. } => *inner,
                    receiver => receiver,
                };
                rils_frontend::standard_library::builtin_member_type(
                    &receiver,
                    rils_builtins::runtime_member(id)?.1.name,
                )
                .unwrap_or_else(Type::opaque_function)
            }
            crate::value::BuiltinMethod::IntegerIntrinsic(id)
            | crate::value::BuiltinMethod::FloatIntrinsic(id) => {
                let Some(intrinsic) = rils_builtins::intrinsic(id) else {
                    return Some(Type::opaque_function());
                };
                let receiver = Type::of_value(method.receiver.as_ref()).unwrap_or(Type::Unknown);
                fn resolve(kind: rils_builtins::TypePattern, receiver: &Type) -> Type {
                    use rils_builtins::TypePattern;
                    match kind {
                        TypePattern::SelfType => receiver.clone(),
                        TypePattern::AnyInteger | TypePattern::Unknown => Type::Unknown,
                        TypePattern::Generic(name) => Type::Variable(name.into()),
                        TypePattern::Unit => Type::Unit,
                        TypePattern::Bool => Type::Bool,
                        TypePattern::Char => Type::Char,
                        TypePattern::String => Type::String,
                        TypePattern::F32 => Type::Float(crate::FloatType::F32),
                        TypePattern::F64 => Type::Float(crate::FloatType::F64),
                        TypePattern::U32 => Type::Integer(crate::IntegerType::U32),
                        TypePattern::U8 => Type::Integer(crate::IntegerType::U8),
                        TypePattern::Usize => Type::USIZE,
                        TypePattern::Named { path, arguments } => Type::Named {
                            name: path.into(),
                            arguments: arguments
                                .iter()
                                .map(|value| resolve(*value, receiver))
                                .collect(),
                        },
                        TypePattern::Option(inner) => {
                            Type::Option(Box::new(resolve(*inner, receiver)))
                        }
                        TypePattern::Result { ok, error } => Type::Result(
                            Box::new(resolve(*ok, receiver)),
                            Box::new(resolve(*error, receiver)),
                        ),
                        TypePattern::Tuple(values) => Type::Tuple(
                            values
                                .iter()
                                .map(|value| resolve(*value, receiver))
                                .collect(),
                        ),
                        TypePattern::Function { parameters, result } => Type::function(
                            parameters
                                .iter()
                                .map(|value| resolve(*value, receiver))
                                .collect(),
                            resolve(*result, receiver),
                        ),
                        TypePattern::Reference { mutable, inner } => Type::Reference {
                            mutable,
                            inner: Box::new(resolve(*inner, receiver)),
                        },
                        TypePattern::Associated {
                            base,
                            trait_name,
                            name,
                            arguments,
                        } => Type::Associated {
                            base: Box::new(resolve(*base, receiver)),
                            trait_name: trait_name.map(str::to_owned),
                            name: name.into(),
                            arguments: arguments
                                .iter()
                                .map(|value| resolve(*value, receiver))
                                .collect(),
                        },
                    }
                }
                Type::function(
                    intrinsic
                        .signature
                        .parameters
                        .iter()
                        .copied()
                        .map(|value| resolve(value, &receiver))
                        .collect(),
                    resolve(intrinsic.signature.result, &receiver),
                )
            }
        }),
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
        Value::Struct(instance) => Some(Type::Named {
            name: instance.type_definition.name.clone(),
            arguments: instance.type_arguments.clone(),
        }),
        Value::Enum(instance) => Some(Type::Named {
            name: instance.type_definition.name.clone(),
            arguments: instance.type_arguments.clone(),
        }),
        Value::Range(range) => Some(Type::Named {
            name: "Range".into(),
            arguments: vec![range.element_type()],
        }),
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

fn type_arguments_compatible(expected: &[Type], actual: &[Type]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(expected, actual)| merge_types(expected, actual).is_some())
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
