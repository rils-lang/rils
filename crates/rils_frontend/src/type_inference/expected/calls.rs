//! Propagate result constraints through declared generic function signatures.
use super::*;

impl Inferencer<'_> {
    pub(super) fn apply_expected_call(
        &mut self,
        expression: &Expr,
        callee: &Expr,
        arguments: &[Expr],
        expected: &Type,
    ) {
        let callee_id = self.expression_ids.id(callee);
        let Some(Type::Function {
            parameters: Some(parameters),
            return_type,
        }) = self.result.expression_types_by_id.get(&callee_id).cloned()
        else {
            return;
        };
        let mut substitutions = HashMap::new();
        infer_type_variables(&return_type, expected, &mut substitutions);
        for (parameter, argument) in parameters.iter().zip(arguments) {
            let id = self.expression_ids.id(argument);
            if let Some(actual) = self.result.expression_types_by_id.get(&id) {
                infer_type_variables(parameter, actual, &mut substitutions);
            }
        }
        for (parameter, argument) in parameters.iter().zip(arguments) {
            self.apply_expected_type(argument, &parameter.substitute(&substitutions));
        }
        self.record_compatible_type(expression, expected);
    }
}
