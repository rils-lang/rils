//! One projection of manifest enum metadata into Rils variant declarations.

use rils_syntax::{IntegerType, Type, ast::EnumVariant, source::Span};

use crate::HostEnumDefinition;

pub const HOST_FLAGS_RAW_VARIANT: &str = "#rils_host_flags_raw";

impl HostEnumDefinition {
    pub fn rils_variants(&self) -> Vec<EnumVariant> {
        self.variants
            .keys()
            .map(|name| EnumVariant::Unit {
                name: name.clone(),
                span: Span::default(),
            })
            .chain(self.flags.then(|| EnumVariant::Tuple {
                name: HOST_FLAGS_RAW_VARIANT.into(),
                fields: vec![Type::Integer(IntegerType::U128)],
                span: Span::default(),
            }))
            .collect()
    }
}
