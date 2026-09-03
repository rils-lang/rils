use crate::{source::Span, token::Token};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delimiter {
    Parenthesis,
    Brace,
    Bracket,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TokenTree {
    Token(Token),
    Group {
        delimiter: Delimiter,
        open: Token,
        children: Box<[TokenTree]>,
        close: Token,
    },
}

impl TokenTree {
    pub(crate) fn flatten_into(&self, output: &mut Vec<Token>) {
        match self {
            Self::Token(token) => output.push(token.clone()),
            Self::Group {
                open,
                children,
                close,
                ..
            } => {
                output.push(open.clone());
                for child in children.iter() {
                    child.flatten_into(output);
                }
                output.push(close.clone());
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TreeCursor<'a> {
    trees: &'a [TokenTree],
    position: usize,
}

impl<'a> TreeCursor<'a> {
    pub(crate) fn new(trees: &'a [TokenTree]) -> Self {
        Self { trees, position: 0 }
    }
    pub(crate) fn is_empty(self) -> bool {
        self.position >= self.trees.len()
    }
    pub(crate) fn position(self) -> usize {
        self.position
    }
    pub(crate) fn first(self) -> Option<&'a TokenTree> {
        self.trees.get(self.position)
    }
    pub(crate) fn remaining(self) -> &'a [TokenTree] {
        &self.trees[self.position.min(self.trees.len())..]
    }
    pub(crate) fn step(self) -> Option<(&'a TokenTree, Self)> {
        let tree = self.first()?;
        Some((
            tree,
            Self {
                position: self.position + 1,
                ..self
            },
        ))
    }

    pub(crate) fn token(self) -> Option<&'a Token> {
        match self.first()? {
            TokenTree::Token(token) => Some(token),
            TokenTree::Group { .. } => None,
        }
    }

    pub(crate) fn group(self) -> Option<(Delimiter, Span, TreeCursor<'a>, Self)> {
        let (tree, next) = self.step()?;
        match tree {
            TokenTree::Group {
                delimiter,
                open,
                children,
                close,
            } => Some((
                *delimiter,
                group_span(open.span, close.span),
                TreeCursor::new(children),
                next,
            )),
            TokenTree::Token(_) => None,
        }
    }
}

pub(crate) fn delimiter_pair(token: &Token) -> Option<(Delimiter, bool)> {
    Some(match token.kind {
        crate::token::TokenKind::LeftParen => (Delimiter::Parenthesis, true),
        crate::token::TokenKind::RightParen => (Delimiter::Parenthesis, false),
        crate::token::TokenKind::LeftBrace => (Delimiter::Brace, true),
        crate::token::TokenKind::RightBrace => (Delimiter::Brace, false),
        crate::token::TokenKind::LeftBracket => (Delimiter::Bracket, true),
        crate::token::TokenKind::RightBracket => (Delimiter::Bracket, false),
        _ => return None,
    })
}

pub(crate) fn group_span(open: Span, close: Span) -> Span {
    open.merge(close)
}

pub(crate) fn build_tree(tokens: &[Token]) -> Result<Vec<TokenTree>, Span> {
    fn parse(
        tokens: &[Token],
        index: &mut usize,
        closing: Option<Delimiter>,
    ) -> Result<Vec<TokenTree>, Span> {
        let mut output = Vec::new();
        while let Some(token) = tokens.get(*index) {
            if let Some((delimiter, is_open)) = delimiter_pair(token) {
                if !is_open {
                    if Some(delimiter) == closing {
                        return Ok(output);
                    }
                    return Err(token.span);
                }
                let open = token.clone();
                *index += 1;
                let children = parse(tokens, index, Some(delimiter))?;
                let Some(close) = tokens.get(*index) else {
                    return Err(open.span);
                };
                *index += 1;
                output.push(TokenTree::Group {
                    delimiter,
                    open,
                    children: children.into_boxed_slice(),
                    close: close.clone(),
                });
            } else {
                output.push(TokenTree::Token(token.clone()));
                *index += 1;
            }
        }
        if closing.is_some() {
            Err(tokens.last().map_or(Span::default(), |token| token.span))
        } else {
            Ok(output)
        }
    }
    let mut index = 0;
    let trees = parse(tokens, &mut index, None)?;
    Ok(trees)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::token::TokenKind;

    #[test]
    fn builds_nested_groups() {
        let tokens = lex("foo!(a[1])").unwrap();
        let trees = build_tree(&tokens).unwrap();
        assert!(matches!(
            trees[2],
            TokenTree::Group {
                delimiter: Delimiter::Parenthesis,
                ..
            }
        ));
    }

    #[test]
    fn rejects_unclosed_groups() {
        let tokens = lex("foo!(a").unwrap();
        assert!(build_tree(&tokens).is_err());
    }

    #[test]
    fn tree_cursor_is_copyable_and_advances_without_mutating_storage() {
        let trees = build_tree(&lex("call!(1)").unwrap()).unwrap();
        let cursor = TreeCursor::new(&trees);
        let fork = cursor;
        let (_, next) = cursor.step().unwrap();
        assert_eq!(fork.position(), 0);
        assert_eq!(next.position(), 1);
        assert!(matches!(
            next.first(),
            Some(TokenTree::Token(_)) | Some(TokenTree::Group { .. })
        ));
    }

    #[test]
    fn tree_cursor_enters_delimiter_groups() {
        let trees = build_tree(&lex("call!(1)").unwrap()).unwrap();
        let cursor = TreeCursor::new(&trees);
        let (_, cursor) = cursor.step().unwrap();
        let (_, cursor) = cursor.step().unwrap();
        let (delimiter, _, inner, rest) = cursor.group().expect("call arguments group");
        assert_eq!(delimiter, Delimiter::Parenthesis);
        assert!(
            inner
                .token()
                .is_some_and(|token| matches!(token.kind, TokenKind::Integer(1)))
        );
        assert!(
            rest.token()
                .is_some_and(|token| matches!(token.kind, TokenKind::Eof))
        );
    }
}
