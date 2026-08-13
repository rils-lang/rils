//! Static descriptions of APIs shipped as part of Rils.
//!
//! Nothing in this crate performs I/O or executes script code. Tooling and
//! runtimes share these declarations while choosing their own implementation
//! strategy for runtime, intrinsic and host-backed items.

use core::fmt;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum IntegerType {
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
}

impl IntegerType {
    pub const ALL: [Self; 12] = [
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::I128,
        Self::Isize,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::U128,
        Self::Usize,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::Isize => "isize",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::Usize => "usize",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub const fn is_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128 | Self::Isize
        )
    }
    pub const fn bits(self) -> u32 {
        match self {
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 => 32,
            Self::I64 | Self::U64 => 64,
            Self::I128 | Self::U128 => 128,
            Self::Isize | Self::Usize => usize::BITS,
        }
    }
    pub const fn can_cast_losslessly_to(self, target: Self) -> bool {
        match (self.is_signed(), target.is_signed()) {
            (true, true) | (false, false) | (true, false) => target.bits() >= self.bits(),
            (false, true) => target.bits() > self.bits(),
        }
    }
    pub const fn can_represent_all(self, source: Self) -> bool {
        source.can_cast_losslessly_to(self)
    }
}

impl fmt::Display for IntegerType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// A recursive type expression independent of the frontend's concrete `Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypePattern {
    SelfType,
    Generic(&'static str),
    AnyInteger,
    Unknown,
    Unit,
    Bool,
    String,
    F32,
    F64,
    Usize,
    Named {
        path: &'static str,
        arguments: &'static [TypePattern],
    },
    Option(&'static TypePattern),
    Result {
        ok: &'static TypePattern,
        error: &'static TypePattern,
    },
    Tuple(&'static [TypePattern]),
    Function {
        parameters: &'static [TypePattern],
        result: &'static TypePattern,
    },
    Reference {
        mutable: bool,
        inner: &'static TypePattern,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinSignature {
    pub parameters: &'static [TypePattern],
    pub result: TypePattern,
    pub variadic: bool,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(u16)]
pub enum IntrinsicId {
    IntegerTryFrom = 1,
    IntegerToF32 = 2,
    IntegerToF64 = 3,
    IntegerCheckedAdd = 16,
    IntegerCheckedSub = 17,
    IntegerCheckedMul = 18,
    IntegerCheckedDiv = 19,
    IntegerCheckedRem = 20,
    IntegerWrappingAdd = 32,
    IntegerWrappingSub = 33,
    IntegerWrappingMul = 34,
    IntegerSaturatingAdd = 48,
    IntegerSaturatingSub = 49,
    IntegerSaturatingMul = 50,
    IntegerOverflowingAdd = 64,
    IntegerOverflowingSub = 65,
    IntegerOverflowingMul = 66,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntrinsicKind {
    Method,
    AssociatedFunction,
}

#[derive(Clone, Copy, Debug)]
pub struct IntrinsicDeclaration {
    pub id: IntrinsicId,
    pub name: &'static str,
    pub kind: IntrinsicKind,
    pub signature: BuiltinSignature,
    pub documentation: &'static str,
}

const SELF: TypePattern = TypePattern::SelfType;
const BOOL: TypePattern = TypePattern::Bool;
const STRING: TypePattern = TypePattern::String;
const OPTION_SELF: TypePattern = TypePattern::Option(&SELF);
const RESULT_SELF_STRING: TypePattern = TypePattern::Result {
    ok: &SELF,
    error: &STRING,
};
const SELF_BOOL: &[TypePattern] = &[SELF, BOOL];

macro_rules! intrinsic {
    ($id:ident, $name:literal, $kind:ident, [$($parameter:expr),* $(,)?] -> $result:expr, $documentation:literal) => {
        IntrinsicDeclaration {
            id: IntrinsicId::$id,
            name: $name,
            kind: IntrinsicKind::$kind,
            signature: BuiltinSignature {
                parameters: &[$($parameter),*], result: $result, variadic: false,
            },
            documentation: $documentation,
        }
    };
}

pub const INTEGER_INTRINSICS: &[IntrinsicDeclaration] = &[
    intrinsic!(IntegerTryFrom, "try_from", AssociatedFunction, [TypePattern::AnyInteger] -> RESULT_SELF_STRING, "Converts an integer when its value is representable by the target type."),
    intrinsic!(IntegerToF32, "to_f32", Method, [] -> TypePattern::F32, "Converts to f32, allowing IEEE-754 precision rounding."),
    intrinsic!(IntegerToF64, "to_f64", Method, [] -> TypePattern::F64, "Converts to f64, allowing IEEE-754 precision rounding."),
    intrinsic!(IntegerCheckedAdd, "checked_add", Method, [SELF] -> OPTION_SELF, "Returns None on overflow."),
    intrinsic!(IntegerCheckedSub, "checked_sub", Method, [SELF] -> OPTION_SELF, "Returns None on overflow."),
    intrinsic!(IntegerCheckedMul, "checked_mul", Method, [SELF] -> OPTION_SELF, "Returns None on overflow."),
    intrinsic!(IntegerCheckedDiv, "checked_div", Method, [SELF] -> OPTION_SELF, "Returns None on division failure or overflow."),
    intrinsic!(IntegerCheckedRem, "checked_rem", Method, [SELF] -> OPTION_SELF, "Returns None on remainder failure or overflow."),
    intrinsic!(IntegerWrappingAdd, "wrapping_add", Method, [SELF] -> SELF, "Adds with two's-complement wrapping."),
    intrinsic!(IntegerWrappingSub, "wrapping_sub", Method, [SELF] -> SELF, "Subtracts with two's-complement wrapping."),
    intrinsic!(IntegerWrappingMul, "wrapping_mul", Method, [SELF] -> SELF, "Multiplies with two's-complement wrapping."),
    intrinsic!(IntegerSaturatingAdd, "saturating_add", Method, [SELF] -> SELF, "Adds while saturating at the numeric bounds."),
    intrinsic!(IntegerSaturatingSub, "saturating_sub", Method, [SELF] -> SELF, "Subtracts while saturating at the numeric bounds."),
    intrinsic!(IntegerSaturatingMul, "saturating_mul", Method, [SELF] -> SELF, "Multiplies while saturating at the numeric bounds."),
    intrinsic!(IntegerOverflowingAdd, "overflowing_add", Method, [SELF] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped sum and whether overflow occurred."),
    intrinsic!(IntegerOverflowingSub, "overflowing_sub", Method, [SELF] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped difference and whether overflow occurred."),
    intrinsic!(IntegerOverflowingMul, "overflowing_mul", Method, [SELF] -> TypePattern::Tuple(SELF_BOOL), "Returns the wrapped product and whether overflow occurred."),
];

pub fn integer_method(name: &str) -> Option<&'static IntrinsicDeclaration> {
    INTEGER_INTRINSICS
        .iter()
        .find(|item| item.kind == IntrinsicKind::Method && item.name == name)
}
pub fn integer_associated_function(name: &str) -> Option<&'static IntrinsicDeclaration> {
    INTEGER_INTRINSICS
        .iter()
        .find(|item| item.kind == IntrinsicKind::AssociatedFunction && item.name == name)
}

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
    RangeNext = 32,
    RangeIntoIter = 33,
    ResultIsOk = 48,
    ResultIsErr = 49,
    ResultUnwrap = 50,
    ResultUnwrapOr = 51,
    OptionIsSome = 64,
    OptionIsNone = 65,
    OptionUnwrap = 66,
    OptionUnwrapOr = 67,
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
];
const RESULT_MEMBERS: &[BuiltinMember] = &[
    member!("Ok", Variant, T, "A successful result."),
    member!("Err", Variant, E, "A failed result."),
    member!("is_ok", method Shared [] -> TypePattern::Bool, ResultIsOk, "Returns true for Ok."),
    member!("is_err", method Shared [] -> TypePattern::Bool, ResultIsErr, "Returns true for Err."),
    member!("unwrap", method Owned [] -> T, ResultUnwrap, "Returns the Ok value or fails."),
    member!("unwrap_or", method Owned [T] -> T, ResultUnwrapOr, "Returns the Ok value or the supplied default."),
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
    member!("push", method Mutable [T] -> TypePattern::Unit, VecPush, "Appends one element."),
    member!("pop", method Mutable [] -> TypePattern::Option(&T), VecPop, "Removes and returns the last element."),
    member!("into_iter", method Owned [] -> TypePattern::Unknown, SequenceIntoIter, "Consumes the Vec and creates an iterator."),
];
const ARRAY_MEMBERS: &[BuiltinMember] = &[
    member!("len", method Shared [] -> TypePattern::Usize, SequenceLen, "Returns the element count."),
    member!("into_iter", method Owned [] -> TypePattern::Unknown, SequenceIntoIter, "Consumes the array and creates an iterator."),
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
    fn runtime_members_are_discoverable_from_the_catalog() {
        for id in [
            RuntimeMemberId::Clone,
            RuntimeMemberId::SequenceLen,
            RuntimeMemberId::VecPush,
            RuntimeMemberId::VecPop,
            RuntimeMemberId::SequenceIntoIter,
            RuntimeMemberId::IteratorNext,
            RuntimeMemberId::RangeNext,
            RuntimeMemberId::RangeIntoIter,
            RuntimeMemberId::ResultIsOk,
            RuntimeMemberId::ResultIsErr,
            RuntimeMemberId::ResultUnwrap,
            RuntimeMemberId::ResultUnwrapOr,
            RuntimeMemberId::OptionIsSome,
            RuntimeMemberId::OptionIsNone,
            RuntimeMemberId::OptionUnwrap,
            RuntimeMemberId::OptionUnwrapOr,
        ] {
            assert!(
                runtime_member(id).is_some(),
                "missing runtime member {id:?}"
            );
        }
    }
}
