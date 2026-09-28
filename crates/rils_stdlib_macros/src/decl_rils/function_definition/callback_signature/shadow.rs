//! Fallible implementation synthesized from a plain callback body.

use std::collections::HashSet;

use syn::{
    Error, Expr, FnArg, ImplItemFn, ItemFn, Pat, ReturnType, Type, TypeParamBound, WherePredicate,
    fold::{self, Fold},
    parse_quote,
};

use super::{expose, simple_name};

struct CallbackCalls {
    names: HashSet<String>,
    error: Option<Error>,
}

impl Fold for CallbackCalls {
    fn fold_expr(&mut self, expression: Expr) -> Expr {
        if matches!(
            expression,
            Expr::Return(_) | Expr::Closure(_) | Expr::Macro(_)
        ) {
            self.error = Some(Error::new_spanned(
                &expression,
                "plain callback export cannot contain return, nested closures, or expression macros",
            ));
            return expression;
        }
        if let Expr::Call(mut call) = expression {
            if let Expr::Path(path) = call.func.as_ref()
                && path
                    .path
                    .get_ident()
                    .is_some_and(|name| self.names.contains(&name.to_string()))
            {
                call.args = call
                    .args
                    .into_iter()
                    .map(|argument| self.fold_expr(argument))
                    .collect();
                let call = Expr::Call(call);
                return parse_quote!((#call)?);
            }
            return fold::fold_expr(self, Expr::Call(call));
        }
        if let Expr::Path(path) = &expression
            && path
                .path
                .get_ident()
                .is_some_and(|name| self.names.contains(&name.to_string()))
        {
            self.error = Some(Error::new_spanned(
                path,
                "callback must be called directly in an exported body",
            ));
        }
        fold::fold_expr(self, expression)
    }
}

pub(crate) fn function(original: &ItemFn) -> syn::Result<Option<ItemFn>> {
    let (_, _, plain) = expose(original.clone())?;
    if !plain {
        return Ok(None);
    }
    let mut generated = original.clone();
    let mut names = HashSet::new();
    for argument in &original.sig.inputs {
        let FnArg::Typed(argument) = argument else {
            continue;
        };
        let Some(generic) = simple_name(&argument.ty) else {
            continue;
        };
        let Pat::Ident(binding) = argument.pat.as_ref() else {
            return Err(Error::new_spanned(
                &argument.pat,
                "callback parameter needs a simple name",
            ));
        };
        let Some(where_clause) = &original.sig.generics.where_clause else {
            continue;
        };
        if where_clause.predicates.iter().any(|predicate| {
            matches!(predicate, WherePredicate::Type(predicate) if simple_name(&predicate.bounded_ty) == Some(generic))
        }) {
            names.insert(binding.ident.to_string());
        }
    }
    let mut calls = CallbackCalls { names, error: None };
    let body = calls.fold_block(*generated.block);
    if let Some(error) = calls.error {
        return Err(error);
    }
    let output: Type = match &original.sig.output {
        ReturnType::Default => parse_quote!(()),
        ReturnType::Type(_, output) => *output.clone(),
    };
    let error: syn::Ident = parse_quote!(__RilsCallbackError);
    generated.sig.ident = syn::Ident::new(
        &format!("__rils_try_{}", original.sig.ident),
        original.sig.ident.span(),
    );
    generated.sig.generics.params.push(parse_quote!(#error));
    let where_clause = generated
        .sig
        .generics
        .where_clause
        .as_mut()
        .expect("plain callback bound has where clause");
    for predicate in &mut where_clause.predicates {
        let WherePredicate::Type(predicate) = predicate else {
            continue;
        };
        for bound in &mut predicate.bounds {
            let TypeParamBound::Trait(bound) = bound else {
                continue;
            };
            let Some(segment) = bound.path.segments.last_mut() else {
                continue;
            };
            let syn::PathArguments::Parenthesized(arguments) = &mut segment.arguments else {
                continue;
            };
            let result: Type = match &arguments.output {
                ReturnType::Default => parse_quote!(()),
                ReturnType::Type(_, output) => *output.clone(),
            };
            arguments.output = parse_quote!(-> std::result::Result<#result, #error>);
        }
    }
    generated.sig.output = parse_quote!(-> std::result::Result<#output, #error>);
    *generated.block = parse_quote!({ Ok({ #body }) });
    generated
        .attrs
        .retain(|attribute| !attribute.path().is_ident("rils_fn"));
    generated.attrs.push(parse_quote!(#[doc(hidden)]));
    Ok(Some(generated))
}

pub(crate) fn method(original: &ImplItemFn) -> syn::Result<Option<ImplItemFn>> {
    let candidate = ItemFn {
        attrs: original.attrs.clone(),
        vis: original.vis.clone(),
        sig: original.sig.clone(),
        block: Box::new(original.block.clone()),
    };
    let Some(generated) = function(&candidate)? else {
        return Ok(None);
    };
    let mut method = original.clone();
    method.attrs = generated.attrs;
    method
        .attrs
        .retain(|attribute| !attribute.path().is_ident("export_rils"));
    method.sig = generated.sig;
    method.block = *generated.block;
    Ok(Some(method))
}
