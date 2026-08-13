use std::fmt;

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceId(pub u32);

impl SourceId {
    pub const UNKNOWN: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct SymbolId {
    pub source: SourceId,
    pub local: u32,
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn merge(self, other: Self) -> Self {
        Self::new(self.start, other.end)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

pub fn locate(source: &str, offset: usize) -> SourceLocation {
    let mut line = 1;
    let mut column = 1;

    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    SourceLocation { line, column }
}

pub fn format_diagnostic(source_name: &str, source: &str, span: Span, message: &str) -> String {
    let location = locate(source, span.start);
    let line_text = source
        .lines()
        .nth(location.line.saturating_sub(1))
        .unwrap_or("");
    let marker_len = span.end.saturating_sub(span.start).max(1);

    format!(
        "{message}\n  --> {source_name}:{}:{}\n   |\n{:>3} | {line_text}\n   | {}{}",
        location.line,
        location.column,
        location.line,
        " ".repeat(location.column.saturating_sub(1)),
        "^".repeat(marker_len.min(line_text.len().max(1)))
    )
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}
