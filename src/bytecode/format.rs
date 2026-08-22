use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    error::Error,
    fmt, fs, io,
    path::Path,
    rc::Rc,
};

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
pub const BYTECODE_FORMAT_VERSION: u16 = 6;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BytecodeFormatError {
    pub message: String,
}

impl BytecodeFormatError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn io(action: &str, path: &Path, error: io::Error) -> Self {
        Self::new(format!("failed to {action} `{}`: {error}", path.display()))
    }
}

impl fmt::Display for BytecodeFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "bytecode format error: {}", self.message)
    }
}

impl Error for BytecodeFormatError {}

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

fn encode_container<const N: usize>(sections: [(u16, Vec<u8>); N]) -> Result<Vec<u8>> {
    let directory_len = N
        .checked_mul(DIRECTORY_ENTRY_LEN)
        .ok_or_else(|| BytecodeFormatError::new("section directory is too large"))?;
    let payload_offset = HEADER_LEN
        .checked_add(directory_len)
        .ok_or_else(|| BytecodeFormatError::new("bytecode header is too large"))?;
    let payload_len = sections.iter().try_fold(0usize, |total, (_, data)| {
        total
            .checked_add(data.len())
            .ok_or_else(|| BytecodeFormatError::new("bytecode payload is too large"))
    })?;
    let file_len = payload_offset
        .checked_add(payload_len)
        .ok_or_else(|| BytecodeFormatError::new("bytecode file is too large"))?;
    if file_len > MAX_FILE_BYTES {
        return Err(BytecodeFormatError::new(format!(
            "bytecode file exceeds the {MAX_FILE_BYTES} byte limit"
        )));
    }
    let mut output = Vec::with_capacity(file_len);
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&BYTECODE_FORMAT_VERSION.to_le_bytes());
    output.extend_from_slice(&BYTECODE_LANGUAGE_VERSION.0.to_le_bytes());
    output.extend_from_slice(&BYTECODE_LANGUAGE_VERSION.1.to_le_bytes());
    output.extend_from_slice(&BYTECODE_LANGUAGE_VERSION.2.to_le_bytes());
    output.extend_from_slice(&BYTECODE_HOST_ABI_VERSION.to_le_bytes());
    output.push(usize::BITS as u8);
    output.push(0);
    output.extend_from_slice(&(N as u16).to_le_bytes());
    output.extend_from_slice(&(payload_len as u32).to_le_bytes());
    let payload: Vec<u8> = sections
        .iter()
        .flat_map(|(_, data)| data.iter().copied())
        .collect();
    output.extend_from_slice(&crc32(&payload).to_le_bytes());
    debug_assert_eq!(output.len(), HEADER_LEN);
    let mut offset = payload_offset;
    for (id, data) in &sections {
        output.extend_from_slice(&id.to_le_bytes());
        output.extend_from_slice(&REQUIRED_SECTION.to_le_bytes());
        output.extend_from_slice(&(offset as u32).to_le_bytes());
        output.extend_from_slice(&(data.len() as u32).to_le_bytes());
        offset += data.len();
    }
    output.extend_from_slice(&payload);
    Ok(output)
}

fn decode_container(bytes: &[u8]) -> Result<HashMap<u16, &[u8]>> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(BytecodeFormatError::new(format!(
            "bytecode file exceeds the {MAX_FILE_BYTES} byte limit"
        )));
    }
    if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
        return Err(BytecodeFormatError::new("invalid bytecode magic"));
    }
    let mut header = Reader::new(&bytes[8..HEADER_LEN]);
    let format_version = header.u16()?;
    if format_version != BYTECODE_FORMAT_VERSION {
        return Err(BytecodeFormatError::new(format!(
            "unsupported bytecode format version {format_version}"
        )));
    }
    let language = (header.u16()?, header.u16()?, header.u16()?);
    if language != BYTECODE_LANGUAGE_VERSION {
        return Err(BytecodeFormatError::new(format!(
            "bytecode language version {}.{}.{} is incompatible with {}.{}.{}",
            language.0,
            language.1,
            language.2,
            BYTECODE_LANGUAGE_VERSION.0,
            BYTECODE_LANGUAGE_VERSION.1,
            BYTECODE_LANGUAGE_VERSION.2
        )));
    }
    let abi = header.u32()?;
    if abi != BYTECODE_HOST_ABI_VERSION {
        return Err(BytecodeFormatError::new(format!(
            "bytecode requires host ABI {abi}, runtime provides {BYTECODE_HOST_ABI_VERSION}"
        )));
    }
    let pointer_width = header.u8()?;
    if pointer_width != usize::BITS as u8 {
        return Err(BytecodeFormatError::new(format!(
            "bytecode targets a {pointer_width}-bit runtime, current runtime is {}-bit",
            usize::BITS
        )));
    }
    let flags = header.u8()?;
    if flags != 0 {
        return Err(BytecodeFormatError::new(format!(
            "unsupported bytecode header flags 0x{flags:02x}"
        )));
    }
    let section_count = usize::from(header.u16()?);
    let payload_len = header.u32()? as usize;
    let checksum = header.u32()?;
    header.finish()?;
    if section_count == 0 || section_count > 32 {
        return Err(BytecodeFormatError::new("invalid section count"));
    }
    let directory_end = HEADER_LEN
        .checked_add(section_count * DIRECTORY_ENTRY_LEN)
        .ok_or_else(|| BytecodeFormatError::new("section directory overflow"))?;
    let expected_len = directory_end
        .checked_add(payload_len)
        .ok_or_else(|| BytecodeFormatError::new("payload length overflow"))?;
    if expected_len != bytes.len() {
        return Err(BytecodeFormatError::new(
            "bytecode file length does not match header",
        ));
    }
    if crc32(&bytes[directory_end..]) != checksum {
        return Err(BytecodeFormatError::new(
            "bytecode payload checksum mismatch",
        ));
    }

    let mut directory = Reader::new(&bytes[HEADER_LEN..directory_end]);
    let mut sections = HashMap::with_capacity(section_count);
    let mut ranges = Vec::with_capacity(section_count);
    for _ in 0..section_count {
        let id = directory.u16()?;
        let flags = directory.u16()?;
        let offset = directory.u32()? as usize;
        let length = directory.u32()? as usize;
        let known = matches!(
            id,
            SECTION_MODULE
                | SECTION_IMPORTS
                | SECTION_TYPES
                | SECTION_ITERATORS
                | SECTION_FUNCTIONS
                | SECTION_SOURCES
                | SECTION_TRAIT_IMPLEMENTATIONS
        );
        if !known && flags & REQUIRED_SECTION != 0 {
            return Err(BytecodeFormatError::new(format!(
                "unknown required section {id}"
            )));
        }
        if flags & !REQUIRED_SECTION != 0 {
            return Err(BytecodeFormatError::new(format!(
                "unsupported flags for section {id}"
            )));
        }
        let end = offset
            .checked_add(length)
            .ok_or_else(|| BytecodeFormatError::new("section range overflow"))?;
        if offset < directory_end || end > bytes.len() {
            return Err(BytecodeFormatError::new(format!(
                "section {id} is out of bounds"
            )));
        }
        if known && sections.insert(id, &bytes[offset..end]).is_some() {
            return Err(BytecodeFormatError::new(format!("duplicate section {id}")));
        }
        ranges.push((offset, end, id));
    }
    directory.finish()?;
    ranges.sort_by_key(|range| range.0);
    for pair in ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(BytecodeFormatError::new(format!(
                "sections {} and {} overlap",
                pair[0].2, pair[1].2
            )));
        }
    }
    for id in [
        SECTION_MODULE,
        SECTION_IMPORTS,
        SECTION_TYPES,
        SECTION_ITERATORS,
        SECTION_FUNCTIONS,
        SECTION_SOURCES,
        SECTION_TRAIT_IMPLEMENTATIONS,
    ] {
        if !sections.contains_key(&id) {
            return Err(BytecodeFormatError::new(format!(
                "missing required section {id}"
            )));
        }
    }
    Ok(sections)
}

