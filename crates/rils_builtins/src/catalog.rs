use crate::{BuiltinSignature, IntrinsicId, TypePattern};

const STRING: TypePattern = TypePattern::String;
const BOOL: TypePattern = TypePattern::Bool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinKind {
    Module,
    Primitive,
    Struct,
    Enum,
    Trait,
    Function,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinBackend {
    /// Implemented by the Rils runtime without a bytecode-level intrinsic ID.
    Runtime,
    /// Implemented by the VM through the stable ID.
    Intrinsic(IntrinsicId),
    /// Supplied by an embedding host and protected by the named capability.
    Host(&'static str),
    /// A namespace or semantic declaration with no independently callable body.
    Metadata,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(u16)]
pub enum RuntimeMemberId {
    Clone = 1,
    SequenceLen = 16,
    VecPush = 17,
    VecPop = 18,
    SequenceIntoIter = 19,
    IteratorNext = 20,
    SequenceIsEmpty = 21,
    VecClear = 22,
    VecTruncate = 23,
    SequenceContains = 24,
    VecInsert = 25,
    VecRemove = 26,
    VecSwapRemove = 27,
    VecExtend = 28,
    RangeNext = 32,
    RangeIntoIter = 33,
    ResultIsOk = 48,
    ResultIsErr = 49,
    ResultUnwrap = 50,
    ResultUnwrapOr = 51,
    ResultExpect = 52,
    ResultOk = 53,
    ResultErr = 54,
    ResultUnwrapErr = 55,
    ResultExpectErr = 56,
    ResultMap = 57,
    ResultMapErr = 58,
    ResultAndThen = 59,
    ResultOrElse = 60,
    OptionIsSome = 64,
    OptionIsNone = 65,
    OptionUnwrap = 66,
    OptionUnwrapOr = 67,
    OptionExpect = 68,
    OptionTake = 69,
    OptionOr = 70,
    OptionXor = 71,
    OptionReplace = 72,
    OptionMap = 73,
    OptionAndThen = 74,
    OptionOrElse = 75,
    StringLen = 80,
    StringIsEmpty = 81,
    StringContains = 82,
    StringStartsWith = 83,
    StringEndsWith = 84,
    StringFind = 85,
    StringTrim = 86,
    StringReplace = 87,
}

impl RuntimeMemberId {
    /// Returns the stable core import used by the bytecode backend, when this
    /// runtime member is available to compiled programs.
    pub const fn bytecode_import(self) -> Option<&'static str> {
        Some(match self {
            Self::Clone => "clone",
            Self::SequenceLen | Self::StringLen => "core::sequence::len",
            Self::SequenceIsEmpty | Self::StringIsEmpty => "core::value::is_empty",
            Self::VecPush => "core::vec::push",
            Self::VecPop => "core::vec::pop",
            Self::VecClear => "core::vec::clear",
            Self::VecTruncate => "core::vec::truncate",
            Self::SequenceContains | Self::StringContains => "core::value::contains",
            Self::VecInsert => "core::vec::insert",
            Self::VecRemove => "core::vec::remove",
            Self::VecSwapRemove => "core::vec::swap_remove",
            Self::VecExtend => "core::vec::extend",
            Self::ResultIsOk => "core::result::is_ok",
            Self::ResultIsErr => "core::result::is_err",
            Self::ResultUnwrap | Self::OptionUnwrap => "unwrap",
            Self::ResultUnwrapOr | Self::OptionUnwrapOr => "unwrap_or",
            Self::ResultExpect | Self::OptionExpect => "core::value::expect",
            Self::ResultOk => "core::result::ok",
            Self::ResultErr => "core::result::err",
            Self::ResultUnwrapErr => "core::result::unwrap_err",
            Self::ResultExpectErr => "core::result::expect_err",
            Self::OptionIsSome => "core::option::is_some",
            Self::OptionIsNone => "core::option::is_none",
            Self::OptionTake => "core::option::take",
            Self::OptionOr => "core::option::or",
            Self::OptionXor => "core::option::xor",
            Self::OptionReplace | Self::StringReplace => "core::value::replace",
            Self::StringStartsWith => "core::string::starts_with",
            Self::StringEndsWith => "core::string::ends_with",
            Self::StringFind => "core::string::find",
            Self::StringTrim => "core::string::trim",
            Self::SequenceIntoIter
            | Self::IteratorNext
            | Self::RangeNext
            | Self::RangeIntoIter
            | Self::ResultMap
            | Self::ResultMapErr
            | Self::ResultAndThen
            | Self::ResultOrElse
            | Self::OptionMap
            | Self::OptionAndThen
            | Self::OptionOrElse => {
                return None;
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiverMode {
    Owned,
    Shared,
    Mutable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinMemberKind {
    Field,
    Variant,
    Method,
    AssociatedFunction,
    AssociatedType,
    Constant,
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinMember {
    pub name: &'static str,
    pub kind: BuiltinMemberKind,
    pub signature: Option<BuiltinSignature>,
    pub value_type: Option<TypePattern>,
    pub receiver: Option<ReceiverMode>,
    pub runtime: Option<RuntimeMemberId>,
    pub type_parameters: &'static [&'static str],
    pub documentation: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinDeclaration {
    pub path: &'static str,
    pub kind: BuiltinKind,
    pub type_parameters: &'static [&'static str],
    pub members: &'static [BuiltinMember],
    pub signature: Option<BuiltinSignature>,
    pub backend: BuiltinBackend,
    pub documentation: &'static str,
}

macro_rules! member {
    ($name:literal, $kind:ident, $type:expr, $documentation:literal) => {
        BuiltinMember {
            name: $name,
            kind: BuiltinMemberKind::$kind,
            signature: None,
            value_type: Some($type),
            receiver: None,
            runtime: None,
            type_parameters: &[],
            documentation: $documentation,
        }
    };
    ($name:literal, method $receiver:ident [$($parameter:expr),* $(,)?] -> $result:expr, $runtime:ident, $documentation:literal) => {
        BuiltinMember {
            name: $name,
            kind: BuiltinMemberKind::Method,
            signature: Some(BuiltinSignature { parameters: &[$($parameter),*], result: $result, variadic: false }),
            value_type: None,
            receiver: Some(ReceiverMode::$receiver),
            runtime: Some(RuntimeMemberId::$runtime),
            type_parameters: &[],
            documentation: $documentation,
        }
    };
    ($name:literal, generic [$($generic:literal),+ $(,)?] method $receiver:ident [$($parameter:expr),* $(,)?] -> $result:expr, $runtime:ident, $documentation:literal) => {
        BuiltinMember {
            name: $name,
            kind: BuiltinMemberKind::Method,
            signature: Some(BuiltinSignature { parameters: &[$($parameter),*], result: $result, variadic: false }),
            value_type: None,
            receiver: Some(ReceiverMode::$receiver),
            runtime: Some(RuntimeMemberId::$runtime),
            type_parameters: &[$($generic),+],
            documentation: $documentation,
        }
    };
    ($name:literal, associated [$($parameter:expr),* $(,)?] -> $result:expr, $documentation:literal) => {
        BuiltinMember {
            name: $name,
            kind: BuiltinMemberKind::AssociatedFunction,
            signature: Some(BuiltinSignature { parameters: &[$($parameter),*], result: $result, variadic: false }),
            value_type: None,
            receiver: None,
            runtime: None,
            type_parameters: &[],
            documentation: $documentation,
        }
    };
}
macro_rules! builtin {
    ($path:literal, $kind:ident, [$($generic:literal),* $(,)?], $members:expr, $backend:expr, $documentation:literal) => {
        BuiltinDeclaration { path: $path, kind: BuiltinKind::$kind, type_parameters: &[$($generic),*], members: $members, signature: None, backend: $backend, documentation: $documentation }
    };
    ($path:literal, fn $parameters:expr => $result:expr, $backend:expr, $documentation:literal) => {
        BuiltinDeclaration { path: $path, kind: BuiltinKind::Function, type_parameters: &[], members: &[], signature: Some(BuiltinSignature { parameters: $parameters, result: $result, variadic: false }), backend: $backend, documentation: $documentation }
    };
    ($path:literal, variadic fn => $result:expr, $backend:expr, $documentation:literal) => {
        BuiltinDeclaration { path: $path, kind: BuiltinKind::Function, type_parameters: &[], members: &[], signature: Some(BuiltinSignature { parameters: &[], result: $result, variadic: true }), backend: $backend, documentation: $documentation }
    };
}

const T: TypePattern = TypePattern::Generic("T");
const E: TypePattern = TypePattern::Generic("E");
const U: TypePattern = TypePattern::Generic("U");
const F: TypePattern = TypePattern::Generic("F");
const FN_T_U: TypePattern = TypePattern::Function {
    parameters: &[T],
    result: &U,
};
const FN_T_OPTION_U: TypePattern = TypePattern::Function {
    parameters: &[T],
    result: &TypePattern::Option(&U),
};
const FN_OPTION_T: TypePattern = TypePattern::Function {
    parameters: &[],
    result: &TypePattern::Option(&T),
};
const FN_E_F: TypePattern = TypePattern::Function {
    parameters: &[E],
    result: &F,
};
const FN_T_RESULT_U_E: TypePattern = TypePattern::Function {
    parameters: &[T],
    result: &TypePattern::Result { ok: &U, error: &E },
};
const FN_E_RESULT_T_F: TypePattern = TypePattern::Function {
    parameters: &[E],
    result: &TypePattern::Result { ok: &T, error: &F },
};
const OPTION_MEMBERS: &[BuiltinMember] = &[
    member!(
        "None",
        Variant,
        TypePattern::Unit,
        "An absent optional value."
    ),
    member!("Some", Variant, T, "A present optional value."),
    member!("is_some", method Shared [] -> TypePattern::Bool, OptionIsSome, "Returns true when a value is present."),
    member!("is_none", method Shared [] -> TypePattern::Bool, OptionIsNone, "Returns true when no value is present."),
    member!("unwrap", method Owned [] -> T, OptionUnwrap, "Returns the present value or fails."),
    member!("unwrap_or", method Owned [T] -> T, OptionUnwrapOr, "Returns the present value or the supplied default."),
    member!("expect", method Owned [STRING] -> T, OptionExpect, "Returns the present value or fails with the supplied message."),
    member!("take", method Mutable [] -> TypePattern::SelfType, OptionTake, "Moves the value out, leaving None."),
    member!("or", method Owned [TypePattern::SelfType] -> TypePattern::SelfType, OptionOr, "Returns this Option when present, otherwise the supplied Option."),
    member!("xor", method Owned [TypePattern::SelfType] -> TypePattern::SelfType, OptionXor, "Returns the present Option only when exactly one operand is present."),
    member!("replace", method Mutable [T] -> TypePattern::SelfType, OptionReplace, "Replaces the contained value and returns the previous Option."),
    member!("map", generic ["U"] method Owned [FN_T_U] -> TypePattern::Option(&U), OptionMap, "Maps a present value with the supplied function."),
    member!("and_then", generic ["U"] method Owned [FN_T_OPTION_U] -> TypePattern::Option(&U), OptionAndThen, "Calls the supplied function for a present value and flattens its Option result."),
    member!("or_else", method Owned [FN_OPTION_T] -> TypePattern::SelfType, OptionOrElse, "Calls the supplied fallback only when the Option is None."),
];
const RESULT_MEMBERS: &[BuiltinMember] = &[
    member!("Ok", Variant, T, "A successful result."),
    member!("Err", Variant, E, "A failed result."),
    member!("is_ok", method Shared [] -> TypePattern::Bool, ResultIsOk, "Returns true for Ok."),
    member!("is_err", method Shared [] -> TypePattern::Bool, ResultIsErr, "Returns true for Err."),
    member!("unwrap", method Owned [] -> T, ResultUnwrap, "Returns the Ok value or fails."),
    member!("unwrap_or", method Owned [T] -> T, ResultUnwrapOr, "Returns the Ok value or the supplied default."),
    member!("expect", method Owned [STRING] -> T, ResultExpect, "Returns the Ok value or fails with the supplied message."),
    member!("ok", method Owned [] -> TypePattern::Option(&T), ResultOk, "Converts Result<T, E> to Option<T>."),
    member!("err", method Owned [] -> TypePattern::Option(&E), ResultErr, "Converts Result<T, E> to Option<E>."),
    member!("unwrap_err", method Owned [] -> E, ResultUnwrapErr, "Returns the Err value or fails when the Result is Ok."),
    member!("expect_err", method Owned [STRING] -> E, ResultExpectErr, "Returns the Err value or fails with the supplied message when the Result is Ok."),
    member!("map", generic ["U"] method Owned [FN_T_U] -> TypePattern::Result { ok: &U, error: &E }, ResultMap, "Maps an Ok value while preserving Err."),
    member!("map_err", generic ["F"] method Owned [FN_E_F] -> TypePattern::Result { ok: &T, error: &F }, ResultMapErr, "Maps an Err value while preserving Ok."),
    member!("and_then", generic ["U"] method Owned [FN_T_RESULT_U_E] -> TypePattern::Result { ok: &U, error: &E }, ResultAndThen, "Calls the supplied function for Ok and flattens its Result."),
    member!("or_else", generic ["F"] method Owned [FN_E_RESULT_T_F] -> TypePattern::Result { ok: &T, error: &F }, ResultOrElse, "Calls the supplied fallback for Err and flattens its Result."),
];
const ITERATOR_MEMBERS: &[BuiltinMember] = &[
    member!(
        "Item",
        AssociatedType,
        TypePattern::Unknown,
        "The yielded item type."
    ),
    member!("next", method Mutable [] -> TypePattern::Option(&T), IteratorNext, "Advances the iterator."),
];
const INTO_ITERATOR_MEMBERS: &[BuiltinMember] = &[
    member!(
        "Item",
        AssociatedType,
        TypePattern::Unknown,
        "The yielded item type."
    ),
    member!("into_iter", method Owned [] -> TypePattern::Unknown, SequenceIntoIter, "Consumes a value and creates an iterator."),
];
const CLONE_MEMBERS: &[BuiltinMember] = &[
    member!("clone", method Shared [] -> TypePattern::SelfType, Clone, "Explicitly duplicates an owned value."),
];
const VEC_MEMBERS: &[BuiltinMember] = &[
    member!("new", associated [] -> TypePattern::SelfType, "Creates an empty Vec."),
    member!("from", associated [TypePattern::Unknown] -> TypePattern::SelfType, "Creates a Vec from an owned array."),
    member!("len", method Shared [] -> TypePattern::Usize, SequenceLen, "Returns the element count."),
    member!("is_empty", method Shared [] -> TypePattern::Bool, SequenceIsEmpty, "Returns true when the Vec has no elements."),
    member!("push", method Mutable [T] -> TypePattern::Unit, VecPush, "Appends one element."),
    member!("pop", method Mutable [] -> TypePattern::Option(&T), VecPop, "Removes and returns the last element."),
    member!("clear", method Mutable [] -> TypePattern::Unit, VecClear, "Removes all elements."),
    member!("truncate", method Mutable [TypePattern::Usize] -> TypePattern::Unit, VecTruncate, "Shortens the Vec to at most the supplied length."),
    member!("contains", method Shared [TypePattern::Reference { mutable: false, inner: &T }] -> TypePattern::Bool, SequenceContains, "Returns true when an equal element is present."),
    member!("insert", method Mutable [TypePattern::Usize, T] -> TypePattern::Unit, VecInsert, "Inserts an element at the supplied index."),
    member!("remove", method Mutable [TypePattern::Usize] -> T, VecRemove, "Removes and returns the element at the supplied index."),
    member!("swap_remove", method Mutable [TypePattern::Usize] -> T, VecSwapRemove, "Removes an element by replacing it with the final element."),
    member!("extend", method Mutable [TypePattern::SelfType] -> TypePattern::Unit, VecExtend, "Moves every element from another Vec into this Vec."),
    member!("into_iter", method Owned [] -> TypePattern::Unknown, SequenceIntoIter, "Consumes the Vec and creates an iterator."),
];
const ARRAY_MEMBERS: &[BuiltinMember] = &[
    member!("len", method Shared [] -> TypePattern::Usize, SequenceLen, "Returns the element count."),
    member!("is_empty", method Shared [] -> TypePattern::Bool, SequenceIsEmpty, "Returns true when the array has no elements."),
    member!("contains", method Shared [TypePattern::Reference { mutable: false, inner: &T }] -> TypePattern::Bool, SequenceContains, "Returns true when an equal element is present."),
    member!("into_iter", method Owned [] -> TypePattern::Unknown, SequenceIntoIter, "Consumes the array and creates an iterator."),
];
const STRING_MEMBERS: &[BuiltinMember] = &[
    member!("len", method Shared [] -> TypePattern::Usize, StringLen, "Returns the UTF-8 byte length."),
    member!("is_empty", method Shared [] -> TypePattern::Bool, StringIsEmpty, "Returns true when the string has no bytes."),
    member!("contains", method Shared [STRING] -> TypePattern::Bool, StringContains, "Returns true when the substring is present."),
    member!("starts_with", method Shared [STRING] -> TypePattern::Bool, StringStartsWith, "Tests the string prefix."),
    member!("ends_with", method Shared [STRING] -> TypePattern::Bool, StringEndsWith, "Tests the string suffix."),
    member!("find", method Shared [STRING] -> TypePattern::Option(&TypePattern::Usize), StringFind, "Returns the byte offset of the first match."),
    member!("trim", method Shared [] -> STRING, StringTrim, "Returns a string without leading or trailing whitespace."),
    member!("replace", method Shared [STRING, STRING] -> STRING, StringReplace, "Replaces every matching substring."),
];
const RANGE_MEMBERS: &[BuiltinMember] = &[
    member!("next", method Mutable [] -> TypePattern::Option(&T), RangeNext, "Advances the range."),
    member!("into_iter", method Owned [] -> TypePattern::SelfType, RangeIntoIter, "Consumes the range and creates its iterator."),
];
const IO_ERROR: TypePattern = TypePattern::Named {
    path: "std::io::Error",
    arguments: &[],
};
const RESULT_STRING_IO: TypePattern = TypePattern::Result {
    ok: &STRING,
    error: &IO_ERROR,
};
const RESULT_UNIT_IO: TypePattern = TypePattern::Result {
    ok: &TypePattern::Unit,
    error: &IO_ERROR,
};
const RESULT_BOOL_IO: TypePattern = TypePattern::Result {
    ok: &BOOL,
    error: &IO_ERROR,
};
const VEC_STRING: TypePattern = TypePattern::Named {
    path: "Vec",
    arguments: &[STRING],
};
const RESULT_VEC_STRING_IO: TypePattern = TypePattern::Result {
    ok: &VEC_STRING,
    error: &IO_ERROR,
};
const OPTION_T: TypePattern = TypePattern::Option(&T);
const RESULT_T_E: TypePattern = TypePattern::Result { ok: &T, error: &E };
const REF_T: TypePattern = TypePattern::Reference {
    mutable: false,
    inner: &T,
};

/// Non-intrinsic built-ins. Integer primitives use `IntegerType::ALL` plus
/// `INTEGER_INTRINSICS`, avoiding twelve duplicate declarations.
pub const BUILTINS: &[BuiltinDeclaration] = &[
    builtin!(
        "core",
        Module,
        [],
        &[],
        BuiltinBackend::Metadata,
        "Host-independent core APIs."
    ),
    builtin!(
        "std",
        Module,
        [],
        &[],
        BuiltinBackend::Metadata,
        "Host and platform APIs."
    ),
    builtin!(
        "prelude",
        Module,
        [],
        &[],
        BuiltinBackend::Metadata,
        "Names imported into every script."
    ),
    builtin!(
        "string",
        Primitive,
        [],
        STRING_MEMBERS,
        BuiltinBackend::Runtime,
        "An owned UTF-8 string."
    ),
    builtin!(
        "Option",
        Enum,
        ["T"],
        OPTION_MEMBERS,
        BuiltinBackend::Runtime,
        "An optional value."
    ),
    builtin!(
        "Result",
        Enum,
        ["T", "E"],
        RESULT_MEMBERS,
        BuiltinBackend::Runtime,
        "A success or error value."
    ),
    builtin!(
        "Vec",
        Struct,
        ["T"],
        VEC_MEMBERS,
        BuiltinBackend::Runtime,
        "A growable owned sequence."
    ),
    builtin!(
        "Array",
        Primitive,
        ["T"],
        ARRAY_MEMBERS,
        BuiltinBackend::Runtime,
        "A fixed-length owned sequence."
    ),
    builtin!(
        "Range",
        Struct,
        ["T"],
        RANGE_MEMBERS,
        BuiltinBackend::Runtime,
        "A half-open integer range."
    ),
    builtin!(
        "Copy",
        Trait,
        [],
        &[],
        BuiltinBackend::Metadata,
        "Values duplicated by ordinary reads."
    ),
    builtin!(
        "Clone",
        Trait,
        [],
        CLONE_MEMBERS,
        BuiltinBackend::Metadata,
        "Explicit owned duplication."
    ),
    builtin!(
        "Iterator",
        Trait,
        [],
        ITERATOR_MEMBERS,
        BuiltinBackend::Metadata,
        "A stateful sequence producer."
    ),
    builtin!(
        "IntoIterator",
        Trait,
        [],
        INTO_ITERATOR_MEMBERS,
        BuiltinBackend::Metadata,
        "Conversion into an iterator."
    ),
    builtin!("Some", fn &[T] => OPTION_T, BuiltinBackend::Runtime, "Constructs a present Option value."),
    builtin!("Ok", fn &[T] => RESULT_T_E, BuiltinBackend::Runtime, "Constructs a successful Result value."),
    builtin!("Err", fn &[E] => RESULT_T_E, BuiltinBackend::Runtime, "Constructs an error Result value."),
    builtin!("is_some", fn &[OPTION_T] => TypePattern::Bool, BuiltinBackend::Runtime, "Tests whether an Option contains a value."),
    builtin!("is_none", fn &[OPTION_T] => TypePattern::Bool, BuiltinBackend::Runtime, "Tests whether an Option is empty."),
    builtin!("is_ok", fn &[RESULT_T_E] => TypePattern::Bool, BuiltinBackend::Runtime, "Tests whether a Result is Ok."),
    builtin!("is_err", fn &[RESULT_T_E] => TypePattern::Bool, BuiltinBackend::Runtime, "Tests whether a Result is Err."),
    builtin!("unwrap", fn &[TypePattern::Unknown] => TypePattern::Unknown, BuiltinBackend::Runtime, "Extracts an Option or Result success value."),
    builtin!("unwrap_or", fn &[TypePattern::Unknown, T] => T, BuiltinBackend::Runtime, "Extracts a value or returns a default."),
    builtin!("clone", fn &[REF_T] => T, BuiltinBackend::Runtime, "Explicitly duplicates an owned value."),
    builtin!("std::io::read_line", fn &[] => RESULT_STRING_IO, BuiltinBackend::Host("std::io"), "Reads one line from standard input."),
    builtin!("std::io::print", variadic fn => TypePattern::Unit, BuiltinBackend::Host("std::io"), "Prints values without a trailing newline."),
    builtin!("std::io::println", variadic fn => TypePattern::Unit, BuiltinBackend::Host("std::io"), "Prints values followed by a newline."),
    builtin!("std::io::write", fn &[TypePattern::Unknown] => RESULT_UNIT_IO, BuiltinBackend::Host("std::io"), "Writes a value to standard output."),
    builtin!("std::io::write_line", fn &[TypePattern::Unknown] => RESULT_UNIT_IO, BuiltinBackend::Host("std::io"), "Writes a value and a newline."),
    builtin!("std::io::flush", fn &[] => RESULT_UNIT_IO, BuiltinBackend::Host("std::io"), "Flushes standard output."),
    builtin!("std::fs::read_to_string", fn &[STRING] => RESULT_STRING_IO, BuiltinBackend::Host("std::fs"), "Reads a UTF-8 file."),
    builtin!("std::fs::write", fn &[STRING, STRING] => RESULT_UNIT_IO, BuiltinBackend::Host("std::fs"), "Writes a UTF-8 file."),
    builtin!("std::fs::append", fn &[STRING, STRING] => RESULT_UNIT_IO, BuiltinBackend::Host("std::fs"), "Appends to a UTF-8 file."),
    builtin!("std::fs::try_exists", fn &[STRING] => RESULT_BOOL_IO, BuiltinBackend::Host("std::fs"), "Checks whether a path exists."),
    builtin!("std::fs::create_dir_all", fn &[STRING] => RESULT_UNIT_IO, BuiltinBackend::Host("std::fs"), "Creates a directory tree."),
    builtin!("std::fs::remove_file", fn &[STRING] => RESULT_UNIT_IO, BuiltinBackend::Host("std::fs"), "Removes a file."),
    builtin!("std::fs::remove_dir", fn &[STRING] => RESULT_UNIT_IO, BuiltinBackend::Host("std::fs"), "Removes an empty directory."),
    builtin!("std::fs::read_dir", fn &[STRING] => RESULT_VEC_STRING_IO, BuiltinBackend::Host("std::fs"), "Lists directory entries."),
];

pub fn builtin(path: &str) -> Option<&'static BuiltinDeclaration> {
    BUILTINS.iter().find(|item| item.path == path)
}
pub fn builtin_function(path: &str) -> Option<&'static BuiltinDeclaration> {
    builtin(path).filter(|item| item.kind == BuiltinKind::Function)
}

pub fn builtin_member(owner: &str, name: &str) -> Option<&'static BuiltinMember> {
    builtin(owner)?
        .members
        .iter()
        .find(|member| member.name == name)
}

pub fn runtime_member(id: RuntimeMemberId) -> Option<(&'static str, &'static BuiltinMember)> {
    BUILTINS.iter().find_map(|owner| {
        owner
            .members
            .iter()
            .find(|member| member.runtime == Some(id))
            .map(|member| (owner.path, member))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::INTEGER_INTRINSICS;
    #[test]
    fn declarations_have_unique_stable_identity() {
        for (index, left) in INTEGER_INTRINSICS.iter().enumerate() {
            assert!(
                INTEGER_INTRINSICS[index + 1..]
                    .iter()
                    .all(|right| left.id != right.id)
            );
            assert!(
                INTEGER_INTRINSICS[index + 1..]
                    .iter()
                    .all(|right| left.kind != right.kind || left.name != right.name)
            );
        }
        for (index, left) in BUILTINS.iter().enumerate() {
            assert!(
                BUILTINS[index + 1..]
                    .iter()
                    .all(|right| left.path != right.path)
            );
            assert!(!left.documentation.is_empty());
            for (member_index, member) in left.members.iter().enumerate() {
                assert!(!member.documentation.is_empty());
                assert!(
                    left.members[member_index + 1..]
                        .iter()
                        .all(|right| member.name != right.name),
                    "duplicate member {}::{}",
                    left.path,
                    member.name
                );
                if member.kind == BuiltinMemberKind::Method {
                    assert!(
                        member.signature.is_some(),
                        "{}::{} has no signature",
                        left.path,
                        member.name
                    );
                    assert!(
                        member.receiver.is_some(),
                        "{}::{} has no receiver",
                        left.path,
                        member.name
                    );
                }
            }
        }
    }

    #[test]
    fn bytecode_imports_are_consistent_for_shared_method_names() {
        for declaration in BUILTINS {
            for member in declaration.members {
                let Some(import) = member.runtime.and_then(RuntimeMemberId::bytecode_import) else {
                    continue;
                };
                for other in BUILTINS
                    .iter()
                    .flat_map(|declaration| declaration.members)
                    .filter(|other| other.name == member.name)
                {
                    if let Some(other_import) =
                        other.runtime.and_then(RuntimeMemberId::bytecode_import)
                    {
                        assert_eq!(import, other_import, "method `{}`", member.name);
                    }
                }
            }
        }
    }

    #[test]
    fn runtime_members_are_discoverable_from_the_catalog() {
        for id in [
            RuntimeMemberId::Clone,
            RuntimeMemberId::SequenceLen,
            RuntimeMemberId::VecPush,
            RuntimeMemberId::VecPop,
            RuntimeMemberId::SequenceIsEmpty,
            RuntimeMemberId::VecClear,
            RuntimeMemberId::VecTruncate,
            RuntimeMemberId::SequenceContains,
            RuntimeMemberId::VecInsert,
            RuntimeMemberId::VecRemove,
            RuntimeMemberId::VecSwapRemove,
            RuntimeMemberId::VecExtend,
            RuntimeMemberId::SequenceIntoIter,
            RuntimeMemberId::IteratorNext,
            RuntimeMemberId::RangeNext,
            RuntimeMemberId::RangeIntoIter,
            RuntimeMemberId::ResultIsOk,
            RuntimeMemberId::ResultIsErr,
            RuntimeMemberId::ResultUnwrap,
            RuntimeMemberId::ResultUnwrapOr,
            RuntimeMemberId::ResultExpect,
            RuntimeMemberId::ResultOk,
            RuntimeMemberId::ResultErr,
            RuntimeMemberId::ResultUnwrapErr,
            RuntimeMemberId::ResultExpectErr,
            RuntimeMemberId::ResultMap,
            RuntimeMemberId::ResultMapErr,
            RuntimeMemberId::ResultAndThen,
            RuntimeMemberId::ResultOrElse,
            RuntimeMemberId::OptionIsSome,
            RuntimeMemberId::OptionIsNone,
            RuntimeMemberId::OptionUnwrap,
            RuntimeMemberId::OptionUnwrapOr,
            RuntimeMemberId::OptionExpect,
            RuntimeMemberId::OptionTake,
            RuntimeMemberId::OptionOr,
            RuntimeMemberId::OptionXor,
            RuntimeMemberId::OptionReplace,
            RuntimeMemberId::OptionMap,
            RuntimeMemberId::OptionAndThen,
            RuntimeMemberId::OptionOrElse,
            RuntimeMemberId::StringLen,
            RuntimeMemberId::StringIsEmpty,
            RuntimeMemberId::StringContains,
            RuntimeMemberId::StringStartsWith,
            RuntimeMemberId::StringEndsWith,
            RuntimeMemberId::StringFind,
            RuntimeMemberId::StringTrim,
            RuntimeMemberId::StringReplace,
        ] {
            assert!(
                runtime_member(id).is_some(),
                "missing runtime member {id:?}"
            );
        }
    }
}
