//! Public facade for the Rils toolchain.

pub use rils_bytecode::{
    BYTECODE_FORMAT_VERSION, BYTECODE_HOST_ABI_VERSION, BYTECODE_LANGUAGE_VERSION, BytecodeError,
    BytecodeFormatError, BytecodeHost, BytecodeImport, BytecodeModule, CompileError,
    LibraryFormatError, RilsLibrary, compile, compile_file, compile_file_with_host,
    compile_library, compile_with_host,
};
pub use rils_runtime::*;

#[cfg(test)]
mod tests;
