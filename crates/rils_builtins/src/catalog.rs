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
    IteratorCount = 29,
    IteratorLast = 30,
    IteratorNth = 31,
    RangeNext = 32,
    RangeIntoIter = 33,
    IteratorCollectVec = 34,
    IteratorTake = 35,
    IteratorSkip = 36,
    IteratorRev = 37,
    IteratorIntoIter = 38,
    IteratorMap = 100,
    IteratorFilter = 101,
    IteratorFilterMap = 102,
    IteratorFold = 103,
    IteratorForEach = 104,
    IteratorAny = 105,
    IteratorAll = 106,
    IteratorFind = 107,
    IteratorPosition = 108,
    IteratorEnumerate = 109,
    HashMapLen = 120,
    HashMapIsEmpty = 121,
    HashMapClear = 122,
    HashMapContainsKey = 123,
    HashMapInsert = 124,
    HashMapGetCloned = 125,
    HashMapRemove = 126,
    HashMapKeysCloned = 127,
    HashMapValuesCloned = 128,
    HashMapIntoIter = 129,
    HashSetLen = 140,
    HashSetIsEmpty = 141,
    HashSetClear = 142,
    HashSetContains = 143,
    HashSetInsert = 144,
    HashSetRemove = 145,
    HashSetIsSubset = 146,
    HashSetIsSuperset = 147,
    HashSetIsDisjoint = 148,
    HashSetUnion = 149,
    HashSetIntersection = 150,
    HashSetDifference = 151,
    HashSetSymmetricDifference = 152,
    HashSetIntoIter = 153,
    FormatterWriteStr = 180,
    FormatterWriteDerivedDebug = 181,
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
    StringTrimStart = 88,
    StringTrimEnd = 89,
    StringToLowercase = 90,
    StringToUppercase = 91,
    StringRepeat = 92,
    StringRfind = 93,
    StringStripPrefix = 94,
    StringStripSuffix = 95,
    StringChars = 96,
    StringBytes = 97,
    StringLines = 98,
    StringSplit = 99,
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
            Self::StringTrimStart => "core::string::trim_start",
            Self::StringTrimEnd => "core::string::trim_end",
            Self::StringToLowercase => "core::string::to_lowercase",
            Self::StringToUppercase => "core::string::to_uppercase",
            Self::StringRepeat => "core::string::repeat",
            Self::StringRfind => "core::string::rfind",
            Self::StringStripPrefix => "core::string::strip_prefix",
            Self::StringStripSuffix => "core::string::strip_suffix",
            Self::StringChars => "core::string::chars",
            Self::StringBytes => "core::string::bytes",
            Self::StringLines => "core::string::lines",
            Self::StringSplit => "core::string::split",
            Self::IteratorCount => "core::iterator::count",
            Self::IteratorNext => "core::iterator::next",
            Self::IteratorLast => "core::iterator::last",
            Self::IteratorNth => "core::iterator::nth",
            Self::IteratorCollectVec => "core::iterator::collect_vec",
            Self::IteratorTake => "core::iterator::take",
            Self::IteratorSkip => "core::iterator::skip",
            Self::IteratorRev => "core::iterator::rev",
            Self::HashMapLen => "core::hash_map::len",
            Self::HashMapIsEmpty => "core::hash_map::is_empty",
            Self::HashMapClear => "core::hash_map::clear",
            Self::HashMapContainsKey => "core::hash_map::contains_key",
            Self::HashMapInsert => "core::hash_map::insert",
            Self::HashMapGetCloned => "core::hash_map::get_cloned",
            Self::HashMapRemove => "core::hash_map::remove",
            Self::HashMapKeysCloned => "core::hash_map::keys_cloned",
            Self::HashMapValuesCloned => "core::hash_map::values_cloned",
            Self::HashMapIntoIter => "core::hash_map::into_iter",
            Self::HashSetLen => "core::hash_set::len",
            Self::HashSetIsEmpty => "core::hash_set::is_empty",
            Self::HashSetClear => "core::hash_set::clear",
            Self::HashSetContains => "core::hash_set::contains",
            Self::HashSetInsert => "core::hash_set::insert",
            Self::HashSetRemove => "core::hash_set::remove",
            Self::HashSetIsSubset => "core::hash_set::is_subset",
            Self::HashSetIsSuperset => "core::hash_set::is_superset",
            Self::HashSetIsDisjoint => "core::hash_set::is_disjoint",
            Self::HashSetUnion => "core::hash_set::union",
            Self::HashSetIntersection => "core::hash_set::intersection",
            Self::HashSetDifference => "core::hash_set::difference",
            Self::HashSetSymmetricDifference => "core::hash_set::symmetric_difference",
            Self::HashSetIntoIter => "core::hash_set::into_iter",
            Self::FormatterWriteStr => "core::fmt::write_str",
            Self::FormatterWriteDerivedDebug => "core::fmt::write_derived_debug",
            Self::SequenceIntoIter
            | Self::IteratorIntoIter
            | Self::IteratorMap
            | Self::IteratorFilter
            | Self::IteratorFilterMap
            | Self::IteratorFold
            | Self::IteratorForEach
            | Self::IteratorAny
            | Self::IteratorAll
            | Self::IteratorFind
            | Self::IteratorPosition
            | Self::IteratorEnumerate
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
    ($name:literal, method $receiver:ident [$($parameter:expr),* $(,)?] -> $result:expr, $documentation:literal) => {
        BuiltinMember {
            name: $name,
            kind: BuiltinMemberKind::Method,
            signature: Some(BuiltinSignature { parameters: &[$($parameter),*], result: $result, variadic: false }),
            value_type: None,
            receiver: Some(ReceiverMode::$receiver),
            runtime: None,
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
const K: TypePattern = TypePattern::Generic("K");
const V: TypePattern = TypePattern::Generic("V");
const FN_T_U: TypePattern = TypePattern::Function {
    parameters: &[T],
    result: &U,
};
const FN_REF_T_BOOL: TypePattern = TypePattern::Function {
    parameters: &[REF_T],
    result: &TypePattern::Bool,
};
const FN_T_BOOL: TypePattern = TypePattern::Function {
    parameters: &[T],
    result: &TypePattern::Bool,
};
const FN_T_OPTION_U_ITERATOR: TypePattern = TypePattern::Function {
    parameters: &[T],
    result: &TypePattern::Option(&U),
};
const FN_U_T_U: TypePattern = TypePattern::Function {
    parameters: &[U, T],
    result: &U,
};
const FN_T_UNIT: TypePattern = TypePattern::Function {
    parameters: &[T],
    result: &TypePattern::Unit,
};
const ITERATOR_T: TypePattern = TypePattern::Named {
    path: "SequenceIterator",
    arguments: &[T],
};
const ITERATOR_U: TypePattern = TypePattern::Named {
    path: "SequenceIterator",
    arguments: &[U],
};
const INDEXED_T: TypePattern = TypePattern::Tuple(&[TypePattern::Usize, T]);
const ITERATOR_INDEXED_T: TypePattern = TypePattern::Named {
    path: "SequenceIterator",
    arguments: &[INDEXED_T],
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
    member!("count", method Owned [] -> TypePattern::Usize, IteratorCount, "Consumes the iterator and returns the remaining item count."),
    member!("last", method Owned [] -> TypePattern::Option(&T), IteratorLast, "Consumes the iterator and returns its final item."),
    member!("nth", method Mutable [TypePattern::Usize] -> TypePattern::Option(&T), IteratorNth, "Advances to and returns the nth remaining item."),
    member!("collect_vec", method Owned [] -> TypePattern::Named { path: "Vec", arguments: &[T] }, IteratorCollectVec, "Consumes the iterator and collects its items into a Vec."),
    member!("take", method Owned [TypePattern::Usize] -> TypePattern::SelfType, IteratorTake, "Returns an iterator over at most the first n remaining items."),
    member!("skip", method Owned [TypePattern::Usize] -> TypePattern::SelfType, IteratorSkip, "Returns an iterator after discarding the first n remaining items."),
    member!("rev", method Owned [] -> TypePattern::SelfType, IteratorRev, "Reverses the remaining items of this double-ended built-in iterator."),
    member!("map", generic ["U"] method Owned [FN_T_U] -> ITERATOR_U, IteratorMap, "Transforms every remaining item with the supplied function."),
    member!("filter", method Owned [FN_REF_T_BOOL] -> ITERATOR_T, IteratorFilter, "Keeps items for which the predicate returns true."),
    member!("filter_map", generic ["U"] method Owned [FN_T_OPTION_U_ITERATOR] -> ITERATOR_U, IteratorFilterMap, "Transforms and keeps items for which the function returns Some."),
    member!("fold", generic ["U"] method Owned [U, FN_U_T_U] -> U, IteratorFold, "Accumulates all remaining items from an initial value."),
    member!("for_each", method Owned [FN_T_UNIT] -> TypePattern::Unit, IteratorForEach, "Calls a function for every remaining item."),
    member!("any", method Owned [FN_T_BOOL] -> TypePattern::Bool, IteratorAny, "Returns true when any item satisfies the predicate."),
    member!("all", method Owned [FN_T_BOOL] -> TypePattern::Bool, IteratorAll, "Returns true when every item satisfies the predicate."),
    member!("find", method Owned [FN_REF_T_BOOL] -> TypePattern::Option(&T), IteratorFind, "Returns the first item satisfying the predicate."),
    member!("position", method Owned [FN_T_BOOL] -> TypePattern::Option(&TypePattern::Usize), IteratorPosition, "Returns the index of the first item satisfying the predicate."),
    member!("enumerate", method Owned [] -> ITERATOR_INDEXED_T, IteratorEnumerate, "Yields each remaining item together with its zero-based index."),
    member!("into_iter", method Owned [] -> TypePattern::SelfType, IteratorIntoIter, "Returns this iterator unchanged."),
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
const DEFAULT_MEMBERS: &[BuiltinMember] = &[
    member!("default", associated [] -> TypePattern::SelfType, "Constructs the default value for this type."),
];
const FORMATTER: TypePattern = TypePattern::Named {
    path: "Formatter",
    arguments: &[],
};
const MUT_FORMATTER: TypePattern = TypePattern::Reference {
    mutable: true,
    inner: &FORMATTER,
};
const FORMAT_ERROR: TypePattern = TypePattern::Named {
    path: "FormatError",
    arguments: &[],
};
const FORMAT_RESULT: TypePattern = TypePattern::Result {
    ok: &TypePattern::Unit,
    error: &FORMAT_ERROR,
};
const FORMATTER_MEMBERS: &[BuiltinMember] = &[
    member!("write_str", method Mutable [STRING] -> FORMAT_RESULT, FormatterWriteStr, "Appends text to this formatting destination."),
    member!("write_derived_debug", method Mutable [TypePattern::Reference { mutable: false, inner: &TypePattern::Unknown }] -> FORMAT_RESULT, FormatterWriteDerivedDebug, "Writes the structural Debug representation used by derived implementations."),
];
const DISPLAY_MEMBERS: &[BuiltinMember] = &[
    member!("fmt", method Shared [MUT_FORMATTER] -> FORMAT_RESULT, "Writes the user-facing representation into a formatter."),
];
const DEBUG_MEMBERS: &[BuiltinMember] = &[
    member!("fmt", method Shared [MUT_FORMATTER] -> FORMAT_RESULT, "Writes the diagnostic representation into a formatter."),
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
    member!("into_iter", method Owned [] -> ITERATOR_T, SequenceIntoIter, "Consumes the Vec and creates an iterator."),
];
const ARRAY_MEMBERS: &[BuiltinMember] = &[
    member!("len", method Shared [] -> TypePattern::Usize, SequenceLen, "Returns the element count."),
    member!("is_empty", method Shared [] -> TypePattern::Bool, SequenceIsEmpty, "Returns true when the array has no elements."),
    member!("contains", method Shared [TypePattern::Reference { mutable: false, inner: &T }] -> TypePattern::Bool, SequenceContains, "Returns true when an equal element is present."),
    member!("into_iter", method Owned [] -> ITERATOR_T, SequenceIntoIter, "Consumes the array and creates an iterator."),
];
#[path = "catalog/hash_collections.rs"]
mod hash_collections;
use hash_collections::{HASH_MAP_MEMBERS, HASH_SET_MEMBERS};
const STRING_MEMBERS: &[BuiltinMember] = &[
    member!("len", method Shared [] -> TypePattern::Usize, StringLen, "Returns the UTF-8 byte length."),
    member!("is_empty", method Shared [] -> TypePattern::Bool, StringIsEmpty, "Returns true when the string has no bytes."),
    member!("contains", method Shared [STRING] -> TypePattern::Bool, StringContains, "Returns true when the substring is present."),
    member!("starts_with", method Shared [STRING] -> TypePattern::Bool, StringStartsWith, "Tests the string prefix."),
    member!("ends_with", method Shared [STRING] -> TypePattern::Bool, StringEndsWith, "Tests the string suffix."),
    member!("find", method Shared [STRING] -> TypePattern::Option(&TypePattern::Usize), StringFind, "Returns the byte offset of the first match."),
    member!("trim", method Shared [] -> STRING, StringTrim, "Returns a string without leading or trailing whitespace."),
    member!("replace", method Shared [STRING, STRING] -> STRING, StringReplace, "Replaces every matching substring."),
    member!("trim_start", method Shared [] -> STRING, StringTrimStart, "Returns a string without leading whitespace."),
    member!("trim_end", method Shared [] -> STRING, StringTrimEnd, "Returns a string without trailing whitespace."),
    member!("to_lowercase", method Shared [] -> STRING, StringToLowercase, "Returns the Unicode lowercase mapping."),
    member!("to_uppercase", method Shared [] -> STRING, StringToUppercase, "Returns the Unicode uppercase mapping."),
    member!("repeat", method Shared [TypePattern::Usize] -> STRING, StringRepeat, "Repeats the string n times."),
    member!("rfind", method Shared [STRING] -> TypePattern::Option(&TypePattern::Usize), StringRfind, "Returns the byte offset of the final match."),
    member!("strip_prefix", method Shared [STRING] -> TypePattern::Option(&STRING), StringStripPrefix, "Removes one matching prefix."),
    member!("strip_suffix", method Shared [STRING] -> TypePattern::Option(&STRING), StringStripSuffix, "Removes one matching suffix."),
    member!("chars", method Shared [] -> TypePattern::Named { path: "SequenceIterator", arguments: &[TypePattern::Char] }, StringChars, "Iterates over Unicode scalar values."),
    member!("bytes", method Shared [] -> TypePattern::Named { path: "SequenceIterator", arguments: &[TypePattern::U8] }, StringBytes, "Iterates over UTF-8 bytes."),
    member!("lines", method Shared [] -> TypePattern::Named { path: "SequenceIterator", arguments: &[STRING] }, StringLines, "Iterates over lines without their terminators."),
    member!("split", method Shared [STRING] -> TypePattern::Named { path: "SequenceIterator", arguments: &[STRING] }, StringSplit, "Iterates over substrings separated by the pattern."),
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
        "HashMap",
        Struct,
        ["K", "V"],
        HASH_MAP_MEMBERS,
        BuiltinBackend::Runtime,
        "An owned hash map."
    ),
    builtin!(
        "HashSet",
        Struct,
        ["T"],
        HASH_SET_MEMBERS,
        BuiltinBackend::Runtime,
        "An owned hash set."
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
        "Default",
        Trait,
        [],
        DEFAULT_MEMBERS,
        BuiltinBackend::Metadata,
        "Types with a canonical default value."
    ),
    builtin!(
        "Formatter",
        Struct,
        [],
        FORMATTER_MEMBERS,
        BuiltinBackend::Runtime,
        "A transient formatting destination supplied by format macros."
    ),
    builtin!(
        "FormatError",
        Struct,
        [],
        &[],
        BuiltinBackend::Runtime,
        "An error produced while formatting a value."
    ),
    builtin!(
        "Display",
        Trait,
        [],
        DISPLAY_MEMBERS,
        BuiltinBackend::Metadata,
        "User-facing textual formatting."
    ),
    builtin!(
        "Debug",
        Trait,
        [],
        DEBUG_MEMBERS,
        BuiltinBackend::Metadata,
        "Diagnostic textual formatting."
    ),
    builtin!(
        "Eq",
        Trait,
        [],
        &[],
        BuiltinBackend::Metadata,
        "Values with reflexive equality suitable for hashed collections."
    ),
    builtin!(
        "Hash",
        Trait,
        [],
        &[],
        BuiltinBackend::Metadata,
        "Values that can be used as hash collection keys."
    ),
    builtin!(
        "BitFlags",
        Trait,
        [],
        &[],
        BuiltinBackend::Metadata,
        "Enum values whose discriminants may be combined as a bit set."
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

pub fn standard_host_capabilities() -> Vec<&'static str> {
    let mut capabilities = BUILTINS
        .iter()
        .filter_map(|item| match item.backend {
            BuiltinBackend::Host(capability) if capability.starts_with("std::") => Some(capability),
            _ => None,
        })
        .collect::<Vec<_>>();
    capabilities.sort_unstable();
    capabilities.dedup();
    capabilities
}

pub fn builtin_member(owner: &str, name: &str) -> Option<&'static BuiltinMember> {
    builtin(owner)?
        .members
        .iter()
        .find(|member| member.name == name)
}

pub fn builtin_module_members(path: &str) -> &'static [&'static str] {
    match path {
        "std" => &["collections", "io", "fs"],
        "std::collections" => &["Vec", "HashMap", "HashSet"],
        "core" => &[
            "option", "result", "iter", "clone", "default", "fmt", "cmp", "hash",
        ],
        "core::option" => &["Option", "Some", "None"],
        "core::result" => &["Result", "Ok", "Err"],
        "core::iter" => &["Iterator", "IntoIterator", "Range"],
        "core::clone" => &["Copy", "Clone", "clone"],
        "core::default" => &["Default"],
        "core::fmt" => &["Display", "Debug", "Formatter", "FormatError"],
        "core::cmp" => &["Eq"],
        "core::hash" => &["Hash"],
        _ => &[],
    }
}

pub fn is_iterator_default_method(name: &str) -> bool {
    matches!(
        name,
        "count"
            | "last"
            | "collect_vec"
            | "take"
            | "skip"
            | "rev"
            | "map"
            | "filter"
            | "filter_map"
            | "fold"
            | "for_each"
            | "any"
            | "all"
            | "find"
            | "position"
            | "enumerate"
    )
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
