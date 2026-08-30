use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    rc::Rc,
};

mod container;
mod error;
mod instruction;
mod io;
mod types;
mod values;

#[cfg(test)]
use container::crc32;
use container::{decode_container, encode_container, section_reader};
pub use error::BytecodeFormatError;
use instruction::*;
use io::{Reader, Writer};
use types::*;
use values::*;

use crate::{
    FloatType, IntegerType,
    ast::{BinaryOp, EnumVariant, GenericParameter, NamedField, UnaryOp},
    hir::{HirLiteral, HirPattern},
    source::{SourceFile, SourceId, Span},
    types::{FunctionSignature, Type},
    value::{EnumType, StructType},
};

use super::{
    BYTECODE_HOST_ABI_VERSION, BytecodeFunction, BytecodeImport, BytecodeIteratorMethods,
    BytecodeModule, BytecodePlace, BytecodeProjection, BytecodeTraitImplementation, Constant,
    Instruction, RuntimeType, SpannedInstruction,
};

const MAGIC: &[u8; 8] = b"RILBC\0\0\0";
pub const BYTECODE_FORMAT_VERSION: u16 = 7;
pub const BYTECODE_LANGUAGE_VERSION: (u16, u16, u16) = (0, 1, 0);

const HEADER_LEN: usize = 32;
const DIRECTORY_ENTRY_LEN: usize = 12;
const SECTION_MODULE: u16 = 1;
const SECTION_IMPORTS: u16 = 2;
const SECTION_TYPES: u16 = 3;
const SECTION_ITERATORS: u16 = 4;
const SECTION_FUNCTIONS: u16 = 5;
const SECTION_SOURCES: u16 = 6;
const SECTION_TRAIT_IMPLEMENTATIONS: u16 = 7;
const REQUIRED_SECTION: u16 = 1;
const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
const MAX_STRING_BYTES: usize = 1024 * 1024;
const MAX_COLLECTION_ITEMS: usize = 1_000_000;
const MAX_NESTING: usize = 128;
const MAX_FUNCTIONS: usize = 65_536;
const MAX_IMPORTS: usize = 65_536;
const MAX_TYPES: usize = 65_536;
const MAX_REGISTERS_PER_FUNCTION: usize = 262_144;
const MAX_LOCALS_PER_FUNCTION: usize = 262_144;
const MAX_INSTRUCTIONS: usize = 2_000_000;

type Result<T> = std::result::Result<T, BytecodeFormatError>;

fn ensure_limit(value: usize, maximum: usize, label: &str) -> Result<()> {
    if value > maximum {
        Err(BytecodeFormatError::new(format!(
            "{label} exceeds the {maximum} item limit"
        )))
    } else {
        Ok(())
    }
}

impl BytecodeModule {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.verify()
            .map_err(|error| BytecodeFormatError::new(error.message))?;

        ensure_limit(self.functions.len(), MAX_FUNCTIONS, "function table")?;
        ensure_limit(self.imports.len(), MAX_IMPORTS, "import table")?;
        ensure_limit(self.types.len(), MAX_TYPES, "type table")?;
        ensure_limit(self.sources.len(), MAX_COLLECTION_ITEMS, "source table")?;
        ensure_limit(
            self.instruction_count(),
            MAX_INSTRUCTIONS,
            "instruction table",
        )?;
        for function in &self.functions {
            ensure_limit(
                function.register_count,
                MAX_REGISTERS_PER_FUNCTION,
                "function register count",
            )?;
            ensure_limit(
                function.local_count,
                MAX_LOCALS_PER_FUNCTION,
                "function local count",
            )?;
        }

        let mut module = Writer::default();
        module.index(self.entry, "entry function")?;
        let mut imports = Writer::default();
        imports.collection(&self.imports, write_import)?;
        let mut types = Writer::default();
        types.collection(&self.types, write_runtime_type)?;
        let mut iterators = Writer::default();
        let mut iterator_entries: Vec<_> = self.iterators.iter().collect();
        iterator_entries.sort_by(|left, right| left.0.cmp(right.0));
        iterators.len(iterator_entries.len(), "iterator table")?;
        for (name, methods) in iterator_entries {
            iterators.string(name)?;
            iterators.option_index(methods.into_iter, "iterator into_iter function")?;
            iterators.option_index(methods.next, "iterator next function")?;
        }
        let mut functions = Writer::default();
        functions.collection(&self.functions, write_function)?;
        let mut sources = Writer::default();
        sources.collection(&self.sources, write_source_file)?;
        let mut trait_implementations = Writer::default();
        trait_implementations
            .collection(&self.trait_implementations, write_trait_implementation)?;

