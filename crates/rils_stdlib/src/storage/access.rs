//! Validated collection storage and declaration-derived entry projections.
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue, DynamicValueMut, DynamicValueRef};
use std::rc::Rc;

mod sequence;

/// The language type parameters that occupy one stored entry.
#[derive(Clone, Copy, Debug)]
pub enum EntryShape {
    Parameter(usize),
    Tuple(&'static [EntryShape]),
}

/// Generated from a collection's storage declaration, independently of its name.
#[derive(Clone, Copy, Debug)]
pub struct EntrySchema {
    pub owner: &'static str,
    pub parameters: usize,
    pub entry: EntryShape,
}

impl EntrySchema {
    pub fn validate(&self, layout: &DynamicLayout) -> Result<(), String> {
        if matches!(self.entry, EntryShape::Parameter(0))
            && self.parameters == 1
            && matches!(layout.rils_type(), Type::Array { .. })
        {
            return Ok(());
        }
        let Type::Named { name, arguments } = layout.rils_type() else {
            return Err(format!("wrong {} receiver", self.owner));
        };
        if name != self.owner || arguments.len() != self.parameters {
            return Err(format!("wrong {} receiver", self.owner));
        }
        let entry = layout
            .sequence_item()
            .ok_or("collection requires sequence storage")?;
        let mut seen = vec![false; self.parameters];
        self.entry.validate(entry, arguments, &mut seen)?;
        if seen.iter().any(|seen| !seen) {
            return Err("entry schema omits a language type parameter".into());
        }
        Ok(())
    }

    pub fn parameter_path(&self, parameter: usize) -> Result<Vec<DynamicPathStep>, String> {
        let mut path = Vec::new();
        if parameter < self.parameters && self.entry.find(parameter, &mut path) {
            Ok(path)
        } else {
            Err(format!("entry schema has no parameter {parameter}"))
        }
    }

    pub fn parameter_layout(
        &self,
        layout: &DynamicLayout,
        parameter: usize,
    ) -> Result<Rc<DynamicLayout>, String> {
        self.validate(layout)?;
        let mut current = layout
            .sequence_item()
            .ok_or("collection requires sequence storage")?
            .clone();
        for step in self.parameter_path(parameter)? {
            let DynamicPathStep::Field(index) = step else {
                unreachable!()
            };
            current = current
                .record_fields()
                .and_then(|fields| fields.get(index))
                .ok_or("entry schema requires a tuple field")?
                .layout_handle();
        }
        Ok(current)
    }

    pub fn project<'a>(
        &self,
        mut entry: DynamicValueRef<'a>,
        parameter: usize,
    ) -> Result<DynamicValueRef<'a>, String> {
        for step in self.parameter_path(parameter)? {
            entry = entry.project(step)?;
        }
        Ok(entry)
    }

    pub fn take(&self, mut entry: DynamicValue, parameter: usize) -> Result<DynamicValue, String> {
        let path = self.parameter_path(parameter)?;
        if path.is_empty() {
            Ok(entry)
        } else {
            entry.take_path_field(&path)
        }
    }

    /// Move language arguments into the declared entry shape, including reordered tuples.
    pub fn build_entry(
        &self,
        layout: Rc<DynamicLayout>,
        parameters: Vec<DynamicValue>,
    ) -> Result<DynamicValue, String> {
        if parameters.len() != self.parameters {
            return Err("entry argument count does not match its declaration".into());
        }
        let mut parameters: Vec<_> = parameters.into_iter().map(Some).collect();
        let result = self.entry.build(layout, &mut parameters)?;
        if parameters.iter().any(Option::is_some) {
            return Err("entry schema omits a language type parameter".into());
        }
        Ok(result)
    }
}

