use super::*;

pub(super) struct ResolvedPlace {
    local: usize,
    projections: Vec<ResolvedProjection>,
}

enum ResolvedProjection {
    Field(String),
    RecordField { type_id: usize, index: usize },
    Index(usize),
}

enum PlaceContainer {
    NativeOwner(rils_execution::value::native_instance::NativeInstancePlace),
    DynamicIndexed(rils_execution::value::DynamicObject),
    DynamicRecord(rils_execution::value::DynamicObject),
    NativeReference(Rc<ReferenceValue>),
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
                BytecodeProjection::RecordField { type_id, index } => {
                    ResolvedProjection::RecordField { type_id, index }
                }
                BytecodeProjection::Index(register) => {
                    let value = self.take_register(register, span)?;
                    let Ok(index) = value.get_cloned::<usize>() else {
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
        self.frame().locals[local]
            .borrow()
            .with_value(|value| {
                self.place_container(value, span)
                    .map_err(|error| error.message)
            })
            .map_err(|message| BytecodeError::new(message, span))
    }

    pub(super) fn place_is_mutable(&self, local: usize, span: Span) -> Result<bool, BytecodeError> {
        self.frame().locals[local]
            .borrow()
            .with_value(|value| {
                Ok(value.as_reference().map_or(
                    self.current_function().local_mutability[local],
                    |reference| reference.mutable,
                ))
            })
            .map_err(|message| BytecodeError::new(message, span))
    }

    fn place_container(&self, value: &Value, span: Span) -> Result<PlaceContainer, BytecodeError> {
        if let Some(reference) = value.as_reference() {
            if reference
                .native_layout()
                .map_err(|message| BytecodeError::new(message, span))?
                .is_some()
            {
                return Ok(PlaceContainer::NativeReference(reference));
            }
            return Err(BytecodeError::new(
                format!("cannot project into {}", value.type_name()),
                span,
            ));
        }
        match value {
            Value::Dynamic(object)
                if matches!(
                    rils_execution::value::native_instance::definition(object),
                    Some(
                        crate::value::Declaration::StructType(_)
                            | crate::value::Declaration::EnumType(_)
                    )
                ) || matches!(
                    object.descriptor().layout().rils_type(),
                    Type::Tuple(_) | Type::Array { .. }
                ) =>
            {
                rils_execution::value::native_instance::NativeInstancePlace::new(object.clone())
                    .map(PlaceContainer::NativeOwner)
                    .map_err(|message| BytecodeError::new(message, span))
            }

            Value::Dynamic(object)
                if rils_execution::value::native_layouts::vec::matches(
                    object.descriptor().layout().rils_type(),
                ) =>
            {
                Ok(PlaceContainer::DynamicIndexed(object.clone()))
            }
            Value::Dynamic(object) if object.descriptor().layout().record_fields().is_some() => {
                Ok(PlaceContainer::DynamicRecord(object.clone()))
            }

            value => Err(BytecodeError::new(
                format!("cannot project into {}", value.type_name()),
                span,
            )),
        }
    }

    fn dynamic_field(
        &self,
        layout: &rils_value::DynamicLayout,
        projection: &ResolvedProjection,
        span: Span,
    ) -> Result<usize, BytecodeError> {
        match projection {
            ResolvedProjection::Field(name) => layout
                .record_field_index(name)
                .ok_or_else(|| BytecodeError::new(format!("unknown field `{name}`"), span)),
            ResolvedProjection::RecordField { type_id, index } => {
                let Some(RuntimeType::Struct(definition)) = self.module.types.get(*type_id) else {
                    return Err(BytecodeError::new("invalid record field type", span));
                };
                if !matches!(layout.rils_type(), Type::Named { name, .. } if name == &definition.name)
                {
                    return Err(BytecodeError::new(
                        "record field projection does not match its type",
                        span,
                    ));
                }
                let field = layout
                    .record_fields()
                    .and_then(|fields| fields.get(*index))
                    .ok_or_else(|| {
                        BytecodeError::new("record field index is out of bounds", span)
                    })?;
                let expected = definition.fields.get(*index).ok_or_else(|| {
                    BytecodeError::new("record field index is out of bounds", span)
                })?;
                if field.name() != expected.name {
                    return Err(BytecodeError::new(
                        "record field projection does not match its declaration",
                        span,
                    ));
                }
                Ok(*index)
            }
            ResolvedProjection::Index(_) => Err(BytecodeError::new(
                "expected a record field projection",
                span,
            )),
        }
    }

    fn native_owner_projection(
        &self,
        owner: &rils_execution::value::native_instance::NativeInstancePlace,
        projection: &ResolvedProjection,
        span: Span,
    ) -> Result<rils_execution::value::native_instance::NativeInstancePlace, BytecodeError> {
        let layout = owner
            .with_view(|view| view.layout())
            .map_err(|message| BytecodeError::new(message, span))?
            .map_err(|message| BytecodeError::new(message, span))?;
        if let ResolvedProjection::Index(index) = projection {
            if let Type::Array { length, .. } = layout.rils_type()
                && index >= length
            {
                return Err(BytecodeError::new(
                    format!("index {index} is out of bounds"),
                    span,
                ));
            }
            if layout.record_fields().is_some() {
                return owner
                    .field(&index.to_string())
                    .map_err(|message| BytecodeError::new(message, span));
            }
            return owner
                .project(rils_value::DynamicPathStep::Index(*index))
                .map_err(|message| BytecodeError::new(message, span));
        }
        let index = self.dynamic_field(&layout, projection, span)?;
        let name = layout.record_fields().expect("validated record")[index].name();
        owner
            .field(name)
            .map_err(|message| BytecodeError::new(message, span))
    }

    fn dynamic_field_reference(
        &self,
        object: rils_execution::value::DynamicObject,
        projection: &ResolvedProjection,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
        span: Span,
    ) -> Result<ReferenceValue, BytecodeError> {
        let index = self.dynamic_field(object.descriptor().layout(), projection, span)?;
        let (structs, enums) = self.type_definitions();
        ReferenceValue::new_dynamic_field(object, index, mutable, guard, structs, enums)
            .map_err(|message| BytecodeError::new(message, span))
    }

    fn dynamic_index_reference(
        &self,
        object: rils_execution::value::DynamicObject,
        index: usize,
        mutable: bool,
        guard: Option<Rc<ReferenceValue>>,
    ) -> Result<ReferenceValue, String> {
        let (structs, enums) = self.type_definitions();
        let codec = Rc::new(
            rils_execution::value::record_codec::NativeRecordCodec::with_definitions(
                &structs, &enums,
            ),
        );
        ReferenceValue::new_guarded_dynamic_indexed_element_with_codec(
            object,
            index,
            mutable,
            guard,
            Some(codec),
        )
    }

    fn native_projection(
        &self,
        reference: &Rc<ReferenceValue>,
        projection: &ResolvedProjection,
        span: Span,
    ) -> Result<ReferenceValue, BytecodeError> {
        let result = match projection {
            ResolvedProjection::Index(index) => reference.project_native_index(*index),
            _ => {
                let layout = reference
                    .native_layout()
                    .map_err(|message| BytecodeError::new(message, span))?
                    .ok_or_else(|| BytecodeError::new("reference has no native layout", span))?;
                let index = self.dynamic_field(&layout, projection, span)?;
                let name = layout.record_fields().expect("validated record")[index].name();
                reference.project_native_field(name)
            }
        };
        result
            .map_err(|message| BytecodeError::new(message, span))?
            .ok_or_else(|| {
                BytecodeError::new("place projection does not match its native reference", span)
            })
    }

    fn projected_reference(
        &self,
        container: &PlaceContainer,
        projection: &ResolvedProjection,
        mutable: bool,
        span: Span,
    ) -> Result<Rc<ReferenceValue>, BytecodeError> {
        match (container, projection) {
            (PlaceContainer::DynamicIndexed(object), ResolvedProjection::Index(index)) => self
                .dynamic_index_reference(object.clone(), *index, mutable, None)
                .map(Rc::new)
                .map_err(|message| BytecodeError::new(message, span)),
            (
                PlaceContainer::DynamicRecord(object),
                projection
                @ (ResolvedProjection::Field(_) | ResolvedProjection::RecordField { .. }),
            ) => self
                .dynamic_field_reference(object.clone(), projection, mutable, None, span)
                .map(Rc::new),
            (PlaceContainer::NativeReference(reference), projection) => self
                .native_projection(reference, projection, span)
                .map(Rc::new),
            _ => Err(BytecodeError::new(
                "place projection does not match its value",
                span,
            )),
        }
    }

    fn native_place_container(
        &self,
        owner: rils_execution::value::native_instance::NativeInstancePlace,
        span: Span,
    ) -> Result<PlaceContainer, BytecodeError> {
        match owner
            .stored_reference()
            .map_err(|message| BytecodeError::new(message, span))?
        {
            Some(reference) => Ok(PlaceContainer::NativeReference(reference)),
            None => Ok(PlaceContainer::NativeOwner(owner)),
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
        let mutable = self.place_is_mutable(place.local, span)?;
        for projection in parents {
            if let PlaceContainer::NativeOwner(owner) = container {
                container = self.native_place_container(
                    self.native_owner_projection(&owner, projection, span)?,
                    span,
                )?;
                continue;
            }
            container = PlaceContainer::NativeReference(
                self.projected_reference(&container, projection, mutable, span)?,
            );
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
            (PlaceContainer::NativeOwner(owner), projection) => {
                let indexed_array = matches!(projection, ResolvedProjection::Index(_))
                    && matches!(
                        owner
                            .field_layout()
                            .map_err(|message| BytecodeError::new(message, span))?
                            .rils_type(),
                        Type::Array { .. }
                    );
                let child = self.native_owner_projection(&owner, projection, span)?;
                if indexed_array
                    && !child
                        .field_layout()
                        .map_err(|message| BytecodeError::new(message, span))?
                        .is_copy()
                {
                    return Err(BytecodeError::new(
                        "cannot move a non-Copy value out through indexing",
                        span,
                    ));
                }
                child
                    .take()
                    .map_err(|message| BytecodeError::new(message, span))
            }

            (PlaceContainer::DynamicIndexed(object), ResolvedProjection::Index(index)) => {
                rils_execution::value::dynamic_sequence::copy_item(&object, *index)
                    .map_err(|message| BytecodeError::new(message, span))
            }
            (PlaceContainer::NativeReference(reference), projection) => self
                .native_projection(&reference, projection, span)?
                .copy_native()
                .map_err(|message| BytecodeError::new(message, span))?
                .ok_or_else(|| BytecodeError::new("reference has no native storage", span)),
            (
                PlaceContainer::DynamicRecord(object),
                projection
                @ (ResolvedProjection::Field(_) | ResolvedProjection::RecordField { .. }),
            ) => self
                .dynamic_field_reference(object, projection, false, None, span)?
                .read()
                .map_err(|message| BytecodeError::new(message, span)),
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
            (PlaceContainer::NativeOwner(owner), projection) => self
                .native_owner_projection(&owner, projection, span)?
                .assign(value)
                .map_err(|message| BytecodeError::new(message, span)),

            (PlaceContainer::DynamicIndexed(object), ResolvedProjection::Index(index)) => {
                rils_execution::value::dynamic_sequence::replace_item(&object, *index, value)
                    .map_err(|message| BytecodeError::new(message, span))
            }
            (PlaceContainer::NativeReference(reference), projection) => self
                .native_projection(&reference, projection, span)?
                .write(value)
                .map_err(|error| BytecodeError::new(format!("{error:?}"), span)),
            (
                PlaceContainer::DynamicRecord(object),
                projection
                @ (ResolvedProjection::Field(_) | ResolvedProjection::RecordField { .. }),
            ) => self
                .dynamic_field_reference(object, projection, true, None, span)?
                .write(value)
                .map_err(|error| BytecodeError::new(format!("{error:?}"), span)),
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
            if let PlaceContainer::NativeOwner(owner) = &container {
                let projected = self.native_owner_projection(owner, projection, span)?;
                if index + 1 == place.projections.len() {
                    if guard.is_none() {
                        guard = Some(Rc::new(ReferenceValue::new_storage(
                            self.frame().locals[place.local].clone(),
                            mutable,
                        )));
                    }
                    return projected
                        .borrow(mutable, guard)
                        .map_err(|message| BytecodeError::new(message, span));
                }
                container = self.native_place_container(projected, span)?;
                continue;
            }
            let reference = match (&container, projection) {
                (PlaceContainer::DynamicIndexed(object), ResolvedProjection::Index(element)) => {
                    self.dynamic_index_reference(object.clone(), *element, mutable, guard)
                }
                (PlaceContainer::NativeReference(reference), projection) => self
                    .native_projection(reference, projection, span)
                    .and_then(|projected| {
                        projected
                            .reborrow(mutable)
                            .map_err(|message| BytecodeError::new(message, span))
                    })
                    .map_err(|error| error.message),
                (
                    PlaceContainer::DynamicRecord(object),
                    projection @ (ResolvedProjection::Field(_)
                    | ResolvedProjection::RecordField { .. }),
                ) => self
                    .dynamic_field_reference(object.clone(), projection, mutable, guard, span)
                    .map_err(|error| error.message),
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
            container = PlaceContainer::NativeReference(reference.clone());
            guard = Some(reference);
        }
        unreachable!("empty place projections are rejected")
    }
}
