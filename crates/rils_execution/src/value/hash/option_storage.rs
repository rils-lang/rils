//! Retain typed Option outputs even after a compatibility collection becomes empty.
use std::collections::HashMap;

use crate::{
    Type,
    value::{
        Value,
        storage::{PreparedOption, TypedStorageContext},
    },
};

#[derive(Clone, Default)]
pub struct CollectionOptionStorage {
    outputs: HashMap<(bool, Type), PreparedOption>,
}

impl CollectionOptionStorage {
    pub(crate) fn prepare<'a>(
        &mut self,
        element: &Type,
        samples: impl IntoIterator<Item = &'a Value>,
        context: &TypedStorageContext<'_>,
    ) -> Result<PreparedOption, String> {
        self.prepare_output(false, element, samples, context)
    }

    pub(crate) fn prepare_key<'a>(
        &mut self,
        element: &Type,
        samples: impl IntoIterator<Item = &'a Value>,
        context: &TypedStorageContext<'_>,
    ) -> Result<PreparedOption, String> {
        self.prepare_output(true, element, samples, context)
    }

    fn prepare_output<'a>(
        &mut self,
        key: bool,
        element: &Type,
        samples: impl IntoIterator<Item = &'a Value>,
        context: &TypedStorageContext<'_>,
    ) -> Result<PreparedOption, String> {
        let identity = (key, element.clone());
        let mut samples = samples.into_iter();
        let first = samples.next();
        let ty = Type::Option(Box::new(element.clone()));
        let mut output = match self.outputs.get(&identity) {
            Some(output) => {
                let mut output = output.clone();
                if let Some(value) = first {
                    output.retain_sample(value)?;
                }
                output
            }
            None => PreparedOption::new(context, &ty, first)?,
        };
        for value in samples {
            output.retain_sample(value)?;
        }
        self.outputs.insert(identity, output.clone());
        Ok(output)
    }

    pub(crate) fn merge(&self, other: &Self) -> Result<Self, String> {
        let mut merged = self.clone();
        for (ty, output) in &other.outputs {
            match merged.outputs.get_mut(ty) {
                Some(existing) => existing.merge(output)?,
                None => {
                    merged.outputs.insert(ty.clone(), output.clone());
                }
            }
        }
        Ok(merged)
    }
}
