use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rils_syntax::{FloatType, FunctionSignature, IntegerType, Type};
use serde_json::{Map, Value, json};

mod binary_v2;
mod contract;
mod legacy_binary;
mod manifest;
mod manifest_json;
mod types;
mod validation;

use legacy_binary::*;
use manifest_json::*;
pub use types::{
    HOST_INLINE_VALUE_MAX_BYTES, HOST_INLINE_VALUE_MAX_FIELDS, HostEnumDefinition,
    HostTypeDeclaration, HostTypeTransport, HostValueFieldType, HostValueLayout,
};
use types::{is_assignable, validate_type_graph, validate_type_name};
use validation::*;

pub const HOST_MANIFEST_FORMAT_VERSION: u32 = 5;
pub const HOST_MANIFEST_JSON_FORMAT_VERSION: u32 = 5;
const HOST_MANIFEST_V4_FORMAT_VERSION: u32 = 4;
const HOST_MANIFEST_V4_JSON_FORMAT_VERSION: u32 = 4;
const HOST_MANIFEST_V3_FORMAT_VERSION: u32 = 3;
const HOST_MANIFEST_V3_JSON_FORMAT_VERSION: u32 = 3;
const HOST_MANIFEST_V2_FORMAT_VERSION: u32 = 2;
const HOST_MANIFEST_V2_JSON_FORMAT_VERSION: u32 = 2;
const HOST_MANIFEST_LEGACY_FORMAT_VERSION: u32 = 1;
const HOST_MANIFEST_LEGACY_JSON_FORMAT_VERSION: u32 = 1;
pub const HOST_CONTRACT_ABI_VERSION: u32 = 1;
pub const HOST_CONTRACT_HASH_ALGORITHM: &str = "fnv1a128";
pub const HOST_MANIFEST_MAGIC: [u8; 8] = *b"RILHOST\0";
pub const HOST_MANIFEST_HEADER_SIZE: usize = 64;
pub const HOST_MANIFEST_MAX_BYTES: usize = 256 * 1024 * 1024;
pub const HOST_MANIFEST_JSON_MAX_BYTES: usize = 64 * 1024 * 1024;
pub const HOST_MANIFEST_MAX_MODULES: usize = 4_096;
pub const HOST_MANIFEST_MAX_TYPES: usize = 65_536;
pub const HOST_MANIFEST_MAX_FUNCTIONS: usize = 65_536;
pub const HOST_MANIFEST_MAX_PARAMETERS: usize = 1_048_576;
pub const HOST_MANIFEST_MAX_ENUM_VARIANTS: usize = 1_048_576;
const HOST_MANIFEST_HASH_ALGORITHM_ID: u32 = 1;
const HOST_MANIFEST_MODULE_ENTRY_SIZE: usize = 8;
const HOST_MANIFEST_FUNCTION_ENTRY_SIZE: usize = 32;
const HOST_MANIFEST_MAX_NAME_BYTES: usize = 1_024;
const HOST_MANIFEST_MAX_CAPABILITY_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCallKind {
    Direct,
}

impl HostCallKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostThreadAffinity {
    MainThread,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostReceiver {
    Value,
    Ref,
    RefMut,
}

impl HostReceiver {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Value => "self",
            Self::Ref => "&self",
            Self::RefMut => "&mut self",
        }
    }

    const fn as_tag(self) -> u8 {
        match self {
            Self::Value => 1,
            Self::Ref => 2,
            Self::RefMut => 3,
        }
    }

    pub fn from_tag(tag: u8) -> Result<Option<Self>, String> {
        match tag {
            0 => Ok(None),
            1 => Ok(Some(Self::Value)),
            2 => Ok(Some(Self::Ref)),
            3 => Ok(Some(Self::RefMut)),
            value => Err(format!("unsupported binary host receiver kind {value}")),
        }
    }
}

impl HostThreadAffinity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MainThread => "main_thread",
        }
    }
}

/// Compile-time description of host functions available to a Rils module.
///
/// The contract contains declarations only. Runtime implementations are linked
/// separately through `BytecodeHost`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostContract {
    host_abi_version: u32,
    contract_version: u32,
    modules: BTreeMap<String, HostModuleDeclaration>,
    types: BTreeMap<String, HostTypeDeclaration>,
    functions: BTreeMap<String, HostFunctionDeclaration>,
    function_ids: HashSet<u64>,
    parameter_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostModuleDeclaration {
    pub name: String,
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFunctionDeclaration {
    pub function_id: u64,
    pub name: String,
    pub signature: FunctionSignature,
    pub capability: String,
    pub call_kind: HostCallKind,
    pub thread_affinity: HostThreadAffinity,
    pub receiver: Option<HostReceiver>,
}

impl Default for HostContract {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../tests/unit/host.rs"]
mod tests;
