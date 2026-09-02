use crate::token::{Token, TokenKind};

/// Bounds-safe cursor; end of input is represented by position, not a token.
pub(super) struct TokenStream {
    tokens: Vec<Token>,
    current: usize,
}

impl TokenStream {
    pub(super) fn new(mut tokens: Vec<Token>) -> Self {
        if matches!(tokens.last().map(|token| &token.kind), Some(TokenKind::Eof)) {
            tokens.pop();
        }
        Self { tokens, current: 0 }
    }
    pub(super) fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len()
    }
    pub(super) fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.current)?;
        self.current += 1;
        Some(token)
    }
    pub(super) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.current)
    }
    pub(super) fn previous(&self) -> Option<&Token> {
        self.current.checked_sub(1).and_then(|i| self.tokens.get(i))
    }
    pub(super) fn check(&self, kind: &TokenKind) -> bool {
        self.peek().is_some_and(|token| {
            std::mem::discriminant(&token.kind) == std::mem::discriminant(kind)
        })
    }
    pub(super) fn take(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.check(kind) {
            self.advance().cloned()
        } else {
            None
        }
    }
    pub(super) fn get(&self, index: usize) -> Option<&Token> {
        self.tokens.get(index)
    }
    pub(super) fn position(&self) -> usize {
        self.current
    }
}
