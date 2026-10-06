use super::*;

pub(super) enum Place {
    Storage {
        slot: crate::environment::StorageRef,
        name: String,
    },
    StructField {
        instance: Rc<StructInstance>,
        name: String,
        owner: String,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    },
    NativeField {
        place: rils_execution::value::native_instance::NativeInstancePlace,
        name: String,
        owner: String,
        mutable: bool,
        source: Option<crate::environment::StorageRef>,
    },
    IndexedElement {
        sequence: Rc<IndexedStorage>,
        index: usize,
        owner: String,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    },
    DynamicIndexedElement {
        sequence: rils_execution::value::DynamicObject,
        codec: Rc<rils_execution::value::record_codec::NativeRecordCodec>,
        index: usize,
        owner: String,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    },
    Reference {
        reference: Rc<ReferenceValue>,
    },
}

impl Place {
    fn native_source(&self) -> Option<crate::environment::StorageRef> {
        match self {
            Self::Storage { slot, .. } => Some(slot.clone()),
            Self::NativeField { source, .. } => source.clone(),
            _ => None,
        }
    }
    pub(super) fn native_owner(
        &self,
        span: Span,
    ) -> Result<Option<rils_execution::value::native_instance::NativeInstancePlace>, RuntimeError>
    {
        if let Self::NativeField { place, .. } = self {
            return Ok(Some(place.clone()));
        }
        let value = match self {
            Self::Storage { slot, .. } => slot.borrow().read().ok(),
            Self::StructField { instance, name, .. } => instance
                .fields
                .borrow()
                .get(name)
                .and_then(|slot| slot.value.clone()),
            Self::IndexedElement {
                sequence, index, ..
            } => sequence
                .elements
                .borrow()
                .get(*index)
                .and_then(|slot| slot.value.clone()),
            _ => None,
        };
        match value {
            Some(Value::Dynamic(object))
                if rils_execution::value::native_instance::definition(&object).is_some() =>
            {
                rils_execution::value::native_instance::NativeInstancePlace::new(object)
                    .map(Some)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn read(&self, span: Span) -> Result<Value, RuntimeError> {
        match self {
            Self::NativeField { place, .. } => place
                .take()
                .map_err(|message| RuntimeError::new(message, span)),
            Self::Storage { slot, name } => slot.borrow().read().map_err(|_| {
                RuntimeError::new(format!("cannot access moved value `{name}`"), span)
            }),
            Self::StructField { instance, name, .. } => instance
                .fields
                .borrow()
                .get(name)
                .and_then(|field| field.value.clone())
                .ok_or_else(|| RuntimeError::new(format!("use of moved field `{name}`"), span)),
            Self::IndexedElement {
                sequence, index, ..
            } => {
                let elements = sequence.elements.borrow();
                let value = elements
                    .get(*index)
                    .and_then(|slot| slot.value.as_ref())
                    .ok_or_else(|| {
                        RuntimeError::new(format!("use of moved element at index {index}"), span)
                    })?;
                if !value.is_copy() {
                    return Err(RuntimeError::new(
                        "cannot move a non-Copy value out through indexing",
                        span,
                    ));
                }
                value
                    .clone_owned()
                    .map_err(|message| RuntimeError::new(message, span))
            }
            Self::DynamicIndexedElement {
                sequence, index, ..
            } => rils_execution::value::dynamic_sequence::copy_item(sequence, *index)
                .map_err(|message| RuntimeError::new(message, span)),
            Self::Reference { reference } => reference
                .read()
                .map_err(|message| RuntimeError::new(message, span)),
        }
    }

    pub(super) fn assign(self, value: Value, span: Span) -> Result<(), RuntimeError> {
        match self {
            Self::NativeField {
                place,
                name,
                owner,
                mutable,
                ..
            } => {
                if !mutable {
                    return Err(RuntimeError::new(
                        format!("cannot assign to field `{name}` of immutable place `{owner}`"),
                        span,
                    ));
                }
                let ty = place
                    .field_layout()
                    .map_err(|message| RuntimeError::new(message, span))?
                    .rils_type()
                    .clone();
                place.assign(value).map_err(|message| {
                    RuntimeError::new(
                        format!("cannot assign to field `{name}` of type {ty}: {message}"),
                        span,
                    )
                })
            }
            Self::Storage { slot, name } => slot
                .borrow_mut()
                .assign(value)
                .map_err(|error| super::evaluation::assignment_error(error, &name, span)),
            Self::Reference { reference } => reference
                .write(value)
                .map_err(|error| super::evaluation::assignment_error(error, "reference", span)),
            Self::StructField {
                instance,
                name,
                owner,
                mutable,
                guard: _guard,
            } => {
                if !mutable {
                    return Err(RuntimeError::new(
                        format!("cannot assign to field `{name}` of immutable place `{owner}`"),
                        span,
                    ));
                }
                let mut fields = instance.fields.borrow_mut();
                let field = fields
                    .get_mut(&name)
                    .ok_or_else(|| RuntimeError::new(format!("unknown field `{name}`"), span))?;
                if field.references > 0
                    || field
                        .value
                        .as_ref()
                        .is_some_and(Value::has_active_references)
                {
                    return Err(RuntimeError::new(
                        format!("cannot replace field `{name}` while it is referenced"),
                        span,
                    ));
                }
                field.assign(value).map_err(|_| {
                    RuntimeError::new(
                        format!(
                            "cannot assign a value incompatible with field `{name}` of type {}",
                            field.type_annotation
                        ),
                        span,
                    )
                })?;
                Ok(())
            }
            Self::IndexedElement {
                sequence,
                index,
                owner,
                mutable,
                guard: _guard,
            } => {
                if !mutable {
                    return Err(RuntimeError::new(
                        format!("cannot assign through immutable place `{owner}`"),
                        span,
                    ));
                }
                let mut elements = sequence.elements.borrow_mut();
                let slot = elements.get_mut(index).ok_or_else(|| {
                    RuntimeError::new(format!("index {index} is out of bounds"), span)
                })?;
                if slot.references > 0
                    || slot
                        .value
                        .as_ref()
                        .is_some_and(Value::has_active_references)
                {
                    return Err(RuntimeError::new(
                        format!("cannot replace element {index} while it is referenced"),
                        span,
                    ));
                }
                slot.assign(value).map_err(|_| {
                    RuntimeError::new(
                        format!(
                            "value is incompatible with element type {}",
                            slot.type_annotation
                        ),
                        span,
                    )
                })?;
                Ok(())
            }
            Self::DynamicIndexedElement {
                sequence,
                codec: _,
                index,
                owner,
                mutable,
                guard: _guard,
            } => {
                if !mutable {
                    return Err(RuntimeError::new(
                        format!("cannot assign through immutable place `{owner}`"),
                        span,
                    ));
                }
                rils_execution::value::dynamic_sequence::replace_item(&sequence, index, value)
                    .map_err(|message| RuntimeError::new(message, span))
            }
        }
    }

    pub(super) fn borrow(self, mutable: bool, span: Span) -> Result<Value, RuntimeError> {
        let reference = match self {
            Self::NativeField {
                place,
                owner,
                mutable: owner_mutable,
                source,
                ..
            } => {
                if mutable && !owner_mutable {
                    return Err(RuntimeError::new(
                        format!("cannot mutably reference immutable place `{owner}`"),
                        span,
                    ));
                }
                Rc::new(
                    place
                        .borrow(
                            mutable,
                            source.map(|slot| Rc::new(ReferenceValue::new_storage(slot, mutable))),
                        )
                        .map_err(|message| RuntimeError::new(message, span))?,
                )
            }
            Self::Storage { slot, name } => {
                {
                    let storage = slot.borrow();
                    let current = storage.read().map_err(|_| {
                        RuntimeError::new(format!("cannot reference moved value `{name}`"), span)
                    })?;
                    if current.is_partially_moved() {
                        return Err(RuntimeError::new(
                            format!("cannot reference partially moved value `{name}`"),
                            span,
                        ));
                    }
                    if mutable && !storage.is_mutable() {
                        return Err(RuntimeError::new(
                            format!("cannot mutably reference immutable variable `{name}`"),
                            span,
                        ));
                    }
                }
                Rc::new(ReferenceValue::new_storage(slot, mutable))
            }
            Self::StructField {
                instance,
                name,
                owner,
                mutable: owner_mutable,
                guard,
            } => {
                if mutable && !owner_mutable {
                    return Err(RuntimeError::new(
                        format!(
                            "cannot mutably reference field `{name}` of immutable place `{owner}`"
                        ),
                        span,
                    ));
                }
                Rc::new(
                    ReferenceValue::new_guarded_struct_field(instance, name, mutable, guard)
                        .map_err(|message| RuntimeError::new(message, span))?,
                )
            }
            Self::IndexedElement {
                sequence,
                index,
                owner,
                mutable: owner_mutable,
                guard,
            } => {
                if mutable && !owner_mutable {
                    return Err(RuntimeError::new(
                        format!("cannot mutably reference an element of immutable place `{owner}`"),
                        span,
                    ));
                }
                Rc::new(
                    ReferenceValue::new_guarded_indexed_element(sequence, index, mutable, guard)
                        .map_err(|message| RuntimeError::new(message, span))?,
                )
            }
            Self::DynamicIndexedElement {
                sequence,
                codec,
                index,
                owner,
                mutable: owner_mutable,
                guard,
            } => {
                if mutable && !owner_mutable {
                    return Err(RuntimeError::new(
                        format!("cannot mutably reference an element of immutable place `{owner}`"),
                        span,
                    ));
                }
                Rc::new(
                    ReferenceValue::new_guarded_dynamic_indexed_element_with_codec(
                        sequence,
                        index,
                        mutable,
                        guard,
                        Some(codec),
                    )
                    .map_err(|message| RuntimeError::new(message, span))?,
                )
            }
            Self::Reference { reference } => Rc::new(
                reference
                    .reborrow(mutable)
                    .map_err(|message| RuntimeError::new(message, span))?,
            ),
        };
        Ok(Value::Reference(reference))
    }

    fn is_mutable(&self) -> bool {
        match self {
            Self::NativeField { mutable, .. } => *mutable,
            Self::Storage { slot, .. } => match slot.borrow().read() {
                Ok(Value::Reference(reference)) => reference.mutable,
                _ => slot.borrow().is_mutable(),
            },
            Self::StructField { mutable, .. } => *mutable,
            Self::IndexedElement { mutable, .. } => *mutable,
            Self::DynamicIndexedElement { mutable, .. } => *mutable,
            Self::Reference { reference } => reference.mutable,
        }
    }

    fn description(&self) -> String {
        match self {
            Self::NativeField { owner, name, .. } => format!("{owner}.{name}"),
            Self::Storage { name, .. } => name.clone(),
            Self::StructField { owner, name, .. } => format!("{owner}.{name}"),
            Self::IndexedElement { owner, index, .. } => format!("{owner}[{index}]"),
            Self::DynamicIndexedElement { owner, index, .. } => format!("{owner}[{index}]"),
            Self::Reference { .. } => "reference".into(),
        }
    }

    pub(super) fn projection_value(&self, span: Span) -> Result<Value, RuntimeError> {
        match self {
            Self::NativeField { place, .. } => place
                .borrow(false, None)
                .and_then(|reference| reference.read())
                .map_err(|message| RuntimeError::new(message, span)),
            Self::Storage { slot, name } => {
                let value = slot.borrow().read().map_err(|_| {
                    RuntimeError::new(format!("cannot access moved value `{name}`"), span)
                })?;
                match value {
                    Value::Reference(reference) => reference
                        .read()
                        .map_err(|message| RuntimeError::new(message, span)),
                    value => Ok(value),
                }
            }
            Self::StructField { instance, name, .. } => instance
                .fields
                .borrow()
                .get(name)
                .and_then(|field| field.value.clone())
                .ok_or_else(|| RuntimeError::new(format!("use of moved field `{name}`"), span)),
            Self::IndexedElement {
                sequence, index, ..
            } => sequence
                .elements
                .borrow()
                .get(*index)
                .and_then(|slot| slot.value.clone())
                .ok_or_else(|| {
                    RuntimeError::new(format!("use of moved element at index {index}"), span)
                }),
            Self::DynamicIndexedElement {
                sequence, index, ..
            } => rils_execution::value::dynamic_sequence::copy_item(sequence, *index)
                .map_err(|message| RuntimeError::new(message, span)),
            Self::Reference { reference } => reference
                .read()
                .map_err(|message| RuntimeError::new(message, span)),
        }
    }

    pub(super) fn projection_guard(
        &self,
        span: Span,
    ) -> Result<Option<Rc<ReferenceValue>>, RuntimeError> {
        match self {
            Self::NativeField {
                place,
                mutable,
                source,
                ..
            } => Ok(Some(Rc::new(
                place
                    .borrow(
                        *mutable,
                        source
                            .clone()
                            .map(|slot| Rc::new(ReferenceValue::new_storage(slot, *mutable))),
                    )
                    .map_err(|message| RuntimeError::new(message, span))?,
            ))),
            Self::Storage { slot, name } => match slot.borrow().read().map_err(|_| {
                RuntimeError::new(format!("cannot access moved value `{name}`"), span)
            })? {
                Value::Reference(reference) => Ok(Some(reference)),
                _ => Ok(None),
            },
            Self::StructField {
                instance,
                name,
                mutable,
                guard,
                ..
            } => Ok(Some(Rc::new(
                ReferenceValue::new_guarded_struct_field(
                    instance.clone(),
                    name.clone(),
                    *mutable,
                    guard.clone(),
                )
                .map_err(|message| RuntimeError::new(message, span))?,
            ))),
            Self::IndexedElement {
                sequence,
                index,
                mutable,
                guard,
                ..
            } => Ok(Some(Rc::new(
                ReferenceValue::new_guarded_indexed_element(
                    sequence.clone(),
                    *index,
                    *mutable,
                    guard.clone(),
                )
                .map_err(|message| RuntimeError::new(message, span))?,
            ))),
            Self::DynamicIndexedElement {
                sequence,
                codec,
                index,
                mutable,
                guard,
                ..
            } => Ok(Some(Rc::new(
                ReferenceValue::new_guarded_dynamic_indexed_element_with_codec(
                    sequence.clone(),
                    *index,
                    *mutable,
                    guard.clone(),
                    Some(codec.clone()),
                )
                .map_err(|message| RuntimeError::new(message, span))?,
            ))),
            Self::Reference { reference } => Ok(Some(reference.clone())),
        }
    }
}

impl Interpreter {
    pub(super) fn resolve_place(
        &mut self,
        expression: &Expr,
        environment: &EnvironmentRef,
        span: Span,
    ) -> Result<Place, RuntimeError> {
        match expression {
            Expr::Variable { name, .. } => {
                let slot = environment.borrow().slot(name).ok_or_else(|| {
                    RuntimeError::new(format!("undefined variable `{name}`"), span)
                })?;
                Ok(Place::Storage {
                    slot,
                    name: name.clone(),
                })
            }
            Expr::Unary {
                operator: UnaryOp::Dereference,
                operand,
                ..
            } => {
                let value = self.evaluate(operand, environment.clone())?;
                let Value::Reference(reference) = value else {
                    return Err(RuntimeError::new(
                        "dereference target is not a reference",
                        span,
                    ));
                };
                Ok(Place::Reference { reference })
            }
            Expr::Member { object, name, .. } => {
                let owner = self.resolve_place(object, environment, span)?;
                let mutable = owner.is_mutable();
                let owner_name = owner.description();
                if let Some(place) = owner.native_owner(span)? {
                    return Ok(Place::NativeField {
                        place: place
                            .field(name)
                            .map_err(|message| RuntimeError::new(message, span))?,
                        name: name.clone(),
                        owner: owner_name,
                        mutable,
                        source: owner.native_source(),
                    });
                }
                let guard = owner.projection_guard(span)?;
                if let Some(reference) = &guard
                    && let Some(projected) = reference
                        .project_native_field(name)
                        .map_err(|message| RuntimeError::new(message, span))?
                {
                    return Ok(Place::Reference {
                        reference: Rc::new(projected),
                    });
                }
                let value = owner.projection_value(span)?;
                if let Value::Tuple(sequence) = value {
                    let index = name.parse::<usize>().map_err(|_| {
                        RuntimeError::new(format!("tuple has no field `{name}`"), span)
                    })?;
                    if index >= sequence.elements.borrow().len() {
                        return Err(RuntimeError::new(
                            format!("tuple index {index} is out of bounds"),
                            span,
                        ));
                    }
                    return Ok(Place::IndexedElement {
                        sequence,
                        index,
                        owner: owner_name,
                        mutable,
                        guard,
                    });
                }
                if let Value::Dynamic(object) = value {
                    let index = object
                        .descriptor()
                        .layout()
                        .record_field_index(name)
                        .ok_or_else(|| {
                            RuntimeError::new(
                                format!(
                                    "{} has no field `{name}`",
                                    object.descriptor().layout().rils_type()
                                ),
                                span,
                            )
                        })?;
                    let (structs, enums) = environment.borrow().visible_type_definitions();
                    let reference = ReferenceValue::new_dynamic_field(
                        object, index, mutable, guard, structs, enums,
                    )
                    .map_err(|message| RuntimeError::new(message, span))?;
                    return Ok(Place::Reference {
                        reference: Rc::new(reference),
                    });
                }
                let Value::Struct(instance) = value else {
                    return Err(RuntimeError::new(
                        format!("{} has no field `{name}`", value.type_name()),
                        span,
                    ));
                };
                if instance.type_definition.field_index(name).is_none() {
                    return Err(RuntimeError::new(
                        format!(
                            "struct `{}` has no field `{name}`",
                            instance.type_definition.name
                        ),
                        span,
                    ));
                }
                Ok(Place::StructField {
                    instance,
                    name: name.clone(),
                    owner: owner_name,
                    mutable,
                    guard,
                })
            }
            Expr::Index { object, index, .. } => {
                let owner = self.resolve_place(object, environment, span)?;
                let mutable = owner.is_mutable();
                let owner_name = owner.description();
                let guard = owner.projection_guard(span)?;
                let index = self.evaluate(index, environment.clone())?;
                let Some(index) = index.as_usize() else {
                    return Err(RuntimeError::new("collection indices must be usize", span));
                };
                if let Some(reference) = &guard
                    && let Some(projected) = reference
                        .project_native_index(index)
                        .map_err(|message| RuntimeError::new(message, span))?
                {
                    return Ok(Place::Reference {
                        reference: Rc::new(projected),
                    });
                }
                let value = owner.projection_value(span)?;
                let sequence = match value {
                    Value::Array(sequence) | Value::Vec(sequence) => sequence,
                    Value::Dynamic(sequence)
                        if rils_execution::value::native_layouts::vec::matches(
                            sequence.descriptor().layout().rils_type(),
                        ) =>
                    {
                        let length = sequence
                            .with(|value| value.sequence_len())
                            .map_err(|message| RuntimeError::new(message, span))?
                            .map_err(|message| RuntimeError::new(message, span))?;
                        if index >= length {
                            return Err(RuntimeError::new(
                                format!("index {index} is out of bounds"),
                                span,
                            ));
                        }
                        return Ok(Place::DynamicIndexedElement {
                            sequence,
                            codec: {
                                let (structs, enums) =
                                    environment.borrow().visible_type_definitions();
                                Rc::new(rils_execution::value::record_codec::NativeRecordCodec::with_definitions(&structs, &enums))
                            },
                            index,
                            owner: owner_name,
                            mutable,
                            guard,
                        });
                    }
                    value => {
                        return Err(RuntimeError::new(
                            format!("type `{}` does not support indexing", value.type_name()),
                            span,
                        ));
                    }
                };
                if index >= sequence.elements.borrow().len() {
                    return Err(RuntimeError::new(
                        format!("index {index} is out of bounds"),
                        span,
                    ));
                }
                Ok(Place::IndexedElement {
                    sequence,
                    index,
                    owner: owner_name,
                    mutable,
                    guard,
                })
            }
            _ => Err(RuntimeError::new(
                "expression does not refer to an assignable place",
                span,
            )),
        }
    }
}
