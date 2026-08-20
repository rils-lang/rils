use std::collections::{BTreeMap, HashSet};

use rils_frontend::Type;

use super::{HOST_MANIFEST_MAX_NAME_BYTES, is_identifier};

/// ABI transport used for a nominal host type. The logical type remains visible
/// to Rils while values cross the host boundary through this portable carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostTypeTransport {
    HostHandle,
}

impl HostTypeTransport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostHandle => "HostHandle",
        }
    }

    pub(crate) const fn as_tag(self) -> u8 {
        match self {
            Self::HostHandle => 9,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, String> {
        match tag {
            9 => Ok(Self::HostHandle),
            value => Err(format!(
                "unsupported binary host type transport tag {value}"
            )),
        }
    }

    pub fn as_type(self) -> Type {
        match self {
            Self::HostHandle => Type::named("HostHandle"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTypeDeclaration {
    pub name: String,
    pub base_type: Option<String>,
    pub transport: HostTypeTransport,
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
