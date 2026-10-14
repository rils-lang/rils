use std::fmt;

use super::Value;

#[path = "display/native_view.rs"]
mod native_view;

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(callable) = self.callable() {
            return f.write_str(&callable.description());
        }
        match self {
            __reference_value if __reference_value.as_reference().is_some() => {
                let reference = __reference_value
                    .as_reference()
                    .expect("checked reference handle");
                native_view::display_reference(&reference, f)
            }

            Self::HostObject(object) => write!(f, "<{}>", object.type_definition.name),
            Self::Native(object) => write!(f, "{}", super::native_ops::display(object)),
            Self::Dynamic(object) => native_view::display(object, f),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(callable) = self.callable() {
            return f.write_str(&callable.description());
        }
        if f.alternate() {
            return match self {
                __reference_value if __reference_value.as_reference().is_some() => {
                    let reference = __reference_value
                        .as_reference()
                        .expect("checked reference handle");
                    native_view::debug_reference(&reference, f)
                }
                Self::Dynamic(object) => native_view::debug(object, f),

                Self::Native(object) if object.descriptor().rils_type() == &crate::Type::String => {
                    write!(
                        f,
                        "{:#?}",
                        self.get_cloned::<String>().ok().unwrap_or_default()
                    )
                }
                Self::Native(object) if object.descriptor().rils_type() == &crate::Type::Char => {
                    write!(
                        f,
                        "{:#?}",
                        self.get_cloned::<char>().expect("native char payload")
                    )
                }
                _ => write!(f, "{self}"),
            };
        }
        match self {
            __reference_value if __reference_value.as_reference().is_some() => {
                let reference = __reference_value
                    .as_reference()
                    .expect("checked reference handle");
                native_view::debug_reference(&reference, f)
            }

            Self::Native(object) if object.descriptor().rils_type() == &crate::Type::String => {
                write!(
                    f,
                    "{:?}",
                    self.get_cloned::<String>().ok().unwrap_or_default()
                )
            }
            Self::Native(object) if object.descriptor().rils_type() == &crate::Type::Char => {
                write!(
                    f,
                    "{:?}",
                    self.get_cloned::<char>().expect("native char payload")
                )
            }
            Self::Dynamic(object) => native_view::debug(object, f),
            _ => write!(f, "{self}"),
        }
    }
}
