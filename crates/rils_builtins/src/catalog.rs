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
    /// Implemented by the Rils runtime.
    Runtime,
    /// Implemented as a compiler or VM intrinsic.
    Intrinsic,
    /// Supplied by an embedding host and protected by the named capability.
    Host(&'static str),
    /// A namespace or semantic declaration with no independently callable body.
    Metadata,
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
    /// Whether this method is also available on fixed arrays and slices.
    pub indexed_view: bool,
    pub runtime_import: Option<&'static str>,
    /// Generated native implementation path, when this method has a direct bridge.
    pub native_symbol: Option<&'static str>,
    /// The declaration explicitly supplies a type-erased native adapter.
    pub native_bridge: bool,
    /// Whether a trait member must be supplied by user implementations.
    pub required: bool,
    pub type_parameters: &'static [&'static str],
    pub documentation: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinDeclaration {
    pub path: &'static str,
    pub kind: BuiltinKind,
    /// Rust-backed struct storage has fields Rils cannot initialize directly.
    pub opaque_native: bool,
    /// Generated Rils declaration source, including trait default bodies when present.
    pub source: Option<&'static str>,
    /// Rils trait bounds; empty for other declaration kinds.
    pub supertraits: &'static [&'static str],
    pub type_parameters: &'static [&'static str],
    pub members: &'static [BuiltinMember],
    pub signature: Option<BuiltinSignature>,
    /// Native method target for an exported free-function alias.
    pub native_symbol: Option<&'static str>,
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
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinSource {
    pub path: &'static str,
    pub module: &'static str,
    pub kind: BuiltinSourceKind,
}

/// A native standard-library type checked against its Rust trait binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltinTraitImpl {
    pub type_name: &'static str,
    pub trait_name: &'static str,
    /// Generic parameter and trait pairs required for this implementation.
    pub requirements: &'static [(&'static str, &'static str)],
}

#[inline]
pub fn native_implements(type_name: &str, trait_name: &str) -> bool {
    native_implements_with(type_name, trait_name, |_, _| false)
}

#[inline]
pub fn native_implements_with(
    type_name: &str,
    trait_name: &str,
    mut satisfies: impl FnMut(&str, &str) -> bool,
) -> bool {
    [
        crate::native_definitions::option::TRAIT_IMPLS,
        crate::native_definitions::result::TRAIT_IMPLS,
        crate::native_definitions::integer::TRAIT_IMPLS,
        crate::native_definitions::float::TRAIT_IMPLS,
        crate::native_definitions::string::TRAIT_IMPLS,
        crate::native_definitions::rc::TRAIT_IMPLS,
        crate::native_definitions::btree_map_into_iter::TRAIT_IMPLS,
        crate::native_definitions::hash_map_into_iter::TRAIT_IMPLS,
        crate::native_definitions::vec_deque_into_iter::TRAIT_IMPLS,
        crate::native_definitions::binary_heap_into_iter::TRAIT_IMPLS,
        crate::native_definitions::btree_set_into_iter::TRAIT_IMPLS,
        crate::native_definitions::hash_set_into_iter::TRAIT_IMPLS,
    ]
    .into_iter()
    .flatten()
    .any(|implementation| {
        implementation.type_name == type_name
            && implementation.trait_name == trait_name
            && implementation
                .requirements
                .iter()
                .all(|(parameter, required_trait)| satisfies(parameter, required_trait))
    })
}

impl BuiltinDeclaration {
    pub fn member(&self, name: &str) -> Option<&BuiltinMember> {
        self.members.iter().find(|member| member.name == name)
    }

    pub fn contains_member(&self, name: &str) -> bool {
        self.member(name).is_some()
    }
}

rils_builtins_macros::builtin_stdlib! {
    "stdlib";
    pub const BUILTINS, BUILTIN_MODULES, BUILTIN_SOURCES;
}

pub use crate::native_definitions::float::{
    CONSTANTS as FLOAT_CONSTANTS, INTRINSICS as FLOAT_INTRINSICS,
};
pub use crate::native_definitions::integer::{
    CONSTANTS as INTEGER_CONSTANTS, INTRINSICS as INTEGER_INTRINSICS,
};

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
    static MEMBERS: std::sync::OnceLock<
        std::collections::HashMap<&'static str, Vec<&'static str>>,
    > = std::sync::OnceLock::new();
    MEMBERS
        .get_or_init(|| {
            let mut members = BUILTIN_MODULES
                .iter()
                .map(|module| (module.path, module.members.to_vec()))
                .collect::<std::collections::HashMap<_, _>>();
            for declaration in BUILTINS {
                if declaration.kind == BuiltinKind::Function
                    && let Some((module, name)) = declaration.path.rsplit_once("::")
                {
                    members.entry(module).or_default().push(name);
                } else if matches!(declaration.kind, BuiltinKind::Struct | BuiltinKind::Enum)
                    && let Some(module) = declaration
                        .members
                        .iter()
                        .filter_map(|member| member.native_symbol)
                        .find_map(|symbol| {
                            symbol
                                .rsplit_once("::")?
                                .0
                                .rsplit_once("::")
                                .map(|(module, _)| module)
                        })
                {
                    members.entry(module).or_default().push(
                        declaration
                            .path
                            .rsplit("::")
                            .next()
                            .unwrap_or(declaration.path),
                    );
                }
            }
            for names in members.values_mut() {
                names.sort_unstable();
                names.dedup();
            }
            members
        })
        .get(path)
        .map_or(&[], Vec::as_slice)
}

pub fn is_iterator_default_method(name: &str) -> bool {
    builtin_member("Iterator", name)
        .is_some_and(|member| member.kind == BuiltinMemberKind::Method && !member.required)
}

pub fn native_member_owner(
    symbol: &str,
) -> Option<(&'static BuiltinDeclaration, &'static BuiltinMember)> {
    BUILTINS.iter().find_map(|owner| {
        owner
            .members
            .iter()
            .find(|member| member.native_symbol == Some(symbol))
            .map(|member| (owner, member))
    })
}

pub fn native_member(symbol: &str) -> Option<&'static BuiltinMember> {
    native_member_owner(symbol).map(|(_, member)| member)
}
