//! Numeric inference bindings and recursive type resolution.

use super::*;

impl Inferencer<'_> {
    pub(super) fn numeric_root(&mut self, variable: ExprId) -> ExprId {
        let parent = *self.numeric_parents.entry(variable).or_insert(variable);
        if parent == variable {
            variable
        } else {
            let root = self.numeric_root(parent);
            self.numeric_parents.insert(variable, root);
            root
        }
    }

    pub(super) fn unify(&mut self, left: &Type, right: &Type) {
        match (left, right) {
            (Type::IntegerInference(left), Type::IntegerInference(right))
            | (Type::FloatInference(left), Type::FloatInference(right)) => {
                let left = self.numeric_root(*left);
                let right = self.numeric_root(*right);
                if left != right {
                    let fixed = self
                        .numeric_fixed
                        .remove(&left)
                        .or_else(|| self.numeric_fixed.remove(&right));
                    self.numeric_parents.insert(right, left);
                    if let Some(fixed) = fixed {
                        self.numeric_fixed.insert(left, fixed);
                    }
                }
            }
            (Type::IntegerInference(variable), fixed @ Type::Integer(_))
            | (fixed @ Type::Integer(_), Type::IntegerInference(variable))
            | (Type::FloatInference(variable), fixed @ Type::Float(_))
            | (fixed @ Type::Float(_), Type::FloatInference(variable)) => {
                let root = self.numeric_root(*variable);
                self.numeric_fixed
                    .entry(root)
                    .or_insert_with(|| fixed.clone());
            }
            (Type::Option(left), Type::Option(right)) => self.unify(left, right),
            (Type::Result(left_ok, left_error), Type::Result(right_ok, right_error)) => {
                self.unify(left_ok, right_ok);
                self.unify(left_error, right_error);
            }
            (Type::Tuple(left), Type::Tuple(right)) if left.len() == right.len() => {
                for (left, right) in left.iter().zip(right) {
                    self.unify(left, right);
                }
            }
            (Type::Array { element: left, .. }, Type::Array { element: right, .. }) => {
                self.unify(left, right)
            }
            (Type::Reference { inner: left, .. }, Type::Reference { inner: right, .. }) => {
                self.unify(left, right)
            }
            (
                Type::Named {
                    name: left_name,
                    arguments: left,
                },
                Type::Named {
                    name: right_name,
                    arguments: right,
                },
            ) if left_name == right_name && left.len() == right.len() => {
                for (left, right) in left.iter().zip(right) {
                    self.unify(left, right);
                }
            }
            (
                Type::Function {
                    parameters: left,
                    return_type: left_return,
                },
                Type::Function {
                    parameters: right,
                    return_type: right_return,
                },
            ) => {
                if let (Some(left), Some(right)) = (left, right)
                    && left.len() == right.len()
                {
                    for (left, right) in left.iter().zip(right) {
                        self.unify(left, right);
                    }
                }
                self.unify(left_return, right_return);
            }
            _ => {}
        }
    }

    pub(super) fn resolve_type(&mut self, ty: &Type) -> Type {
        match ty {
            Type::IntegerInference(variable) => {
                let root = self.numeric_root(*variable);
                self.numeric_fixed.get(&root).cloned().unwrap_or(Type::I32)
            }
            Type::FloatInference(variable) => {
                let root = self.numeric_root(*variable);
                self.numeric_fixed.get(&root).cloned().unwrap_or(Type::F64)
            }
            Type::Option(inner) => Type::Option(Box::new(self.resolve_type(inner))),
            Type::Result(ok, error) => Type::Result(
                Box::new(self.resolve_type(ok)),
                Box::new(self.resolve_type(error)),
            ),
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.resolve_type(element))
                    .collect(),
            ),
            Type::ArrayParameter { element, length } => Type::ArrayParameter {
                element: Box::new(self.resolve_type(element)),
                length: length.clone(),
            },
            Type::Array { element, length } => Type::Array {
                element: Box::new(self.resolve_type(element)),
                length: *length,
            },
            Type::Reference { mutable, inner } => Type::Reference {
                mutable: *mutable,
                inner: Box::new(self.resolve_type(inner)),
            },
            Type::Function {
                parameters,
                return_type,
            } => Type::Function {
                parameters: parameters.as_ref().map(|parameters| {
                    parameters
                        .iter()
                        .map(|parameter| self.resolve_type(parameter))
                        .collect()
                }),
                return_type: Box::new(self.resolve_type(return_type)),
            },
            Type::Named { name, arguments } => Type::Named {
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|argument| self.resolve_type(argument))
                    .collect(),
            },
            Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => Type::Associated {
                base: Box::new(self.resolve_type(base)),
                trait_name: trait_name.clone(),
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|argument| self.resolve_type(argument))
                    .collect(),
            },
            other => other.clone(),
        }
    }
}