        encode_container([
            (SECTION_MODULE, module.finish()),
            (SECTION_IMPORTS, imports.finish()),
            (SECTION_TYPES, types.finish()),
            (SECTION_ITERATORS, iterators.finish()),
            (SECTION_FUNCTIONS, functions.finish()),
            (SECTION_SOURCES, sources.finish()),
            (
                SECTION_TRAIT_IMPLEMENTATIONS,
                trait_implementations.finish(),
            ),
        ])
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let sections = decode_container(bytes)?;
        let mut module_reader = section_reader(&sections, SECTION_MODULE, "module")?;
        let entry = module_reader.index()?;
        module_reader.finish()?;

        let mut imports_reader = section_reader(&sections, SECTION_IMPORTS, "imports")?;
        let imports =
            imports_reader.collection_limited(read_import, MAX_IMPORTS, "import table")?;
        imports_reader.finish()?;
        let mut types_reader = section_reader(&sections, SECTION_TYPES, "types")?;
        let types = types_reader.collection_limited(read_runtime_type, MAX_TYPES, "type table")?;
        types_reader.finish()?;
        let mut iterator_reader = section_reader(&sections, SECTION_ITERATORS, "iterators")?;
        let iterator_count = iterator_reader.len()?;
        ensure_limit(iterator_count, MAX_TYPES, "iterator table")?;
        if iterator_count > iterator_reader.remaining() {
            return Err(BytecodeFormatError::new(
                "iterator count exceeds remaining section bytes",
            ));
        }
        let mut iterators = HashMap::with_capacity(iterator_count);
        for _ in 0..iterator_count {
            let name = iterator_reader.string()?;
            let methods = BytecodeIteratorMethods {
                into_iter: iterator_reader.option_index()?,
                next: iterator_reader.option_index()?,
            };
            if iterators.insert(name.clone(), methods).is_some() {
                return Err(BytecodeFormatError::new(format!(
                    "duplicate iterator type `{name}`"
                )));
            }
        }
        iterator_reader.finish()?;
        let mut functions_reader = section_reader(&sections, SECTION_FUNCTIONS, "functions")?;
        let functions =
            functions_reader.collection_limited(read_function, MAX_FUNCTIONS, "function table")?;
        functions_reader.finish()?;
        let mut sources_reader = section_reader(&sections, SECTION_SOURCES, "sources")?;
        let sources = sources_reader.collection_limited(
            read_source_file,
            MAX_COLLECTION_ITEMS,
            "source table",
        )?;
        sources_reader.finish()?;
        let mut trait_reader = section_reader(
            &sections,
            SECTION_TRAIT_IMPLEMENTATIONS,
            "trait implementations",
        )?;
        let trait_implementations = trait_reader.collection_limited(
            read_trait_implementation,
            MAX_TYPES,
            "trait implementation table",
        )?;
        trait_reader.finish()?;

        let instruction_count = functions.iter().try_fold(0usize, |total, function| {
            total
                .checked_add(function.instructions.len())
                .ok_or_else(|| BytecodeFormatError::new("instruction count overflow"))
        })?;
        ensure_limit(instruction_count, MAX_INSTRUCTIONS, "instruction table")?;

        let module = Self {
            sources,
            functions,
            types,
            imports,
            iterators,
            trait_implementations,
            entry,
        };
        module.verify().map_err(|error| {
            BytecodeFormatError::new(format!("bytecode verification failed: {}", error.message))
        })?;
        Ok(module)
    }

    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let bytes = self.to_bytes()?;
        fs::write(path, bytes).map_err(|error| BytecodeFormatError::io("write", path, error))
    }

    pub fn read_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|error| BytecodeFormatError::io("read", path, error))?;
        Self::from_bytes(&bytes)
    }
}

