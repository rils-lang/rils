//! Supply declaration metadata without imposing standard-library behavior.
use crate::value::{Declaration, record_codec::NativeRecordCodec};
use rils_stdlib::storage::NativeServices;
pub(super) struct Services(pub NativeRecordCodec);
impl NativeServices for Services {
    fn implements(&self, ty: &crate::Type, required: &str) -> bool {
        match self.0.nominal_definition(ty) {
            Some(Declaration::StructType(definition)) => {
                definition.implemented_traits.borrow().contains(required)
            }
            Some(Declaration::EnumType(definition)) => {
                definition.implemented_traits.borrow().contains(required)
            }
            _ => false,
        }
    }
    fn clone_value(
        &self,
        value: rils_value::DynamicValueRef<'_>,
    ) -> Result<rils_value::DynamicValue, String> {
        self.0.clone_view(value)
    }
}
