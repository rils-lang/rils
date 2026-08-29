use super::*;

pub(super) fn resolve_qualified_path(
    target: &Type,
    trait_name: &str,
    member: &str,
    environment: &EnvironmentRef,
    span: Span,
) -> Result<Value, RuntimeError> {
    let trait_value = environment
        .borrow()
        .get(trait_name)
        .ok_or_else(|| RuntimeError::new(format!("unknown trait `{trait_name}`"), span))?;
    let Value::TraitType(definition) = trait_value else {
        return Err(RuntimeError::new(
            format!("`{trait_name}` is not a trait"),
            span,
        ));
    };
    if !definition
        .methods
        .iter()
        .any(|method| method.name == member)
    {
        return Err(RuntimeError::new(
            format!("trait `{trait_name}` has no method `{member}`"),
            span,
        ));
    }
    Ok(Value::TraitMethodSelector(Rc::new(TraitMethodSelector {
        target: Some(target.clone()),
        trait_name: trait_name.into(),
        method_name: member.into(),
        environment: environment.clone(),
    })))
}
