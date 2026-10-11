//! Preserve declaration identity when moving an already native instance.

use std::{collections::HashMap, rc::Rc};

use rils_value::DynamicLayout;

use super::{NativeRecordCodec, Type};

impl NativeRecordCodec {
    pub(crate) fn resolve_layout(
        &self,
        ty: &Type,
        hosts: &[Rc<super::super::HostType>],
    ) -> Result<Rc<DynamicLayout>, String> {
        let structs = self.structs.values().cloned().collect::<Vec<_>>();
        let enums = self.enums.values().cloned().collect::<Vec<_>>();
        crate::runtime_builtins::resolve_layout(ty, &structs, &enums, hosts)
    }

    pub(crate) fn retain_layout_declarations(
        &mut self,
        source: &Self,
        layout: &DynamicLayout,
    ) -> Result<(), String> {
        // Zero-sized arrays and phantom arguments have types without physical fields.
        self.retain_type_declarations(source, layout.rils_type())?;
        if let Some(fields) = layout.record_fields() {
            for field in fields {
                self.retain_layout_declarations(source, field.layout())?;
            }
        }
        if let Some(item) = layout.option_item().or_else(|| layout.sequence_item()) {
            self.retain_layout_declarations(source, item)?;
        }
        if let Some(variants) = layout.variant_alternatives() {
            for variant in variants {
                self.retain_layout_declarations(source, variant)?;
            }
        }
        Ok(())
    }
    fn retain_type_declarations(&mut self, source: &Self, ty: &Type) -> Result<(), String> {
        let mut pending = vec![ty.clone()];
        let mut visited = std::collections::HashSet::new();
        while let Some(ty) = pending.pop() {
            match ty {
                Type::Named { name, arguments } => {
                    pending.extend(arguments);
                    if !visited.insert(name.clone()) {
                        continue;
                    }
                    if (source.structs.contains_key(&name) && self.enums.contains_key(&name))
                        || (source.enums.contains_key(&name) && self.structs.contains_key(&name))
                    {
                        return Err(format!("conflicting native declaration for `{name}`"));
                    }
                    retain(&mut self.structs, &source.structs, &name, Rc::ptr_eq)?;
                    retain(&mut self.enums, &source.enums, &name, |left, right| {
                        Rc::ptr_eq(left, right)
                            || matches!((&left.host_definition, &right.host_definition), (Some(left), Some(right)) if left == right)
                    })?;
                    if let Some(definition) = source.structs.get(&name) {
                        pending.extend(
                            definition
                                .fields
                                .iter()
                                .map(|field| field.type_annotation.clone()),
                        );
                    }
                    if let Some(definition) = source.enums.get(&name) {
                        for variant in &definition.variants {
                            match variant {
                                crate::ast::EnumVariant::Unit { .. } => {}
                                crate::ast::EnumVariant::Tuple { fields, .. } => {
                                    pending.extend(fields.iter().cloned())
                                }
                                crate::ast::EnumVariant::Record { fields, .. } => pending.extend(
                                    fields.iter().map(|field| field.type_annotation.clone()),
                                ),
                            }
                        }
                    }
                }
                Type::Tuple(fields) => pending.extend(fields),
                Type::Array { element, .. }
                | Type::ArrayParameter { element, .. }
                | Type::Slice(element)
                | Type::Option(element)
                | Type::Reference { inner: element, .. } => pending.push(*element),
                Type::Result(ok, error) => {
                    pending.push(*ok);
                    pending.push(*error);
                }
                Type::Function {
                    parameters,
                    return_type,
                } => {
                    pending.extend(parameters.into_iter().flatten());
                    pending.push(*return_type);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn retain<T>(
    target: &mut HashMap<String, Rc<T>>,
    source: &HashMap<String, Rc<T>>,
    name: &str,
    same_declaration: impl FnOnce(&Rc<T>, &Rc<T>) -> bool,
) -> Result<(), String> {
    if let Some(declaration) = source.get(name) {
        if let Some(existing) = target.get(name) {
            return if same_declaration(existing, declaration) {
                Ok(())
            } else {
                Err(format!("conflicting native declaration for `{name}`"))
            };
        }
        target.insert(name.to_owned(), declaration.clone());
    }
    Ok(())
}
