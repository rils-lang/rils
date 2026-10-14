//! Unit and boolean values share the native scalar storage model.

use std::rc::Rc;

use super::{NativeObject, NativeType, Value, native_ops};
use crate::Type;

thread_local! {
    static UNIT: Rc<NativeType> = Rc::new(
        NativeType::new::<()>(Type::Unit)
            .with_copy::<()>()
            .register_method(native_ops::EQUAL, |context| {
                let [other] = context.arguments() else {
                    return Err("unit equality expects one argument".into());
                };
                Ok(Value::from(crate::value::native_bool(other.is_unit())))
            })
            .register_method(native_ops::DISPLAY, |_| Ok(Value::from(crate::value::native_string("()")))),
    );
    static BOOL: Rc<NativeType> = Rc::new(
        NativeType::new::<bool>(Type::Bool)
            .with_copy::<bool>()
            .register_method(native_ops::EQUAL, |context| {
                let [other] = context.arguments() else {
                    return Err("bool equality expects one argument".into());
                };
                let other = other.get_cloned::<bool>().ok().ok_or("bool equality expects bool")?;
                Ok(Value::from(crate::value::native_bool(context.receiver::<bool, _>(|value| *value == other)?)))
            })
            .register_method(native_ops::DISPLAY, |context| {
                Ok(Value::from(crate::value::native_string(context.receiver::<bool, _>(ToString::to_string)?)))
            }),
    );
}

impl Value {
    pub fn is_unit(&self) -> bool {
        matches!(self, Self::Native(object) if object.descriptor().rils_type() == &Type::Unit)
    }
}

pub fn native_unit() -> NativeObject {
    NativeObject::new(UNIT.with(Rc::clone), ()).expect("unit descriptor matches its Rust payload")
}

pub fn native_bool(value: bool) -> NativeObject {
    NativeObject::new(BOOL.with(Rc::clone), value)
        .expect("bool descriptor matches its Rust payload")
}
