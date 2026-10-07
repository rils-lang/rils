//! Binds macro-exported trait bodies to native iterator values.

use std::{collections::HashMap, sync::OnceLock};

use super::*;

fn iterator_defaults() -> &'static HashMap<String, crate::ast::TraitMethod> {
    static DEFAULTS: OnceLock<HashMap<String, crate::ast::TraitMethod>> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        let declaration = rils_builtins::builtin("Iterator")
            .expect("Iterator is declared by the standard library");
        let source = declaration
            .source
            .expect("Iterator exports its trait source");
        let tokens = rils_frontend::lexer::lex(source).expect("generated Iterator source must lex");
        let program = rils_frontend::parser::parse_builtin_declarations(tokens)
            .expect("generated Iterator source must parse");
        program
            .statements
            .into_iter()
            .find_map(|statement| match statement {
                crate::ast::Stmt::Trait { methods, .. } => Some(methods),
                _ => None,
            })
            .expect("generated Iterator source contains a trait")
            .into_iter()
            .filter(|method| method.body.is_some())
            .map(|method| (method.name.clone(), method))
            .collect()
    })
}

pub(in crate::interpreter) fn builtin_iterator_default_receiver(
    value: &Value,
    name: &str,
) -> Option<rils_builtins::ReceiverMode> {
    if !matches!(
        value,
        Value::OwnedIterator(_)
            | Value::BorrowedIndexedIterator(_)
            | Value::BorrowedMapIterator(_)
            | Value::BorrowedSetIterator(_)
    ) && !crate::value::native_ops::is_iterator(value)
    {
        return None;
    }
    let member = rils_builtins::builtin_member("Iterator", name)?;
    (!member.required).then_some(member.receiver?)
}

impl Interpreter {
    pub(in crate::interpreter) fn bind_builtin_iterator_default(
        &self,
        object: &Value,
        name: &str,
    ) -> Option<Value> {
        if !rils_builtins::is_iterator_default_method(name)
            || matches!(
                rils_execution::value::native_instance::value_definition(object)
                    .ok()
                    .flatten(),
                Some(Value::StructType(_) | Value::EnumType(_))
            )
        {
            return None;
        }
        let borrowed = match object {
            Value::Reference(reference) => Some(reference.read().ok()?),
            _ => None,
        };
        let lookup = borrowed.as_ref().unwrap_or(object);
        let receiver_mode = builtin_iterator_default_receiver(lookup, name)?;
        if matches!(receiver_mode, rils_builtins::ReceiverMode::Mutable)
            && matches!(object, Value::Reference(reference) if !reference.mutable)
        {
            return None;
        }
        let receiver = match receiver_mode {
            rils_builtins::ReceiverMode::Owned => object.clone(),
            rils_builtins::ReceiverMode::Shared | rils_builtins::ReceiverMode::Mutable
                if !matches!(object, Value::Reference(_)) =>
            {
                let mutable = matches!(receiver_mode, rils_builtins::ReceiverMode::Mutable);
                let storage = Rc::new(RefCell::new(
                    crate::environment::StorageSlot::uninitialized(mutable),
                ));
                storage.borrow_mut().initialize(object.clone());
                Value::Reference(Rc::new(ReferenceValue::new_storage(storage, mutable)))
            }
            _ => object.clone(),
        };
        self.bind_exported_iterator_default(&receiver, name)
    }

    pub(in crate::interpreter) fn bind_exported_iterator_default(
        &self,
        object: &Value,
        name: &str,
    ) -> Option<Value> {
        let method = iterator_defaults().get(name)?;
        let mut parameters = method.parameters.clone();
        for parameter in &mut parameters {
            if parameter.name == "self" {
                if let Some(Type::Reference { inner, .. }) = &mut parameter.type_annotation {
                    **inner = Type::Unknown;
                } else {
                    parameter.type_annotation = None;
                }
            } else {
                parameter.type_annotation = None;
            }
        }
        let function = Rc::new(UserFunction {
            name: format!("Iterator::{name}"),
            generic_parameters: Vec::new(),
            parameters,
            return_type: None,
            body: method.body.clone().expect("default method has a body"),
            closure: self.globals.clone(),
            semantic_expression_ids: None,
        });
        Some(Value::BoundMethod(Rc::new(BoundMethod {
            receiver: Rc::new(object.clone()),
            function,
        })))
    }
}
