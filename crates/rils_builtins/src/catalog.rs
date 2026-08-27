use crate::{
    BuiltinSignature, FloatConstantDeclaration, FloatConstantId, IntegerConstantDeclaration,
    IntegerConstantId, IntrinsicDeclaration, IntrinsicKind, TypePattern,
};

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
    /// Returns whether this runtime member has a direct bytecode instruction.
    pub fn has_direct_runtime_call(self) -> bool {
        runtime_member(self).is_some()
            && !matches!(
                self,
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
                    | Self::OptionOrElse
            )
    }

    /// Returns whether two member IDs use the same type-erased runtime implementation.
    pub fn shares_direct_runtime_implementation(self, other: Self) -> bool {
        self == other
            || (matches!(self, Self::SequenceLen | Self::StringLen)
                && matches!(other, Self::SequenceLen | Self::StringLen))
            || (matches!(self, Self::SequenceIsEmpty | Self::StringIsEmpty)
                && matches!(other, Self::SequenceIsEmpty | Self::StringIsEmpty))
            || (matches!(self, Self::SequenceContains | Self::StringContains)
                && matches!(other, Self::SequenceContains | Self::StringContains))
            || (matches!(self, Self::OptionUnwrap | Self::ResultUnwrap)
                && matches!(other, Self::OptionUnwrap | Self::ResultUnwrap))
            || (matches!(self, Self::OptionUnwrapOr | Self::ResultUnwrapOr)
                && matches!(other, Self::OptionUnwrapOr | Self::ResultUnwrapOr))
            || (matches!(self, Self::OptionExpect | Self::ResultExpect)
                && matches!(other, Self::OptionExpect | Self::ResultExpect))
            || (matches!(self, Self::OptionReplace | Self::StringReplace)
                && matches!(other, Self::OptionReplace | Self::StringReplace))
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

#[derive(Clone, Copy, Debug)]
pub struct BuiltinModule {
    pub path: &'static str,
    pub members: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinSourceKind {
    ModuleTree,
    Catalog,
    Type,
    Numeric,
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinSource {
    pub path: &'static str,
    pub module: &'static str,
    pub kind: BuiltinSourceKind,
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

rils_builtins_macros::builtin_stdlib! {
    "builtin_ids.toml";
    "stdlib";
    pub const BUILTINS, BUILTIN_MODULES, BUILTIN_SOURCES;
}

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
    BUILTIN_MODULES
        .iter()
        .find(|module| module.path == path)
        .map_or(&[], |module| module.members)
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
