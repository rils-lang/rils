use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormatAlignment {
    #[default]
    Unspecified,
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormatKind {
    #[default]
    Display,
    Debug,
    Binary,
    Octal,
    LowerHex,
    UpperHex,
    LowerExp,
    UpperExp,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatSpec {
    pub fill: Option<char>,
    pub alignment: FormatAlignment,
    pub sign_plus: bool,
    pub alternate: bool,
    pub zero_pad: bool,
    pub width: Option<usize>,
    pub precision: Option<usize>,
    pub kind: FormatKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatPiece {
    Text(String),
    Placeholder { argument: usize, spec: FormatSpec },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatStringError {
    pub message: String,
    pub offset: usize,
}

impl fmt::Display for FormatStringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for FormatStringError {}

pub fn parse_format_string(source: &str) -> Result<Vec<FormatPiece>, FormatStringError> {
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut chars = source.char_indices().peekable();
    let mut implicit_argument = 0usize;
    let mut argument_mode = None;

    while let Some((offset, character)) = chars.next() {
        match character {
            '{' if chars.peek().is_some_and(|(_, next)| *next == '{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek().is_some_and(|(_, next)| *next == '}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                if !text.is_empty() {
                    pieces.push(FormatPiece::Text(std::mem::take(&mut text)));
                }
                let start = offset;
                let mut body = String::new();
                let mut closed = false;
                for (_, next) in chars.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    if next == '{' {
                        return Err(format_error("nested `{` in format placeholder", start));
                    }
                    body.push(next);
                }
                if !closed {
                    return Err(format_error("unclosed `{` in format string", start));
                }
                let (argument, explicit) = parse_argument(&body, implicit_argument, start)?;
                match argument_mode {
                    None => argument_mode = Some(explicit),
                    Some(mode) if mode != explicit => {
                        return Err(format_error(
                            "cannot mix implicit and explicit positional arguments",
                            start,
                        ));
                    }
                    _ => {}
                }
                if !explicit {
                    implicit_argument += 1;
                }
                let spec_source = body.split_once(':').map_or("", |(_, spec)| spec);
                pieces.push(FormatPiece::Placeholder {
                    argument,
                    spec: parse_spec(spec_source, start)?,
                });
            }
            '}' => return Err(format_error("unmatched `}` in format string", offset)),
            character => text.push(character),
        }
    }
    if !text.is_empty() {
        pieces.push(FormatPiece::Text(text));
    }
    Ok(pieces)
}

fn parse_argument(
    body: &str,
    implicit: usize,
    offset: usize,
) -> Result<(usize, bool), FormatStringError> {
    let argument = body.split_once(':').map_or(body, |(argument, _)| argument);
    if argument.is_empty() {
        return Ok((implicit, false));
    }
    argument
        .parse::<usize>()
        .map(|argument| (argument, true))
        .map_err(|_| format_error("format arguments must use numeric positions", offset))
}

fn parse_spec(source: &str, offset: usize) -> Result<FormatSpec, FormatStringError> {
    let mut spec = FormatSpec::default();
    let characters = source.chars().collect::<Vec<_>>();
    let mut current = 0usize;
    if characters
        .get(1)
        .is_some_and(|character| matches!(character, '<' | '^' | '>'))
    {
        spec.fill = characters.first().copied();
        spec.alignment = alignment(characters[1]);
        current = 2;
    } else if characters
        .first()
        .is_some_and(|character| matches!(character, '<' | '^' | '>'))
    {
        spec.alignment = alignment(characters[0]);
        current = 1;
    }
    if characters.get(current) == Some(&'+') {
        spec.sign_plus = true;
        current += 1;
    }
    if characters.get(current) == Some(&'#') {
        spec.alternate = true;
        current += 1;
    }
    if characters.get(current) == Some(&'0') {
        spec.zero_pad = true;
        current += 1;
    }
    let width_start = current;
    while characters.get(current).is_some_and(char::is_ascii_digit) {
        current += 1;
    }
    if current > width_start {
        spec.width = Some(parse_number(&characters[width_start..current], offset)?);
    }
    if characters.get(current) == Some(&'.') {
        current += 1;
        let precision_start = current;
        while characters.get(current).is_some_and(char::is_ascii_digit) {
            current += 1;
        }
        if current == precision_start {
            return Err(format_error("format precision requires a number", offset));
        }
        spec.precision = Some(parse_number(&characters[precision_start..current], offset)?);
    }
    if let Some(kind) = characters.get(current).copied() {
        spec.kind = match kind {
            '?' => FormatKind::Debug,
            'b' => FormatKind::Binary,
            'o' => FormatKind::Octal,
            'x' => FormatKind::LowerHex,
            'X' => FormatKind::UpperHex,
            'e' => FormatKind::LowerExp,
            'E' => FormatKind::UpperExp,
            _ => {
                return Err(format_error(
                    format!("unsupported format type `{kind}`"),
                    offset,
                ));
            }
        };
        current += 1;
    }
    if current != characters.len() {
        return Err(format_error("unsupported format specifier", offset));
    }
    if spec.alternate && spec.kind == FormatKind::Display {
        return Err(format_error(
            "`#` requires a typed format such as `?` or `x`",
            offset,
        ));
    }
    Ok(spec)
}

fn alignment(character: char) -> FormatAlignment {
    match character {
        '<' => FormatAlignment::Left,
        '^' => FormatAlignment::Center,
        '>' => FormatAlignment::Right,
        _ => FormatAlignment::Unspecified,
    }
}

fn parse_number(characters: &[char], offset: usize) -> Result<usize, FormatStringError> {
    characters
        .iter()
        .collect::<String>()
        .parse()
        .map_err(|_| format_error("format width or precision is too large", offset))
}

fn format_error(message: impl Into<String>, offset: usize) -> FormatStringError {
    FormatStringError {
        message: message.into(),
        offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rust_style_format_pieces() {
        let pieces = parse_format_string("value={:+08.2} debug={:#?} hex={:#x} {{ok}}")
            .expect("valid format string");
        assert_eq!(
            pieces
                .iter()
                .filter(|piece| matches!(piece, FormatPiece::Placeholder { .. }))
                .count(),
            3
        );
        assert!(matches!(
            &pieces[1],
            FormatPiece::Placeholder { argument: 0, spec } if spec.sign_plus && spec.zero_pad && spec.width == Some(8) && spec.precision == Some(2)
        ));
    }

    #[test]
    fn rejects_malformed_format_strings() {
        for source in ["{", "}", "{name}", "{} {1}", "{:#}", "{:.}"] {
            assert!(parse_format_string(source).is_err(), "{source}");
        }
    }
}
