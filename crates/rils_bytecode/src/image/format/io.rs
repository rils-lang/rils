use super::*;

#[derive(Default)]
pub(super) struct Writer(pub(super) Vec<u8>);

impl Writer {
    pub(super) fn finish(self) -> Vec<u8> {
        self.0
    }
    pub(super) fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    pub(super) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }
    pub(super) fn u16(&mut self, value: u16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(super) fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(super) fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(super) fn u128(&mut self, value: u128) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(super) fn i8(&mut self, value: i8) {
        self.u8(value as u8);
    }
    pub(super) fn i16(&mut self, value: i16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(super) fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(super) fn i64(&mut self, value: i64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(super) fn i128(&mut self, value: i128) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(super) fn index(&mut self, value: usize, label: &str) -> Result<()> {
        self.u32(
            u32::try_from(value)
                .map_err(|_| BytecodeFormatError::new(format!("{label} exceeds u32")))?,
        );
        Ok(())
    }
    pub(super) fn len(&mut self, value: usize, label: &str) -> Result<()> {
        if value > MAX_COLLECTION_ITEMS {
            return Err(BytecodeFormatError::new(format!(
                "{label} exceeds item limit"
            )));
        }
        self.index(value, label)
    }
    pub(super) fn string(&mut self, value: &str) -> Result<()> {
        if value.len() > MAX_STRING_BYTES {
            return Err(BytecodeFormatError::new("string exceeds byte limit"));
        }
        self.index(value.len(), "string length")?;
        self.0.extend_from_slice(value.as_bytes());
        Ok(())
    }
    pub(super) fn span(&mut self, span: Span) -> Result<()> {
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
    pub(super) fn collection<T>(
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
    pub(super) fn indices(&mut self, values: &[usize]) -> Result<()> {
        self.len(values.len(), "index collection")?;
        for value in values {
            self.index(*value, "index")?;
        }
        Ok(())
    }
    pub(super) fn option_index(&mut self, value: Option<usize>, label: &str) -> Result<()> {
        self.bool(value.is_some());
        if let Some(value) = value {
            self.index(value, label)?;
        }
        Ok(())
    }
}

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    pub(super) position: usize,
    depth: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            depth: 0,
        }
    }
    pub(super) fn take(&mut self, count: usize) -> Result<&'a [u8]> {
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
    pub(super) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub(super) fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(BytecodeFormatError::new(format!("invalid boolean {value}"))),
        }
    }
    pub(super) fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("length checked"),
        ))
    }
    pub(super) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("length checked"),
        ))
    }
    pub(super) fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("length checked"),
        ))
    }
    pub(super) fn u128(&mut self) -> Result<u128> {
        Ok(u128::from_le_bytes(
            self.take(16)?.try_into().expect("length checked"),
        ))
    }
    pub(super) fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    pub(super) fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(
            self.take(2)?.try_into().expect("length checked"),
        ))
    }
    pub(super) fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().expect("length checked"),
        ))
    }
    pub(super) fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(
            self.take(8)?.try_into().expect("length checked"),
        ))
    }
    pub(super) fn i128(&mut self) -> Result<i128> {
        Ok(i128::from_le_bytes(
            self.take(16)?.try_into().expect("length checked"),
        ))
    }
    pub(super) fn index(&mut self) -> Result<usize> {
        Ok(self.u32()? as usize)
    }
    pub(super) fn len(&mut self) -> Result<usize> {
        let value = self.index()?;
        if value > MAX_COLLECTION_ITEMS {
            return Err(BytecodeFormatError::new("collection exceeds item limit"));
        }
        Ok(value)
    }
    pub(super) fn string(&mut self) -> Result<String> {
        let length = self.index()?;
        if length > MAX_STRING_BYTES {
            return Err(BytecodeFormatError::new("string exceeds byte limit"));
        }
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| BytecodeFormatError::new("string is not valid UTF-8"))
    }
    pub(super) fn span(&mut self) -> Result<Span> {
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
    pub(super) fn collection<T>(&mut self, read: fn(&mut Self) -> Result<T>) -> Result<Vec<T>> {
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
    pub(super) fn collection_limited<T>(
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
    pub(super) fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    pub(super) fn indices(&mut self) -> Result<Vec<usize>> {
        self.collection(Self::index)
    }
    pub(super) fn option_index(&mut self) -> Result<Option<usize>> {
        if self.bool()? {
            Ok(Some(self.index()?))
        } else {
            Ok(None)
        }
    }
    pub(super) fn nested<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
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
    pub(super) fn finish(&self) -> Result<()> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(BytecodeFormatError::new("trailing bytes in section"))
        }
    }
}
