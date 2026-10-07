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
        let tokens = rils_frontend::lexer::lex_with_source_id(
            source,
            rils_frontend::SourceId::new(rils_frontend::SourceId::GENERATED_BIT),
        )
        .expect("generated Iterator source must lex");
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
        let object_type = Type::of_value(object)?;
        let object_type = match object_type {
            Type::Reference { inner, .. } => *inner,
            ty => ty,
        };
        let key = (object_type.to_string(), name.to_owned());
        if let Some(function) = self.builtin_default_functions.borrow().get(&key).cloned() {
            return Some(Value::BoundMethod(Rc::new(BoundMethod {
                receiver: Rc::new(object.clone()),
                function,
            })));
        }
        let Type::Function {
            parameters: Some(argument_types),
            return_type,
        } = rils_frontend::standard_library::builtin_trait_member_type(
            "Iterator",
            &object_type,
            name,
        )?
        else {
            return None;
        };
        let receiver_mode = rils_builtins::builtin_member("Iterator", name)?.receiver?;
        let mut parameters = method.parameters.clone();
        let mut argument_types = argument_types.into_iter();
        for parameter in &mut parameters {
            if parameter.name == "self" {
                parameter.type_annotation = Some(match receiver_mode {
                    rils_builtins::ReceiverMode::Owned => object_type.clone(),
                    rils_builtins::ReceiverMode::Shared | rils_builtins::ReceiverMode::Mutable => {
                        Type::Reference {
                            mutable: receiver_mode == rils_builtins::ReceiverMode::Mutable,
                            inner: Box::new(object_type.clone()),
                        }
                    }
                });
            } else {
                parameter.type_annotation = Some(argument_types.next()?);
            }
        }
        let body = method.body.clone().expect("default method has a body");
        let program = crate::ast::Program {
            statements: vec![crate::ast::Stmt::Function {
                visibility: crate::ast::Visibility::Private,
                attributes: Vec::new(),
                name: method.name.clone(),
                name_span: method.name_span,
                generic_parameters: method.generic_parameters.clone(),
                parameters: parameters.clone(),
                return_type: Some(*return_type.clone()),
                span: body.span,
                body,
            }],
            language_declaration_spans: Vec::new(),
            type_references: Vec::new(),
            macros: Vec::new(),
            generated_sources: Vec::new(),
        };
        let analysis = rils_frontend::analysis::analyze_program(&program);
        let crate::ast::Stmt::Function { body, .. } = &program.statements[0] else {
            unreachable!()
        };
        let function_body = body.clone();
        let ids =
            rils_frontend::semantic::ExpressionIdentityMap::allocate(&program, body.span.source)
                .for_cloned_block(body, &function_body);
        let function = Rc::new(UserFunction {
            name: format!("Iterator::{name}"),
            generic_parameters: method.generic_parameters.clone(),
            parameters,
            return_type: Some(*return_type),
            body: function_body,
            closure: self.globals.clone(),
            semantic_expression_ids: Some(ids),
            typeck_results: Some(Rc::new(analysis.typeck_results)),
        });
        self.builtin_default_functions
            .borrow_mut()
            .insert(key, function.clone());
        Some(Value::BoundMethod(Rc::new(BoundMethod {
            receiver: Rc::new(object.clone()),
            function,
        })))
    }
}
