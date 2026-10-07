//! Infer unresolved receiver arguments from a method's declared signature.

use super::*;

impl Inferencer<'_> {
    pub(super) fn trait_call_type(&self, callee: &Expr, arguments: &[Type]) -> Option<Type> {
        let Expr::Path { segments, .. } = callee else {
            return None;
        };
        let (method, owner) = segments.split_last()?;
        let declaration = rils_builtins::builtin(&owner.join("::"))?;
        if declaration.kind != rils_builtins::BuiltinKind::Trait {
            return None;
        }
        let receiver_mode = declaration.member(method)?.receiver?;
        let actual = arguments.first()?;
        let target = match actual {
            Type::Reference { inner, .. } => inner.as_ref(),
            ty => ty,
        };
        let item = self.iterable_item_type(target);
        let Type::Function {
            parameters: Some(mut parameters),
            return_type,
        } = crate::standard_library::builtin_trait_member_type_with_iterator_item(
            declaration.path,
            target,
            method,
            (item != Type::Unknown).then_some(item),
        )?
        else {
            return None;
        };
        parameters.insert(
            0,
            match receiver_mode {
                rils_builtins::ReceiverMode::Owned => target.clone(),
                rils_builtins::ReceiverMode::Shared | rils_builtins::ReceiverMode::Mutable => {
                    Type::Reference {
                        mutable: receiver_mode == rils_builtins::ReceiverMode::Mutable,
                        inner: Box::new(target.clone()),
                    }
                }
            },
        );
        let function = map_type(&Type::function(parameters, *return_type), &mut |ty| {
            let resolved = self.syntax_type(ty);
            (resolved != *ty).then_some(resolved)
        });
        let Type::Named { name, arguments } = target else {
            return Some(function);
        };
        let Some(definition) = self.types.get(name) else {
            return Some(function);
        };
        let substitutions = definition
            .generic_parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        Some(map_type(&function, &mut |ty| {
            let Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            } = ty
            else {
                return None;
            };
            if base.as_ref() != target || !arguments.is_empty() {
                return None;
            }
            let owner = trait_name.as_deref().unwrap_or(declaration.path);
            let owner = owner.rsplit("::").next()?;
            definition
                .associated_types
                .get(&(owner.to_owned(), name.clone()))
                .map(|ty| ty.substitute(&substitutions))
        }))
    }

    pub(super) fn partial_member_call_type(
        &mut self,
        callee: &Expr,
        arguments: &[Type],
        id: ExprId,
    ) -> Option<Type> {
        let Expr::Member { object, name, .. } = callee else {
            return None;
        };
        let object_id = self.expression_ids.id(object);
        let ty = self.result.expression_types_by_id.get(&object_id)?;
        let prefix = format!("#receiver:{}:{}:", id.source.0, id.local);
        let mut serial = 0;
        let receiver = map_type(ty, &mut |ty| {
            if *ty != Type::Unknown {
                return None;
            }
            let name = format!("{prefix}{serial}");
            serial += 1;
            Some(Type::Variable(name))
        });
        if serial == 0 {
            return None;
        }
        let function = self.field_type(&receiver, name);
        let Type::Function {
            parameters: Some(parameters),
            ..
        } = &function
        else {
            return None;
        };
        let mut substitutions = HashMap::new();
        for (parameter, argument) in parameters.iter().zip(arguments) {
            infer_type_variables(parameter, argument, &mut substitutions);
        }
        let mut unresolved = false;
        let receiver = map_type(&receiver.substitute(&substitutions), &mut |ty| {
            if matches!(ty, Type::Variable(name) if name.starts_with(&prefix)) {
                unresolved = true;
                Some(Type::Unknown)
            } else {
                None
            }
        });
        if !unresolved {
            self.apply_expected_type(object, &receiver);
        }
        let function = map_type(&function.substitute(&substitutions), &mut |ty| {
            matches!(ty, Type::Variable(name) if name.starts_with(&prefix)).then_some(Type::Unknown)
        });
        let callee_id = self.expression_ids.id(callee);
        self.result
            .expression_types_by_id
            .insert(callee_id, function.clone());
        Some(function)
    }
}

fn map_type(ty: &Type, replace: &mut impl FnMut(&Type) -> Option<Type>) -> Type {
    if let Some(ty) = replace(ty) {
        return ty;
    }
    match ty {
        Type::Named { name, arguments } => Type::Named {
            name: name.clone(),
            arguments: arguments.iter().map(|ty| map_type(ty, replace)).collect(),
        },
        Type::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => Type::Associated {
            base: Box::new(map_type(base, replace)),
            trait_name: trait_name.clone(),
            name: name.clone(),
            arguments: arguments.iter().map(|ty| map_type(ty, replace)).collect(),
        },
        Type::Option(item) => Type::Option(Box::new(map_type(item, replace))),
        Type::Result(ok, error) => Type::Result(
            Box::new(map_type(ok, replace)),
            Box::new(map_type(error, replace)),
        ),
        Type::Reference { mutable, inner } => Type::Reference {
            mutable: *mutable,
            inner: Box::new(map_type(inner, replace)),
        },
        Type::Tuple(items) => Type::Tuple(items.iter().map(|ty| map_type(ty, replace)).collect()),
        Type::Array { element, length } => Type::Array {
            element: Box::new(map_type(element, replace)),
            length: *length,
        },
        Type::Slice(element) => Type::Slice(Box::new(map_type(element, replace))),
        Type::ArrayParameter { element, length } => Type::ArrayParameter {
            element: Box::new(map_type(element, replace)),
            length: length.clone(),
        },
        Type::Function {
            parameters,
            return_type,
        } => Type::Function {
            parameters: parameters
                .as_ref()
                .map(|items| items.iter().map(|ty| map_type(ty, replace)).collect()),
            return_type: Box::new(map_type(return_type, replace)),
        },
        _ => ty.clone(),
    }
}
