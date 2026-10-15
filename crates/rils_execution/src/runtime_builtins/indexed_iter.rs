use std::{cell::Cell, rc::Rc};

use crate::{
    types::Type,
    value::{
        BorrowedIndexedIteratorValue, BorrowedIteratorProjection, IndexedIteratorStorage, Value,
    },
};
use rils_builtins::{BuiltinDeclaration, TypePattern};
use rils_stdlib::storage::EntrySchema;

pub(super) fn borrow(
    arguments: &[Value],
    schema: &EntrySchema,
    method: &str,
) -> Result<Value, String> {
    let Some(receiver) = arguments.first().and_then(Value::as_reference) else {
        return Err("indexed iterator method requires a borrowed receiver".into());
    };
    let native = crate::value::native_receiver::NativeReceiver::from_value(&arguments[0])?;
    let codec = native
        .as_ref()
        .map(|object| object.codec())
        .unwrap_or_default();
    let object = native
        .as_ref()
        .ok_or("iter receiver is not a native indexed sequence")?;
    let source_type = object.descriptor().layout().rils_type();
    let (owner_type, type_arguments) = match source_type {
        Type::Array { element, .. } if schema.parameters == 1 => {
            if !schema.parameter_path(0)?.is_empty() {
                return Err("array iterator requires a single element storage schema".into());
            }
            let arguments = vec![*element.clone()];
            (
                Type::Named {
                    name: schema.owner.into(),
                    arguments: arguments.clone(),
                },
                arguments,
            )
        }
        Type::Named { arguments, .. } => {
            schema.validate(object.descriptor().layout())?;
            (source_type.clone(), arguments.clone())
        }
        _ => return Err("iterator receiver does not match its storage schema".into()),
    };
    let declaration =
        rils_builtins::builtin(schema.owner).ok_or("iterator owner has no shared declaration")?;
    let member = declaration
        .member(method)
        .ok_or("iterator method has no declaration")?;
    let TypePattern::Named {
        arguments: [item], ..
    } = member
        .signature
        .ok_or("iterator method has no signature")?
        .result
    else {
        return Err("borrowed iterator requires a declared item type".into());
    };
    let projection = projection(*item, declaration, &type_arguments, schema)?;
    let Some(Type::Function { return_type, .. }) =
        rils_frontend::standard_library::builtin_member_type(&owner_type, method)
    else {
        return Err("iterator method has no resolved signature".into());
    };
    if rils_frontend::standard_library::builtin_iterator_item_type(&return_type)
        != Some(projection.item_type())
    {
        return Err("borrowed iterator projection does not match its declared item".into());
    }
    let (storage, length, element_type) = if let Some(object) = &native
        && let Type::Array { element, length } = object.descriptor().layout().rils_type()
    {
        object.with(|view| {
            if view.is_partially_moved()? {
                return Err("cannot iterate a partially moved array".into());
            }
            Ok::<_, String>(())
        })??;
        (
            IndexedIteratorStorage::NativeArray,
            *length,
            element.as_ref().clone(),
        )
    } else if let Some(object) = native
        && object.descriptor().layout().sequence_item().is_some()
    {
        let element_type = object
            .descriptor()
            .layout()
            .sequence_item()
            .expect("checked sequence")
            .rils_type()
            .clone();
        let (length, ledger) = object
            .with(|view| Ok::<_, String>((view.sequence_len()?, view.sequence_borrows()?)))??;
        let lease = ledger.begin_iteration()?;
        (
            IndexedIteratorStorage::Native {
                ledger,
                _lease: lease,
            },
            length,
            element_type,
        )
    } else {
        return Err("iter receiver is not a native indexed sequence".into());
    };
    Ok(Value::from_borrowed_iterator(
        BorrowedIndexedIteratorValue {
            source: receiver.clone(),
            storage,
            index: Cell::new(0),
            length,
            element_type,
            projection,
            iterator_type: *return_type,
            native_codec: Some(Rc::new(codec)),
        },
    ))
}

fn projection(
    pattern: TypePattern,
    owner: &BuiltinDeclaration,
    arguments: &[Type],
    schema: &EntrySchema,
) -> Result<BorrowedIteratorProjection, String> {
    match pattern {
        TypePattern::Reference {
            mutable: false,
            inner,
        } => {
            let (TypePattern::Generic(name) | TypePattern::BoundGeneric { name, .. }) = inner
            else {
                return Err(
                    "borrowed iterator reference must name a declared type parameter".into(),
                );
            };
            let index = owner
                .type_parameters
                .iter()
                .position(|parameter| parameter == name)
                .ok_or("borrowed iterator references an unknown type parameter")?;
            let inner = arguments
                .get(index)
                .ok_or("borrowed iterator has incomplete type arguments")?
                .clone();
            let path = schema
                .parameter_path(index)?
                .into_iter()
                .map(|step| {
                    if let rils_value::DynamicPathStep::Field(index) = step {
                        Ok(index)
                    } else {
                        Err("borrowed iterator requires a structural field projection".to_owned())
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BorrowedIteratorProjection::Reference { path, inner })
        }
        TypePattern::Tuple(fields) => fields
            .iter()
            .map(|field| projection(*field, owner, arguments, schema))
            .collect::<Result<Vec<_>, _>>()
            .map(BorrowedIteratorProjection::Tuple),
        _ => Err("borrowed iterator item must contain shared references or tuples".into()),
    }
}

pub(super) fn next(arguments: &[Value]) -> Result<Value, String> {
    let Some(receiver) = arguments.first().and_then(Value::as_reference) else {
        return Err("indexed iterator method requires a borrowed receiver".into());
    };
    if !receiver.mutable {
        return Err("Iter::next requires `&mut self`".into());
    }
    super::range::next(arguments)
}

#[cfg(test)]
#[path = "../../tests/unit/borrowed_projection.rs"]
mod tests;
