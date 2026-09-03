use crate::token::{Token, TokenKind};
use crate::token_tree::{TokenTree, build_tree};

/// Owned, stable token storage shared by parser and macro processing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TokenStream {
    tokens: Box<[Token]>,
    trees: Box<[TokenTree]>,
    source_end: usize,
}

impl TokenStream {
    pub(crate) fn new(mut tokens: Vec<Token>) -> Self {
        if matches!(tokens.last().map(|token| &token.kind), Some(TokenKind::Eof)) {
            tokens.pop();
        }
        let source_end = tokens.last().map_or(0, |token| token.span.end);
        let trees = build_tree(&tokens).unwrap_or_default().into_boxed_slice();
        Self {
            tokens: tokens.into_boxed_slice(),
            trees,
            source_end,
        }
    }

    pub(crate) fn cursor(&self) -> Cursor<'_> {
        self.cursor_at(0)
    }
    pub(crate) fn cursor_at(&self, position: usize) -> Cursor<'_> {
        Cursor {
            stream: self,
            position: position.min(self.tokens.len()),
        }
    }
    pub(crate) fn source_end(&self) -> usize {
        self.source_end
    }
    pub(crate) fn as_slice(&self) -> &[Token] {
        &self.tokens
    }
    pub(crate) fn trees(&self) -> &[TokenTree] {
        &self.trees
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Cursor<'a> {
    stream: &'a TokenStream,
    position: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn is_at_end(self) -> bool {
        self.position >= self.stream.tokens.len()
    }
    pub(crate) fn position(self) -> usize {
        self.position
    }
    pub(crate) fn peek(self) -> Option<&'a Token> {
        self.stream.tokens.get(self.position)
    }
    pub(crate) fn previous(self) -> Option<&'a Token> {
        self.position
            .checked_sub(1)
            .and_then(|i| self.stream.tokens.get(i))
    }
    pub(crate) fn advance(self) -> Option<(&'a Token, Self)> {
        let token = self.peek()?;
        Some((
            token,
            Self {
                position: self.position + 1,
                ..self
            },
        ))
    }
    pub(crate) fn check(self, kind: &TokenKind) -> bool {
        self.peek().is_some_and(|token| {
            std::mem::discriminant(&token.kind) == std::mem::discriminant(kind)
        })
    }
    pub(crate) fn take(self, kind: &TokenKind) -> Option<(&'a Token, Self)> {
        self.check(kind).then(|| self.advance()).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    #[test]
    fn stream_builds_tree_view() {
        let stream = TokenStream::new(lex("call!(1)").unwrap());
        assert_eq!(stream.trees().len(), 3);
    }
}
