use super::*;

pub(super) enum Place {
    Storage {
        slot: crate::environment::StorageRef,
        name: String,
    },
    NativeField {
        indexed: bool,
        place: rils_execution::value::native_instance::NativeInstancePlace,
        name: String,
        owner: String,
        mutable: bool,
        source: Option<crate::environment::StorageRef>,
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
            let layout = place
                .field_layout()
                .map_err(|message| RuntimeError::new(message, span))?;
            return Ok(
                (!matches!(layout.rils_type(), Type::Reference { .. })).then(|| place.clone())
            );
        }
        let value = match self {
            Self::Storage { slot, .. } => slot
                .borrow()
                .with_value(|value| {
                    Ok(match value {
                        Value::Dynamic(object) => Some(object.clone()),
                        _ => None,
                    })
                })
                .ok()
                .flatten(),

            _ => None,
        };
        match value {
            Some(object)
                if rils_execution::value::native_instance::definition(&object).is_some()
                    || matches!(
                        object.descriptor().layout().rils_type(),
                        Type::Tuple(_) | Type::Array { .. }
                    ) =>
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
            Self::NativeField { place, indexed, .. } => {
                if *indexed
                    && !place
                        .field_layout()
                        .map_err(|message| RuntimeError::new(message, span))?
                        .is_copy()
                {
                    return Err(RuntimeError::new(
                        "cannot move a non-Copy value out through indexing",
                        span,
                    ));
                }
                place
                    .take()
                    .map_err(|message| RuntimeError::new(message, span))
            }
            Self::Storage { slot, name } => slot.borrow().read().map_err(|_| {
                RuntimeError::new(format!("cannot access moved value `{name}`"), span)
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
                    let partially_moved = storage
                        .with_value(|value| Ok(value.is_partially_moved()))
                        .map_err(|_| {
                            RuntimeError::new(
                                format!("cannot reference moved value `{name}`"),
                                span,
                            )
                        })?;
                    if partially_moved {
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
        Ok(Value::from_reference(reference))
    }

    fn is_mutable(&self) -> bool {
        match self {
            Self::NativeField { mutable, .. } => *mutable,
            Self::Storage { slot, .. } => {
                let storage = slot.borrow();
                storage
                    .with_value(|value| Ok(value.as_reference().map(|reference| reference.mutable)))
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| storage.is_mutable())
            }

            Self::DynamicIndexedElement { mutable, .. } => *mutable,
            Self::Reference { reference } => reference.mutable,
        }
    }

    fn description(&self) -> String {
        match self {
            Self::NativeField { owner, name, .. } => format!("{owner}.{name}"),
            Self::Storage { name, .. } => name.clone(),

            Self::DynamicIndexedElement { owner, index, .. } => format!("{owner}[{index}]"),
            Self::Reference { .. } => "reference".into(),
        }
    }

    /// Keep projections as references; only root storage needs an owner handle.
    pub(super) fn projection_receiver(&self, span: Span) -> Result<Value, RuntimeError> {
        if let Some(reference) = self.projection_guard(span)? {
            return Ok(Value::from_reference(reference));
        }
        match self {
            Self::Storage { slot, name } => slot
                .borrow()
                .with_value(|value| Ok(value.clone()))
                .map_err(|_| {
                    RuntimeError::new(format!("cannot access moved value `{name}`"), span)
                }),
            _ => unreachable!("projected places retain a reference guard"),
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
            } => {
                if let Some(reference) = place
                    .stored_reference()
                    .map_err(|message| RuntimeError::new(message, span))?
                {
                    return Ok(Some(reference));
                }
                Ok(Some(Rc::new(
                    place
                        .borrow(
                            *mutable,
                            source
                                .clone()
                                .map(|slot| Rc::new(ReferenceValue::new_storage(slot, *mutable))),
                        )
                        .map_err(|message| RuntimeError::new(message, span))?,
                )))
            }
            Self::Storage { slot, name } => slot
                .borrow()
                .with_value(|value| Ok(value.as_reference()))
                .map_err(|_| {
                    RuntimeError::new(format!("cannot access moved value `{name}`"), span)
                }),

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
                let Some(reference) = value.as_reference() else {
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
                        indexed: false,
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
                let value = owner.projection_receiver(span)?;
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
                Err(RuntimeError::new(
                    format!("{} has no field `{name}`", value.type_name()),
                    span,
                ))
            }
            Expr::Index { object, index, .. } => {
                let owner = self.resolve_place(object, environment, span)?;
                let mutable = owner.is_mutable();
                let owner_name = owner.description();
                let index = self.evaluate(index, environment.clone())?;
                let Ok(index) = index.get_cloned::<usize>() else {
                    return Err(RuntimeError::new("collection indices must be usize", span));
                };
                if let Some(place) = owner.native_owner(span)?
                    && matches!(
                        place
                            .field_layout()
                            .map_err(|message| RuntimeError::new(message, span))?
                            .rils_type(),
                        Type::Array { .. }
                    )
                {
                    let layout = place
                        .field_layout()
                        .map_err(|message| RuntimeError::new(message, span))?;
                    let Type::Array { length, .. } = layout.rils_type() else {
                        unreachable!()
                    };
                    if index >= *length {
                        return Err(RuntimeError::new(
                            format!("index {index} is out of bounds"),
                            span,
                        ));
                    }
                    return Ok(Place::NativeField {
                        indexed: true,
                        place: place
                            .field(&index.to_string())
                            .map_err(|message| RuntimeError::new(message, span))?,
                        name: index.to_string(),
                        owner: owner_name,
                        mutable,
                        source: owner.native_source(),
                    });
                }
                let guard = owner.projection_guard(span)?;
                if let Some(reference) = &guard
                    && let Some(projected) = reference
                        .project_native_index(index)
                        .map_err(|message| RuntimeError::new(message, span))?
                {
                    return Ok(Place::Reference {
                        reference: Rc::new(projected),
                    });
                }
                let value = owner.projection_receiver(span)?;
                match value {
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
                        Ok(Place::DynamicIndexedElement {
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
                        })
                    }
                    value => Err(RuntimeError::new(
                        format!("type `{}` does not support indexing", value.type_name()),
                        span,
                    )),
                }
            }
            _ => Err(RuntimeError::new(
                "expression does not refer to an assignable place",
                span,
            )),
        }
    }
}
