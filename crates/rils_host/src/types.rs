use std::collections::{BTreeMap, HashSet};

use rils_syntax::{IntegerType, Type};

use super::{HOST_MANIFEST_MAX_NAME_BYTES, is_identifier};

/// ABI transport used for a nominal host type. The logical type remains visible
/// to Rils while values cross the host boundary through this portable carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostTypeTransport {
    HostHandle,
    InlineValue,
    Enum,
}

impl HostTypeTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostHandle => "HostHandle",
            Self::InlineValue => "InlineValue",
            Self::Enum => "Integer",
        }
    }

    pub(crate) fn as_tag(self) -> u8 {
        match self {
            Self::HostHandle => 9,
            Self::InlineValue => 10,
            Self::Enum => panic!("host enum transports encode their underlying integer"),
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, String> {
        match tag {
            9 => Ok(Self::HostHandle),
            10 => Ok(Self::InlineValue),
            value => Err(format!(
                "unsupported binary host type transport tag {value}"
            )),
        }
    }

    pub fn as_type(self) -> Type {
        match self {
            Self::HostHandle => Type::named("HostHandle"),
            Self::InlineValue => Type::named("InlineValue"),
            Self::Enum => Type::Unknown,
        }
    }
}

pub const HOST_INLINE_VALUE_MAX_BYTES: usize = 16;
pub const HOST_INLINE_VALUE_MAX_FIELDS: usize = 16;

/// One scalar field in the canonical packed representation of an inline host
/// value. Host structs are encoded field-by-field in declaration order without
/// native padding. This is deliberately independent of Rust, CLR, IL2CPP, and
/// platform memory layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostValueFieldType {
    Bool,
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
}

impl HostValueFieldType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "bool" => Ok(Self::Bool),
            "i8" => Ok(Self::I8),
            "i16" => Ok(Self::I16),
            "i32" => Ok(Self::I32),
            "i64" => Ok(Self::I64),
            "i128" => Ok(Self::I128),
            "u8" => Ok(Self::U8),
            "u16" => Ok(Self::U16),
            "u32" => Ok(Self::U32),
            "u64" => Ok(Self::U64),
            "u128" => Ok(Self::U128),
            "f32" => Ok(Self::F32),
            "f64" => Ok(Self::F64),
            other => Err(format!(
                "unsupported inline host value field type `{other}`"
            )),
        }
    }

    pub const fn byte_len(self) -> usize {
        match self {
            Self::Bool | Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
            Self::I128 | Self::U128 => 16,
        }
    }
}

/// Canonical field layout for an inline host value. The maximum encoded size
/// matches the fixed 16-byte C ABI payload. Legacy `f32xN` spellings remain
/// readable, while newly declared layouts use the general `fields(...)` form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostValueLayout {
    fields: [Option<HostValueFieldType>; HOST_INLINE_VALUE_MAX_FIELDS],
    field_count: u8,
    byte_len: u8,
}

impl HostValueLayout {
    #[allow(non_upper_case_globals)]
    pub const F32x2: Self = Self::from_f32_count(2);
    #[allow(non_upper_case_globals)]
    pub const F32x3: Self = Self::from_f32_count(3);
    #[allow(non_upper_case_globals)]
    pub const F32x4: Self = Self::from_f32_count(4);

    const fn from_f32_count(count: u8) -> Self {
        let mut fields = [None; HOST_INLINE_VALUE_MAX_FIELDS];
        let mut index = 0;
        while index < count as usize {
            fields[index] = Some(HostValueFieldType::F32);
            index += 1;
        }
        Self {
            fields,
            field_count: count,
            byte_len: count * 4,
        }
    }

