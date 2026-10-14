//! Native interior-mutability operations independent of execution values.
use super::{ErasedCell, ErasedRefCell};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue, DynamicValueRef};
use std::rc::Rc;

pub mod cell {
    use super::*;
    pub fn new(value: DynamicValue) -> Result<DynamicValue, String> {
        let ty = Type::Named {
            name: "Cell".into(),
            arguments: vec![value.descriptor().rils_type().clone()],
        };
        DynamicValue::from_rust(
            DynamicLayout::of::<ErasedCell>(ty),
            ErasedCell(std::cell::RefCell::new(value)),
        )
    }
    pub fn item_layout(view: DynamicValueRef<'_>) -> Result<Rc<DynamicLayout>, String> {
        view.with_rust::<ErasedCell, _>(|cell| {
            cell.0
                .try_borrow()
                .map(|value| value.layout_handle())
                .map_err(|_| "Cell value is already accessed".into())
        })?
    }
    pub fn get(view: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
        view.with_rust::<ErasedCell, _>(|cell| {
            let value = cell
                .0
                .try_borrow()
                .map_err(|_| "Cell value is already accessed")?;
            if !value.descriptor().is_copy() {
                return Err("Cell::get requires a Copy value".into());
            }
            value.copy_path(&[])
        })?
    }
    pub fn replace(view: DynamicValueRef<'_>, value: DynamicValue) -> Result<DynamicValue, String> {
        view.with_rust::<ErasedCell, _>(|cell| {
            let mut current = cell
                .0
                .try_borrow_mut()
                .map_err(|_| "Cell value is already accessed")?;
            if !current.descriptor().compatible_with(value.descriptor()) {
                return Err("Cell item layout mismatch".into());
            }
            Ok(std::mem::replace(&mut *current, value))
        })?
    }
    pub fn set(view: DynamicValueRef<'_>, value: DynamicValue) -> Result<(), String> {
        drop(replace(view, value)?);
        Ok(())
    }
}

pub mod ref_cell {
    use super::*;
    pub fn new(value: DynamicValue) -> Result<DynamicValue, String> {
        let ty = Type::Named {
            name: "RefCell".into(),
            arguments: vec![value.descriptor().rils_type().clone()],
        };
        let descriptor = Rc::new(DynamicType::new(value.layout_handle()));
        DynamicValue::from_rust(
            DynamicLayout::of::<ErasedRefCell>(ty),
            ErasedRefCell {
                value: DynamicObject::new_shared(descriptor, value)?,
                references: std::cell::Cell::new(0),
            },
        )
    }
    pub fn item_layout(view: DynamicValueRef<'_>) -> Result<Rc<DynamicLayout>, String> {
        view.with_rust::<ErasedRefCell, _>(|cell| cell.value.descriptor().layout_handle())
    }
    pub fn replace(view: DynamicValueRef<'_>, value: DynamicValue) -> Result<DynamicValue, String> {
        view.with_rust::<ErasedRefCell, _>(|cell| {
            if cell.references.get() > 0 {
                return Err("cannot replace RefCell while borrowed".into());
            }
            if !cell
                .value
                .descriptor()
                .layout()
                .compatible_with(value.descriptor())
            {
                return Err("RefCell item layout mismatch".into());
            }
            cell.value
                .with_mut(|current| std::mem::replace(current, value))
        })?
    }
}
