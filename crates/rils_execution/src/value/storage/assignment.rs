//! Owned assignment into a place with an established native declaration.

use super::*;

/// Payload-free storage declarations retained when an aggregate is moved out.
#[derive(Clone)]
pub struct NativeDeclaration(Rc<Declaration>);

enum Declaration {
    Dynamic(Rc<DynamicType<Value>>),
    Indexed {
        ty: Type,
        children: Vec<Option<NativeDeclaration>>,
    },
}

impl NativeDeclaration {
    pub fn rils_type(&self) -> &Type {
        match self.0.as_ref() {
            Declaration::Dynamic(declaration) => declaration.layout().rils_type(),
            Declaration::Indexed { ty, .. } => ty,
        }
    }

    fn needs_conversion(&self, value: &Value) -> bool {
        match (self.0.as_ref(), value) {
            (Declaration::Dynamic(declaration), Value::Dynamic(value)) => {
                !declaration
                    .layout()
                    .compatible_with(value.descriptor().layout())
                    || (declaration
                        .metadata::<NativeRecordCodec>()
                        .is_some_and(|codec| {
                            codec
                                .nominal_definition(declaration.layout().rils_type())
                                .is_some()
                        })
                        && value.descriptor().metadata::<NativeRecordCodec>().is_none())
            }
            (Declaration::Indexed { ty, children }, Value::Tuple(value) | Value::Array(value)) => {
                if let Type::Array { element, .. } = ty
                    && value.element_type.borrow().as_ref() != Some(element.as_ref())
                {
                    return true;
                }
                let elements = value.elements.borrow();
                let annotations_match = elements.iter().enumerate().all(|(index, slot)| match ty {
                    Type::Tuple(types) => types.get(index) == Some(&slot.type_annotation),
                    Type::Array { element, .. } => element.as_ref() == &slot.type_annotation,
                    _ => false,
                });
                !annotations_match
                    || children.iter().zip(elements.iter()).any(|(child, slot)| {
                        child.as_ref().is_some_and(|child| {
                            slot.value
                                .as_ref()
                                .is_some_and(|value| child.needs_conversion(value))
                        })
                    })
            }
            _ => true,
        }
    }

    fn apply_owned(&self, value: Value, expected: &Type) -> Result<Value, String> {
        if !expected.accepts(&value) {
            return Err(format!("assigned value does not match {expected}"));
        }
        if !self.needs_conversion(&value) {
            return Ok(value);
        }
        match self.0.as_ref() {
            Declaration::Dynamic(declaration)
                if matches!(expected, Type::Option(_) | Type::Result(_, _))
                    || declaration
                        .metadata::<NativeRecordCodec>()
                        .is_some_and(|codec| codec.nominal_definition(expected).is_some()) =>
            {
                let mut codec = declaration
                    .metadata::<NativeRecordCodec>()
                    .map(|codec| codec.as_ref().clone())
                    .unwrap_or_default();
                let payload = codec.into_native(value, declaration.layout_handle())?;
                super::super::native_instance::with_descriptor(declaration.clone(), payload)
                    .map(Value::Dynamic)
            }
            Declaration::Indexed { children, .. } => {
                indexed::map_owned(value, expected, |value, ty, index| {
                    constrain_assignment(value, ty, children[index].clone())
                })
            }
            _ => value
                .constrain_owned(expected)
                .ok_or_else(|| format!("assigned value does not match {expected}")),
        }
    }
}

pub fn native_declaration(value: &Value) -> Option<NativeDeclaration> {
    match value {
        Value::Dynamic(value) => Some(NativeDeclaration(Rc::new(Declaration::Dynamic(
            value.descriptor_handle(),
        )))),
        Value::Tuple(sequence) | Value::Array(sequence) => {
            let elements = sequence.elements.borrow();
            if elements
                .iter()
                .all(|slot| slot.native_declaration.is_none())
                && !(elements.is_empty()
                    && matches!(value, Value::Array(_))
                    && sequence
                        .element_type
                        .borrow()
                        .as_ref()
                        .is_some_and(|ty| *ty != Type::Unknown))
            {
                return None;
            }
            let children = elements
                .iter()
                .map(|slot| slot.native_declaration.clone())
                .collect::<Vec<_>>();
            let ty = if matches!(value, Value::Tuple(_)) {
                Type::Tuple(
                    elements
                        .iter()
                        .map(|slot| slot.type_annotation.clone())
                        .collect(),
                )
            } else {
                Type::Array {
                    element: Box::new(sequence.element_type.borrow().clone()?),
                    length: elements.len(),
                }
            };
            Some(NativeDeclaration(Rc::new(Declaration::Indexed {
                ty,
                children,
            })))
        }
        _ => None,
    }
}

pub fn constrain_assignment(
    value: Value,
    expected: &Type,
    declaration: Option<NativeDeclaration>,
) -> Result<Value, String> {
    let expected = declarations::storage_type(expected);
    if let Some(declaration) = declaration
        && declaration.rils_type() == &expected
    {
        return declaration.apply_owned(value, &expected);
    }
    value
        .constrain_owned(&expected)
        .ok_or_else(|| format!("assigned value does not match {expected}"))
}
