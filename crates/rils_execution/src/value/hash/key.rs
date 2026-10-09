//! Immutable query identity is independent of the owned key payload.
use super::super::Value;
use crate::Type;
use std::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    rc::Rc,
};
#[path = "key/view.rs"]
mod view;

/// Payload-free lookup key. Type witnesses distinguish integer widths, nominal
/// declarations, container kinds, and inactive generic alternatives.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyIdentity {
    ty: Type,
    key: rils_native::NativeKey,
}

impl KeyIdentity {
    pub fn from_value(
        value: &Value,
        expected: Option<&Type>,
        ordered: bool,
    ) -> Result<Self, String> {
        let (ty, key) = super::super::native_key::identity(value, expected, ordered)?;
        Ok(Self { ty, key })
    }
    pub fn ty(&self) -> &Type {
        &self.ty
    }
}

/// An owned compatibility entry. Hashing and ordering never inspect its payload.
#[derive(Clone)]
pub struct HashKey {
    identity: Rc<KeyIdentity>,
    value: Rc<Value>,
}

impl HashKey {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        Self::snapshot(value, false)
    }
    pub fn from_ordered_value(value: &Value) -> Result<Self, String> {
        Self::snapshot(value, true)
    }
    fn snapshot(value: &Value, ordered: bool) -> Result<Self, String> {
        let identity = KeyIdentity::from_value(value, None, ordered)?;
        let value = match value {
            Value::Reference(reference) => reference.read()?.clone_owned()?,
            value => value.clone_owned()?,
        };
        Ok(Self {
            identity: Rc::new(identity),
            value: Rc::new(value),
        })
    }
    pub fn to_value(&self) -> Value {
        self.value
            .clone_owned()
            .expect("stored key remains complete")
    }
    pub fn into_value(self) -> Value {
        Rc::try_unwrap(self.value).unwrap_or_else(|value| {
            value
                .clone_owned()
                .expect("shared stored key remains cloneable")
        })
    }
    pub fn ty(&self) -> Type {
        self.identity.ty.clone()
    }
    /// Read the original key through the registered host representation.
    pub fn with_ref<T: crate::RilsHostType, R>(
        &self,
        callback: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        crate::host_value::with_host_value(&self.value, callback)
    }
}
impl Borrow<KeyIdentity> for HashKey {
    fn borrow(&self) -> &KeyIdentity {
        &self.identity
    }
}
impl fmt::Debug for HashKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.identity.fmt(formatter)
    }
}
impl PartialEq for HashKey {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}
impl Eq for HashKey {}
impl PartialOrd for HashKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HashKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.identity.cmp(&other.identity)
    }
}
impl Hash for HashKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identity.hash(state);
    }
}
