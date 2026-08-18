use crate::{
    source::{SourceId, Span},
    token::{Token, TokenKind},
};

#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    lex_with_source_id(source, SourceId::UNKNOWN)
}

pub fn lex_with_source_id(source: &str, source_id: SourceId) -> Result<Vec<Token>, LexError> {
    Lexer::new(source, source_id).scan_tokens()
}

struct Lexer<'a> {
    source: &'a str,
    source_id: SourceId,
    start: usize,
    current: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str, source_id: SourceId) -> Self {
        Self {
            source,
            source_id,
            start: 0,
            current: 0,
            tokens: Vec::new(),
        }
    }

    fn scan_tokens(mut self) -> Result<Vec<Token>, LexError> {
        while !self.is_at_end() {
            self.start = self.current;
            self.scan_token()?;
        }
        self.tokens.push(Token::new(
            TokenKind::Eof,
            self.span(self.current, self.current),
        ));
        Ok(self.tokens)
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::in_source(self.source_id, start, end)
    }

    fn scan_token(&mut self) -> Result<(), LexError> {
        let ch = self.advance().expect("not at end");
        match ch {
            '(' => self.add(TokenKind::LeftParen),
            ')' => self.add(TokenKind::RightParen),
            '{' => self.add(TokenKind::LeftBrace),
            '}' => self.add(TokenKind::RightBrace),
            '[' => self.add(TokenKind::LeftBracket),
            ']' => self.add(TokenKind::RightBracket),
            ',' => self.add(TokenKind::Comma),
            ':' => {
                let kind = if self.take(':') {
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                };
                self.add(kind);
            }
            '.' => {
                let kind = if self.take('.') {
                    TokenKind::DotDot
                } else {
                    TokenKind::Dot
                };
                self.add(kind);
            }
            ';' => self.add(TokenKind::Semicolon),
            '+' => self.add(TokenKind::Plus),
            '*' => self.add(TokenKind::Star),
            '%' => self.add(TokenKind::Percent),
            '$' => self.add(TokenKind::Dollar),
            '#' => self.add(TokenKind::Hash),
            '-' => {
                let kind = if self.take('>') {
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                };
                self.add(kind);
            }
            '!' => {
                let kind = if self.take('=') {
                    TokenKind::BangEqual
                } else {
                    TokenKind::Bang
                };
                self.add(kind);
            }
            '?' => self.add(TokenKind::Question),
            '=' => {
                let kind = if self.take('>') {
                    TokenKind::FatArrow
                } else if self.take('=') {
                    TokenKind::EqualEqual
                } else {
                    TokenKind::Equal
                };
                self.add(kind);
            }
            '<' => {
                let kind = if self.take('=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                };
                self.add(kind);
            }
            '>' => {
                let kind = if self.take('=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                };
                self.add(kind);
            }
            '&' => {
                let kind = if self.take('&') {
                    TokenKind::AndAnd
                } else {
                    TokenKind::Ampersand
                };
                self.add(kind);
            }
            '|' if self.take('|') => self.add(TokenKind::OrOr),
            '/' if self.take('/') => {
                while self.peek().is_some_and(|next| next != '\n') {
                    self.advance();
                }
            }
            '/' => self.add(TokenKind::Slash),
            ' ' | '\r' | '\t' | '\n' => {}
            '"' => self.string()?,
            '\'' => self.character()?,
            ch if ch.is_ascii_digit() => self.number()?,
            ch if is_identifier_start(ch) => self.identifier(),
            _ => {
                return Err(self.error(format!("unexpected character `{ch}`")));
            }
        }
        Ok(())
    }

    fn string(&mut self) -> Result<(), LexError> {
        let mut value = String::new();
        loop {
            match self.advance() {
                Some('"') => break,
                Some('\\') => match self.advance() {
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some(other) => {
                        return Err(self.error(format!("unsupported escape sequence `\\{other}`")));
                    }
                    None => return Err(self.error("unterminated string".into())),
                },
                Some(ch) => value.push(ch),
                None => return Err(self.error("unterminated string".into())),
            }
        }
        self.add(TokenKind::String(value));
        Ok(())
    }

    fn character(&mut self) -> Result<(), LexError> {
        let value = match self.advance() {
            Some('\\') => match self.advance() {
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                Some('0') => '\0',
                Some('\'') => '\'',
                Some('\\') => '\\',
                Some(other) => {
                    return Err(self.error(format!("unsupported escape sequence `\\{other}`")));
                }
                None => return Err(self.error("unterminated character literal".into())),
            },
            Some('\'') | None => return Err(self.error("empty character literal".into())),
            Some(value) => value,
        };
        if self.advance() != Some('\'') {
            return Err(self.error("character literal must contain exactly one character".into()));
        }
        self.add(TokenKind::Char(value));
        Ok(())
    }

    fn number(&mut self) -> Result<(), LexError> {
        while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            self.advance();
        }

        let is_float =
            self.peek() == Some('.') && self.peek_next().is_some_and(|next| next.is_ascii_digit());
        if is_float {
            self.advance();
            while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                self.advance();
            }
        }

        let number_end = self.current;
        while self.peek().is_some_and(is_identifier_continue) {
            self.advance();
        }
        let text = &self.source[self.start..number_end];
        let raw_suffix = &self.source[number_end..self.current];
        let suffix = match raw_suffix.strip_prefix('_') {
            Some("") | None => raw_suffix,
            Some(suffix) => suffix,
        };
        let invalid = || self.error(format!("invalid numeric literal suffix `{raw_suffix}`"));
        let kind = match (is_float, suffix) {
            (true, "") => TokenKind::Float(
                text.parse()
                    .map_err(|_| self.error("invalid f64 literal".into()))?,
            ),
            (true, "f64") => TokenKind::F64(
                text.parse()
                    .map_err(|_| self.error("invalid f64 literal".into()))?,
            ),
            (true, "f32") => TokenKind::F32(
                text.parse()
                    .map_err(|_| self.error("invalid f32 literal".into()))?,
            ),
            (false, "f64") => TokenKind::F64(
                text.parse()
                    .map_err(|_| self.error("invalid f64 literal".into()))?,
            ),
            (false, "f32") => TokenKind::F32(
                text.parse()
                    .map_err(|_| self.error("invalid f32 literal".into()))?,
            ),
            (false, "") => TokenKind::Integer(
                text.parse()
                    .map_err(|_| self.error("integer literal is out of range".into()))?,
            ),
            (false, "i32") => TokenKind::I32(
                text.parse()
                    .map_err(|_| self.error("i32 literal is out of range".into()))?,
            ),
            (false, "i8") => TokenKind::I8(text.parse().map_err(|_| invalid())?),
            (false, "i16") => TokenKind::I16(text.parse().map_err(|_| invalid())?),
            (false, "i64") => TokenKind::I64(text.parse().map_err(|_| invalid())?),
            (false, "i128") => TokenKind::I128(text.parse().map_err(|_| invalid())?),
            (false, "isize") => TokenKind::Isize(text.parse().map_err(|_| invalid())?),
            (false, "u8") => TokenKind::U8(text.parse().map_err(|_| invalid())?),
            (false, "u16") => TokenKind::U16(text.parse().map_err(|_| invalid())?),
            (false, "u32") => TokenKind::U32(text.parse().map_err(|_| invalid())?),
            (false, "u64") => TokenKind::U64(text.parse().map_err(|_| invalid())?),
            (false, "u128") => TokenKind::U128(text.parse().map_err(|_| invalid())?),
            (false, "usize") => TokenKind::Usize(text.parse().map_err(|_| invalid())?),
            _ => return Err(invalid()),
        };
        self.add(kind);
        Ok(())
    }

    fn identifier(&mut self) {
        while self.peek().is_some_and(is_identifier_continue) {
            self.advance();
        }
        let text = &self.source[self.start..self.current];
        let kind = match text {
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "fn" => TokenKind::Fn,
            "macro" => TokenKind::Macro,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "loop" => TokenKind::Loop,
            "match" => TokenKind::Match,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "impl" => TokenKind::Impl,
            "trait" => TokenKind::Trait,
            "type" => TokenKind::Type,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "as" => TokenKind::As,
            "return" => TokenKind::Return,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "mod" => TokenKind::Mod,
            "use" => TokenKind::Use,
            "crate" => TokenKind::Crate,
            "super" => TokenKind::Super,
            "pub" => TokenKind::Pub,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "nil" => TokenKind::Nil,
            _ => TokenKind::Identifier(text.to_owned()),
        };
        self.add(kind);
    }

    fn add(&mut self, kind: TokenKind) {
        self.tokens
            .push(Token::new(kind, self.span(self.start, self.current)));
    }

    fn error(&self, message: String) -> LexError {
        LexError {
            message,
            span: self.span(self.start, self.current),
        }
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.current += ch.len_utf8();
        Some(ch)
    }

    fn take(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.current..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.source[self.current..].chars();
        chars.next()?;
        chars.next()
    }
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_keywords_numbers_and_comments() {
        let tokens = lex("let mut answer = 40 + 2.5; // ok").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Let));
        assert!(matches!(tokens[1].kind, TokenKind::Mut));
        assert!(matches!(tokens[2].kind, TokenKind::Identifier(_)));
        assert!(matches!(tokens[4].kind, TokenKind::Integer(40)));
        assert!(matches!(tokens[6].kind, TokenKind::Float(value) if value == 2.5));
    }

    #[test]
    fn scans_multi_character_arrows_and_comparisons() {
        let tokens = lex("-> <= => >= == !=").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Arrow));
        assert!(matches!(tokens[1].kind, TokenKind::LessEqual));
        assert!(matches!(tokens[2].kind, TokenKind::FatArrow));
        assert!(matches!(tokens[3].kind, TokenKind::GreaterEqual));
        assert!(matches!(tokens[4].kind, TokenKind::EqualEqual));
        assert!(matches!(tokens[5].kind, TokenKind::BangEqual));
    }

    #[test]
    fn scans_macro_keyword_and_metavariables() {
        let tokens = lex("macro twice($value) { $value + $value }").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Macro));
        assert!(matches!(tokens[3].kind, TokenKind::Dollar));
        assert!(matches!(tokens[4].kind, TokenKind::Identifier(_)));
    }
}
