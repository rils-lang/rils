use std::{error::Error, fmt, fs, io, path::Path};

use crate::{
    BYTECODE_HOST_ABI_VERSION, BYTECODE_LANGUAGE_VERSION, BytecodeFormatError, BytecodeModule,
};

const MAGIC: &[u8; 8] = b"RILSLIB\0";
const HEADER_LEN: usize = 64;
const FORMAT_VERSION: u16 = 1;
const HASH_ALGORITHM_FNV1A_128: u32 = 1;
const MAX_LIBRARY_BYTES: usize = 64 * 1024 * 1024;
const MAX_NAME_BYTES: usize = 255;

#[derive(Clone)]
pub struct RilsLibrary {
    name: String,
    module: BytecodeModule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryFormatError {
    pub message: String,
}

impl LibraryFormatError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn io(action: &str, path: &Path, error: io::Error) -> Self {
        Self::new(format!("failed to {action} `{}`: {error}", path.display()))
    }
}

impl fmt::Display for LibraryFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Rils library format error: {}", self.message)
    }
}

impl Error for LibraryFormatError {}

impl From<BytecodeFormatError> for LibraryFormatError {
    fn from(error: BytecodeFormatError) -> Self {
        Self::new(error.message)
    }
}

impl RilsLibrary {
    pub fn new(
        name: impl Into<String>,
        module: BytecodeModule,
    ) -> Result<Self, LibraryFormatError> {
        let name = name.into();
        validate_name(&name)?;
        module.to_bytes()?;
        Ok(Self { name, module })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn module(&self) -> &BytecodeModule {
        &self.module
    }

    pub fn content_hash(&self) -> Result<u128, LibraryFormatError> {
        let module = self.module.to_bytes()?;
        Ok(fnv1a128(self.name.as_bytes(), &module))
    }

    pub fn into_module(self) -> BytecodeModule {
        self.module
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, LibraryFormatError> {
        validate_name(&self.name)?;
        let module = self.module.to_bytes()?;
        let name = self.name.as_bytes();
        let total = HEADER_LEN
            .checked_add(name.len())
            .and_then(|value| value.checked_add(module.len()))
            .ok_or_else(|| LibraryFormatError::new("library size overflow"))?;
        if total > MAX_LIBRARY_BYTES {
            return Err(LibraryFormatError::new(format!(
                "library exceeds the {MAX_LIBRARY_BYTES} byte limit"
            )));
        }

        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(MAGIC);
        push_u16(&mut bytes, FORMAT_VERSION);
        push_u16(&mut bytes, HEADER_LEN as u16);
        push_u16(&mut bytes, BYTECODE_LANGUAGE_VERSION.0);
        push_u16(&mut bytes, BYTECODE_LANGUAGE_VERSION.1);
        push_u16(&mut bytes, BYTECODE_LANGUAGE_VERSION.2);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, BYTECODE_HOST_ABI_VERSION);
        bytes.push(usize::BITS as u8);
        bytes.push(0);
        push_u16(&mut bytes, 0);
        push_u32(&mut bytes, name.len() as u32);
        push_u32(&mut bytes, module.len() as u32);
        push_u32(&mut bytes, crc32(name, &module));
        push_u32(&mut bytes, HASH_ALGORITHM_FNV1A_128);
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(&fnv1a128(name, &module).to_le_bytes());
        debug_assert_eq!(bytes.len(), HEADER_LEN);
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(&module);
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LibraryFormatError> {
        if bytes.len() < HEADER_LEN {
            return Err(LibraryFormatError::new("library header is truncated"));
        }
        if bytes.len() > MAX_LIBRARY_BYTES {
            return Err(LibraryFormatError::new(format!(
                "library exceeds the {MAX_LIBRARY_BYTES} byte limit"
            )));
        }
        if &bytes[..8] != MAGIC {
            return Err(LibraryFormatError::new("invalid library magic"));
        }
        let format = read_u16(bytes, 8)?;
        if format != FORMAT_VERSION {
            return Err(LibraryFormatError::new(format!(
                "unsupported library format version {format}"
            )));
        }
        if read_u16(bytes, 10)? as usize != HEADER_LEN {
            return Err(LibraryFormatError::new("invalid library header size"));
        }
        let language = (
            read_u16(bytes, 12)?,
            read_u16(bytes, 14)?,
            read_u16(bytes, 16)?,
        );
        if language != BYTECODE_LANGUAGE_VERSION {
            return Err(LibraryFormatError::new(format!(
                "library language version {language:?} is incompatible with {BYTECODE_LANGUAGE_VERSION:?}"
            )));
        }
        if read_u16(bytes, 18)? != 0
            || bytes[25] != 0
            || read_u16(bytes, 26)? != 0
            || read_u32(bytes, 44)? != 0
        {
            return Err(LibraryFormatError::new(
                "library reserved fields must be zero",
            ));
        }
        let abi = read_u32(bytes, 20)?;
        if abi != BYTECODE_HOST_ABI_VERSION {
            return Err(LibraryFormatError::new(format!(
                "library requires host ABI {abi}, runtime provides {BYTECODE_HOST_ABI_VERSION}"
            )));
        }
        if bytes[24] != usize::BITS as u8 {
            return Err(LibraryFormatError::new(format!(
                "library targets {}-bit pointers, runtime uses {}-bit pointers",
                bytes[24],
                usize::BITS
            )));
        }
        let name_len = read_u32(bytes, 28)? as usize;
        let module_len = read_u32(bytes, 32)? as usize;
        let expected = HEADER_LEN
            .checked_add(name_len)
            .and_then(|value| value.checked_add(module_len))
            .ok_or_else(|| LibraryFormatError::new("library size overflow"))?;
        if expected != bytes.len() {
            return Err(LibraryFormatError::new(
                "library length does not match its header",
            ));
        }
        let name_bytes = &bytes[HEADER_LEN..HEADER_LEN + name_len];
        let module_bytes = &bytes[HEADER_LEN + name_len..];
        if read_u32(bytes, 36)? != crc32(name_bytes, module_bytes) {
            return Err(LibraryFormatError::new("library payload checksum mismatch"));
        }
        if read_u32(bytes, 40)? != HASH_ALGORITHM_FNV1A_128 {
            return Err(LibraryFormatError::new(
                "unsupported library content hash algorithm",
            ));
        }
        let stored_hash = u128::from_le_bytes(
            bytes[48..64]
                .try_into()
                .expect("the fixed library header was length checked"),
        );
        if stored_hash != fnv1a128(name_bytes, module_bytes) {
            return Err(LibraryFormatError::new("library content hash mismatch"));
        }
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| LibraryFormatError::new("library name is not valid UTF-8"))?;
        validate_name(name)?;
        let module = BytecodeModule::from_bytes(module_bytes)?;
        Self::new(name, module)
    }

    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<(), LibraryFormatError> {
        let path = path.as_ref();
        fs::write(path, self.to_bytes()?)
            .map_err(|error| LibraryFormatError::io("write", path, error))
    }

    pub fn read_file(path: impl AsRef<Path>) -> Result<Self, LibraryFormatError> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|error| LibraryFormatError::io("read", path, error))?;
        Self::from_bytes(&bytes)
    }
}

