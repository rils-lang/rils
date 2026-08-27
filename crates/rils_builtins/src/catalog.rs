use crate::{BuiltinSignature, TypePattern};

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
    /// Implemented by the Rils runtime through its built-in ID.
    Runtime,
    /// Implemented as a compiler or VM intrinsic.
    Intrinsic,
    /// Supplied by an embedding host and protected by the named capability.
    Host(&'static str),
    /// A namespace or semantic declaration with no independently callable body.
    Metadata,
}

rils_builtins_macros::builtin_id_declarations!("builtin_ids.toml");

impl BuiltinId {
    /// Returns the stable core import used by the bytecode backend, when this
    /// built-in member is available to compiled programs.
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
            _ => return None,
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
    pub builtin_id: Option<BuiltinId>,
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

impl BuiltinDeclaration {
    pub fn member(&self, name: &str) -> Option<&BuiltinMember> {
        self.members.iter().find(|member| member.name == name)
    }

    pub fn contains_member(&self, name: &str) -> bool {
        self.member(name).is_some()
    }

    pub fn contains_builtin(&self, id: BuiltinId) -> bool {
        self.members
            .iter()
            .any(|member| member.builtin_id == Some(id))
    }
}

rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/option.rils";
    complete "core::option";
    backend Runtime;
    const OPTION_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/result.rils";
    complete "core::result";
    backend Runtime;
    const RESULT_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/iterator.rils";
    complete "core::iterator";
    backend Metadata;
    const ITERATOR_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/into_iterator.rils";
    partial "core::sequence";
    backend Metadata;
    const INTO_ITERATOR_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/clone.rils";
    complete "core";
    backend Metadata;
    const CLONE_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/default.rils";
    partial "core";
    backend Metadata;
    const DEFAULT_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/formatter.rils";
    complete "core::fmt";
    backend Runtime;
    const FORMATTER_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/display.rils";
    partial "core::fmt";
    backend Metadata;
    const DISPLAY_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/debug.rils";
    partial "core::fmt";
    backend Metadata;
    const DEBUG_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/copy.rils";
    partial "core";
    backend Metadata;
    const COPY_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/eq.rils";
    partial "core";
    backend Metadata;
    const EQ_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/hash.rils";
    partial "core";
    backend Metadata;
    const HASH_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/bit_flags.rils";
    partial "core";
    backend Metadata;
    const BIT_FLAGS_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/format_error.rils";
    partial "core::fmt";
    backend Runtime;
    const FORMAT_ERROR_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/vec.rils";
    complete "core::vec";
    backend Runtime;
    const VEC_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/array.rils";
    partial "core::sequence";
    kind Primitive;
    backend Runtime;
    const ARRAY_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/hash_map.rils";
    complete "core::hash_map";
    backend Runtime;
    const HASH_MAP_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/hash_set.rils";
    complete "core::hash_set";
    backend Runtime;
    const HASH_SET_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/string.rils";
    complete "core::string";
    backend Runtime;
    const STRING_BUILTIN;
}
rils_builtins_macros::builtin_file! {
    "builtin_ids.toml";
    "stdlib/core/range.rils";
    complete "core::range";
    backend Runtime;
    const RANGE_BUILTIN;
}
rils_builtins_macros::builtin_catalog_file! {
    "stdlib/modules.rils";
    prefix "";
    backend Metadata;
    const MODULE_BUILTINS;
}
rils_builtins_macros::builtin_catalog_file! {
    "stdlib/prelude.rils";
    prefix "";
    backend Runtime;
    const PRELUDE_BUILTINS;
}
rils_builtins_macros::builtin_catalog_file! {
    "stdlib/std/io.rils";
    prefix "std::io";
    backend Host("std::io");
    const STD_IO_BUILTINS;
}
rils_builtins_macros::builtin_catalog_file! {
    "stdlib/std/fs.rils";
    prefix "std::fs";
    backend Host("std::fs");
    const STD_FS_BUILTINS;
}

/// Non-intrinsic built-ins. Integer primitives use `IntegerType::ALL` plus
/// `INTEGER_INTRINSICS`, avoiding twelve duplicate declarations.
pub const BUILTINS: &[BuiltinDeclaration] = &[
    MODULE_BUILTINS[0],
    MODULE_BUILTINS[1],
    MODULE_BUILTINS[2],
    STRING_BUILTIN,
    OPTION_BUILTIN,
    RESULT_BUILTIN,
    VEC_BUILTIN,
    HASH_MAP_BUILTIN,
    HASH_SET_BUILTIN,
    ARRAY_BUILTIN,
    RANGE_BUILTIN,
    COPY_BUILTIN,
    CLONE_BUILTIN,
    DEFAULT_BUILTIN,
    FORMATTER_BUILTIN,
    FORMAT_ERROR_BUILTIN,
    DISPLAY_BUILTIN,
    DEBUG_BUILTIN,
    EQ_BUILTIN,
    HASH_BUILTIN,
    BIT_FLAGS_BUILTIN,
    ITERATOR_BUILTIN,
    INTO_ITERATOR_BUILTIN,
    PRELUDE_BUILTINS[0],
    PRELUDE_BUILTINS[1],
    PRELUDE_BUILTINS[2],
    PRELUDE_BUILTINS[3],
    PRELUDE_BUILTINS[4],
    PRELUDE_BUILTINS[5],
    PRELUDE_BUILTINS[6],
    PRELUDE_BUILTINS[7],
    PRELUDE_BUILTINS[8],
    PRELUDE_BUILTINS[9],
    STD_IO_BUILTINS[0],
    STD_IO_BUILTINS[1],
    STD_IO_BUILTINS[2],
    STD_IO_BUILTINS[3],
    STD_IO_BUILTINS[4],
    STD_IO_BUILTINS[5],
    STD_FS_BUILTINS[0],
    STD_FS_BUILTINS[1],
    STD_FS_BUILTINS[2],
    STD_FS_BUILTINS[3],
    STD_FS_BUILTINS[4],
    STD_FS_BUILTINS[5],
    STD_FS_BUILTINS[6],
    STD_FS_BUILTINS[7],
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
    builtin(owner)?.member(name)
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

pub const fn is_iterator_default_builtin(id: BuiltinId) -> bool {
    matches!(
        id,
        BuiltinId::IteratorCount
            | BuiltinId::IteratorLast
            | BuiltinId::IteratorCollectVec
            | BuiltinId::IteratorTake
            | BuiltinId::IteratorSkip
            | BuiltinId::IteratorRev
            | BuiltinId::IteratorMap
            | BuiltinId::IteratorFilter
            | BuiltinId::IteratorFilterMap
            | BuiltinId::IteratorFold
            | BuiltinId::IteratorForEach
            | BuiltinId::IteratorAny
            | BuiltinId::IteratorAll
            | BuiltinId::IteratorFind
            | BuiltinId::IteratorPosition
            | BuiltinId::IteratorEnumerate
    )
}

pub fn is_iterator_default_method(name: &str) -> bool {
    builtin_member("Iterator", name)
        .and_then(|member| member.builtin_id)
        .is_some_and(is_iterator_default_builtin)
}

pub fn runtime_member(id: BuiltinId) -> Option<(&'static str, &'static BuiltinMember)> {
    BUILTINS.iter().find_map(|owner| {
        owner
            .members
            .iter()
            .find(|member| member.builtin_id == Some(id))
            .map(|member| (owner.path, member))
    })
}
