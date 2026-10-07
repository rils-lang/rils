//! Preserve declaration identity when moving an already native instance.

use std::{collections::HashMap, rc::Rc};

use rils_value::DynamicLayout;

use super::{NativeRecordCodec, Type};

impl NativeRecordCodec {
    pub(super) fn retain_layout_declarations(
        &mut self,
        source: &Self,
        layout: &DynamicLayout,
    ) -> Result<(), String> {
        if let Type::Named { name, .. } = layout.rils_type() {
            if (source.structs.contains_key(name) && self.enums.contains_key(name))
                || (source.enums.contains_key(name) && self.structs.contains_key(name))
            {
                return Err(format!("conflicting native declaration for `{name}`"));
            }
            retain(&mut self.structs, &source.structs, name, Rc::ptr_eq)?;
            retain(&mut self.enums, &source.enums, name, |left, right| {
                Rc::ptr_eq(left, right)
                    || matches!(
                        (&left.host_definition, &right.host_definition),
                        (Some(left), Some(right)) if left == right
                    )
            })?;
        }
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