fn validate_name(name: &str) -> Result<(), LibraryFormatError> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES {
        return Err(LibraryFormatError::new(format!(
            "library name must contain 1 to {MAX_NAME_BYTES} UTF-8 bytes"
        )));
    }
    let mut characters = name.chars();
    if !characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        || !characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return Err(LibraryFormatError::new(format!(
            "invalid library name `{name}`"
        )));
    }
    Ok(())
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, LibraryFormatError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| LibraryFormatError::new("library header is truncated"))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, LibraryFormatError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| LibraryFormatError::new("library header is truncated"))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn crc32(name: &[u8], module: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in name.iter().chain(module) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320_u32 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn fnv1a128(name: &[u8], module: &[u8]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    name.iter().chain(module).fold(OFFSET, |hash, byte| {
        (hash ^ u128::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn library_round_trip_verifies_embedded_bytecode() {
        let module = crate::compile("pub fn answer() -> i32 { 42 }").unwrap();
        let library = RilsLibrary::new("sample", module).unwrap();
        let bytes = library.to_bytes().unwrap();
        let decoded = RilsLibrary::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.name(), "sample");
        assert_eq!(
            decoded.module().call("answer", Vec::new()).unwrap(),
            crate::Value::I32(42)
        );
    }

    #[test]
    fn library_rejects_corrupted_payload() {
        let module = crate::compile("42").unwrap();
        let library = RilsLibrary::new("sample", module).unwrap();
        let mut bytes = library.to_bytes().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        assert!(RilsLibrary::from_bytes(&bytes).is_err());
    }

    #[test]
    fn compiles_library_prelude_into_artifact() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rils-library-{unique}"));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("rils.toml"),
            "[project]\nname = \"sample\"\nsrc = \"src\"\n\n[lib]\nprelude = \"src/prelude.rils\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/prelude.rils"),
            "pub fn value() -> i32 { 42 }",
        )
        .unwrap();
        fs::write(root.join("src/module.rils"), "pub fn other() -> i32 { 1 }").unwrap();

        let library = crate::compile_library(root.join("rils.toml")).unwrap();
        assert_eq!(
            library.module().call("value", Vec::new()).unwrap(),
            crate::Value::I32(42)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compiles_library_prelude_file_as_an_asset_entry() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rils-prelude-entry-{unique}"));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("rils.toml"),
            "[project]\nname = \"sample\"\nsrc = \"src\"\n\n[lib]\nprelude = \"src/prelude.rils\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/prelude.rils"),
            "pub fn prelude_value() -> i32 { 42 }",
        )
        .unwrap();
        fs::write(
            root.join("src/module.rils"),
            "pub fn module_value() -> i32 { 7 }",
        )
        .unwrap();

        let module = crate::compile_file(root.join("src/prelude.rils")).unwrap();
        assert_eq!(
            module.call("prelude_value", Vec::new()).unwrap(),
            crate::Value::I32(42)
        );
        fs::remove_dir_all(root).unwrap();
    }
}