    pub fn from_fields(fields: &[HostValueFieldType]) -> Result<Self, String> {
        if fields.is_empty() {
            return Err("inline host value layout must declare at least one field".into());
        }
        if fields.len() > HOST_INLINE_VALUE_MAX_FIELDS {
            return Err(format!(
                "inline host value layout exceeds the {HOST_INLINE_VALUE_MAX_FIELDS} field limit"
            ));
        }
        let byte_len = fields.iter().try_fold(0usize, |total, field| {
            total
                .checked_add(field.byte_len())
                .ok_or_else(|| "inline host value layout size overflow".to_string())
        })?;
        if byte_len > HOST_INLINE_VALUE_MAX_BYTES {
            return Err(format!(
                "inline host value layout requires {byte_len} bytes, exceeding the {HOST_INLINE_VALUE_MAX_BYTES}-byte ABI payload"
            ));
        }
        let mut packed = [None; HOST_INLINE_VALUE_MAX_FIELDS];
        for (index, field) in fields.iter().copied().enumerate() {
            packed[index] = Some(field);
        }
        Ok(Self {
            fields: packed,
            field_count: fields.len() as u8,
            byte_len: byte_len as u8,
        })
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "f32x2" => return Ok(Self::F32x2),
            "f32x3" => return Ok(Self::F32x3),
            "f32x4" => return Ok(Self::F32x4),
            _ => {}
        }
        let fields = value
            .strip_prefix("fields(")
            .and_then(|value| value.strip_suffix(')'))
            .ok_or_else(|| format!("unsupported inline host value layout `{value}`"))?;
        let fields = fields
            .split(',')
            .map(HostValueFieldType::parse)
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_fields(&fields)
    }

    pub fn canonical_name(self) -> String {
        format!(
            "fields({})",
            self.fields()
                .map(|field| field.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn fields(&self) -> impl ExactSizeIterator<Item = HostValueFieldType> + '_ {
        self.fields[..usize::from(self.field_count)]
            .iter()
            .map(|field| field.expect("declared inline layout fields are populated"))
    }

    pub const fn field_count(self) -> usize {
        self.field_count as usize
    }

    pub const fn byte_len(self) -> usize {
        self.byte_len as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTypeDeclaration {
    pub name: String,
    pub base_type: Option<String>,
    pub transport: HostTypeTransport,
    pub value_layout: Option<HostValueLayout>,
    pub enum_definition: Option<HostEnumDefinition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEnumDefinition {
    pub underlying_type: IntegerType,
    pub flags: bool,
    pub variants: BTreeMap<String, u128>,
}

pub(crate) fn validate_type_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > HOST_MANIFEST_MAX_NAME_BYTES
        || name.split("::").any(|segment| !is_identifier(segment))
    {
        return Err(format!("`{name}` is not a valid host type path"));
    }
    Ok(())
}

pub(crate) fn validate_type_graph(
    types: &BTreeMap<String, HostTypeDeclaration>,
) -> Result<(), String> {
    for declaration in types.values() {
        match (
            declaration.transport,
            declaration.value_layout,
            declaration.enum_definition.as_ref(),
        ) {
            (HostTypeTransport::HostHandle, None, None) => {}
            (HostTypeTransport::InlineValue, Some(_), None) if declaration.base_type.is_none() => {}
            (HostTypeTransport::Enum, None, Some(_)) => {
                if declaration.base_type.is_some() {
                    return Err(format!(
                        "host enum type `{}` cannot inherit another host type",
                        declaration.name
                    ));
                }
            }
            (HostTypeTransport::HostHandle, Some(_), _) => {
                return Err(format!(
                    "opaque host type `{}` cannot declare an inline value layout",
                    declaration.name
                ));
            }
            (HostTypeTransport::InlineValue, None, _) => {
                return Err(format!(
                    "inline host type `{}` must declare a value layout",
                    declaration.name
                ));
            }
            (HostTypeTransport::InlineValue, Some(_), _) => {
                return Err(format!(
                    "inline host type `{}` cannot inherit another host type",
                    declaration.name
                ));
            }
            (HostTypeTransport::HostHandle, None, Some(_))
            | (HostTypeTransport::Enum, _, None)
            | (HostTypeTransport::Enum, Some(_), Some(_)) => {
                return Err(format!(
                    "host enum type `{}` has inconsistent transport metadata",
                    declaration.name
                ));
            }
        }
        let mut visited = HashSet::new();
        let mut current = declaration;
        while let Some(base_name) = current.base_type.as_deref() {
            if !visited.insert(current.name.as_str()) {
                return Err(format!(
                    "host type inheritance contains a cycle at `{}`",
                    current.name
                ));
            }
            let base = types.get(base_name).ok_or_else(|| {
                format!(
                    "host type `{}` inherits unknown host type `{base_name}`",
                    declaration.name
                )
            })?;
            if base.transport != declaration.transport {
                return Err(format!(
                    "host type `{}` and base type `{base_name}` use different ABI transports",
                    declaration.name
                ));
            }
            current = base;
        }
    }
    Ok(())
}

pub(crate) fn is_assignable(
    types: &BTreeMap<String, HostTypeDeclaration>,
    expected: &str,
    actual: &str,
) -> bool {
    if expected == actual {
        return true;
    }
    let mut current = types.get(actual);
    while let Some(declaration) = current {
        let Some(base) = declaration.base_type.as_deref() else {
            return false;
        };
        if base == expected {
            return true;
        }
        current = types.get(base);
    }
    false
}