impl EntryShape {
    fn validate(
        &self,
        layout: &DynamicLayout,
        types: &[Type],
        seen: &mut [bool],
    ) -> Result<(), String> {
        match self {
            Self::Parameter(index) => {
                let expected = types
                    .get(*index)
                    .ok_or("entry parameter index is out of bounds")?;
                if std::mem::replace(&mut seen[*index], true) {
                    return Err("entry schema repeats a language type parameter".into());
                }
                if layout.rils_type() != expected {
                    return Err("entry parameter layout does not match its declaration".into());
                }
            }
            Self::Tuple(shapes) => {
                if !matches!(layout.rils_type(), Type::Tuple(_)) {
                    return Err("entry schema requires a tuple layout".into());
                }
                let fields = layout.record_fields().ok_or("entry tuple has no fields")?;
                if fields.len() != shapes.len() {
                    return Err("entry tuple arity does not match its declaration".into());
                }
                for (shape, field) in shapes.iter().zip(fields) {
                    shape.validate(field.layout(), types, seen)?;
                }
            }
        }
        Ok(())
    }

    fn find(&self, parameter: usize, path: &mut Vec<DynamicPathStep>) -> bool {
        match self {
            Self::Parameter(index) => *index == parameter,
            Self::Tuple(shapes) => {
                for (index, shape) in shapes.iter().enumerate() {
                    path.push(DynamicPathStep::Field(index));
                    if shape.find(parameter, path) {
                        return true;
                    }
                    path.pop();
                }
                false
            }
        }
    }

    fn build(
        &self,
        layout: Rc<DynamicLayout>,
        parameters: &mut [Option<DynamicValue>],
    ) -> Result<DynamicValue, String> {
        match self {
            Self::Parameter(index) => {
                let value = parameters
                    .get_mut(*index)
                    .and_then(Option::take)
                    .ok_or("entry parameter is absent or repeated")?;
                if !layout.compatible_with(value.descriptor()) {
                    return Err("entry argument has a different native layout".into());
                }
                Ok(value)
            }
            Self::Tuple(shapes) => {
                let fields = layout.record_fields().ok_or("entry tuple has no fields")?;
                if fields.len() != shapes.len() {
                    return Err("entry tuple arity does not match its declaration".into());
                }
                let values = shapes
                    .iter()
                    .zip(fields)
                    .map(|(shape, field)| shape.build(field.layout_handle(), parameters))
                    .collect::<Result<Vec<_>, _>>()?;
                DynamicValue::record(layout, values)
            }
        }
    }
}

/// Execution supplies nominal metadata and cloning without defining collection policy.
pub trait NativeServices {
    fn implements(&self, ty: &Type, required: &str) -> bool;
    fn clone_value(&self, value: DynamicValueRef<'_>) -> Result<DynamicValue, String>;
}

pub struct SharedAccess<'a> {
    pub(crate) view: DynamicValueRef<'a>,
    pub(crate) schema: &'a EntrySchema,
    pub(crate) services: &'a dyn NativeServices,
}
impl<'a> SharedAccess<'a> {
    pub fn new(
        view: DynamicValueRef<'a>,
        schema: &'a EntrySchema,
        services: &'a dyn NativeServices,
    ) -> Result<Self, String> {
        schema.validate(view.layout()?.as_ref())?;
        view.indexed()?;
        Ok(Self {
            view,
            schema,
            services,
        })
    }
}

pub struct MutableAccess<'a> {
    pub(crate) view: DynamicValueMut<'a>,
    pub(crate) schema: &'a EntrySchema,
    pub(crate) services: &'a dyn NativeServices,
}
impl<'a> MutableAccess<'a> {
    pub fn new(
        view: DynamicValueMut<'a>,
        schema: &'a EntrySchema,
        services: &'a dyn NativeServices,
    ) -> Result<Self, String> {
        schema.validate(view.view().layout()?.as_ref())?;
        view.sequence_len()?;
        Ok(Self {
            view,
            schema,
            services,
        })
    }
}

impl super::SequenceRead for SharedAccess<'_> {
    fn len(&self) -> usize {
        self.view
            .indexed()
            .expect("validated indexed storage")
            .len()
    }
}
impl super::SequenceRead for MutableAccess<'_> {
    fn len(&self) -> usize {
        self.view
            .sequence_len()
            .expect("validated sequence storage")
    }
}
impl super::SequenceClear for MutableAccess<'_> {
    type ClearOutput = Result<(), String>;
    fn clear(&mut self) -> Self::ClearOutput {
        self.view.clear_sequence()
    }
}
