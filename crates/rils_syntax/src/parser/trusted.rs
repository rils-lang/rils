//! Provenance and body policy for trusted declaration packages.

use super::*;

pub(super) fn record_declarations(program: &mut Program) {
    program.language_declaration_spans = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Module { span, .. }
            | Stmt::Use { span, .. }
            | Stmt::Function { span, .. }
            | Stmt::Struct { span, .. }
            | Stmt::Enum { span, .. }
            | Stmt::Trait { span, .. }
            | Stmt::Impl { span, .. }
            | Stmt::TypeAlias { span, .. } => Some(*span),
            _ => None,
        })
        .collect();
}

pub(super) fn mark_bodies(statements: &mut [Stmt]) {
    for statement in statements {
        match statement {
            Stmt::Module {
                statements: Some(children),
                ..
            } => mark_bodies(children),
            Stmt::Function {
                attributes, span, ..
            } => mark(attributes, *span),
            Stmt::Impl { methods, .. } => {
                for method in methods {
                    mark(&mut method.attributes, method.span);
                }
            }
            _ => {}
        }
    }
}

fn mark(attributes: &mut Vec<Attribute>, span: Span) {
    if !crate::ast::has_compiler_internal_attribute(attributes) {
        attributes.push(Attribute {
            path: vec!["compiler_internal".into()],
            arguments: Vec::new(),
            span,
        });
    }
}
