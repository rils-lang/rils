use super::*;

pub(super) struct ResolvedPlace {
    local: usize,
    projections: Vec<ResolvedProjection>,
}

enum ResolvedProjection {
    Field(String),
    Index(usize),
}

enum PlaceContainer {
    Struct(Rc<StructInstance>),
    Sequence(Rc<SequenceValue>),
}

impl VirtualMachine<'_> {
    pub(super) fn resolve_place(
        &mut self,
        place: BytecodePlace,
        span: Span,
    ) -> Result<ResolvedPlace, BytecodeError> {
        let mut projections = Vec::with_capacity(place.projections.len());
        for projection in place.projections {
            projections.push(match projection {
                BytecodeProjection::Field(field) => ResolvedProjection::Field(field),
                BytecodeProjection::Index(register) => {
                    let value = self.take_register(register, span)?;
                    let Value::Usize(index) = value else {
                        return Err(BytecodeError::new("collection index must be usize", span));
                    };
                    ResolvedProjection::Index(index)
                }
            });
        }
        Ok(ResolvedPlace {
            local: place.local,
            projections,
        })
    }

    fn place_root(&self, local: usize, span: Span) -> Result<PlaceContainer, BytecodeError> {
        let value = self.frame().locals[local]
            .borrow()
            .read()
            .map_err(|error| access_error(error, span))?;
        self.place_container(value, span)
    }

    pub(super) fn place_is_mutable(&self, local: usize, span: Span) -> Result<bool, BytecodeError> {
        let value = self.frame().locals[local]
            .borrow()
            .read()
            .map_err(|error| access_error(error, span))?;
        match value {
            Value::Reference(reference) => Ok(reference.mutable),
            _ => Ok(self.current_function().local_mutability[local]),
        }
    }

    fn place_container(&self, value: Value, span: Span) -> Result<PlaceContainer, BytecodeError> {
        match value {
            Value::Struct(instance) => Ok(PlaceContainer::Struct(instance)),
            Value::Tuple(sequence) | Value::Array(sequence) | Value::Vec(sequence) => {
                Ok(PlaceContainer::Sequence(sequence))
            }
            Value::Reference(reference) => self.place_container(
                reference
                    .read()
                    .map_err(|message| BytecodeError::new(message, span))?,
                span,
            ),
            value => Err(BytecodeError::new(
                format!("cannot project into {}", value.type_name()),
                span,
            )),
        }
    }

    fn projected_value(
        &self,
        container: &PlaceContainer,
        projection: &ResolvedProjection,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        match (container, projection) {
            (PlaceContainer::Struct(instance), ResolvedProjection::Field(field)) => {
                let fields = instance.fields.borrow();
                let slot = fields
                    .get(field)
                    .ok_or_else(|| BytecodeError::new(format!("unknown field `{field}`"), span))?;
                slot.value.clone().ok_or_else(|| {
                    BytecodeError::new(format!("field `{field}` has been moved"), span)
                })
            }
            (PlaceContainer::Sequence(sequence), ResolvedProjection::Index(index)) => {
                let elements = sequence.elements.borrow();
                let slot = elements.get(*index).ok_or_else(|| {
                    BytecodeError::new(format!("index {index} is out of bounds"), span)
                })?;
                slot.value.clone().ok_or_else(|| {
                    BytecodeError::new(format!("element at index {index} has been moved"), span)
                })
            }
            _ => Err(BytecodeError::new(
                "place projection does not match its value",
                span,
            )),
        }
    }

    fn place_parent<'p>(
        &self,
        place: &'p ResolvedPlace,
        span: Span,
    ) -> Result<(PlaceContainer, &'p ResolvedProjection), BytecodeError> {
        let (last, parents) = place
            .projections
            .split_last()
            .ok_or_else(|| BytecodeError::new("place projection cannot be empty", span))?;
        let mut container = self.place_root(place.local, span)?;
        for projection in parents {
            let value = self.projected_value(&container, projection, span)?;
            container = self.place_container(value, span)?;
        }
        Ok((container, last))
    }

    pub(super) fn take_place(
        &self,
        place: &ResolvedPlace,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let (container, projection) = self.place_parent(place, span)?;
        match (container, projection) {
            (PlaceContainer::Struct(instance), ResolvedProjection::Field(field)) => {
                take_field_slot(instance.fields.borrow_mut().get_mut(field), field, span)
            }
            (PlaceContainer::Sequence(sequence), ResolvedProjection::Index(index)) => {
                if *index >= sequence.elements.borrow().len() {
                    return Err(BytecodeError::new(
                        format!("index {index} is out of bounds"),
                        span,
                    ));
                }
                take_field_slot(
                    sequence.elements.borrow_mut().get_mut(*index),
                    &format!("index {index}"),
                    span,
                )
            }
            _ => Err(BytecodeError::new(
                "place projection does not match its value",
                span,
            )),
        }
    }

    pub(super) fn store_place(
        &self,
        place: &ResolvedPlace,
        value: Value,
        span: Span,
    ) -> Result<(), BytecodeError> {
        let (container, projection) = self.place_parent(place, span)?;
        match (container, projection) {
            (PlaceContainer::Struct(instance), ResolvedProjection::Field(field)) => {
                store_field_slot(
                    instance.fields.borrow_mut().get_mut(field),
                    field,
                    value,
                    span,
                )
            }
            (PlaceContainer::Sequence(sequence), ResolvedProjection::Index(index)) => {
                if *index >= sequence.elements.borrow().len() {
                    return Err(BytecodeError::new(
                        format!("index {index} is out of bounds"),
                        span,
                    ));
                }
                store_field_slot(
                    sequence.elements.borrow_mut().get_mut(*index),
                    &format!("index {index}"),
                    value,
                    span,
                )
            }
            _ => Err(BytecodeError::new(
                "place projection does not match its value",
                span,
            )),
        }
    }

    pub(super) fn place_reference(
        &self,
        place: &ResolvedPlace,
        mutable: bool,
        span: Span,
    ) -> Result<ReferenceValue, BytecodeError> {
        let mut container = self.place_root(place.local, span)?;
        let mut guard = None;
        for (index, projection) in place.projections.iter().enumerate() {
            let reference = match (&container, projection) {
                (PlaceContainer::Struct(instance), ResolvedProjection::Field(field)) => {
                    ReferenceValue::new_guarded_struct_field(
                        instance.clone(),
                        field.clone(),
                        mutable,
                        guard,
                    )
                }
                (PlaceContainer::Sequence(sequence), ResolvedProjection::Index(element)) => {
                    ReferenceValue::new_guarded_sequence_element(
                        sequence.clone(),
                        *element,
                        mutable,
                        guard,
                    )
                }
                _ => {
                    return Err(BytecodeError::new(
                        "place projection does not match its value",
                        span,
                    ));
                }
            }
            .map_err(|message| BytecodeError::new(message, span))?;
            if index + 1 == place.projections.len() {
                return Ok(reference);
            }
            let reference = Rc::new(reference);
            let value = reference
                .read()
                .map_err(|message| BytecodeError::new(message, span))?;
            container = self.place_container(value, span)?;
            guard = Some(reference);
        }
        unreachable!("empty place projections are rejected")
    }
}