fn section_reader<'a>(
    sections: &HashMap<u16, &'a [u8]>,
    id: u16,
    name: &str,
) -> Result<Reader<'a>> {
    sections
        .get(&id)
        .copied()
        .map(Reader::new)
        .ok_or_else(|| BytecodeFormatError::new(format!("missing {name} section")))
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn finish(self) -> Vec<u8> {
        self.0
    }
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }
    fn u16(&mut self, value: u16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn u128(&mut self, value: u128) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i8(&mut self, value: i8) {
        self.u8(value as u8);
    }
    fn i16(&mut self, value: i16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i64(&mut self, value: i64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i128(&mut self, value: i128) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn index(&mut self, value: usize, label: &str) -> Result<()> {
        self.u32(
            u32::try_from(value)
                .map_err(|_| BytecodeFormatError::new(format!("{label} exceeds u32")))?,
        );
        Ok(())
    }
    fn len(&mut self, value: usize, label: &str) -> Result<()> {
        if value > MAX_COLLECTION_ITEMS {
            return Err(BytecodeFormatError::new(format!(
                "{label} exceeds item limit"
            )));
        }
        self.index(value, label)
    }
    fn string(&mut self, value: &str) -> Result<()> {
        if value.len() > MAX_STRING_BYTES {
            return Err(BytecodeFormatError::new("string exceeds byte limit"));
        }
        self.index(value.len(), "string length")?;
        self.0.extend_from_slice(value.as_bytes());
        Ok(())
    }
    fn span(&mut self, span: Span) -> Result<()> {
        self.u32(span.source.0);
        self.u64(
            u64::try_from(span.start)
                .map_err(|_| BytecodeFormatError::new("span start exceeds u64"))?,
        );
        self.u64(
            u64::try_from(span.end)
                .map_err(|_| BytecodeFormatError::new("span end exceeds u64"))?,
        );
        Ok(())
    }
    fn collection<T>(
        &mut self,
        values: &[T],
        write: fn(&mut Self, &T) -> Result<()>,
    ) -> Result<()> {
        self.len(values.len(), "collection")?;
        for value in values {
            write(self, value)?;
        }
        Ok(())
    }
    fn indices(&mut self, values: &[usize]) -> Result<()> {
        self.len(values.len(), "index collection")?;
        for value in values {
            self.index(*value, "index")?;
        }
        Ok(())
    }
    fn option_index(&mut self, value: Option<usize>, label: &str) -> Result<()> {
        self.bool(value.is_some());
        if let Some(value) = value {
            self.index(value, label)?;
        }
        Ok(())
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    depth: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            depth: 0,
        }
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| BytecodeFormatError::new("read offset overflow"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| BytecodeFormatError::new("unexpected end of bytecode section"))?;
        self.position = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(BytecodeFormatError::new(format!("invalid boolean {value}"))),
        }
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("length checked"),
        ))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("length checked"),
        ))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("length checked"),
        ))
    }
    fn u128(&mut self) -> Result<u128> {
        Ok(u128::from_le_bytes(
            self.take(16)?.try_into().expect("length checked"),
        ))
    }
    fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(
            self.take(2)?.try_into().expect("length checked"),
        ))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().expect("length checked"),
        ))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(
            self.take(8)?.try_into().expect("length checked"),
        ))
    }
    fn i128(&mut self) -> Result<i128> {
        Ok(i128::from_le_bytes(
            self.take(16)?.try_into().expect("length checked"),
        ))
    }
    fn index(&mut self) -> Result<usize> {
        Ok(self.u32()? as usize)
    }
    fn len(&mut self) -> Result<usize> {
        let value = self.index()?;
        if value > MAX_COLLECTION_ITEMS {
            return Err(BytecodeFormatError::new("collection exceeds item limit"));
        }
        Ok(value)
    }
    fn string(&mut self) -> Result<String> {
        let length = self.index()?;
        if length > MAX_STRING_BYTES {
            return Err(BytecodeFormatError::new("string exceeds byte limit"));
        }
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| BytecodeFormatError::new("string is not valid UTF-8"))
    }
    fn span(&mut self) -> Result<Span> {
        let source = SourceId::new(self.u32()?);
        let start = usize::try_from(self.u64()?)
            .map_err(|_| BytecodeFormatError::new("span start exceeds usize"))?;
        let end = usize::try_from(self.u64()?)
            .map_err(|_| BytecodeFormatError::new("span end exceeds usize"))?;
        if start > end {
            return Err(BytecodeFormatError::new("span start exceeds end"));
        }
        Ok(Span::in_source(source, start, end))
    }
    fn collection<T>(&mut self, read: fn(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let count = self.len()?;
        if count > self.remaining() {
            return Err(BytecodeFormatError::new(
                "collection count exceeds remaining section bytes",
            ));
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(read(self)?);
        }
        Ok(values)
    }
    fn collection_limited<T>(
        &mut self,
        read: fn(&mut Self) -> Result<T>,
        maximum: usize,
        label: &str,
    ) -> Result<Vec<T>> {
        let count = self.len()?;
        ensure_limit(count, maximum, label)?;
        if count > self.remaining() {
            return Err(BytecodeFormatError::new(format!(
                "{label} count exceeds remaining section bytes"
            )));
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(read(self)?);
        }
        Ok(values)
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    fn indices(&mut self) -> Result<Vec<usize>> {
        self.collection(Self::index)
    }
    fn option_index(&mut self) -> Result<Option<usize>> {
        if self.bool()? {
            Ok(Some(self.index()?))
        } else {
            Ok(None)
        }
    }
    fn nested<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(BytecodeFormatError::new(
                "type or pattern nesting exceeds limit",
            ));
        }
        let result = read(self);
        self.depth -= 1;
        result
    }
    fn finish(&self) -> Result<()> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(BytecodeFormatError::new("trailing bytes in section"))
        }
    }
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

fn write_signature(writer: &mut Writer, signature: &FunctionSignature) -> Result<()> {
    writer.bool(signature.parameters.is_some());
    if let Some(parameters) = &signature.parameters {
        writer.collection(parameters, |writer, value| write_type(writer, value, 0))?;
    }
    write_type(writer, &signature.return_type, 0)
}

fn read_signature(reader: &mut Reader<'_>) -> Result<FunctionSignature> {
    let parameters = if reader.bool()? {
        Some(reader.collection(read_type)?)
    } else {
        None
    };
    Ok(FunctionSignature {
        parameters,
        return_type: read_type(reader)?,
    })
}

