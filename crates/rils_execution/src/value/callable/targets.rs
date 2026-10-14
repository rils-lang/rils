//! Signatures and captured values belong to the concrete call targets.
use super::*;
use crate::value::*;
use crate::{ast::EnumVariant, types::FunctionSignature};
impl CallTarget for UserFunction {
    fn signature(&self) -> Option<Type> {
        let function = self;
        Some(function_type(function))
    }
    fn description(&self) -> String {
        format!("<fn {}>", self.name)
    }
}
impl CallTarget for BytecodeFunctionValue {
    fn is_copy(&self) -> bool {
        self.bound_arguments.iter().all(Value::is_copy)
    }
    fn signature(&self) -> Option<Type> {
        Some(self.signature.substitute(&self.type_bindings))
    }
    fn description(&self) -> String {
        format!("<fn {}>", self.name)
    }
    fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
        for slot in &self.captures {
            if let Ok(value) = slot.borrow().read() {
                visit(&value);
            }
        }
        for value in &self.bound_arguments {
            visit(value);
        }
    }
}
impl CallTarget for NativeFunction {
    fn signature(&self) -> Option<Type> {
        let function = self;
        Some(
            function
                .signature
                .as_ref()
                .map_or_else(Type::opaque_function, FunctionSignature::as_type),
        )
    }
    fn description(&self) -> String {
        format!("<native fn {}>", self.name)
    }
}
impl CallTarget for HostFunction {
    fn signature(&self) -> Option<Type> {
        let function = self;
        Some(
            function
                .signature
                .as_ref()
                .map_or_else(Type::opaque_function, FunctionSignature::as_type),
        )
    }
    fn description(&self) -> String {
        format!("<host fn {}>", self.name)
    }
}
impl CallTarget for HostBoundMethod {
    fn is_copy(&self) -> bool {
        self.receiver.is_copy()
    }
    fn signature(&self) -> Option<Type> {
        let method = self;
        Some(
            method
                .function
                .signature
                .as_ref()
                .map_or_else(Type::opaque_function, FunctionSignature::as_type),
        )
    }
    fn description(&self) -> String {
        format!("<bound host fn {}>", self.function.name)
    }
    fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.receiver);
    }
}
impl CallTarget for BuiltinFunction {
    fn signature(&self) -> Option<Type> {
        Some(Type::opaque_function())
    }
    fn description(&self) -> String {
        "<builtin function>".into()
    }
}
impl CallTarget for VariantConstructor {
    fn signature(&self) -> Option<Type> {
        let constructor = self;
        {
            let variant = constructor
                .type_definition
                .variants
                .iter()
                .find(|variant| enum_variant_name(variant) == constructor.variant)?;
            let EnumVariant::Tuple { fields, .. } = variant else {
                return Some(Type::opaque_function());
            };
            Some(Type::function(
                fields.clone(),
                Type::Named {
                    name: constructor.type_definition.name.clone(),
                    arguments: constructor
                        .type_definition
                        .generic_parameters
                        .iter()
                        .map(|parameter| Type::Variable(parameter.name.clone()))
                        .collect(),
                },
            ))
        }
    }
    fn description(&self) -> String {
        format!(
            "<constructor {}::{}>",
            self.type_definition.name, self.variant
        )
    }
}
impl CallTarget for BoundMethod {
    fn is_copy(&self) -> bool {
        self.receiver.is_copy()
    }
    fn signature(&self) -> Option<Type> {
        let method = self;
        {
            let mut signature = function_type(&method.function);
            if let Type::Function {
                parameters: Some(parameters),
                ..
            } = &mut signature
                && !parameters.is_empty()
            {
                parameters.remove(0);
            }
            Some(signature)
        }
    }
    fn description(&self) -> String {
        format!("<bound fn {}>", self.function.name)
    }
    fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.receiver);
    }
}
impl CallTarget for BuiltinBoundMethod {
    fn is_copy(&self) -> bool {
        self.receiver.is_copy()
    }
    fn signature(&self) -> Option<Type> {
        let method = self;
        {
            let receiver = Type::of_value(method.receiver.as_ref()).unwrap_or(Type::Unknown);
            let receiver = match receiver {
                Type::Reference { inner, .. } => *inner,
                receiver => receiver,
            };
            let signature = match method.method {
                crate::value::BuiltinMethod::Native(symbol) => {
                    if let Some((intrinsic, _)) = rils_builtins::intrinsic_by_symbol(symbol) {
                        match receiver {
                            Type::Integer(integer) => {
                                rils_frontend::standard_library::integer_intrinsic_type(
                                    intrinsic, integer,
                                )
                            }
                            Type::Float(float) => {
                                rils_frontend::standard_library::float_intrinsic_type(
                                    intrinsic, float,
                                )
                            }
                            _ => Type::opaque_function(),
                        }
                    } else {
                        rils_frontend::standard_library::builtin_member_type(
                            &receiver,
                            rils_builtins::native_member(symbol)?.name,
                        )
                        .unwrap_or_else(Type::opaque_function)
                    }
                }
                crate::value::BuiltinMethod::IteratorIdentity => {
                    Type::function(Vec::new(), receiver)
                }
            };
            Some(signature)
        }
    }
    fn description(&self) -> String {
        "<bound builtin method>".into()
    }
    fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.receiver);
    }
}
impl CallTarget for TraitMethodSelector {
    fn signature(&self) -> Option<Type> {
        Some(Type::opaque_function())
    }
    fn description(&self) -> String {
        format!("<trait method {}::{}>", self.trait_name, self.method_name)
    }
}
fn function_type(function: &crate::value::UserFunction) -> Type {
    Type::function(
        function
            .parameters
            .iter()
            .map(|parameter| parameter.type_annotation.clone().unwrap_or(Type::Unknown))
            .collect(),
        function.return_type.clone().unwrap_or(Type::Unknown),
    )
}
