//! Structural formatting contracts shared by static and dynamic checking.
use crate::types::Type;

pub fn implements(ty: &Type, required: &str, nominal: &impl Fn(&Type, &str) -> bool) -> bool {
    match ty {
        Type::Reference { inner, .. } => implements(inner, required, nominal),
        Type::Unit | Type::Bool | Type::Integer(_) | Type::IntegerVariable(_) | Type::IntegerInference(_) | Type::Float(_) | Type::FloatVariable(_) | Type::FloatInference(_) | Type::Char | Type::String => matches!(required, "Display" | "Debug"),
        Type::Tuple(elements) => required == "Debug" && elements.iter().all(|ty| implements(ty, required, nominal)),
        Type::Array { element, .. } | Type::ArrayParameter { element, .. } | Type::Slice(element) | Type::Option(element) => required == "Debug" && implements(element, required, nominal),
        Type::Result(ok, error) => required == "Debug" && implements(ok, required, nominal) && implements(error, required, nominal),
        Type::Named { name, arguments } if matches!(name.as_str(), "Vec" | "HashMap" | "HashSet" | "Range") => required == "Debug" && arguments.iter().all(|ty| implements(ty, required, nominal)),
        Type::BoundVariable { bounds, .. } => bounds.iter().any(|bound| matches!(bound, Type::Named { name, .. } if name.rsplit("::").next() == Some(required))),
        Type::Function { .. } | Type::ConstUsize(_) => false,
        ty => nominal(ty, required),
    }
}
