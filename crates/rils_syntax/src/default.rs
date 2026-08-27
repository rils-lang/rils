use crate::types::{FloatType, IntegerType, Type};

#[derive(Clone, Debug, PartialEq)]
pub enum DefaultPlan {
    Unit,
    Bool,
    Integer(IntegerType),
    Float(FloatType),
    Char,
    String,
    Tuple(Vec<DefaultPlan>),
    Array {
        element: Box<DefaultPlan>,
        element_type: Type,
        length: usize,
    },
    Option(Type),
    EmptyCollection {
        name: String,
        arguments: Vec<Type>,
    },
    TraitCall(Type),
}

pub fn default_plan(ty: &Type) -> Option<DefaultPlan> {
    Some(match ty {
        Type::Unit => DefaultPlan::Unit,
        Type::Bool => DefaultPlan::Bool,
        Type::Integer(integer) => DefaultPlan::Integer(*integer),
        Type::Float(float) => DefaultPlan::Float(*float),
        Type::Char => DefaultPlan::Char,
        Type::String => DefaultPlan::String,
        Type::Tuple(elements) => DefaultPlan::Tuple(
            elements
                .iter()
                .map(default_plan)
                .collect::<Option<Vec<_>>>()?,
        ),
        Type::Array { element, length } => DefaultPlan::Array {
            element: Box::new(default_plan(element)?),
            element_type: element.as_ref().clone(),
            length: *length,
        },
        Type::Option(inner) => DefaultPlan::Option(inner.as_ref().clone()),
        Type::Named { name, arguments }
            if (name == "Vec" && arguments.len() == 1)
                || (name == "HashMap" && arguments.len() == 2)
                || (name == "HashSet" && arguments.len() == 1) =>
        {
            DefaultPlan::EmptyCollection {
                name: name.clone(),
                arguments: arguments.clone(),
            }
        }
        Type::Named { .. } | Type::Variable(_) => DefaultPlan::TraitCall(ty.clone()),
        Type::Reference { .. }
        | Type::Function { .. }
        | Type::Result(_, _)
        | Type::Associated { .. }
        | Type::IntegerVariable(_)
        | Type::FloatVariable(_)
        | Type::Unknown => return None,
    })
}

#[cfg(test)]
#[path = "../tests/unit/default.rs"]
mod tests;
