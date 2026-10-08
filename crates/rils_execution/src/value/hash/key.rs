//! Compatibility key ownership and immutable identity.
use super::super::Value;
use crate::Type;
use std::{
    fmt,
    hash::{Hash, Hasher},
    rc::Rc,
};
#[path = "key/view.rs"]
mod view;

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum HashKey {
    Unit,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),
    Char(char),
    String(Rc<rils_stdlib::stdlib::string::String>),
    Composite(Box<StructuralKey>),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum StructuralIdentity {
    Tuple(Vec<HashKey>),
    Array(Vec<HashKey>),
    Option(Option<Box<HashKey>>),
    ResultOk(Box<HashKey>),
    ResultErr(Box<HashKey>),
    Struct {
        name: String,
        arguments: Vec<String>,
        fields: Vec<(String, HashKey)>,
    },
    Enum {
        name: String,
        arguments: Vec<String>,
        variant: String,
        payload: Vec<(String, HashKey)>,
    },
}

#[derive(Clone)]
pub struct StructuralKey {
    identity: StructuralIdentity,
    value: Value,
}

impl fmt::Debug for StructuralKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.identity.fmt(formatter)
    }
}

impl PartialEq for StructuralKey {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for StructuralKey {}

impl PartialOrd for StructuralKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StructuralKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.identity.cmp(&other.identity)
    }
}

impl Hash for StructuralKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identity.hash(state);
    }
}

impl HashKey {
    pub fn from_ordered_value(value: &Value) -> Result<Self, String> {
        let key = Self::from_value(value)?;
        if matches!(&key, Self::Unit | Self::Composite(_)) {
            return Err(format!(
                "{} cannot be used as an ordered collection key",
                value.type_name()
            ));
        }
        Ok(key)
    }

    pub fn from_value(value: &Value) -> Result<Self, String> {
        if let Value::Reference(reference) = value {
            return Self::from_value(&reference.read()?);
        }
        let value = value.clone();
        let value = crate::numeric::lower_migrated_integer(value);
        if let Some(value) = crate::numeric::i8_payload(&value) {
            return Ok(Self::I8(value));
        }
        if let Some(value) = crate::numeric::i32_payload(&value) {
            return Ok(Self::I32(value));
        }
        if let Some(value) = crate::numeric::usize_payload(&value) {
            return Ok(Self::Usize(value));
        }
        if let Some(text) = value.as_string() {
            return Ok(Self::String(Rc::new(text.into())));
        }
        if let Some(character) = super::super::char_payload(&value) {
            return Ok(Self::Char(character));
        }
        Ok(match value {
            Value::Unit => Self::Unit,
            Value::Bool(value) => Self::Bool(value),
            Value::I16(value) => Self::I16(value),
            Value::I64(value) => Self::I64(value),
            Value::I128(value) => Self::I128(value),
            Value::Isize(value) => Self::Isize(value),
            Value::U8(value) => Self::U8(value),
            Value::U16(value) => Self::U16(value),
            Value::U32(value) => Self::U32(value),
            Value::U64(value) => Self::U64(value),
            Value::U128(value) => Self::U128(value),
            Value::Usize(value) => Self::Usize(value),
            Value::Char(value) => Self::Char(value),

            value @ (Value::Tuple(_)
            | Value::Array(_)
            | Value::Option { .. }
            | Value::Result { .. }) => {
                let value = value.clone_owned()?;
                let identity = StructuralIdentity::from_value(&value)?;
                Self::Composite(Box::new(StructuralKey { identity, value }))
            }
            Value::Dynamic(object)
                if matches!(
                    object.descriptor().layout().rils_type(),
                    Type::Option(_) | Type::Result(_, _)
                ) || matches!(
                    super::super::native_instance::definition(&object),
                    Some(Value::StructType(_) | Value::EnumType(_))
                ) =>
            {
                let value = Value::Dynamic(object).clone_owned()?;
                let identity = StructuralIdentity::from_value(&value)?;
                Self::Composite(Box::new(StructuralKey { identity, value }))
            }
            value => {
                return Err(format!(
                    "{} cannot be used as a hash collection key",
                    value.type_name()
                ));
            }
        })
    }

    pub fn to_value(&self) -> Value {
        match self {
            Self::Unit => Value::Unit,
            Self::Bool(value) => Value::Bool(*value),
            Self::I8(value) => crate::numeric::native_i8(*value),
            Self::I16(value) => Value::from_i16(*value),
            Self::I32(value) => crate::numeric::native_i32(*value),
            Self::I64(value) => Value::from_i64(*value),
            Self::I128(value) => Value::from_i128(*value),
            Self::Isize(value) => Value::from_isize(*value),
            Self::U8(value) => Value::from_u8(*value),
            Self::U16(value) => Value::from_u16(*value),
            Self::U32(value) => Value::from_u32(*value),
            Self::U64(value) => Value::from_u64(*value),
            Self::U128(value) => Value::from_u128(*value),
            Self::Usize(value) => crate::numeric::native_usize(*value),
            Self::Char(value) => super::super::native_char(*value),
            Self::String(value) => super::super::native_string(value.as_ref().as_ref().clone()),
            Self::Composite(key) => key
                .value
                .clone_owned()
                .expect("stored key remains complete"),
        }
    }

