//! Effect classification for callable values used by the `Fn*` bounds.

use std::collections::HashSet;

use super::{Block, EnvironmentRef, Expr, Stmt, Value};
use rils_stdlib::stdlib::ops::CallableTraitKind as CallableKind;

pub(super) fn callable_kind(value: &Value) -> Option<CallableKind> {
    match value {
        Value::Function(function) => {
            let locals = function
                .parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<HashSet<_>>();
            let mut kind = CallableKind::Shared;
            inspect_block(&function.body, &locals, &function.closure, &mut kind);
            Some(kind)
        }
        Value::BytecodeFunction(function) => Some(if function.captures.is_empty() {
            CallableKind::Shared
        } else {
            CallableKind::Once
        }),
        Value::NativeFunction(_)
        | Value::HostFunction(_)
        | Value::BuiltinFunction(_)
        | Value::VariantConstructor(_) => Some(CallableKind::Shared),
        Value::BoundMethod(_) | Value::HostBoundMethod(_) | Value::BuiltinBoundMethod(_) => {
            Some(CallableKind::Once)
        }
        _ => None,
    }
}

fn inspect_block(
    block: &Block,
    locals: &HashSet<String>,
    environment: &EnvironmentRef,
    kind: &mut CallableKind,
) {
    let mut locals = locals.clone();
    for statement in &block.statements {
        match statement {
            Stmt::Let {
                name, initializer, ..
            } => {
                inspect(initializer, &locals, environment, kind, false);
                locals.insert(name.clone());
            }
            Stmt::While {
                condition, body, ..
            } => {
                inspect(condition, &locals, environment, kind, false);
                inspect_block(body, &locals, environment, kind);
            }
            Stmt::Loop { body, .. } => inspect_block(body, &locals, environment, kind),
            Stmt::For {
                binding,
                iterable,
                body,
                ..
            } => {
                inspect(iterable, &locals, environment, kind, false);
                let mut child_locals = locals.clone();
                child_locals.insert(binding.clone());
                inspect_block(body, &child_locals, environment, kind);
            }
            Stmt::Return { value, .. } | Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    inspect(value, &locals, environment, kind, false);
                }
            }
            Stmt::Expr { expression, .. } => inspect(expression, &locals, environment, kind, false),
            // A nested function can access the same captured slots. Its effects are
            // currently unknown to this analysis, so do not promise repeatability.
            Stmt::Function { name, .. } => {
                *kind = CallableKind::Once;
                locals.insert(name.clone());
            }
            _ => {}
        }
    }
}

fn inspect(
    expression: &Expr,
    locals: &HashSet<String>,
    environment: &EnvironmentRef,
    kind: &mut CallableKind,
    borrowed: bool,
) {
    match expression {
        Expr::Variable { name, .. } if !locals.contains(name) => {
            if let Some(slot) = environment.borrow().slot(name) {
                if !borrowed && slot.borrow().read().is_ok_and(|value| !value.is_copy()) {
                    *kind = CallableKind::Once;
                }
            }
        }
        Expr::Assign { target, value, .. } => {
            if let Expr::Variable { name, .. } = target.as_ref() {
                if !locals.contains(name) {
                    *kind = (*kind).min(CallableKind::Mut);
                }
            } else {
                inspect(target, locals, environment, kind, true);
            }
            inspect(value, locals, environment, kind, false);
        }
        Expr::Borrow {
            mutable, target, ..
        } => {
            if *mutable
                && matches!(target.as_ref(), Expr::Variable { name, .. } if !locals.contains(name))
            {
                *kind = (*kind).min(CallableKind::Mut);
            }
            inspect(target, locals, environment, kind, true);
        }
        Expr::Member { object, .. }
        | Expr::Try {
            operand: object, ..
        } => {
            inspect(object, locals, environment, kind, borrowed);
        }
        Expr::Unary { operand, .. } | Expr::Cast { operand, .. } => {
            inspect(operand, locals, environment, kind, borrowed);
        }
        Expr::Index { object, index, .. } => {
            inspect(object, locals, environment, kind, borrowed);
            inspect(index, locals, environment, kind, false);
        }
        Expr::Tuple { elements, .. } | Expr::Array { elements, .. } => {
            for element in elements {
                inspect(element, locals, environment, kind, false);
            }
            if let Expr::Array {
                repeat: Some(repeat),
                ..
            } = expression
            {
                inspect(repeat, locals, environment, kind, false);
            }
        }
        Expr::RecordLiteral { fields, .. } => {
            for field in fields {
                inspect(&field.value, locals, environment, kind, false);
            }
        }
        Expr::Binary { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
            ..
        } => {
            inspect(left, locals, environment, kind, false);
            inspect(right, locals, environment, kind, false);
        }
        Expr::Call {
            callee, arguments, ..
        } => {
            if matches!(callee.as_ref(), Expr::Variable { name, .. } if !locals.contains(name)) {
                // A captured callable may consume its own environment. Its
                // signature alone cannot promise repeated calls here.
                *kind = CallableKind::Once;
            }
            inspect(callee, locals, environment, kind, false);
            for argument in arguments {
                inspect(argument, locals, environment, kind, false);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            inspect(condition, locals, environment, kind, false);
            inspect_block(then_branch, locals, environment, kind);
            if let Some(branch) = else_branch {
                inspect(branch, locals, environment, kind, false);
            }
        }
        Expr::Match { value, arms, .. } => {
            inspect(value, locals, environment, kind, false);
            for arm in arms {
                inspect(&arm.expression, locals, environment, kind, false);
            }
        }
        Expr::Block(block) => inspect_block(block, locals, environment, kind),
        _ => {}
    }
}