fn write_type(writer: &mut Writer, value: &Type, depth: usize) -> Result<()> {
    if depth > MAX_NESTING {
        return Err(BytecodeFormatError::new("type nesting exceeds limit"));
    }
    let next = depth + 1;
    match value {
        Type::Unit => writer.u8(0),
        Type::Bool => writer.u8(1),
        Type::Integer(kind) => {
            writer.u8(2);
            writer.u8(write_integer_type(*kind));
        }
        Type::Float(kind) => {
            writer.u8(3);
            writer.u8(match kind {
                FloatType::F32 => 0,
                FloatType::F64 => 1,
            });
        }
        Type::IntegerVariable(span) => {
            writer.u8(4);
            writer.span(*span)?;
        }
        Type::FloatVariable(span) => {
            writer.u8(5);
            writer.span(*span)?;
        }
        Type::Char => writer.u8(6),
        Type::String => writer.u8(7),
        Type::Tuple(elements) => {
            writer.u8(8);
            writer.len(elements.len(), "tuple type")?;
            for element in elements {
                write_type(writer, element, next)?;
            }
        }
        Type::Array { element, length } => {
            writer.u8(9);
            write_type(writer, element, next)?;
            writer.index(*length, "array length")?;
        }
        Type::Reference { mutable, inner } => {
            writer.u8(10);
            writer.bool(*mutable);
            write_type(writer, inner, next)?;
        }
        Type::Function {
            parameters,
            return_type,
        } => {
            writer.u8(11);
            writer.bool(parameters.is_some());
            if let Some(parameters) = parameters {
                writer.len(parameters.len(), "function parameters")?;
                for parameter in parameters {
                    write_type(writer, parameter, next)?;
                }
            }
            write_type(writer, return_type, next)?;
        }
        Type::Option(inner) => {
            writer.u8(12);
            write_type(writer, inner, next)?;
        }
        Type::Result(ok, error) => {
            writer.u8(13);
            write_type(writer, ok, next)?;
            write_type(writer, error, next)?;
        }
        Type::Named { name, arguments } => {
            writer.u8(14);
            writer.string(name)?;
            writer.len(arguments.len(), "type arguments")?;
            for argument in arguments {
                write_type(writer, argument, next)?;
            }
        }
        Type::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => {
            writer.u8(15);
            write_type(writer, base, next)?;
            writer.bool(trait_name.is_some());
            if let Some(trait_name) = trait_name {
                writer.string(trait_name)?;
            }
            writer.string(name)?;
            writer.len(arguments.len(), "associated type arguments")?;
            for argument in arguments {
                write_type(writer, argument, next)?;
            }
        }
        Type::Variable(name) => {
            writer.u8(16);
            writer.string(name)?;
        }
        Type::Unknown => writer.u8(17),
    }
    Ok(())
}

fn read_type(reader: &mut Reader<'_>) -> Result<Type> {
    reader.nested(|reader| match reader.u8()? {
        0 => Ok(Type::Unit),
        1 => Ok(Type::Bool),
        2 => Ok(Type::Integer(read_integer_type(reader.u8()?)?)),
        3 => Ok(Type::Float(match reader.u8()? {
            0 => FloatType::F32,
            1 => FloatType::F64,
            value => {
                return Err(BytecodeFormatError::new(format!(
                    "invalid float type {value}"
                )));
            }
        })),
        4 => Ok(Type::IntegerVariable(reader.span()?)),
        5 => Ok(Type::FloatVariable(reader.span()?)),
        6 => Ok(Type::Char),
        7 => Ok(Type::String),
        8 => Ok(Type::Tuple(reader.collection(read_type)?)),
        9 => Ok(Type::Array {
            element: Box::new(read_type(reader)?),
            length: reader.index()?,
        }),
        10 => Ok(Type::Reference {
            mutable: reader.bool()?,
            inner: Box::new(read_type(reader)?),
        }),
        11 => {
            let parameters = if reader.bool()? {
                Some(reader.collection(read_type)?)
            } else {
                None
            };
            Ok(Type::Function {
                parameters,
                return_type: Box::new(read_type(reader)?),
            })
        }
        12 => Ok(Type::Option(Box::new(read_type(reader)?))),
        13 => Ok(Type::Result(
            Box::new(read_type(reader)?),
            Box::new(read_type(reader)?),
        )),
        14 => Ok(Type::Named {
            name: reader.string()?,
            arguments: reader.collection(read_type)?,
        }),
        15 => {
            let base = Box::new(read_type(reader)?);
            let trait_name = if reader.bool()? {
                Some(reader.string()?)
            } else {
                None
            };
            let name = reader.string()?;
            let arguments = reader.collection(read_type)?;
            Ok(Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            })
        }
        16 => Ok(Type::Variable(reader.string()?)),
        17 => Ok(Type::Unknown),
        value => Err(BytecodeFormatError::new(format!(
            "invalid type tag {value}"
        ))),
    })
}

fn write_integer_type(value: IntegerType) -> u8 {
    match value {
        IntegerType::I8 => 0,
        IntegerType::I16 => 1,
        IntegerType::I32 => 2,
        IntegerType::I64 => 3,
        IntegerType::I128 => 4,
        IntegerType::Isize => 5,
        IntegerType::U8 => 6,
        IntegerType::U16 => 7,
        IntegerType::U32 => 8,
        IntegerType::U64 => 9,
        IntegerType::U128 => 10,
        IntegerType::Usize => 11,
    }
}

fn read_integer_type(value: u8) -> Result<IntegerType> {
    match value {
        0 => Ok(IntegerType::I8),
        1 => Ok(IntegerType::I16),
        2 => Ok(IntegerType::I32),
        3 => Ok(IntegerType::I64),
        4 => Ok(IntegerType::I128),
        5 => Ok(IntegerType::Isize),
        6 => Ok(IntegerType::U8),
        7 => Ok(IntegerType::U16),
        8 => Ok(IntegerType::U32),
        9 => Ok(IntegerType::U64),
        10 => Ok(IntegerType::U128),
        11 => Ok(IntegerType::Usize),
        _ => Err(BytecodeFormatError::new(format!(
            "invalid integer type {value}"
        ))),
    }
}

fn write_generic_parameter(writer: &mut Writer, parameter: &GenericParameter) -> Result<()> {
    writer.string(&parameter.name)?;
    writer.collection(&parameter.bounds, |writer, value| writer.string(value))?;
    writer.span(parameter.span)
}

fn read_generic_parameter(reader: &mut Reader<'_>) -> Result<GenericParameter> {
    Ok(GenericParameter {
        name: reader.string()?,
        bounds: reader.collection(Reader::string)?,
        span: reader.span()?,
    })
}

fn write_named_field(writer: &mut Writer, field: &NamedField) -> Result<()> {
    writer.string(&field.name)?;
    write_type(writer, &field.type_annotation, 0)?;
    writer.span(field.span)
}

fn read_named_field(reader: &mut Reader<'_>) -> Result<NamedField> {
    Ok(NamedField {
        name: reader.string()?,
        type_annotation: read_type(reader)?,
        span: reader.span()?,
    })
}

fn write_enum_variant(writer: &mut Writer, variant: &EnumVariant) -> Result<()> {
    match variant {
        EnumVariant::Unit { name, span } => {
            writer.u8(0);
            writer.string(name)?;
            writer.span(*span)?;
        }
        EnumVariant::Tuple { name, fields, span } => {
            writer.u8(1);
            writer.string(name)?;
            writer.collection(fields, |writer, value| write_type(writer, value, 0))?;
            writer.span(*span)?;
        }
        EnumVariant::Record { name, fields, span } => {
            writer.u8(2);
            writer.string(name)?;
            writer.collection(fields, write_named_field)?;
            writer.span(*span)?;
        }
    }
    Ok(())
}

