//! Preserve a sum method's body while turning its explicit panic into a bridge error.

use syn::{
    Expr, ImplItemFn, ReturnType,
    fold::{self, Fold},
    parse_quote,
};

#[derive(Default)]
struct Failures {
    found: bool,
}

fn message(arguments: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    if arguments.is_empty() {
        quote::quote!(std::string::String::from("explicit panic"))
    } else {
        quote::quote!(format!(#arguments))
    }
}

impl Fold for Failures {
    fn fold_stmt(&mut self, statement: syn::Stmt) -> syn::Stmt {
        if let syn::Stmt::Macro(statement) = &statement
            && statement.mac.path.is_ident("panic")
        {
            self.found = true;
            let message = message(statement.mac.tokens.clone());
            return parse_quote!(return Err(#message););
        }
        fold::fold_stmt(self, statement)
    }

    fn fold_expr(&mut self, expression: Expr) -> Expr {
        match expression {
            Expr::Macro(expression) if expression.mac.path.is_ident("panic") => {
                self.found = true;
                let message = message(expression.mac.tokens);
                parse_quote!(return Err(#message))
            }
            // These returns belong to another function.
            Expr::Closure(_) | Expr::Async(_) => expression,
            Expr::Return(mut expression) => {
                let value = expression
                    .expr
                    .take()
                    .map(|value| self.fold_expr(*value))
                    .unwrap_or_else(|| parse_quote!(()));
                parse_quote!(return Ok(#value))
            }
            expression => fold::fold_expr(self, expression),
        }
    }

    fn fold_item(&mut self, item: syn::Item) -> syn::Item {
        item
    }
}

pub(crate) fn method(original: &ImplItemFn) -> Option<ImplItemFn> {
    let mut failures = Failures::default();
    let body = failures.fold_block(original.block.clone());
    if !failures.found {
        return None;
    }
    let mut generated = original.clone();
    generated.sig.ident = quote::format_ident!("__rils_try_{}", original.sig.ident);
    let output: syn::Type = match &original.sig.output {
        ReturnType::Default => parse_quote!(()),
        ReturnType::Type(_, output) => *output.clone(),
    };
    generated.sig.output = parse_quote!(-> std::result::Result<#output, std::string::String>);
    generated.block = if matches!(body.stmts.last(), Some(syn::Stmt::Expr(Expr::Return(_), _))) {
        body
    } else {
        parse_quote!({ Ok({ #body }) })
    };
    generated
        .attrs
        .retain(|attribute| !attribute.path().is_ident("export_rils"));
    generated.attrs.push(parse_quote!(#[doc(hidden)]));
    Some(generated)
}
