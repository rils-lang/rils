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
];
const RESULT_MEMBERS: &[BuiltinMember] = &[
    member!("Ok", Variant, T, "A successful result."),
    member!("Err", Variant, E, "A failed result."),
];
const ITERATOR_MEMBERS: &[BuiltinMember] = &[member!(
    "Item",
    AssociatedType,
    TypePattern::Unknown,
    "The yielded item type."
)];
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
        &[],
        BuiltinBackend::Runtime,
        "A growable owned sequence."
    ),
    builtin!(
        "Range",
        Struct,
        ["T"],
        &[],
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
        &[],
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
        ITERATOR_MEMBERS,
        BuiltinBackend::Metadata,
        "Conversion into an iterator."
    ),
    builtin!("std::io::read_line", fn &[] => RESULT_STRING_IO, BuiltinBackend::Host("std::io"), "Reads one line from standard input."),
    builtin!("std::io::print", fn &[TypePattern::Unknown] => TypePattern::Unit, BuiltinBackend::Host("std::io"), "Prints values without a trailing newline."),
    builtin!("std::io::println", fn &[TypePattern::Unknown] => TypePattern::Unit, BuiltinBackend::Host("std::io"), "Prints values followed by a newline."),
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
        }
    }
}
