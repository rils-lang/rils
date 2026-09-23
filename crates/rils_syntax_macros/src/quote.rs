//! Quasiquotation for Rust-defined Rils derive generators.

use proc_macro::TokenStream;
use proc_macro2::{Delimiter, TokenStream as Tokens, TokenTree};
use quote::quote;
use syn::Error;

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let template = Tokens::from(input);
    match operations(template) {
        Ok(steps) => quote! {{
            let mut __rils_quote_source = ::std::string::String::new();
            #steps
            rils_syntax::quote::QuotedStatement::new(__rils_quote_source)
        }}
        .into(),
        Err(error) => error.into_compile_error().into(),
    }
}

pub(crate) fn expand_tokens(input: TokenStream) -> TokenStream {
    let template = proc_macro2::TokenStream::from(input);
    match operations(template) {
        Ok(steps) => quote! {{
            let mut __rils_quote_source = ::std::string::String::new();
            #steps
            __rils_quote_source
        }}
        .into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn operations(template: Tokens) -> syn::Result<Tokens> {
    let tokens = template.into_iter().collect::<Vec<_>>();
    let mut steps = Vec::new();
    let mut literal = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        match &tokens[index] {
            TokenTree::Punct(punct) if punct.as_char() == '#' => {
                flush_literal(&mut literal, &mut steps);
                let next = tokens.get(index + 1).ok_or_else(|| {
                    Error::new_spanned(punct, "expected an interpolation after `#`")
                })?;
                match next {
                    TokenTree::Ident(ident) => {
                        steps.push(quote! {
                            __rils_quote_source.push_str(&::std::format!(" {} ", #ident));
                        });
                        index += 2;
                    }
                    TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis => {
                        let body = group.stream().into_iter().collect::<Vec<_>>();
                        let [TokenTree::Punct(hash), TokenTree::Ident(values)] = body.as_slice()
                        else {
                            return Err(Error::new_spanned(
                                group,
                                "repetition must be `#(#values),*`",
                            ));
                        };
                        if hash.as_char() != '#'
                            || !matches!(tokens.get(index + 2), Some(TokenTree::Punct(comma)) if comma.as_char() == ',')
                            || !matches!(tokens.get(index + 3), Some(TokenTree::Punct(star)) if star.as_char() == '*')
                        {
                            return Err(Error::new_spanned(
                                group,
                                "repetition must be `#(#values),*`",
                            ));
                        }
                        steps.push(quote! {
                            for (__rils_index, __rils_value) in (#values).into_iter().enumerate() {
                                if __rils_index != 0 {
                                    __rils_quote_source.push_str(", ");
                                }
                                __rils_quote_source.push_str(&::std::format!("{}", __rils_value));
                            }
                        });
                        index += 4;
                    }
                    _ => {
                        return Err(Error::new_spanned(
                            next,
                            "expected `#name` or `#(#values),*`",
                        ));
                    }
                }
            }
            TokenTree::Group(group) if group.delimiter() != Delimiter::None => {
                flush_literal(&mut literal, &mut steps);
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => unreachable!(),
                };
                let inner = operations(group.stream())?;
                steps.push(quote! {
                    __rils_quote_source.push_str(#open);
                    #inner
                    __rils_quote_source.push_str(#close);
                });
                index += 1;
            }
            token => {
                literal.push(token.clone());
                index += 1;
            }
        }
    }
    flush_literal(&mut literal, &mut steps);
    Ok(quote!(#(#steps)*))
}

fn flush_literal(literal: &mut Vec<TokenTree>, steps: &mut Vec<Tokens>) {
    if literal.is_empty() {
        return;
    }
    let source = Tokens::from_iter(std::mem::take(literal)).to_string();
    steps.push(quote! {
        __rils_quote_source.push_str(" ");
        __rils_quote_source.push_str(#source);
        __rils_quote_source.push_str(" ");
    });
}