fn read_enum_variant(reader: &mut Reader<'_>) -> Result<EnumVariant> {
    match reader.u8()? {
        0 => Ok(EnumVariant::Unit {
            name: reader.string()?,
            span: reader.span()?,
        }),
        1 => Ok(EnumVariant::Tuple {
            name: reader.string()?,
            fields: reader.collection(read_type)?,
            span: reader.span()?,
        }),
        2 => Ok(EnumVariant::Record {
            name: reader.string()?,
            fields: reader.collection(read_named_field)?,
            span: reader.span()?,
        }),
        value => Err(BytecodeFormatError::new(format!(
            "invalid enum variant tag {value}"
        ))),
    }
}

fn write_runtime_type(writer: &mut Writer, runtime_type: &RuntimeType) -> Result<()> {
    match runtime_type {
        RuntimeType::Struct(value) => {
            writer.u8(0);
            writer.string(&value.name)?;
            writer.collection(&value.generic_parameters, write_generic_parameter)?;
            writer.collection(&value.fields, write_named_field)?;
        }
        RuntimeType::Enum(value) => {
            writer.u8(1);
            writer.string(&value.name)?;
            writer.collection(&value.generic_parameters, write_generic_parameter)?;
            writer.collection(&value.variants, write_enum_variant)?;
        }
    }
    Ok(())
}

fn read_runtime_type(reader: &mut Reader<'_>) -> Result<RuntimeType> {
    match reader.u8()? {
        0 => Ok(RuntimeType::Struct(Rc::new(StructType {
            name: reader.string()?,
            generic_parameters: reader.collection(read_generic_parameter)?,
            fields: reader.collection(read_named_field)?,
            methods: RefCell::new(HashMap::new()),
            trait_methods: RefCell::new(HashMap::new()),
            implemented_traits: RefCell::new(HashSet::new()),
            associated_types: RefCell::new(HashMap::new()),
        }))),
        1 => Ok(RuntimeType::Enum(Rc::new(EnumType {
            name: reader.string()?,
            generic_parameters: reader.collection(read_generic_parameter)?,
            variants: reader.collection(read_enum_variant)?,
            methods: RefCell::new(HashMap::new()),
            trait_methods: RefCell::new(HashMap::new()),
            implemented_traits: RefCell::new(HashSet::new()),
            associated_types: RefCell::new(HashMap::new()),
        }))),
        value => Err(BytecodeFormatError::new(format!(
            "invalid runtime type tag {value}"
        ))),
    }
}

fn write_constant(writer: &mut Writer, constant: &Constant) -> Result<()> {
    match constant {
        Constant::Unit => writer.u8(0),
        Constant::Bool(value) => {
            writer.u8(1);
            writer.bool(*value);
        }
        Constant::I8(value) => {
            writer.u8(2);
            writer.i8(*value);
        }
        Constant::I16(value) => {
            writer.u8(3);
            writer.i16(*value);
        }
        Constant::I32(value) => {
            writer.u8(4);
            writer.i32(*value);
        }
        Constant::I64(value) => {
            writer.u8(5);
            writer.i64(*value);
        }
        Constant::I128(value) => {
            writer.u8(6);
            writer.i128(*value);
        }
        Constant::Isize(value) => {
            writer.u8(7);
            writer.i64(*value as i64);
        }
        Constant::U8(value) => {
            writer.u8(8);
            writer.u8(*value);
        }
        Constant::U16(value) => {
            writer.u8(9);
            writer.u16(*value);
        }
        Constant::U32(value) => {
            writer.u8(10);
            writer.u32(*value);
        }
        Constant::U64(value) => {
            writer.u8(11);
            writer.u64(*value);
        }
        Constant::U128(value) => {
            writer.u8(12);
            writer.u128(*value);
        }
        Constant::Usize(value) => {
            writer.u8(13);
            writer.u64(*value as u64);
        }
        Constant::F32(value) => {
            writer.u8(14);
            writer.u32(value.to_bits());
        }
        Constant::F64(value) => {
            writer.u8(15);
            writer.u64(value.to_bits());
        }
        Constant::Char(value) => {
            writer.u8(16);
            writer.u32(*value as u32);
        }
        Constant::String(value) => {
            writer.u8(17);
            writer.string(value)?;
        }
    }
    Ok(())
}

fn read_constant(reader: &mut Reader<'_>) -> Result<Constant> {
    match reader.u8()? {
        0 => Ok(Constant::Unit),
        1 => Ok(Constant::Bool(reader.bool()?)),
        2 => Ok(Constant::I8(reader.i8()?)),
        3 => Ok(Constant::I16(reader.i16()?)),
        4 => Ok(Constant::I32(reader.i32()?)),
        5 => Ok(Constant::I64(reader.i64()?)),
        6 => Ok(Constant::I128(reader.i128()?)),
        7 => Ok(Constant::Isize(isize::try_from(reader.i64()?).map_err(
            |_| BytecodeFormatError::new("isize constant is out of range"),
        )?)),
        8 => Ok(Constant::U8(reader.u8()?)),
        9 => Ok(Constant::U16(reader.u16()?)),
        10 => Ok(Constant::U32(reader.u32()?)),
        11 => Ok(Constant::U64(reader.u64()?)),
        12 => Ok(Constant::U128(reader.u128()?)),
        13 => Ok(Constant::Usize(usize::try_from(reader.u64()?).map_err(
            |_| BytecodeFormatError::new("usize constant is out of range"),
        )?)),
        14 => Ok(Constant::F32(f32::from_bits(reader.u32()?))),
        15 => Ok(Constant::F64(f64::from_bits(reader.u64()?))),
        16 => Ok(Constant::Char(char::from_u32(reader.u32()?).ok_or_else(
            || BytecodeFormatError::new("invalid char scalar value"),
        )?)),
        17 => Ok(Constant::String(reader.string()?)),
        value => Err(BytecodeFormatError::new(format!(
            "invalid constant tag {value}"
        ))),
    }
}

fn write_literal(writer: &mut Writer, literal: &HirLiteral) -> Result<()> {
    let constant = match literal {
        HirLiteral::Unit => Constant::Unit,
        HirLiteral::Bool(v) => Constant::Bool(*v),
        HirLiteral::I8(v) => Constant::I8(*v),
        HirLiteral::I16(v) => Constant::I16(*v),
        HirLiteral::I32(v) => Constant::I32(*v),
        HirLiteral::I64(v) => Constant::I64(*v),
        HirLiteral::I128(v) => Constant::I128(*v),
        HirLiteral::Isize(v) => Constant::Isize(*v),
        HirLiteral::U8(v) => Constant::U8(*v),
        HirLiteral::U16(v) => Constant::U16(*v),
        HirLiteral::U32(v) => Constant::U32(*v),
        HirLiteral::U64(v) => Constant::U64(*v),
        HirLiteral::U128(v) => Constant::U128(*v),
        HirLiteral::Usize(v) => Constant::Usize(*v),
        HirLiteral::F32(v) => Constant::F32(*v),
        HirLiteral::F64(v) => Constant::F64(*v),
        HirLiteral::Char(v) => Constant::Char(*v),
        HirLiteral::String(v) => Constant::String(v.clone()),
    };
    write_constant(writer, &constant)
}

