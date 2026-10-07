//! Construction uses an instantiated type witness before allocating native bytes.

use super::*;

impl VirtualMachine<'_> {
    fn constructor_type(
        &self,
        type_id: usize,
        expected: &Type,
        span: Span,
    ) -> Result<Type, BytecodeError> {
        let name = match &self.module.types[type_id] {
            RuntimeType::Struct(definition) => &definition.name,
            RuntimeType::Enum(definition) => &definition.name,
        };
        let ty = expected.substitute(&self.frame().type_bindings);
        if !matches!(&ty, Type::Named { name: owner, .. } if owner == name) {
            return Err(BytecodeError::new(
                "constructor type witness does not match declaration",
                span,
            ));
        }
        Ok(ty)
    }

    pub(super) fn construct_record(
        &mut self,
        type_id: usize,
        expected: &Type,
        variant: Option<String>,
        fields: Vec<(String, usize)>,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let ty = self.constructor_type(type_id, expected, span)?;
        let values = fields
            .into_iter()
            .map(|(name, register)| Ok((name, self.take_register(register, span)?)))
            .collect::<Result<HashMap<_, _>, BytecodeError>>()?;
        self.native_context
            .storage()
            .construct_record(&ty, variant.as_deref(), values)
            .map_err(|message| BytecodeError::new(message, span))
    }

    pub(super) fn construct_tuple_variant(
        &mut self,
        type_id: usize,
        expected: &Type,
        variant: String,
        fields: Vec<usize>,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let ty = self.constructor_type(type_id, expected, span)?;
        let values = self.take_registers(fields, span)?;
        self.native_context
            .storage()
            .construct_tuple_variant(&ty, &variant, values)
            .map_err(|message| BytecodeError::new(message, span))
    }

    pub(super) fn construct_unit_variant(
        &self,
        type_id: usize,
        expected: &Type,
        variant: &str,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let ty = self.constructor_type(type_id, expected, span)?;
        self.native_context
            .storage()
            .construct_unit_variant(&ty, variant)
            .map_err(|message| BytecodeError::new(message, span))
    }
}
