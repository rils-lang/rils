//! Concrete native layouts derived from the existing user struct declarations.

use std::{collections::HashMap, rc::Rc};

use rils_value::DynamicLayout;

use crate::Type;

use super::{StructType, native_layouts};

/// Resolves a concrete Rils type once per instance, reusing child layout
/// descriptors so nested values can check their exact layout identity.
pub struct RecordLayoutResolver<'a> {
    definitions: &'a [Rc<StructType>],
    cache: Vec<(Type, Rc<DynamicLayout>)>,
    resolving: Vec<Type>,
}

impl<'a> RecordLayoutResolver<'a> {
    pub fn new(definitions: &'a [Rc<StructType>]) -> Self {
        Self {
            definitions,
            cache: Vec::new(),
            resolving: Vec::new(),
        }
    }

    pub fn resolve(&mut self, ty: &Type) -> Result<Rc<DynamicLayout>, String> {
        if let Some((_, layout)) = self.cache.iter().find(|(known, _)| known == ty) {
            return Ok(layout.clone());
        }
        if self.resolving.contains(ty) {
            return Err(format!("recursive inline native layout for {ty}"));
        }
        self.resolving.push(ty.clone());
        let result = self.resolve_uncached(ty);
        self.resolving.pop();
        let layout = result?;
        self.cache.push((ty.clone(), layout.clone()));
        Ok(layout)
    }

    fn resolve_uncached(&mut self, ty: &Type) -> Result<Rc<DynamicLayout>, String> {
        if let Some(layout) = native_layouts::integer::layout(ty)
            .or_else(|| native_layouts::float::layout(ty))
            .or_else(|| (ty == &Type::String).then(native_layouts::string::layout))
        {
            return Ok(layout);
        }
        match ty {
            Type::Unit => Ok(DynamicLayout::copy_of::<()>(Type::Unit)),
            Type::Bool => Ok(DynamicLayout::copy_of::<bool>(Type::Bool)),
            Type::Char => Ok(DynamicLayout::copy_of::<char>(Type::Char)),
            Type::Option(item) => DynamicLayout::option(self.resolve(item)?),
            Type::Named { name, arguments } => self.resolve_named(name, arguments, ty),
            _ => Err(format!("no native layout is registered for {ty}")),
        }
    }

    fn resolve_named(
        &mut self,
        name: &str,
        arguments: &[Type],
        ty: &Type,
    ) -> Result<Rc<DynamicLayout>, String> {
        let mut definitions = self
            .definitions
            .iter()
            .filter(|definition| definition.name == name);
        let definition = definitions
            .next()
            .ok_or_else(|| format!("no user struct declaration for {ty}"))?;
        if definitions.next().is_some() {
            return Err(format!("ambiguous user struct declaration for {ty}"));
        }
        if definition.generic_parameters.len() != arguments.len() {
            return Err(format!(
                "struct `{name}` requires {} type arguments, found {}",
                definition.generic_parameters.len(),
                arguments.len()
            ));
        }
        let substitutions = definition
            .generic_parameters
            .iter()
            .zip(arguments)
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect::<HashMap<_, _>>();
        let fields = definition.fields.clone();
        let mut layouts = Vec::with_capacity(fields.len());
        for field in &fields {
            let field_type = field.type_annotation.substitute(&substitutions);
            let layout = self.resolve(&field_type).map_err(|error| {
                format!("cannot lay out field `{}.{}`: {error}", name, field.name)
            })?;
            layouts.push((field.name.clone(), layout));
        }
        DynamicLayout::record(ty.clone(), layouts)
    }
}