fn write_trait_implementation(
    writer: &mut Writer,
    implementation: &BytecodeTraitImplementation,
) -> Result<()> {
    writer.string(&implementation.target)?;
    writer.string(&implementation.trait_name)?;
    writer.u32(implementation.source.0);
    let mut methods = implementation.methods.iter().collect::<Vec<_>>();
    methods.sort_by(|left, right| left.0.cmp(right.0));
    writer.len(methods.len(), "trait method table")?;
    for (name, function) in methods {
        writer.string(name)?;
        writer.index(*function, "trait method function")?;
    }
    Ok(())
}

fn read_trait_implementation(reader: &mut Reader<'_>) -> Result<BytecodeTraitImplementation> {
    let target = reader.string()?;
    let trait_name = reader.string()?;
    let source = SourceId::new(reader.u32()?);
    let method_count = reader.len()?;
    ensure_limit(method_count, MAX_FUNCTIONS, "trait method table")?;
    let mut methods = HashMap::with_capacity(method_count);
    for _ in 0..method_count {
        let name = reader.string()?;
        let function = reader.index()?;
        if methods.insert(name.clone(), function).is_some() {
            return Err(BytecodeFormatError::new(format!(
                "duplicate trait method `{name}`"
            )));
        }
    }
    Ok(BytecodeTraitImplementation {
        target,
        trait_name,
        source,
        methods,
    })
}

fn write_import(writer: &mut Writer, import: &BytecodeImport) -> Result<()> {
    writer.string(&import.name)?;
    write_signature(writer, &import.signature)?;
    writer.u32(import.abi_version);
    writer.string(&import.capability)
}

fn read_import(reader: &mut Reader<'_>) -> Result<BytecodeImport> {
    Ok(BytecodeImport {
        name: reader.string()?,
        signature: read_signature(reader)?,
        abi_version: reader.u32()?,
        capability: reader.string()?,
    })
}

fn write_function(writer: &mut Writer, function: &BytecodeFunction) -> Result<()> {
    writer.string(&function.name)?;
    writer.bool(function.exported);
    writer.collection(&function.constants, write_constant)?;
    writer.collection(&function.instructions, write_instruction)?;
    writer.index(function.register_count, "register count")?;
    writer.index(function.local_count, "local count")?;
    writer.len(function.local_mutability.len(), "local mutability")?;
    for mutable in &function.local_mutability {
        writer.bool(*mutable);
    }
    writer.index(function.parameter_count, "parameter count")?;
    writer.index(function.capture_count, "capture count")?;
    writer.span(function.span)
}

fn read_function(reader: &mut Reader<'_>) -> Result<BytecodeFunction> {
    let name = reader.string()?;
    let exported = reader.bool()?;
    let constants = reader.collection(read_constant)?;
    let instructions =
        reader.collection_limited(read_instruction, MAX_INSTRUCTIONS, "function instructions")?;
    let register_count = reader.index()?;
    let local_count = reader.index()?;
    ensure_limit(
        register_count,
        MAX_REGISTERS_PER_FUNCTION,
        "function register count",
    )?;
    ensure_limit(local_count, MAX_LOCALS_PER_FUNCTION, "function local count")?;
    let mutability_count = reader.len()?;
    ensure_limit(
        mutability_count,
        MAX_LOCALS_PER_FUNCTION,
        "local mutability table",
    )?;
    let mut local_mutability = Vec::with_capacity(mutability_count);
    for _ in 0..mutability_count {
        local_mutability.push(reader.bool()?);
    }
    Ok(BytecodeFunction {
        name,
        exported,
        constants,
        instructions,
        register_count,
        local_count,
        local_mutability,
        parameter_count: reader.index()?,
        capture_count: reader.index()?,
        span: reader.span()?,
    })
}

fn write_source_file(writer: &mut Writer, source: &SourceFile) -> Result<()> {
    if source.id == SourceId::UNKNOWN {
        return Err(BytecodeFormatError::new(
            "source table cannot contain the unknown source id",
        ));
    }
    writer.u32(source.id.0);
    writer.string(&source.name)
}

fn read_source_file(reader: &mut Reader<'_>) -> Result<SourceFile> {
    let id = SourceId::new(reader.u32()?);
    if id == SourceId::UNKNOWN {
        return Err(BytecodeFormatError::new(
            "source table cannot contain the unknown source id",
        ));
    }
    Ok(SourceFile {
        id,
        name: reader.string()?,
    })
}

#[cfg(test)]
mod tests;