fn read_literal(reader: &mut Reader<'_>) -> Result<HirLiteral> {
    match read_constant(reader)? {
        Constant::Unit => Ok(HirLiteral::Unit),
        Constant::Bool(v) => Ok(HirLiteral::Bool(v)),
        Constant::I8(v) => Ok(HirLiteral::I8(v)),
        Constant::I16(v) => Ok(HirLiteral::I16(v)),
        Constant::I32(v) => Ok(HirLiteral::I32(v)),
        Constant::I64(v) => Ok(HirLiteral::I64(v)),
        Constant::I128(v) => Ok(HirLiteral::I128(v)),
        Constant::Isize(v) => Ok(HirLiteral::Isize(v)),
        Constant::U8(v) => Ok(HirLiteral::U8(v)),
        Constant::U16(v) => Ok(HirLiteral::U16(v)),
        Constant::U32(v) => Ok(HirLiteral::U32(v)),
        Constant::U64(v) => Ok(HirLiteral::U64(v)),
        Constant::U128(v) => Ok(HirLiteral::U128(v)),
        Constant::Usize(v) => Ok(HirLiteral::Usize(v)),
        Constant::F32(v) => Ok(HirLiteral::F32(v)),
        Constant::F64(v) => Ok(HirLiteral::F64(v)),
        Constant::Char(v) => Ok(HirLiteral::Char(v)),
        Constant::String(v) => Ok(HirLiteral::String(v)),
    }
}

fn write_pattern(writer: &mut Writer, pattern: &HirPattern, depth: usize) -> Result<()> {
    if depth > MAX_NESTING {
        return Err(BytecodeFormatError::new("pattern nesting exceeds limit"));
    }
    let next = depth + 1;
    match pattern {
        HirPattern::Wildcard => writer.u8(0),
        HirPattern::Binding(local) => {
            writer.u8(1);
            writer.index(*local, "pattern local")?;
        }
        HirPattern::Literal(literal) => {
            writer.u8(2);
            write_literal(writer, literal)?;
        }
        HirPattern::Some(inner) => {
            writer.u8(3);
            write_pattern(writer, inner, next)?;
        }
        HirPattern::None => writer.u8(4),
        HirPattern::Ok(inner) => {
            writer.u8(5);
            write_pattern(writer, inner, next)?;
        }
        HirPattern::Err(inner) => {
            writer.u8(6);
            write_pattern(writer, inner, next)?;
        }
        HirPattern::TupleVariant { path, fields } => {
            writer.u8(7);
            writer.collection(path, |writer, value| writer.string(value))?;
            writer.len(fields.len(), "tuple pattern fields")?;
            for field in fields {
                write_pattern(writer, field, next)?;
            }
        }
        HirPattern::Record { path, fields } => {
            writer.u8(8);
            writer.collection(path, |writer, value| writer.string(value))?;
            writer.len(fields.len(), "record pattern fields")?;
            for (name, field) in fields {
                writer.string(name)?;
                write_pattern(writer, field, next)?;
            }
        }
        HirPattern::Path(path) => {
            writer.u8(9);
            writer.collection(path, |writer, value| writer.string(value))?;
        }
    }
    Ok(())
}

fn read_pattern(reader: &mut Reader<'_>) -> Result<HirPattern> {
    reader.nested(|reader| match reader.u8()? {
        0 => Ok(HirPattern::Wildcard),
        1 => Ok(HirPattern::Binding(reader.index()?)),
        2 => Ok(HirPattern::Literal(read_literal(reader)?)),
        3 => Ok(HirPattern::Some(Box::new(read_pattern(reader)?))),
        4 => Ok(HirPattern::None),
        5 => Ok(HirPattern::Ok(Box::new(read_pattern(reader)?))),
        6 => Ok(HirPattern::Err(Box::new(read_pattern(reader)?))),
        7 => Ok(HirPattern::TupleVariant {
            path: reader.collection(Reader::string)?,
            fields: reader.collection(read_pattern)?,
        }),
        8 => {
            let path = reader.collection(Reader::string)?;
            let count = reader.len()?;
            let mut fields = Vec::with_capacity(count);
            for _ in 0..count {
                fields.push((reader.string()?, read_pattern(reader)?));
            }
            Ok(HirPattern::Record { path, fields })
        }
        9 => Ok(HirPattern::Path(reader.collection(Reader::string)?)),
        value => Err(BytecodeFormatError::new(format!(
            "invalid pattern tag {value}"
        ))),
    })
}

fn write_place(writer: &mut Writer, place: &BytecodePlace) -> Result<()> {
    writer.index(place.local, "place local")?;
    writer.len(place.projections.len(), "place projections")?;
    for projection in &place.projections {
        match projection {
            BytecodeProjection::Field(name) => {
                writer.u8(0);
                writer.string(name)?;
            }
            BytecodeProjection::Index(register) => {
                writer.u8(1);
                writer.index(*register, "index register")?;
            }
        }
    }
    Ok(())
}

fn read_place(reader: &mut Reader<'_>) -> Result<BytecodePlace> {
    let local = reader.index()?;
    let count = reader.len()?;
    let mut projections = Vec::with_capacity(count);
    for _ in 0..count {
        projections.push(match reader.u8()? {
            0 => BytecodeProjection::Field(reader.string()?),
            1 => BytecodeProjection::Index(reader.index()?),
            value => {
                return Err(BytecodeFormatError::new(format!(
                    "invalid projection tag {value}"
                )));
            }
        });
    }
    Ok(BytecodePlace { local, projections })
}

fn write_unary(value: UnaryOp) -> u8 {
    match value {
        UnaryOp::Negate => 0,
        UnaryOp::Not => 1,
        UnaryOp::Dereference => 2,
    }
}
fn read_unary(value: u8) -> Result<UnaryOp> {
    match value {
        0 => Ok(UnaryOp::Negate),
        1 => Ok(UnaryOp::Not),
        2 => Ok(UnaryOp::Dereference),
        _ => Err(BytecodeFormatError::new(format!(
            "invalid unary operator {value}"
        ))),
    }
}
fn write_binary(value: BinaryOp) -> u8 {
    match value {
        BinaryOp::Add => 0,
        BinaryOp::Subtract => 1,
        BinaryOp::Multiply => 2,
        BinaryOp::Divide => 3,
        BinaryOp::Remainder => 4,
        BinaryOp::Equal => 5,
        BinaryOp::NotEqual => 6,
        BinaryOp::Greater => 7,
        BinaryOp::GreaterEqual => 8,
        BinaryOp::Less => 9,
        BinaryOp::LessEqual => 10,
    }
}
fn read_binary(value: u8) -> Result<BinaryOp> {
    match value {
        0 => Ok(BinaryOp::Add),
        1 => Ok(BinaryOp::Subtract),
        2 => Ok(BinaryOp::Multiply),
        3 => Ok(BinaryOp::Divide),
        4 => Ok(BinaryOp::Remainder),
        5 => Ok(BinaryOp::Equal),
        6 => Ok(BinaryOp::NotEqual),
        7 => Ok(BinaryOp::Greater),
        8 => Ok(BinaryOp::GreaterEqual),
        9 => Ok(BinaryOp::Less),
        10 => Ok(BinaryOp::LessEqual),
        _ => Err(BytecodeFormatError::new(format!(
            "invalid binary operator {value}"
        ))),
    }
}

fn write_fields(writer: &mut Writer, fields: &[(String, usize)]) -> Result<()> {
    writer.len(fields.len(), "record fields")?;
    for (name, register) in fields {
        writer.string(name)?;
        writer.index(*register, "field register")?;
    }
    Ok(())
}

fn read_fields(reader: &mut Reader<'_>) -> Result<Vec<(String, usize)>> {
    let count = reader.len()?;
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        fields.push((reader.string()?, reader.index()?));
    }
    Ok(fields)
}

