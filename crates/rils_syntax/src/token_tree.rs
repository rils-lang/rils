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
}
