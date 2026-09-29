//! Native methods for the built-in owned UTF-8 string.

use super::{iterator::Iter, prelude::Option};
use rils_stdlib_macros::decl_rils;

/// The exported string methods use the common iterator adapter.
pub type Iterator<T> = Iter<T>;

fn optional<T>(value: std::option::Option<T>) -> Option<T> {
    match value {
        Some(value) => Option::Some(value),
        None => Option::None,
    }
}

#[decl_rils(core::string)]
mod native {
    use super::{Iter, Iterator, Option, optional};

    /// An owned UTF-8 string.
    #[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[rils_impl(Clone, Default, Eq, Hash)]
    #[rils_struct]
    pub struct String(std::string::String);

    impl String {
        /// Returns the UTF-8 byte length.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns true when the string has no bytes.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        /// Returns true when the substring is present.
        #[export_rils]
        pub fn contains(&self, pattern: String) -> bool {
            self.0.contains(&pattern.0)
        }
        /// Tests the string prefix.
        #[export_rils]
        pub fn starts_with(&self, prefix: String) -> bool {
            self.0.starts_with(&prefix.0)
        }
        /// Tests the string suffix.
        #[export_rils]
        pub fn ends_with(&self, suffix: String) -> bool {
            self.0.ends_with(&suffix.0)
        }
        /// Returns the byte offset of the first match.
        #[export_rils]
        pub fn find(&self, pattern: String) -> Option<usize> {
            optional(self.0.find(&pattern.0))
        }
        /// Returns a string without leading or trailing whitespace.
        #[export_rils]
        pub fn trim(&self) -> String {
            Self(self.0.trim().to_owned())
        }
        /// Replaces every matching substring.
        #[export_rils]
        pub fn replace(&self, from: String, to: String) -> String {
            Self(self.0.replace(&from.0, &to.0))
        }
        /// Returns a string without leading whitespace.
        #[export_rils]
        pub fn trim_start(&self) -> String {
            Self(self.0.trim_start().to_owned())
        }
        /// Returns a string without trailing whitespace.
        #[export_rils]
        pub fn trim_end(&self) -> String {
            Self(self.0.trim_end().to_owned())
        }
        /// Returns the Unicode lowercase mapping.
        #[export_rils]
        pub fn to_lowercase(&self) -> String {
            Self(self.0.to_lowercase())
        }
        /// Returns the Unicode uppercase mapping.
        #[export_rils]
        pub fn to_uppercase(&self) -> String {
            Self(self.0.to_uppercase())
        }
        /// Repeats the string n times.
        #[export_rils]
        pub fn repeat(&self, count: usize) -> String {
            Self(self.0.repeat(count))
        }
        /// Returns the byte offset of the final match.
        #[export_rils]
        pub fn rfind(&self, pattern: String) -> Option<usize> {
            optional(self.0.rfind(&pattern.0))
        }
        /// Removes one matching prefix.
        #[export_rils]
        pub fn strip_prefix(&self, prefix: String) -> Option<String> {
            optional(
                self.0
                    .strip_prefix(&prefix.0)
                    .map(|value| Self(value.to_owned())),
            )
        }
        /// Removes one matching suffix.
        #[export_rils]
        pub fn strip_suffix(&self, suffix: String) -> Option<String> {
            optional(
                self.0
                    .strip_suffix(&suffix.0)
                    .map(|value| Self(value.to_owned())),
            )
        }
        /// Iterates over Unicode scalar values.
        #[export_rils]
        pub fn chars(&self) -> Iterator<char> {
            let source = self.0.clone();
            let mut offset = 0;
            Iter::from_generator(std::iter::from_fn(move || {
                let value = source.get(offset..)?.chars().next()?;
                offset += value.len_utf8();
                Some(value)
            }))
        }
        /// Iterates over UTF-8 bytes.
        #[export_rils]
        pub fn bytes(&self) -> Iterator<u8> {
            let source = self.0.clone();
            let mut offset = 0;
            Iter::from_generator(std::iter::from_fn(move || {
                let value = source.as_bytes().get(offset).copied()?;
                offset += 1;
                Some(value)
            }))
        }
        /// Iterates over lines without their terminators.
        #[export_rils]
        pub fn lines(&self) -> Iterator<String> {
            let source = self.0.clone();
            let mut offset = 0;
            Iter::from_generator(std::iter::from_fn(move || {
                let rest = source.get(offset..)?;
                if rest.is_empty() {
                    return None;
                }
                let end = rest.find('\n').unwrap_or(rest.len());
                let line = &rest[..end];
                offset += end + usize::from(end < rest.len());
                Some(Self(line.strip_suffix('\r').unwrap_or(line).to_owned()))
            }))
        }
        /// Iterates over substrings separated by the pattern.
        #[export_rils]
        pub fn split(&self, pattern: String) -> Iterator<String> {
            let source = self.0.clone();
            let pattern = pattern.0;
            let mut offset = 0;
            let mut started = false;
            let mut finished = false;
            Iter::from_generator(std::iter::from_fn(move || {
                if finished {
                    return None;
                }
                if pattern.is_empty() {
                    if !started {
                        started = true;
                        return Some(Self(std::string::String::new()));
                    }
                    if let Some(value) = source[offset..].chars().next() {
                        offset += value.len_utf8();
                        return Some(Self(value.to_string()));
                    }
                    finished = true;
                    return Some(Self(std::string::String::new()));
                }
                let rest = &source[offset..];
                if let Some(end) = rest.find(&pattern) {
                    offset += end + pattern.len();
                    Some(Self(rest[..end].to_owned()))
                } else {
                    finished = true;
                    Some(Self(rest.to_owned()))
                }
            }))
        }
    }

    impl From<std::string::String> for String {
        fn from(value: std::string::String) -> Self {
            Self(value)
        }
    }

    impl From<String> for std::string::String {
        fn from(value: String) -> Self {
            value.0
        }
    }

    impl AsRef<std::string::String> for String {
        fn as_ref(&self) -> &std::string::String {
            &self.0
        }
    }
}

pub use native::String;

fn native_element_matches(ty: &rils_syntax::Type) -> bool {
    ty == &rils_syntax::Type::String
}

fn clone_borrowed_element(
    item: rils_value::DynamicValueRef<'_>,
) -> Result<rils_value::DynamicValue, std::string::String> {
    let text = item.with_rust::<String, _>(Clone::clone)?;
    rils_value::DynamicValue::from_rust(item.layout()?, text)
}

pub const NATIVE_ELEMENT_STRING: rils_native::ElementRegistration =
    rils_native::ElementRegistration {
        matches: native_element_matches,
        clone_borrowed: clone_borrowed_element,
    };

fn native_string_key(
    item: rils_value::DynamicValueRef<'_>,
) -> Result<rils_native::NativeKey, std::string::String> {
    item.with_rust::<String, _>(|value| rils_native::NativeKey::String(value.as_ref().clone()))
}

pub const NATIVE_KEY_STRING: rils_native::KeyRegistration = rils_native::KeyRegistration {
    matches: native_element_matches,
    key: native_string_key,
};