fn write_instruction(writer: &mut Writer, value: &SpannedInstruction) -> Result<()> {
    writer.span(value.span)?;
    let i = &value.instruction;
    match i {
        Instruction::LoadConstant {
            destination,
            constant,
        } => {
            writer.u8(0);
            writer.index(*destination, "destination")?;
            writer.index(*constant, "constant")?;
        }
        Instruction::LoadFunction {
            destination,
            function,
        } => {
            writer.u8(1);
            writer.index(*destination, "destination")?;
            writer.index(*function, "function")?;
        }
        Instruction::BindMethod {
            destination,
            function,
            receiver,
        } => {
            writer.u8(2);
            writer.index(*destination, "destination")?;
            writer.index(*function, "function")?;
            writer.index(*receiver, "receiver")?;
        }
        Instruction::BorrowTemporary {
            destination,
            source,
            mutable,
        } => {
            writer.u8(3);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
            writer.bool(*mutable);
        }
        Instruction::Reborrow {
            destination,
            source,
            mutable,
        } => {
            writer.u8(4);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
            writer.bool(*mutable);
        }
        Instruction::CreateClosure {
            destination,
            function,
            captures,
        } => {
            writer.u8(5);
            writer.index(*destination, "destination")?;
            writer.index(*function, "function")?;
            writer.indices(captures)?;
        }
        Instruction::TakeLocal { destination, local } => {
            writer.u8(6);
            writer.index(*destination, "destination")?;
            writer.index(*local, "local")?;
        }
        Instruction::TakePlace { destination, place } => {
            writer.u8(7);
            writer.index(*destination, "destination")?;
            write_place(writer, place)?;
        }
        Instruction::StoreLocal { local, source } => {
            writer.u8(8);
            writer.index(*local, "local")?;
            writer.index(*source, "source")?;
        }
        Instruction::InitLocal { local, source } => {
            writer.u8(9);
            writer.index(*local, "local")?;
            writer.index(*source, "source")?;
        }
        Instruction::DropLocal { local } => {
            writer.u8(10);
            writer.index(*local, "local")?;
        }
        Instruction::BorrowLocal {
            destination,
            local,
            mutable,
        } => {
            writer.u8(11);
            writer.index(*destination, "destination")?;
            writer.index(*local, "local")?;
            writer.bool(*mutable);
        }
        Instruction::BorrowPlace {
            destination,
            place,
            mutable,
        } => {
            writer.u8(12);
            writer.index(*destination, "destination")?;
            write_place(writer, place)?;
            writer.bool(*mutable);
        }
        Instruction::Dereference {
            destination,
            source,
        } => {
            writer.u8(13);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::StoreDereference { reference, source } => {
            writer.u8(14);
            writer.index(*reference, "reference")?;
            writer.index(*source, "source")?;
        }
        Instruction::StorePlace { place, source } => {
            writer.u8(15);
            write_place(writer, place)?;
            writer.index(*source, "source")?;
        }
        Instruction::IntoIterator {
            destination,
            source,
        } => {
            writer.u8(16);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::Move {
            destination,
            source,
        } => {
            writer.u8(17);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::Unary {
            destination,
            operator,
            operand,
        } => {
            writer.u8(18);
            writer.index(*destination, "destination")?;
            writer.u8(write_unary(*operator));
            writer.index(*operand, "operand")?;
        }
        Instruction::Cast {
            destination,
            source,
            target,
        } => {
            writer.u8(42);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
            writer.u8(write_integer_type(*target));
        }
        Instruction::Binary {
            destination,
            left,
            operator,
            right,
        } => {
            writer.u8(19);
            writer.index(*destination, "destination")?;
            writer.index(*left, "left")?;
            writer.u8(write_binary(*operator));
            writer.index(*right, "right")?;
        }
        Instruction::IntegerBinary {
            destination,
            left,
            operator,
            right,
            integer,
        } => {
            writer.u8(44);
            writer.index(*destination, "destination")?;
            writer.index(*left, "left")?;
            writer.u8(write_binary(*operator));
            writer.index(*right, "right")?;
            writer.u8(write_integer_type(*integer));
        }
        Instruction::Call {
            destination,
            function,
            arguments,
        } => {
            writer.u8(20);
            writer.index(*destination, "destination")?;
            writer.index(*function, "function")?;
            writer.indices(arguments)?;
        }
        Instruction::CallValue {
            destination,
            callee,
            arguments,
        } => {
            writer.u8(21);
            writer.index(*destination, "destination")?;
            writer.index(*callee, "callee")?;
            writer.indices(arguments)?;
        }
        Instruction::CallImport {
            destination,
            import,
            arguments,
        } => {
            writer.u8(22);
            writer.index(*destination, "destination")?;
            writer.index(*import, "import")?;
            writer.indices(arguments)?;
        }
        Instruction::CallIntrinsic {
            destination,
            intrinsic,
            target,
            arguments,
        } => {
            writer.u8(43);
            writer.index(*destination, "destination")?;
            writer.u16(*intrinsic as u16);
            writer.bool(target.is_some());
            if let Some(target) = target {
                writer.u8(write_integer_type(*target));
            }
            writer.indices(arguments)?;
        }
        Instruction::ConstructRecord {
            destination,
            type_id,
            variant,
            fields,
        } => {
            writer.u8(23);
            writer.index(*destination, "destination")?;
            writer.index(*type_id, "type")?;
            writer.bool(variant.is_some());
            if let Some(v) = variant {
                writer.string(v)?;
            }
            write_fields(writer, fields)?;
        }
        Instruction::ConstructTupleVariant {
            destination,
            type_id,
            variant,
            fields,
        } => {
            writer.u8(24);
            writer.index(*destination, "destination")?;
            writer.index(*type_id, "type")?;
            writer.string(variant)?;
            writer.indices(fields)?;
        }
        Instruction::ConstructUnitVariant {
            destination,
            type_id,
            variant,
        } => {
            writer.u8(25);
            writer.index(*destination, "destination")?;
            writer.index(*type_id, "type")?;
            writer.string(variant)?;
        }
        Instruction::BuildTuple {
            destination,
            elements,
        } => {
            writer.u8(26);
            writer.index(*destination, "destination")?;
            writer.indices(elements)?;
        }
        Instruction::BuildArray {
            destination,
            elements,
        } => {
            writer.u8(27);
            writer.index(*destination, "destination")?;
            writer.indices(elements)?;
        }
        Instruction::BuildRepeatArray {
            destination,
            value,
            count,
        } => {
            writer.u8(28);
            writer.index(*destination, "destination")?;
            writer.index(*value, "value")?;
            writer.index(*count, "repeat count")?;
        }
        Instruction::BuildRange {
            destination,
            start,
            end,
        } => {
            writer.u8(29);
            writer.index(*destination, "destination")?;
            writer.index(*start, "start")?;
            writer.index(*end, "end")?;
        }
        Instruction::BuildOptionNone { destination } => {
            writer.u8(30);
            writer.index(*destination, "destination")?;
        }
        Instruction::BuildOptionSome {
            destination,
            source,
        } => {
            writer.u8(31);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::BuildResultOk {
            destination,
            source,
        } => {
            writer.u8(32);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::BuildResultErr {
            destination,
            source,
        } => {
            writer.u8(33);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::TryResult {
            destination,
            source,
        } => {
            writer.u8(34);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::MatchPattern {
            destination,
            source,
            pattern,
        } => {
            writer.u8(35);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
            write_pattern(writer, pattern, 0)?;
        }
        Instruction::BindPattern { source, pattern } => {
            writer.u8(36);
            writer.index(*source, "source")?;
            write_pattern(writer, pattern, 0)?;
        }
        Instruction::Jump { target } => {
            writer.u8(37);
            writer.index(*target, "jump target")?;
        }
        Instruction::Branch {
            condition,
            then_target,
            else_target,
        } => {
            writer.u8(38);
            writer.index(*condition, "condition")?;
            writer.index(*then_target, "then target")?;
            writer.index(*else_target, "else target")?;
        }
        Instruction::IteratorNext {
            iterator,
            destination,
            some_target,
            none_target,
        } => {
            writer.u8(39);
            writer.index(*iterator, "iterator")?;
            writer.index(*destination, "destination")?;
            writer.index(*some_target, "some target")?;
            writer.index(*none_target, "none target")?;
        }
        Instruction::Return { source } => {
            writer.u8(40);
            writer.index(*source, "source")?;
        }
        Instruction::MatchFail => writer.u8(41),
    }
    Ok(())
}

fn read_instruction(reader: &mut Reader<'_>) -> Result<SpannedInstruction> {
    let span = reader.span()?;
    let instruction = match reader.u8()? {
        0 => Instruction::LoadConstant {
            destination: reader.index()?,
            constant: reader.index()?,
        },
        1 => Instruction::LoadFunction {
            destination: reader.index()?,
            function: reader.index()?,
        },
        2 => Instruction::BindMethod {
            destination: reader.index()?,
            function: reader.index()?,
            receiver: reader.index()?,
        },
        3 => Instruction::BorrowTemporary {
            destination: reader.index()?,
            source: reader.index()?,
            mutable: reader.bool()?,
        },
        4 => Instruction::Reborrow {
            destination: reader.index()?,
            source: reader.index()?,
            mutable: reader.bool()?,
        },
        5 => Instruction::CreateClosure {
            destination: reader.index()?,
            function: reader.index()?,
            captures: reader.indices()?,
        },
        6 => Instruction::TakeLocal {
            destination: reader.index()?,
            local: reader.index()?,
        },
        7 => Instruction::TakePlace {
            destination: reader.index()?,
            place: read_place(reader)?,
        },
        8 => Instruction::StoreLocal {
            local: reader.index()?,
            source: reader.index()?,
        },
        9 => Instruction::InitLocal {
            local: reader.index()?,
            source: reader.index()?,
        },
        10 => Instruction::DropLocal {
            local: reader.index()?,
        },
        11 => Instruction::BorrowLocal {
            destination: reader.index()?,
            local: reader.index()?,
            mutable: reader.bool()?,
        },
        12 => Instruction::BorrowPlace {
            destination: reader.index()?,
            place: read_place(reader)?,
            mutable: reader.bool()?,
        },
        13 => Instruction::Dereference {
            destination: reader.index()?,
            source: reader.index()?,
        },
        14 => Instruction::StoreDereference {
            reference: reader.index()?,
            source: reader.index()?,
        },
        15 => Instruction::StorePlace {
            place: read_place(reader)?,
            source: reader.index()?,
        },
        16 => Instruction::IntoIterator {
            destination: reader.index()?,
            source: reader.index()?,
        },
        17 => Instruction::Move {
            destination: reader.index()?,
            source: reader.index()?,
        },
        18 => Instruction::Unary {
            destination: reader.index()?,
            operator: read_unary(reader.u8()?)?,
            operand: reader.index()?,
        },
        19 => Instruction::Binary {
            destination: reader.index()?,
            left: reader.index()?,
            operator: read_binary(reader.u8()?)?,
            right: reader.index()?,
        },
        20 => Instruction::Call {
            destination: reader.index()?,
            function: reader.index()?,
            arguments: reader.indices()?,
        },
        21 => Instruction::CallValue {
            destination: reader.index()?,
            callee: reader.index()?,
            arguments: reader.indices()?,
        },
        22 => Instruction::CallImport {
            destination: reader.index()?,
            import: reader.index()?,
            arguments: reader.indices()?,
        },
        23 => {
            let destination = reader.index()?;
            let type_id = reader.index()?;
            let variant = if reader.bool()? {
                Some(reader.string()?)
            } else {
                None
            };
            Instruction::ConstructRecord {
                destination,
                type_id,
                variant,
                fields: read_fields(reader)?,
            }
        }
        24 => Instruction::ConstructTupleVariant {
            destination: reader.index()?,
            type_id: reader.index()?,
            variant: reader.string()?,
            fields: reader.indices()?,
        },
        25 => Instruction::ConstructUnitVariant {
            destination: reader.index()?,
            type_id: reader.index()?,
            variant: reader.string()?,
        },
        26 => Instruction::BuildTuple {
            destination: reader.index()?,
            elements: reader.indices()?,
        },
        27 => Instruction::BuildArray {
            destination: reader.index()?,
            elements: reader.indices()?,
        },
        28 => Instruction::BuildRepeatArray {
            destination: reader.index()?,
            value: reader.index()?,
            count: reader.index()?,
        },
        29 => Instruction::BuildRange {
            destination: reader.index()?,
            start: reader.index()?,
            end: reader.index()?,
        },
        30 => Instruction::BuildOptionNone {
            destination: reader.index()?,
        },
        31 => Instruction::BuildOptionSome {
            destination: reader.index()?,
            source: reader.index()?,
        },
        32 => Instruction::BuildResultOk {
            destination: reader.index()?,
            source: reader.index()?,
        },
        33 => Instruction::BuildResultErr {
            destination: reader.index()?,
            source: reader.index()?,
        },
        34 => Instruction::TryResult {
            destination: reader.index()?,
            source: reader.index()?,
        },
        35 => Instruction::MatchPattern {
            destination: reader.index()?,
            source: reader.index()?,
            pattern: read_pattern(reader)?,
        },
        36 => Instruction::BindPattern {
            source: reader.index()?,
            pattern: read_pattern(reader)?,
        },
        37 => Instruction::Jump {
            target: reader.index()?,
        },
        38 => Instruction::Branch {
            condition: reader.index()?,
            then_target: reader.index()?,
            else_target: reader.index()?,
        },
        39 => Instruction::IteratorNext {
            iterator: reader.index()?,
            destination: reader.index()?,
            some_target: reader.index()?,
            none_target: reader.index()?,
        },
        40 => Instruction::Return {
            source: reader.index()?,
        },
        41 => Instruction::MatchFail,
        42 => Instruction::Cast {
            destination: reader.index()?,
            source: reader.index()?,
            target: read_integer_type(reader.u8()?)?,
        },
        43 => {
            let destination = reader.index()?;
            let raw_intrinsic = reader.u16()?;
            let intrinsic = rils_builtins::INTEGER_INTRINSICS
                .iter()
                .chain(rils_builtins::FLOAT_INTRINSICS)
                .find(|item| item.id as u16 == raw_intrinsic)
                .map(|item| item.id)
                .ok_or_else(|| {
                    BytecodeFormatError::new(format!("invalid intrinsic id {raw_intrinsic}"))
                })?;
            let target = reader
                .bool()?
                .then(|| read_integer_type(reader.u8()?))
                .transpose()?;
            Instruction::CallIntrinsic {
                destination,
                intrinsic,
                target,
                arguments: reader.indices()?,
            }
        }
        44 => Instruction::IntegerBinary {
            destination: reader.index()?,
            left: reader.index()?,
            operator: read_binary(reader.u8()?)?,
            right: reader.index()?,
            integer: read_integer_type(reader.u8()?)?,
        },
        value => {
            return Err(BytecodeFormatError::new(format!(
                "invalid instruction opcode {value}"
            )));
        }
    };
    Ok(SpannedInstruction { instruction, span })
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
mod tests {
    use super::*;

    #[test]
    fn round_trip_executes_the_same_module() {
        let module = crate::bytecode::compile(
            r#"
                struct Pair { left: i32, right: i32 }
                struct CounterRange { current: i32, end: i32 }
                enum Choice { None, Some(i32), Named { value: i32 } }
                impl Iterator for CounterRange {
                    type Item = i32;
                    fn next(&mut self) -> Option<i32> {
                        if self.current < self.end {
                            let value = self.current;
                            let end = self.end;
                            *self = CounterRange { current: value + 1, end: end };
                            Some(value)
                        } else { None }
                    }
                }
                fn calculate() -> i32 {
                    let values = [1, 2, 3];
                    let mut total = 0;
                    for value in values { total = total + value; }
                    for value in CounterRange { current: 1, end: 4 } {
                        total = total + value;
                    }
                    let _kind = type_of(total);
                    let pair = Pair { left: total, right: 4 };
                    match Choice::Named { value: pair.left } {
                        Choice::Named { value } => value + pair.right,
                        _ => 0,
                    }
                }
                calculate()
            "#,
        )
        .expect("source compiles");
        let bytes = module.to_bytes().expect("module serializes");
        let loaded = BytecodeModule::from_bytes(&bytes).expect("module loads");
        let value = loaded.execute().expect("module runs");
        assert_eq!(value, crate::Value::I32(16));
    }

    #[test]
    fn round_trip_preserves_source_ids_and_rejects_unknown_span_sources() {
        let source_id = SourceId::new(9);
        let tokens = crate::lexer::lex_with_source_id("1 / 0", source_id).unwrap();
        let program = crate::parser::parse(tokens).unwrap();
        let module = crate::bytecode::compile_program_with_host_and_sources(
            &program,
            &crate::HostContract::new(),
            vec![SourceFile {
                id: source_id,
                name: "math.rils".into(),
            }],
        )
        .unwrap();
        let mut bytes = module.to_bytes().unwrap();
        let loaded = BytecodeModule::from_bytes(&bytes).unwrap();
        let error = loaded.execute().unwrap_err();
        assert_eq!(error.span.source, source_id);
        assert_eq!(loaded.source_name(source_id), Some("math.rils"));

        let section_count = u16::from_le_bytes(bytes[22..24].try_into().unwrap()) as usize;
        let directory_end = HEADER_LEN + section_count * DIRECTORY_ENTRY_LEN;
        let functions_entry = (0..section_count)
            .find_map(|index| {
                let start = HEADER_LEN + index * DIRECTORY_ENTRY_LEN;
                (u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap())
                    == SECTION_FUNCTIONS)
                    .then_some(start)
            })
            .unwrap();
        let functions_offset = u32::from_le_bytes(
            bytes[functions_entry + 4..functions_entry + 8]
                .try_into()
                .unwrap(),
        ) as usize;
        let mut reader = Reader::new(&bytes[functions_offset..]);
        let _function_count = reader.index().unwrap();
        let _name = reader.string().unwrap();
        let _exported = reader.bool().unwrap();
        let _constants = reader.collection(read_constant).unwrap();
        let _instruction_count = reader.index().unwrap();
        let instruction_source_offset = functions_offset + reader.position;
        bytes[instruction_source_offset..instruction_source_offset + 4]
            .copy_from_slice(&999_u32.to_le_bytes());
        let checksum = crc32(&bytes[directory_end..]);
        bytes[28..32].copy_from_slice(&checksum.to_le_bytes());
        let error = match BytecodeModule::from_bytes(&bytes) {
            Ok(_) => panic!("unknown span source should be rejected"),
            Err(error) => error,
        };
        assert!(error.message.contains("unknown source"));
    }

    #[test]
    fn rejects_corrupted_payload() {
        let module = crate::bytecode::compile("1 + 2").expect("source compiles");
        let mut bytes = module.to_bytes().expect("module serializes");
        *bytes.last_mut().expect("payload exists") ^= 0xff;
        let error = BytecodeModule::from_bytes(&bytes)
            .err()
            .expect("corruption rejected");
        assert!(error.message.contains("checksum"));
    }

    #[test]
    fn rejects_invalid_instruction_after_checksum_is_updated() {
        let module = crate::bytecode::compile("1 + 2").expect("source compiles");
        let mut bytes = module.to_bytes().expect("module serializes");
        let section_count = u16::from_le_bytes(bytes[22..24].try_into().unwrap()) as usize;
        let directory_end = HEADER_LEN + section_count * DIRECTORY_ENTRY_LEN;
        let functions_entry = (0..section_count)
            .find_map(|index| {
                let start = HEADER_LEN + index * DIRECTORY_ENTRY_LEN;
                (u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap())
                    == SECTION_FUNCTIONS)
                    .then_some(start)
            })
            .unwrap();
        let functions_offset = u32::from_le_bytes(
            bytes[functions_entry + 4..functions_entry + 8]
                .try_into()
                .unwrap(),
        ) as usize;
        // Skip function count, name, exported flag, constants, then overwrite the first opcode.
        let mut reader = Reader::new(&bytes[functions_offset..]);
        let _function_count = reader.index().unwrap();
        let _name = reader.string().unwrap();
        let _exported = reader.bool().unwrap();
        let constants = reader.collection(read_constant).unwrap();
        assert!(!constants.is_empty());
        let _instruction_count = reader.index().unwrap();
        let _span = reader.span().unwrap();
        let opcode_offset = functions_offset + reader.position;
        bytes[opcode_offset] = 0xff;
        let checksum = crc32(&bytes[directory_end..]);
        bytes[28..32].copy_from_slice(&checksum.to_le_bytes());
        let error = BytecodeModule::from_bytes(&bytes)
            .err()
            .expect("invalid opcode rejected");
        assert!(error.message.contains("opcode"));
    }

    #[test]
    fn rejects_excessive_register_allocation_before_execution() {
        let module = crate::bytecode::compile("1 + 2").expect("source compiles");
        let mut bytes = module.to_bytes().expect("module serializes");
        let section_count = u16::from_le_bytes(bytes[22..24].try_into().unwrap()) as usize;
        let directory_end = HEADER_LEN + section_count * DIRECTORY_ENTRY_LEN;
        let functions_entry = (0..section_count)
            .find_map(|index| {
                let start = HEADER_LEN + index * DIRECTORY_ENTRY_LEN;
                (u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap())
                    == SECTION_FUNCTIONS)
                    .then_some(start)
            })
            .unwrap();
        let functions_offset = u32::from_le_bytes(
            bytes[functions_entry + 4..functions_entry + 8]
                .try_into()
                .unwrap(),
        ) as usize;
        let mut reader = Reader::new(&bytes[functions_offset..]);
        let _function_count = reader.index().unwrap();
        let _name = reader.string().unwrap();
        let _exported = reader.bool().unwrap();
        let _constants = reader.collection(read_constant).unwrap();
        let _instructions = reader.collection(read_instruction).unwrap();
        let register_count_offset = functions_offset + reader.position;
        bytes[register_count_offset..register_count_offset + 4]
            .copy_from_slice(&((MAX_REGISTERS_PER_FUNCTION as u32) + 1).to_le_bytes());
        let checksum = crc32(&bytes[directory_end..]);
        bytes[28..32].copy_from_slice(&checksum.to_le_bytes());

        let error = BytecodeModule::from_bytes(&bytes)
            .err()
            .expect("excessive register allocation rejected");
        assert!(error.message.contains("register count"));
    }
}
