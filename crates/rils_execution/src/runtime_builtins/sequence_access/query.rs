//! Borrowed queries retain their own declarations and do not clone payloads.
use super::services::Services;
use crate::{
    Value,
    value::{
        borrowed::{Read, with_read},
        record_codec::NativeRecordCodec,
    },
};
use rils_stdlib::storage::{ElementQuery, KeyPolicy, KeyQuery};
pub(crate) struct Query<'a>(&'a Value);
impl<'a> Query<'a> {
    pub(crate) fn new(value: &'a Value) -> Self {
        Self(value)
    }
}
impl ElementQuery for Query<'_> {
    fn equal(&self, element: rils_value::DynamicValueRef<'_>) -> Result<bool, String> {
        crate::value::equality::view_query_equal(element, self.0)
    }
}
impl KeyQuery for Query<'_> {
    fn identity(
        &self,
        expected: &rils_value::DynamicLayout,
        policy: KeyPolicy,
    ) -> Result<rils_native::NativeKey, String> {
        let codec = crate::value::borrowed::native_codec(self.0)?;
        let services = Services(
            codec
                .as_deref()
                .cloned()
                .unwrap_or_else(NativeRecordCodec::default),
        );
        with_read(self.0, true, |value| match value {
            Read::View(view) => {
                let actual = view.layout()?;
                if !expected.compatible_with(&actual) {
                    return Err(format!(
                        "collection key expects {}, found {}",
                        expected.rils_type(),
                        actual.rils_type()
                    ));
                }
                policy.identity(view, &services)
            }
            Read::Leaf(leaf) => {
                if leaf.rils_type() != expected.rils_type() {
                    return Err(format!(
                        "collection key expects {}, found {}",
                        expected.rils_type(),
                        leaf.rils_type()
                    ));
                }
                policy.leaf_identity(&leaf)
            }
            Read::Host(object) => Err(format!(
                "{} has no native key storage",
                object.type_definition.name
            )),
        })?
    }
}
