//! Concrete native layouts derived from the existing user struct declarations.

use std::{collections::HashMap, rc::Rc};

use rils_value::DynamicLayout;

use crate::{Type, ast::EnumVariant};

use super::{EnumType, StructType, native_layouts};

/// A declaration-owned layout factory for types that are not user records or
/// enums. The resolver supplies the same recursive type cache to every child.
pub trait NativeLayoutProvider {
    fn layout(
        &self,
        ty: &Type,
        resolve_child: &mut dyn FnMut(&Type) -> Result<Rc<DynamicLayout>, String>,
    ) -> Option<Result<Rc<DynamicLayout>, String>>;
}

/// Resolves a concrete Rils type once per instance, reusing child layout
/// descriptors so nested values can check their exact layout identity.
pub struct RecordLayoutResolver<'a> {
    definitions: &'a [Rc<StructType>],
    enums: &'a [Rc<EnumType>],
    provider: Option<&'a dyn NativeLayoutProvider>,
    cache: Vec<(Type, Rc<DynamicLayout>)>,
    resolving: Vec<Type>,
}

impl<'a> RecordLayoutResolver<'a> {
    pub fn new(definitions: &'a [Rc<StructType>]) -> Self {
        Self::with_enums(definitions, &[])
    }

    pub fn with_enums(definitions: &'a [Rc<StructType>], enums: &'a [Rc<EnumType>]) -> Self {
        Self::with_provider(definitions, enums, None)
    }

    pub fn with_provider(
        definitions: &'a [Rc<StructType>],
        enums: &'a [Rc<EnumType>],
        provider: Option<&'a dyn NativeLayoutProvider>,
    ) -> Self {
        Self {
            definitions,
            enums,
            provider,
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
        if layout.rils_type() != ty {
            return Err(format!(
                "native layout provider returned {} for requested {ty}",
                layout.rils_type()
            ));
        }
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
        if let Some(result) =
            rils_stdlib::native::registry().layout(ty, &mut |child| self.resolve(child))
        {
            return result;
        }
        if let Some(provider) = self.provider
            && let Some(result) = provider.layout(ty, &mut |child| self.resolve(child))
        {
            return result;
        }
        match ty {
            Type::Unit => Ok(DynamicLayout::copy_of::<()>(Type::Unit)),
            Type::Bool => Ok(DynamicLayout::copy_of::<bool>(Type::Bool)),
            Type::Char => Ok(DynamicLayout::copy_of::<char>(Type::Char)),
            Type::Option(item) => DynamicLayout::option(self.resolve(item)?),
            Type::Result(ok, error) => {
                DynamicLayout::variant(ty.clone(), vec![self.resolve(ok)?, self.resolve(error)?])
            }
            Type::Tuple(elements) => {
                let fields = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| Ok((index.to_string(), self.resolve(element)?)))
                    .collect::<Result<Vec<_>, String>>()?;
                DynamicLayout::aggregate(ty.clone(), fields)
            }
            Type::Array { element, length } => {
                let item = self.resolve(element)?;
                let fields = (0..*length)
                    .map(|index| (index.to_string(), item.clone()))
                    .collect();
                DynamicLayout::aggregate(ty.clone(), fields)
            }
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
        let Some(definition) = definitions.next() else {
            return self.resolve_enum(name, arguments, ty);
        };
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

    fn resolve_enum(
        &mut self,
        name: &str,
        arguments: &[Type],
        ty: &Type,
    ) -> Result<Rc<DynamicLayout>, String> {
        let mut matches = self
            .enums
            .iter()
            .filter(|definition| definition.name == name);
        let definition = matches
            .next()
            .ok_or_else(|| format!("no user struct or enum declaration for {ty}"))?;
        if matches.next().is_some() {
            return Err(format!("ambiguous user enum declaration for {ty}"));
        }
        if definition.generic_parameters.len() != arguments.len() {
            return Err(format!(
                "enum `{name}` requires {} type arguments, found {}",
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
        let variants = definition.variants.clone();
        let mut alternatives = Vec::with_capacity(variants.len());
        for variant in variants {
            let payload = match variant {
                EnumVariant::Unit { .. } => DynamicLayout::copy_of::<()>(Type::Unit),
                EnumVariant::Tuple { fields, .. } => {
                    let types = fields
                        .iter()
                        .map(|field| field.substitute(&substitutions))
                        .collect::<Vec<_>>();
                    let fields = types
                        .iter()
                        .enumerate()
                        .map(|(index, field)| Ok((index.to_string(), self.resolve(field)?)))
                        .collect::<Result<Vec<_>, String>>()?;
                    DynamicLayout::aggregate(Type::Tuple(types), fields)?
                }
                EnumVariant::Record {
                    name: variant_name,
                    fields,
                    ..
                } => {
                    let fields = fields
                        .iter()
                        .map(|field| {
                            Ok((
                                field.name.clone(),
                                self.resolve(&field.type_annotation.substitute(&substitutions))?,
                            ))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    let payload_type = Type::Named {
                        name: format!("{name}::{variant_name}"),
                        arguments: arguments.to_vec(),
                    };
                    DynamicLayout::record(payload_type, fields)?
                }
            };
            alternatives.push(payload);
        }
        DynamicLayout::variant(ty.clone(), alternatives)
    }
}
