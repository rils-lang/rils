use super::*;

pub(super) fn encode_container<const N: usize>(sections: [(u16, Vec<u8>); N]) -> Result<Vec<u8>> {
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

pub(super) fn decode_container(bytes: &[u8]) -> Result<HashMap<u16, &[u8]>> {
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

pub(super) fn section_reader<'a>(
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

pub(super) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
