//! Native methods for the built-in owned UTF-8 string.

use super::prelude::Option;
use rils_builtins_macros::decl_rils;

/// An owned iterator of values produced by a string method.
pub struct Iterator<T>(pub Vec<T>);

fn optional<T>(value: std::option::Option<T>) -> Option<T> {
    match value {
        Some(value) => Option::Some(value),
        None => Option::None,
    }
}

#[decl_rils(core::string)]
mod native {
    use super::{Iterator, Option, optional};

    /// An owned UTF-8 string.
    #[derive(Clone)]
    #[rils_impl(Clone)]
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
            Iterator(self.0.chars().collect())
        }
        /// Iterates over UTF-8 bytes.
        #[export_rils]
        pub fn bytes(&self) -> Iterator<u8> {
            Iterator(self.0.bytes().collect())
        }
        /// Iterates over lines without their terminators.
        #[export_rils]
        pub fn lines(&self) -> Iterator<String> {
            Iterator(self.0.lines().map(|line| Self(line.to_owned())).collect())
        }
        /// Iterates over substrings separated by the pattern.
        #[export_rils]
        pub fn split(&self, pattern: String) -> Iterator<String> {
            Iterator(
                self.0
                    .split(&pattern.0)
                    .map(|part| Self(part.to_owned()))
                    .collect(),
            )
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
}

pub use native::String;
