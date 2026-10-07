use super::Type;

impl Type {
    /// A checked type expression may retain declared generic parameters, but
    /// must not retain inference placeholders.
    pub fn is_type_witness(&self) -> bool {
        self.check_witness(true)
    }

    /// A runtime instance type has substituted every declared parameter.
    pub fn is_concrete_type(&self) -> bool {
        self.check_witness(false)
    }

    fn check_witness(&self, parameters: bool) -> bool {
        match self {
            Self::Unknown
            | Self::IntegerVariable(_)
            | Self::FloatVariable(_)
            | Self::IntegerInference(_)
            | Self::FloatInference(_) => false,
            Self::Variable(_) | Self::BoundVariable { .. } => parameters,
            Self::ArrayParameter { element, .. } => parameters && element.check_witness(parameters),
            Self::Associated {
                base, arguments, ..
            } => {
                parameters
                    && base.check_witness(parameters)
                    && arguments.iter().all(|ty| ty.check_witness(parameters))
            }
            Self::Named { arguments, .. } | Self::Tuple(arguments) => {
                arguments.iter().all(|ty| ty.check_witness(parameters))
            }
            Self::Array { element, .. }
            | Self::Slice(element)
            | Self::Reference { inner: element, .. }
            | Self::Option(element) => element.check_witness(parameters),
            Self::Result(ok, error) => {
                ok.check_witness(parameters) && error.check_witness(parameters)
            }
            Self::Function {
                parameters: arguments,
                return_type,
            } => {
                arguments
                    .as_ref()
                    .is_none_or(|arguments| arguments.iter().all(|ty| ty.check_witness(parameters)))
                    && return_type.check_witness(parameters)
            }
            Self::Unit
            | Self::Bool
            | Self::Integer(_)
            | Self::Float(_)
            | Self::Char
            | Self::String
            | Self::ConstUsize(_) => true,
        }
    }
}
