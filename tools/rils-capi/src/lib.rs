//! Experimental, panic-safe, host-neutral C ABI for embedding Rils.
//!
//! Handles and their backing objects are bound to the thread that created them.
//! This matches Unity's main-thread plugin usage and lets the facade hold the
//! current non-`Send` bytecode representation without exposing Rust layouts.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    sync::atomic::{AtomicU16, Ordering},
};

use rils::{
    BytecodeHost, BytecodeModule, FloatType, FunctionSignature, HostCallKind, HostContract,
    HostReceiver, HostThreadAffinity, HostTypeTransport, HostValueLayout, IntegerType, Span, Type,
    Value,
};

pub const RILS_ABI_VERSION: u32 = 7;
pub const RILS_STATUS_OK: i32 = 0;
pub const RILS_STATUS_INVALID_ARGUMENT: i32 = 1;
pub const RILS_STATUS_INVALID_HANDLE: i32 = 2;
pub const RILS_STATUS_COMPILE_ERROR: i32 = 3;
pub const RILS_STATUS_EXECUTION_ERROR: i32 = 4;
pub const RILS_STATUS_UNSUPPORTED_VALUE: i32 = 5;
pub const RILS_STATUS_BYTECODE_ERROR: i32 = 6;
pub const RILS_STATUS_PANIC: i32 = 255;

pub const RILS_VALUE_UNIT: u32 = 0;
pub const RILS_VALUE_BOOL: u32 = 1;
pub const RILS_VALUE_I8: u32 = 2;
pub const RILS_VALUE_I16: u32 = 3;
pub const RILS_VALUE_I32: u32 = 4;
pub const RILS_VALUE_I64: u32 = 5;
pub const RILS_VALUE_I128: u32 = 6;
pub const RILS_VALUE_ISIZE: u32 = 7;
pub const RILS_VALUE_U8: u32 = 8;
pub const RILS_VALUE_U16: u32 = 9;
pub const RILS_VALUE_U32: u32 = 10;
pub const RILS_VALUE_U64: u32 = 11;
pub const RILS_VALUE_U128: u32 = 12;
pub const RILS_VALUE_USIZE: u32 = 13;
pub const RILS_VALUE_F32: u32 = 14;
pub const RILS_VALUE_F64: u32 = 15;
pub const RILS_VALUE_CHAR: u32 = 16;
pub const RILS_VALUE_HOST_HANDLE: u32 = 17;
pub const RILS_VALUE_INLINE_VALUE: u32 = 18;
pub const RILS_VALUE_STRING: u32 = 19;

pub const RILS_HOST_TYPE_OPAQUE: u32 = 0;
pub const RILS_HOST_TYPE_VALUE: u32 = 1;
pub const RILS_HOST_TYPE_ENUM: u32 = 2;

type Handle = u64;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsSlice {
    pub data: *const u8,
    pub length: usize,
}

impl Default for RilsSlice {
    fn default() -> Self {
        Self {
            data: ptr::null(),
            length: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RilsValue {
    pub tag: u32,
    pub reserved: u32,
    pub low: u64,
    pub high: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsHostFunction {
    pub function_id: u64,
    pub name: RilsSlice,
    pub capability: RilsSlice,
    pub parameter_tags: *const u32,
    pub parameter_count: usize,
    pub return_tag: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsHostType {
    pub name: RilsSlice,
    pub base_type: RilsSlice,
    pub transport_tag: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsHostTypeV2 {
    pub name: RilsSlice,
    pub base_type: RilsSlice,
    pub value_layout: RilsSlice,
    pub transport_tag: u32,
    pub kind: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsHostEnumVariant {
    pub name: RilsSlice,
    pub raw_low: u64,
    pub raw_high: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsHostTypeV3 {
    pub name: RilsSlice,
    pub base_type: RilsSlice,
    pub value_layout: RilsSlice,
    pub enum_variants: *const RilsHostEnumVariant,
    pub enum_variant_count: usize,
    pub transport_tag: u32,
    pub kind: u32,
    pub enum_flags: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsHostParameter {
    pub logical_type: RilsSlice,
    pub transport_tag: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RilsHostFunctionV2 {
    pub function_id: u64,
    pub name: RilsSlice,
    pub capability: RilsSlice,
    pub parameters: *const RilsHostParameter,
    pub parameter_count: usize,
    pub return_parameter: RilsHostParameter,
    pub receiver: u32,
    pub reserved: u32,
}

pub type RilsHostDispatcher = unsafe extern "C" fn(
    user_data: *mut c_void,
    function_id: u64,
    arguments: *const RilsValue,
    argument_count: usize,
    out_value: *mut RilsValue,
    out_error: *mut RilsSlice,
) -> i32;

pub type RilsOutputCallback =
    unsafe extern "C" fn(user_data: *mut c_void, text: RilsSlice, newline: u32);
pub type RilsHostValueFormatCallback = unsafe extern "C" fn(
    user_data: *mut c_void,
    logical_type: RilsSlice,
    value: RilsValue,
    kind: u32,
    alternate: u32,
    precision: usize,
    buffer: *mut u8,
    capacity: usize,
) -> usize;

mod host_bridge;
mod state;
mod value_bridge;

pub(crate) use host_bridge::*;
pub(crate) use state::*;
pub(crate) use value_bridge::*;

mod error_api;
mod instance_api;
mod module_api;
mod runtime_api;
mod runtime_registry_api;
mod script_value_api;
mod string_api;

pub use error_api::*;
pub use instance_api::*;
pub use module_api::*;
pub use runtime_api::*;
pub use runtime_registry_api::*;
pub use script_value_api::*;
pub use string_api::*;

#[cfg(test)]
#[path = "../tests/unit/capi.rs"]
mod tests;
