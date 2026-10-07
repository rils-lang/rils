use super::*;

impl BytecodeNativeImport {
    pub(super) fn valid_callback_specialization(&self, erased: &FunctionSignature) -> bool {
        if !rils_builtins::requires_native_callback(&self.symbol)
            || self.signature.parameters != erased.parameters
            || !self.signature.return_type.is_type_witness()
        {
            return false;
        }
        if let Some((owner, member)) = rils_builtins::native_member_owner(&self.symbol)
            && member
                .signature
                .is_some_and(|signature| signature.result == rils_builtins::TypePattern::SelfType)
        {
            return rils_frontend::standard_library::builtin_owner_name(
                &self.signature.return_type,
            )
            .and_then(rils_builtins::builtin)
            .is_some_and(|result_owner| std::ptr::eq(owner, result_owner));
        }
        rils_frontend::types::merge_types(&erased.return_type, &self.signature.return_type)
            .is_some()
    }

    pub(super) fn specialized_empty_collection_type(&self) -> Option<&Type> {
        let (owner, member) = rils_builtins::native_member_owner(&self.symbol)?;
        let erased = rils_frontend::standard_library::erased_builtin_member_signature(member)?;
        let Type::Named { name, arguments } = &self.signature.return_type else {
            return None;
        };
        (member.name == "new"
            && member.receiver.is_none()
            && erased.parameters.as_deref() == Some(&[])
            && erased.return_type == Type::Unknown
            && self.signature.parameters == erased.parameters
            && owner.path.rsplit("::").next() == Some(name.as_str())
            && arguments.len() == owner.type_parameters.len())
        .then_some(&self.signature.return_type)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native_import.rs"]
mod tests;
