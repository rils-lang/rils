use std::{collections::HashMap, fmt};

use crate::source::{ExprId, Span};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum IntegerType {
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
}

impl IntegerType {
    pub const ALL: [Self; 12] = [
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::I128,
        Self::Isize,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::U128,
        Self::Usize,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::Isize => "isize",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::Usize => "usize",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub const fn is_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128 | Self::Isize
        )
    }

    pub const fn bits(self) -> u32 {
        match self {
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 => 32,
            Self::I64 | Self::U64 => 64,
            Self::I128 | Self::U128 => 128,
            Self::Isize | Self::Usize => usize::BITS,
        }
    }

    pub const fn can_cast_losslessly_to(self, target: Self) -> bool {
        match (self.is_signed(), target.is_signed()) {
            (true, true) | (false, false) | (true, false) => target.bits() >= self.bits(),
            (false, true) => target.bits() > self.bits(),
        }
    }

    pub const fn can_represent_all(self, source: Self) -> bool {
        source.can_cast_losslessly_to(self)
    }
}

impl fmt::Display for IntegerType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum FloatType {
    F32,
    F64,
}

impl FloatType {
    pub const fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            _ => None,
        }
    }
}

impl fmt::Display for FloatType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSignature {
    pub parameters: Option<Vec<Type>>,
    pub return_type: Type,
}

impl FunctionSignature {
    pub fn fixed(parameters: Vec<Type>, return_type: Type) -> Self {
        Self {
            parameters: Some(parameters),
            return_type,
        }
    }

    pub fn variadic(return_type: Type) -> Self {
        Self {
            parameters: None,
            return_type,
        }
    }

