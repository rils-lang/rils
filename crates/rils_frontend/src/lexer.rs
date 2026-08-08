use crate::{
    source::Span,
    token::{Token, TokenKind},
};

#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).scan_tokens()
}

struct Lexer<'a> {
    source: &'a str,
    start: usize,
    current: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
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
            Span::new(self.current, self.current),
        ));
        Ok(self.tokens)
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

        let text = &self.source[self.start..self.current];
        let kind = if is_float {
            TokenKind::Float(
                text.parse()
                    .map_err(|_| self.error("invalid floating-point number".into()))?,
            )
        } else {
            TokenKind::Integer(
                text.parse()
                    .map_err(|_| self.error("integer is out of range".into()))?,
            )
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
            .push(Token::new(kind, Span::new(self.start, self.current)));
    }

    fn error(&self, message: String) -> LexError {
        LexError {
            message,
            span: Span::new(self.start, self.current),
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
