use std::fmt;

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceId(pub u32);

impl SourceId {
    pub const UNKNOWN: Self = Self(0);

    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SymbolId {
    pub source: SourceId,
    pub local: u32,
}

/// Identifies a semantic definition within one compilation session.
///
/// `SymbolId` remains as a compatibility name for the editor API while the
/// compiler and semantic layers migrate to definition-oriented terminology.
pub type DefId = SymbolId;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExprId {
    pub source: SourceId,
    pub local: u32,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeRefId {
    pub source: SourceId,
    pub local: u32,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct PatternId {
    pub source: SourceId,
    pub local: u32,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BodyId(pub DefId);

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImplId {
    pub source: SourceId,
    pub local: u32,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleId(pub u32);

impl ModuleId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct Span {
    pub source: SourceId,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self::in_source(SourceId::UNKNOWN, start, end)
    }

    pub const fn in_source(source: SourceId, start: usize, end: usize) -> Self {
        Self { source, start, end }
    }

    pub const fn with_source(self, source: SourceId) -> Self {
        Self { source, ..self }
    }

    pub const fn merge(self, other: Self) -> Self {
        let source = if self.source.0 == 0 {
            other.source
        } else if other.source.0 == 0 || self.source.0 == other.source.0 {
            self.source
        } else {
            SourceId::UNKNOWN
        };
        Self::in_source(source, self.start, other.end)
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
        if self.source == SourceId::UNKNOWN {
            write!(f, "{}..{}", self.start, self.end)
        } else {
            write!(f, "source#{}:{}..{}", self.source.0, self.start, self.end)
        }
    }
}