    pub fn as_type(&self) -> Type {
        Type::Function {
            parameters: self.parameters.clone(),
            return_type: Box::new(self.return_type.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Unit,
    Bool,
    Integer(IntegerType),
    Float(FloatType),
    IntegerVariable(Span),
    FloatVariable(Span),
    IntegerInference(ExprId),
    FloatInference(ExprId),
    Char,
    String,
    Tuple(Vec<Type>),
    Array {
        element: Box<Type>,
        length: usize,
    },
    Reference {
        mutable: bool,
        inner: Box<Type>,
    },
    Function {
        parameters: Option<Vec<Type>>,
        return_type: Box<Type>,
    },
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Named {
        name: String,
        arguments: Vec<Type>,
    },
    Associated {
        base: Box<Type>,
        trait_name: Option<String>,
        name: String,
        arguments: Vec<Type>,
    },
    Variable(String),
    Unknown,
}

pub trait RuntimeValue: Sized {
    fn is_accepted_by(&self, expected: &Type) -> bool;
    fn constrain_to(&self, expected: &Type) -> Option<Self>;
    fn runtime_type(&self) -> Option<Type>;
}

impl Type {
    pub const I32: Self = Self::Integer(IntegerType::I32);
    pub const F64: Self = Self::Float(FloatType::F64);
    pub const USIZE: Self = Self::Integer(IntegerType::Usize);

    pub const fn is_integer(&self) -> bool {
        matches!(
            self,
            Self::Integer(_) | Self::IntegerVariable(_) | Self::IntegerInference(_)
        )
    }

    pub const fn is_float(&self) -> bool {
        matches!(
            self,
            Self::Float(_) | Self::FloatVariable(_) | Self::FloatInference(_)
        )
    }

    pub const fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::Integer(_)
                | Self::Float(_)
                | Self::IntegerVariable(_)
                | Self::FloatVariable(_)
                | Self::IntegerInference(_)
                | Self::FloatInference(_)
        )
    }

    pub fn contains_reference(&self) -> bool {
        match self {
            Self::Reference { .. } => true,
            Self::Option(inner) => inner.contains_reference(),
            Self::Result(ok, error) => ok.contains_reference() || error.contains_reference(),
            Self::Tuple(elements) => elements.iter().any(Self::contains_reference),
            Self::Array { element, .. } => element.contains_reference(),
            Self::Function {
                parameters,
                return_type,
            } => {
                parameters
                    .as_ref()
                    .is_some_and(|parameters| parameters.iter().any(Self::contains_reference))
                    || return_type.contains_reference()
            }
            Self::Named { arguments, .. } => arguments.iter().any(Self::contains_reference),
            Self::Associated {
                base, arguments, ..
            } => base.contains_reference() || arguments.iter().any(Self::contains_reference),
            _ => false,
        }
    }

    pub fn function(parameters: Vec<Type>, return_type: Type) -> Self {
        Self::Function {
            parameters: Some(parameters),
            return_type: Box::new(return_type),
        }
    }

    pub fn opaque_function() -> Self {
        Self::Function {
            parameters: None,
            return_type: Box::new(Self::Unknown),
        }
    }

    pub fn named(name: impl Into<String>) -> Self {
        Self::Named {
            name: name.into(),
            arguments: Vec::new(),
        }
    }

    pub fn accepts<V: RuntimeValue>(&self, value: &V) -> bool {
        value.is_accepted_by(self)
    }

    pub fn constrain<V: RuntimeValue>(&self, value: &V) -> Option<V> {
        value.constrain_to(self)
    }

    pub fn of_value<V: RuntimeValue>(value: &V) -> Option<Self> {
        value.runtime_type()
    }

    pub fn substitute(&self, substitutions: &HashMap<String, Type>) -> Self {
        match self {
            Self::Variable(name) => substitutions.get(name).cloned().unwrap_or(Self::Unknown),
            Self::Option(inner) => Self::Option(Box::new(inner.substitute(substitutions))),
            Self::Result(ok, error) => Self::Result(
                Box::new(ok.substitute(substitutions)),
                Box::new(error.substitute(substitutions)),
            ),
            Self::Tuple(elements) => Self::Tuple(
                elements
                    .iter()
                    .map(|ty| ty.substitute(substitutions))
                    .collect(),
            ),
            Self::Array { element, length } => Self::Array {
                element: Box::new(element.substitute(substitutions)),
                length: *length,
            },
            Self::Reference { mutable, inner } => Self::Reference {
                mutable: *mutable,
                inner: Box::new(inner.substitute(substitutions)),
            },
            Self::Function {
                parameters,
                return_type,
            } => Self::Function {
                parameters: parameters.as_ref().map(|parameters| {
                    parameters
                        .iter()
                        .map(|parameter| parameter.substitute(substitutions))
                        .collect()
                }),
                return_type: Box::new(return_type.substitute(substitutions)),
            },
            Self::Named { name, arguments } => Self::Named {
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|argument| argument.substitute(substitutions))
                    .collect(),
            },
            Self::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => Self::Associated {
                base: Box::new(base.substitute(substitutions)),
                trait_name: trait_name.clone(),
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|argument| argument.substitute(substitutions))
                    .collect(),
            },
            other => other.clone(),
        }
    }
}

#[doc(hidden)]
pub fn merge_type_arguments(expected: &[Type], actual: &[Type]) -> Option<Vec<Type>> {
    if expected.len() != actual.len() {
        return None;
    }
    expected
        .iter()
        .zip(actual)
        .map(|(expected, actual)| merge_types(expected, actual))
        .collect()
}

