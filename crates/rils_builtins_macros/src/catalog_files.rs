use std::{fs, path::PathBuf};

use proc_macro::TokenStream;
use quote::quote;
use rils_syntax::{Span, ast::Stmt};
use syn::{Error, Ident, LitStr, Token, parenthesized, parse::Parse, parse_macro_input};

use crate::builtin_files;

mod keyword {
    syn::custom_keyword!(backend);
    syn::custom_keyword!(prefix);
}

enum Backend {
    Named(Ident),
    Host(LitStr),
}

struct Input {
    source_path: LitStr,
    prefix: LitStr,
    backend: Backend,
    visibility: syn::Visibility,
    name: Ident,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let source_path = input.parse()?;
        input.parse::<Token![;]>()?;
        input.parse::<keyword::prefix>()?;
        let prefix = input.parse()?;
        input.parse::<Token![;]>()?;
        input.parse::<keyword::backend>()?;
        let backend_name: Ident = input.parse()?;
        let backend = if backend_name == "Host" {
            let content;
            parenthesized!(content in input);
            Backend::Host(content.parse()?)
        } else {
            Backend::Named(backend_name)
        };
        input.parse::<Token![;]>()?;
        let visibility = input.parse()?;
        input.parse::<Token![const]>()?;
        let name = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(Self {
            source_path,
            prefix,
            backend,
            visibility,
            name,
        })
    }
}

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    match expand_input(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand_input(input: Input) -> syn::Result<proc_macro2::TokenStream> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
        Error::new(
            input.source_path.span(),
            "CARGO_MANIFEST_DIR is unavailable",
        )
    })?;
    let source_path = PathBuf::from(manifest).join(input.source_path.value());
    let source = fs::read_to_string(&source_path).map_err(|error| {
        Error::new(
            input.source_path.span(),
            format!("failed to read `{}`: {error}", source_path.display()),
        )
    })?;
    let tokens = rils_syntax::lex(&source)
        .map_err(|error| Error::new(input.source_path.span(), error.message))?;
    let program = rils_syntax::parser::parse_builtin_declarations(tokens)
        .map_err(|error| Error::new(input.source_path.span(), error.message))?;
    let prefix = input.prefix.value();
    let backend = match input.backend {
        Backend::Named(name) => quote!(BuiltinBackend::#name),
        Backend::Host(capability) => quote!(BuiltinBackend::Host(#capability)),
    };
    let declarations = program
        .statements
        .iter()
        .map(|statement| declaration_tokens(statement, &prefix, &backend, &source))
        .collect::<syn::Result<Vec<_>>>()?;
    if declarations.is_empty() {
        return Err(Error::new(
            input.source_path.span(),
            "a built-in catalog file must contain at least one declaration",
        ));
    }
    let source_literal = LitStr::new(&source_path.to_string_lossy(), input.source_path.span());
    let visibility = input.visibility;
    let name = input.name;
    Ok(quote! {
        const _: &str = include_str!(#source_literal);
        #visibility const #name: &[BuiltinDeclaration] = &[#(#declarations),*];
    })
}

fn declaration_tokens(
    statement: &Stmt,
    prefix: &str,
    backend: &proc_macro2::TokenStream,
    source: &str,
) -> syn::Result<proc_macro2::TokenStream> {
    let declaration_span = if statement
        .visibility()
        .is_some_and(|visibility| visibility.is_public())
    {
        statement_span(statement)
    } else {
        Span::default()
    };
    match statement {
        Stmt::Module { name, span, .. } => {
            let path = path_literal(prefix, name);
            let documentation = documentation_literal(
                source,
                if declaration_span == Span::default() {
                    *span
                } else {
                    declaration_span
                },
            );
            Ok(quote! {
                BuiltinDeclaration {
                    path: #path,
                    kind: BuiltinKind::Module,
                    type_parameters: &[],
                    members: &[],
                    signature: None,
                    backend: #backend,
                    documentation: #documentation,
                }
            })
        }
        Stmt::Function {
            attributes,
            name,
            generic_parameters,
            parameters,
            return_type,
            span,
            ..
        } => {
            let mut variadic = false;
            let mut metadata = false;
            for attribute in attributes {
                if attribute.path.as_slice() == ["variadic"] && attribute.arguments.is_empty() {
                    variadic = true;
                } else if attribute.path.as_slice() == ["metadata"]
                    && attribute.arguments.is_empty()
                {
                    metadata = true;
                } else {
                    return Err(Error::new(
                        proc_macro2::Span::call_site(),
                        format!("unsupported built-in function attributes on `{name}`"),
                    ));
                }
            }
            if variadic && !parameters.is_empty() {
                return Err(Error::new(
                    proc_macro2::Span::call_site(),
                    format!("variadic built-in function `{name}` cannot declare fixed parameters"),
                ));
            }
            let parameters = parameters
                .iter()
                .map(|parameter| {
                    parameter
                        .type_annotation
                        .as_ref()
                        .ok_or_else(|| {
                            Error::new(
                                proc_macro2::Span::call_site(),
                                format!("parameter `{}` requires a type", parameter.name),
                            )
                        })
                        .and_then(builtin_files::type_tokens)
                })
                .collect::<syn::Result<Vec<_>>>()?;
            let result = return_type
                .as_ref()
                .map(builtin_files::type_tokens)
                .transpose()?
                .unwrap_or_else(|| quote!(TypePattern::Unit));
            let type_parameters = generic_parameters
                .iter()
                .map(|parameter| LitStr::new(&parameter.name, proc_macro2::Span::call_site()))
                .collect::<Vec<_>>();
            let path = path_literal(prefix, name);
            let documentation_start = attributes.first().map_or_else(
                || {
                    if declaration_span == Span::default() {
                        *span
                    } else {
                        declaration_span
                    }
                },
                |attribute| attribute.span.merge(*span),
            );
            let documentation = documentation_literal(source, documentation_start);
            let backend = if metadata {
                quote!(BuiltinBackend::Metadata)
            } else {
                backend.clone()
            };
            Ok(quote! {
                BuiltinDeclaration {
                    path: #path,
                    kind: BuiltinKind::Function,
                    type_parameters: &[#(#type_parameters),*],
                    members: &[],
                    signature: Some(BuiltinSignature {
                        parameters: &[#(#parameters),*],
                        result: #result,
                        variadic: #variadic,
                    }),
                    backend: #backend,
                    documentation: #documentation,
                }
            })
        }
        _ => Err(Error::new(
            proc_macro2::Span::call_site(),
            "built-in catalog files support only modules and free functions",
        )),
    }
}

fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Module { span, .. } | Stmt::Function { span, .. } => *span,
        _ => Span::default(),
    }
}

fn path_literal(prefix: &str, name: &str) -> LitStr {
    let path = if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}::{name}")
    };
    LitStr::new(&path, proc_macro2::Span::call_site())
}

fn documentation_literal(source: &str, span: Span) -> LitStr {
    LitStr::new(
        &builtin_files::documentation(source, span),
        proc_macro2::Span::call_site(),
    )
}
