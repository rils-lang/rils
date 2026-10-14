//! Shared-handle operations over native payloads, without execution values.

use super::{ErasedRc, ErasedWeak};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue, DynamicValueRef};

fn item_type(view: &DynamicValueRef<'_>, expected: &str) -> Result<Type, String> {
    match view.layout()?.rils_type() {
        Type::Named { name, arguments } if name == expected && arguments.len() == 1 => {
            Ok(arguments[0].clone())
        }
        _ => Err(format!("expected {expected} receiver")),
    }
}

pub mod strong {
    use super::*;

    pub fn new(value: DynamicValue) -> Result<DynamicValue, String> {
        let ty = Type::Named {
            name: "Rc".into(),
            arguments: vec![value.descriptor().rils_type().clone()],
        };
        DynamicValue::from_rust(
            DynamicLayout::of::<ErasedRc>(ty),
            ErasedRc(std::rc::Rc::new(value)),
        )
    }

    pub fn strong_count(view: DynamicValueRef<'_>) -> Result<usize, String> {
        item_type(&view, "Rc")?;
        view.with_rust::<ErasedRc, _>(|handle| std::rc::Rc::strong_count(&handle.0))
    }

    pub fn downgrade(view: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
        let item = item_type(&view, "Rc")?;
        let handle =
            view.with_rust::<ErasedRc, _>(|handle| ErasedWeak(std::rc::Rc::downgrade(&handle.0)))?;
        let ty = Type::Named {
            name: "Weak".into(),
            arguments: vec![item],
        };
        DynamicValue::from_rust(DynamicLayout::of::<ErasedWeak>(ty), handle)
    }
}

pub mod weak {
    use super::*;

    pub fn strong_count(view: DynamicValueRef<'_>) -> Result<usize, String> {
        item_type(&view, "Weak")?;
        view.with_rust::<ErasedWeak, _>(|handle| handle.0.strong_count())
    }

    pub fn weak_count(view: DynamicValueRef<'_>) -> Result<usize, String> {
        item_type(&view, "Weak")?;
        view.with_rust::<ErasedWeak, _>(|handle| handle.0.weak_count())
    }

    pub fn upgrade(view: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
        let item = item_type(&view, "Weak")?;
        let strong = view.with_rust::<ErasedWeak, _>(|handle| handle.0.upgrade().map(ErasedRc))?;
        let rc = DynamicLayout::of::<ErasedRc>(Type::Named {
            name: "Rc".into(),
            arguments: vec![item],
        });
        let option = DynamicLayout::option(rc.clone())?;
        match strong {
            Some(handle) => DynamicValue::some(option, DynamicValue::from_rust(rc, handle)?),
            None => DynamicValue::none(option),
        }
    }
}