pub fn merge_types(expected: &Type, actual: &Type) -> Option<Type> {
    match (expected, actual) {
        (Type::Unknown | Type::Variable(_), actual) => Some(actual.clone()),
        (expected, Type::Unknown | Type::Variable(_)) => Some(expected.clone()),
        (Type::Option(expected), Type::Option(actual)) => {
            Some(Type::Option(Box::new(merge_types(expected, actual)?)))
        }
        (Type::Result(expected_ok, expected_error), Type::Result(actual_ok, actual_error)) => {
            Some(Type::Result(
                Box::new(merge_types(expected_ok, actual_ok)?),
                Box::new(merge_types(expected_error, actual_error)?),
            ))
        }
        (Type::Tuple(expected), Type::Tuple(actual)) if expected.len() == actual.len() => {
            Some(Type::Tuple(
                expected
                    .iter()
                    .zip(actual)
                    .map(|(expected, actual)| merge_types(expected, actual))
                    .collect::<Option<Vec<_>>>()?,
            ))
        }
        (
            Type::Array {
                element: expected,
                length: expected_length,
            },
            Type::Array {
                element: actual,
                length: actual_length,
            },
        ) if expected_length == actual_length => Some(Type::Array {
            element: Box::new(merge_types(expected, actual)?),
            length: *expected_length,
        }),
        (
            Type::Reference {
                mutable: expected_mutable,
                inner: expected_inner,
            },
            Type::Reference {
                mutable: actual_mutable,
                inner: actual_inner,
            },
        ) if !*expected_mutable || *actual_mutable => Some(Type::Reference {
            mutable: *expected_mutable,
            inner: Box::new(merge_types(expected_inner, actual_inner)?),
        }),
        (
            Type::Function {
                parameters: expected_parameters,
                return_type: expected_return,
            },
            Type::Function {
                parameters: actual_parameters,
                return_type: actual_return,
            },
        ) => {
            let parameters = match (expected_parameters, actual_parameters) {
                (Some(expected), Some(actual)) if expected.len() == actual.len() => Some(
                    expected
                        .iter()
                        .zip(actual)
                        .map(|(expected, actual)| merge_types(expected, actual))
                        .collect::<Option<Vec<_>>>()?,
                ),
                (Some(parameters), None) | (None, Some(parameters)) => Some(parameters.clone()),
                (None, None) => None,
                _ => return None,
            };
            Some(Type::Function {
                parameters,
                return_type: Box::new(merge_types(expected_return, actual_return)?),
            })
        }
        (
            Type::Named {
                name: expected_name,
                arguments: expected_arguments,
            },
            Type::Named {
                name: actual_name,
                arguments: actual_arguments,
            },
        ) if expected_name == actual_name => Some(Type::Named {
            name: expected_name.clone(),
            arguments: merge_type_arguments(expected_arguments, actual_arguments)?,
        }),
        (expected, actual) if expected == actual => Some(expected.clone()),
        _ => None,
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unit => write!(f, "()"),
            Self::Bool => write!(f, "bool"),
            Self::Integer(ty) => write!(f, "{ty}"),
            Self::Float(ty) => write!(f, "{ty}"),
            Self::IntegerVariable(_) => write!(f, "{{integer}}"),
            Self::FloatVariable(_) => write!(f, "{{f64}}"),
            Self::IntegerInference(_) => write!(f, "{{integer}}"),
            Self::FloatInference(_) => write!(f, "{{f64}}"),
            Self::Char => write!(f, "char"),
            Self::String => write!(f, "string"),
            Self::Tuple(elements) => {
                write!(f, "(")?;
                for (index, element) in elements.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{element}")?;
                }
                if elements.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            Self::Array { element, length } => write!(f, "[{element}; {length}]"),
            Self::Reference { mutable, inner } => {
                if *mutable {
                    write!(f, "&mut {inner}")
                } else {
                    write!(f, "&{inner}")
                }
            }
            Self::Function {
                parameters: None, ..
            } => write!(f, "function"),
            Self::Function {
                parameters: Some(parameters),
                return_type,
            } => {
                write!(f, "fn(")?;
                for (index, parameter) in parameters.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{parameter}")?;
                }
                write!(f, ") -> {return_type}")
            }
            Self::Option(inner) => write!(f, "Option<{inner}>"),
            Self::Result(ok, error) => write!(f, "Result<{ok}, {error}>"),
            Self::Named { name, arguments } => {
                write!(f, "{name}")?;
                if !arguments.is_empty() {
                    write!(f, "<")?;
                    for (index, argument) in arguments.iter().enumerate() {
                        if index > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{argument}")?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Self::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => {
                if let Some(trait_name) = trait_name {
                    write!(f, "<{base} as {trait_name}>::{name}")?;
                } else {
                    write!(f, "{base}::{name}")?;
                }
                if !arguments.is_empty() {
                    write!(f, "<")?;
                    for (index, argument) in arguments.iter().enumerate() {
                        if index > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{argument}")?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Self::Variable(name) => write!(f, "{name}"),
            Self::Unknown => write!(f, "_"),
        }
    }
}
