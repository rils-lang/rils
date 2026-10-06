//! Copy is a declared capability; field eligibility only validates that declaration.

use std::collections::{HashMap, HashSet};

use crate::Type;

mod declarations;
pub(crate) use declarations::{collect, validate};

#[derive(Clone, Debug)]
struct Definition {
    parameters: Vec<String>,
    fields: Vec<Type>,
    declared: bool,
    clone: bool,
}

/// AST-independent Copy declarations shared by analysis and bytecode validation.
#[derive(Clone, Debug, Default)]
pub struct CopyTypes {
    definitions: HashMap<String, Definition>,
}

impl CopyTypes {
    pub fn define(&mut self, name: String, parameters: Vec<String>, fields: Vec<Type>) {
        self.definitions.insert(
            name,
            Definition {
                parameters,
                fields,
                declared: false,
                clone: false,
            },
        );
    }

    /// Returns false for an unknown target, rather than granting it Copy.
    pub fn implement(&mut self, name: &str) -> bool {
        let Some(definition) = self.definitions.get_mut(name) else {
            return false;
        };
        definition.declared = true;
        true
    }

    pub fn is_declared(&self, name: &str) -> bool {
        self.definitions
            .get(name)
            .is_some_and(|definition| definition.declared)
    }

    pub(crate) fn implement_clone(&mut self, name: &str) {
        if let Some(definition) = self.definitions.get_mut(name) {
            definition.clone = true;
        }
    }

    pub fn has_clone(&self, name: &str) -> bool {
        self.definitions
            .get(name)
            .is_some_and(|definition| definition.clone)
    }

    pub(crate) fn is_unconditional_target(&self, ty: &Type) -> bool {
        let Type::Named { name, arguments } = ty else {
            return false;
        };
        let Some(definition) = self.definitions.get(name) else {
            return false;
        };
        let mut variables = HashSet::new();
        definition.parameters.len() == arguments.len()
            && arguments.iter().all(|argument| match argument {
                Type::Variable(name) | Type::BoundVariable { name, .. } => variables.insert(name),
                _ => false,
            })
    }

    pub fn is_copy(&self, ty: &Type, host_types: &HashSet<String>) -> bool {
        self.check(ty, host_types, &mut HashSet::new(), false)
    }

    /// Validate every field (including inactive enum variants) before granting Copy.
    pub fn fields_are_copy(&self, ty: &Type, host_types: &HashSet<String>) -> bool {
        self.check(ty, host_types, &mut HashSet::new(), true)
    }

    fn check(
        &self,
        ty: &Type,
        hosts: &HashSet<String>,
        visiting: &mut HashSet<String>,
        root: bool,
    ) -> bool {
        match ty {
            Type::Unit
            | Type::Bool
            | Type::Char
            | Type::Reference { .. }
            | Type::Function { .. }
            | Type::IntegerVariable(_)
            | Type::IntegerInference(_)
            | Type::FloatVariable(_)
            | Type::FloatInference(_) => true,
            Type::Integer(integer) => rils_builtins::native_implements(integer.name(), "Copy"),
            Type::Float(float) => rils_builtins::native_implements(float.name(), "Copy"),
            Type::String => rils_builtins::native_implements("string", "Copy"),
            Type::Option(item) => {
                rils_builtins::native_implements_with("Option", "Copy", |parameter, bound| {
                    parameter == "T" && bound == "Copy" && self.check(item, hosts, visiting, false)
                })
            }
            Type::Result(ok, error) => {
                rils_builtins::native_implements_with("Result", "Copy", |parameter, bound| {
                    bound == "Copy"
                        && match parameter {
                            "T" => self.check(ok, hosts, visiting, false),
                            "E" => self.check(error, hosts, visiting, false),
                            _ => false,
                        }
                })
            }
            Type::Tuple(items) => items
                .iter()
                .all(|item| self.check(item, hosts, visiting, false)),
            Type::Array { element, .. } | Type::ArrayParameter { element, .. } => {
                self.check(element, hosts, visiting, false)
            }
            Type::Named { name, arguments } => {
                // Portable host declarations explicitly provide identity-token Copy semantics.
                if !self.definitions.contains_key(name)
                    && arguments.is_empty()
                    && (name == "HostHandle" || hosts.contains(name))
                {
                    return true;
                }
                if !self.definitions.contains_key(name)
                    && let Some(declaration) = rils_builtins::builtin(name)
                    && declaration.type_parameters.len() == arguments.len()
                {
                    let type_name = declaration
                        .path
                        .rsplit("::")
                        .next()
                        .unwrap_or(declaration.path);
                    return rils_builtins::native_implements_with(
                        type_name,
                        "Copy",
                        |parameter, bound| {
                            bound == "Copy"
                                && declaration
                                    .type_parameters
                                    .iter()
                                    .position(|name| *name == parameter)
                                    .is_some_and(|index| {
                                        self.check(&arguments[index], hosts, visiting, false)
                                    })
                        },
                    );
                }
                let Some(definition) = self.definitions.get(name) else {
                    return false;
                };
                if (!root && !definition.declared)
                    || definition.parameters.len() != arguments.len()
                    || !visiting.insert(name.clone())
                {
                    return false;
                }
                let substitutions = definition
                    .parameters
                    .iter()
                    .cloned()
                    .zip(arguments.iter().cloned())
                    .collect();
                let result = definition.fields.iter().all(|field| {
                    self.check(&field.substitute(&substitutions), hosts, visiting, false)
                });
                visiting.remove(name);
                result
            }
            _ => false,
        }
    }
}
