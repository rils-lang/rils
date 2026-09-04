use crate::token::{Token, TokenKind};
use crate::token_tree::{TokenTree, build_tree};

/// Owned, stable token storage shared by parser and macro processing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TokenStream {
    trees: Box<[TokenTree]>,
    token_len: usize,
    source_end: usize,
}

impl TokenStream {
    pub(crate) fn new(tokens: Vec<Token>) -> Result<Self, crate::source::Span> {
        let source_end = tokens.last().map_or(0, |token| token.span.end);
        let token_len = tokens.len();
        let trees = build_tree(&tokens)?.into_boxed_slice();
        Ok(Self {
            trees,
            token_len,
            source_end,
        })
    }

    pub(crate) fn cursor(&self) -> Cursor<'_> {
        self.cursor_at(0)
    }
    pub(crate) fn cursor_at(&self, position: usize) -> Cursor<'_> {
        Cursor {
            stream: self,
            position: position.min(self.token_len),
        }
    }
    #[allow(dead_code)]
    pub(crate) fn source_end(&self) -> usize {
        self.source_end
    }
    pub(crate) fn trees(&self) -> &[TokenTree] {
        &self.trees
    }

    fn token_at(&self, position: usize) -> Option<&Token> {
        fn visit<'a>(
            trees: &'a [TokenTree],
            index: &mut usize,
            target: usize,
        ) -> Option<&'a Token> {
            for tree in trees {
                match tree {
                    TokenTree::Token(token) => {
                        if *index == target {
                            return Some(token);
                        }
                        *index += 1;
                    }
                    TokenTree::Group {
                        open,
                        children,
                        close,
                        ..
                    } => {
                        if *index == target {
                            return Some(open);
                        }
                        *index += 1;
                        if let Some(token) = visit(children, index, target) {
                            return Some(token);
                        }
                        if *index == target {
                            return Some(close);
                        }
                        *index += 1;
                    }
                }
            }
            None
        }
        if position >= self.token_len {
            return None;
        }
        let mut index = 0;
        visit(&self.trees, &mut index, position)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Cursor<'a> {
    stream: &'a TokenStream,
    position: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn is_at_end(self) -> bool {
        self.position >= self.stream.token_len
    }
    pub(crate) fn position(self) -> usize {
        self.position
    }
    pub(crate) fn peek(self) -> Option<&'a Token> {
        self.stream.token_at(self.position)
    }
    pub(crate) fn previous(self) -> Option<&'a Token> {
        self.position
            .checked_sub(1)
            .and_then(|i| self.stream.token_at(i))
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
        let stream = TokenStream::new(lex("call!(1)").unwrap()).unwrap();
        assert_eq!(stream.trees().len(), 3);
        assert_eq!(stream.source_end(), 8);
    }

    #[test]
    fn flattened_cursor_order_matches_lexer() {
        let tokens = lex("assert!([1, 2].into_iter().any(is_two));").unwrap();
        let stream = TokenStream::new(tokens.clone()).unwrap();
        let mut cursor = stream.cursor();
        let mut got = Vec::new();
        while let Some(token) = cursor.peek() {
            got.push(token.kind.clone());
            cursor = cursor.advance().unwrap().1;
        }
        assert_eq!(got, tokens.into_iter().map(|t| t.kind).collect::<Vec<_>>());
    }

    #[test]
    fn rejects_unbalanced_tokens_at_stream_boundary() {
        let tokens = lex("call!(1").unwrap();
        assert!(TokenStream::new(tokens).is_err());
    }
}