    /// Consume the key's owned snapshot when moving it into a native field.
    pub fn into_value(self) -> Value {
        match self {
            Self::Unit => Value::Unit,
            Self::Bool(value) => Value::Bool(value),
            Self::I8(value) => crate::numeric::native_i8(value),
            Self::I16(value) => Value::from_i16(value),
            Self::I32(value) => crate::numeric::native_i32(value),
            Self::I64(value) => Value::from_i64(value),
            Self::I128(value) => Value::from_i128(value),
            Self::Isize(value) => Value::from_isize(value),
            Self::U8(value) => Value::from_u8(value),
            Self::U16(value) => Value::from_u16(value),
            Self::U32(value) => Value::from_u32(value),
            Self::U64(value) => Value::from_u64(value),
            Self::U128(value) => Value::from_u128(value),
            Self::Usize(value) => crate::numeric::native_usize(value),
            Self::Char(value) => super::super::native_char(value),
            Self::String(value) => {
                let value = Rc::try_unwrap(value).unwrap_or_else(|value| value.as_ref().clone());
                super::super::native_string(std::string::String::from(value))
            }
            Self::Composite(key) => key.value,
        }
    }

    pub fn ty(&self) -> Type {
        self.with_read(|value| match value {
            super::super::borrowed::Read::View(view) => {
                view.layout().map(|layout| layout.rils_type().clone())
            }
            super::super::borrowed::Read::Leaf(leaf) => Ok(leaf.rils_type().clone()),
            super::super::borrowed::Read::Legacy(value) => {
                Type::of_value(value).ok_or("key has no runtime type".into())
            }
        })
        .expect("stored key remains readable")
        .expect("hash keys always have a runtime type")
    }
}

impl StructuralIdentity {
    fn from_value(value: &Value) -> Result<Self, String> {
        if let Some(branch) = super::super::sum::branch(value)? {
            use super::super::borrowed_sum::Branch;
            return Ok(match branch {
                Branch::None => Self::Option(None),
                Branch::Some => Self::Option(Some(Box::new(HashKey::from_value(
                    &super::super::sum::borrow_payload(value, branch)?,
                )?))),
                Branch::Ok => Self::ResultOk(Box::new(HashKey::from_value(
                    &super::super::sum::borrow_payload(value, branch)?,
                )?)),
                Branch::Err => Self::ResultErr(Box::new(HashKey::from_value(
                    &super::super::sum::borrow_payload(value, branch)?,
                )?)),
            });
        }
        let unsupported = || {
            format!(
                "{} cannot be used as a hash collection key",
                value.type_name()
            )
        };
        Ok(match value {
            Value::Dynamic(_) if super::super::native_instance::enum_variant(value)?.is_some() => {
                let variant =
                    super::super::native_instance::enum_variant(value)?.expect("checked enum");
                let traits = variant.definition.implemented_traits.borrow();
                if !traits.contains("Eq") || !traits.contains("Hash") {
                    return Err(unsupported());
                }
                let Some(Type::Named { name, arguments }) = Type::of_value(value) else {
                    return Err(unsupported());
                };
                let mut payload = variant
                    .field_names()
                    .iter()
                    .map(|field| {
                        Ok((
                            field.clone(),
                            HashKey::from_value(
                                &super::super::native_instance::borrow_variant_field(
                                    value,
                                    variant.index,
                                    field,
                                )?,
                            )?,
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                payload.sort_by(|left, right| left.0.cmp(&right.0));
                Self::Enum {
                    name,
                    arguments: arguments.iter().map(ToString::to_string).collect(),
                    variant: variant.name().to_owned(),
                    payload,
                }
            }
            Value::Dynamic(_) => {
                let definition = super::super::native_instance::record_definition(value)?
                    .ok_or_else(unsupported)?;
                let traits = definition.implemented_traits.borrow();
                if !traits.contains("Eq") || !traits.contains("Hash") {
                    return Err(unsupported());
                }
                let Some(Type::Named { name, arguments }) = Type::of_value(value) else {
                    return Err(unsupported());
                };
                let mut fields = definition
                    .fields
                    .iter()
                    .map(|field| {
                        Ok((
                            field.name.clone(),
                            HashKey::from_value(&super::super::native_instance::borrow_field(
                                value,
                                &field.name,
                            )?)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                fields.sort_by(|left, right| left.0.cmp(&right.0));
                Self::Struct {
                    name,
                    arguments: arguments.iter().map(ToString::to_string).collect(),
                    fields,
                }
            }
            Value::Tuple(sequence) | Value::Array(sequence) => {
                let elements = sequence
                    .elements
                    .borrow()
                    .iter()
                    .map(|slot| {
                        HashKey::from_value(slot.value.as_ref().ok_or("moved key element")?)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if matches!(value, Value::Tuple(_)) {
                    Self::Tuple(elements)
                } else {
                    Self::Array(elements)
                }
            }
            Value::Option { value, .. } => Self::Option(
                value
                    .as_ref()
                    .map(|value| HashKey::from_value(value).map(Box::new))
                    .transpose()?,
            ),
            Value::Result {
                value: Ok(value), ..
            } => Self::ResultOk(Box::new(HashKey::from_value(value)?)),
            Value::Result {
                value: Err(value), ..
            } => Self::ResultErr(Box::new(HashKey::from_value(value)?)),

            _ => return Err(unsupported()),
        })
    }
}
