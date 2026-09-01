use std::collections::{HashMap, HashSet};

use crate::{
    analysis::AnalysisDiagnostic,
    ast::{Block, Expr, Literal, Program, Stmt},
    format::{FormatKind, FormatPiece, parse_format_string},
    semantic::ExpressionTypes,
    source::Span,
    types::Type,
};

pub(crate) fn analyze(
    program: &Program,
    expression_types: ExpressionTypes<'_>,
    host_types: &HashSet<String>,
) -> Vec<AnalysisDiagnostic> {
    let mut checker = Checker {
        expression_types,
        host_types,
        nominals: HashSet::new(),
        implementations: HashMap::new(),
        diagnostics: Vec::new(),
    };
    checker.collect(&program.statements);
    checker.statements(&program.statements);
    checker.diagnostics
}

struct Checker<'a> {
    expression_types: ExpressionTypes<'a>,
    host_types: &'a HashSet<String>,
    nominals: HashSet<String>,
    implementations: HashMap<String, HashSet<String>>,
    diagnostics: Vec<AnalysisDiagnostic>,
}

impl Checker<'_> {
    fn collect(&mut self, statements: &[Stmt]) {
        for statement in statements {
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.collect(statements),
                Stmt::Struct { name, .. } | Stmt::Enum { name, .. } => {
                    self.nominals.insert(name.clone());
                }
                Stmt::Impl {
                    trait_name: Some(trait_name),
                    target: Type::Named { name, .. },
                    ..
                } => {
                    self.implementations
                        .entry(name.clone())
                        .or_default()
                        .insert(
                            trait_name
                                .rsplit("::")
                                .next()
                                .unwrap_or(trait_name)
                                .to_string(),
                        );
                }
                _ => {}
            }
        }
    }

    fn statements(&mut self, statements: &[Stmt]) {
        for statement in statements {
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.statements(statements),
                Stmt::Function { body, .. } => self.block(body),
                Stmt::Impl { methods, .. } => {
                    for method in methods {
                        self.block(&method.body);
                    }
                }
                Stmt::Let { initializer, .. } => self.expression(initializer),
                Stmt::While {
                    condition, body, ..
                } => {
                    self.expression(condition);
                    self.block(body);
                }
                Stmt::Loop { body, .. } => self.block(body),
                Stmt::For { iterable, body, .. } => {
                    self.expression(iterable);
                    self.block(body);
                }
                Stmt::Return {
                    value: Some(value), ..
                }
                | Stmt::Break {
                    value: Some(value), ..
                } => self.expression(value),
                Stmt::Expr { expression, .. } => self.expression(expression),
                _ => {}
            }
        }
    }

    fn block(&mut self, block: &Block) {
        self.statements(&block.statements);
    }

    fn expression(&mut self, expression: &Expr) {
        if let Expr::Call {
            callee, arguments, ..
        } = expression
            && matches!(callee.as_ref(), Expr::Variable { name, .. } if matches!(name.as_str(), "#rils_native_print" | "#rils_native_println"))
        {
            self.check_call(arguments);
        }
        match expression {
            Expr::Member { object, .. }
            | Expr::Borrow { target: object, .. }
            | Expr::Try {
                operand: object, ..
            }
            | Expr::Unary {
                operand: object, ..
            }
            | Expr::Cast {
                operand: object, ..
            } => self.expression(object),
            Expr::Index { object, index, .. }
            | Expr::Binary {
                left: object,
                right: index,
                ..
            }
            | Expr::Logical {
                left: object,
                right: index,
                ..
            }
            | Expr::Range {
                start: object,
                end: index,
                ..
            } => {
                self.expression(object);
                self.expression(index);
            }
            Expr::Tuple { elements, .. } => {
                for element in elements {
                    self.expression(element);
                }
            }
            Expr::Array {
                elements, repeat, ..
            } => {
                for element in elements {
                    self.expression(element);
                }
                if let Some(repeat) = repeat {
                    self.expression(repeat);
                }
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                self.expression(callee);
                for argument in arguments {
                    self.expression(argument);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expression(condition);
                self.block(then_branch);
                if let Some(branch) = else_branch {
                    self.expression(branch);
                }
            }
            Expr::Assign { target, value, .. } => {
                self.expression(target);
                self.expression(value);
            }
            Expr::RecordLiteral { fields, .. } => {
                for field in fields {
                    self.expression(&field.value);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.expression(value);
                for arm in arms {
                    self.expression(&arm.expression);
                }
            }
            Expr::Block(block) => self.block(block),
            Expr::Literal { .. }
            | Expr::Variable { .. }
            | Expr::Path { .. }
            | Expr::QualifiedPath { .. } => {}
        }
    }

    fn check_call(&mut self, arguments: &[Expr]) {
        let Some(Expr::Literal {
            value: Literal::String(format),
            ..
        }) = arguments.first()
        else {
            return;
        };
        let Ok(pieces) = parse_format_string(format) else {
            return;
        };
        for piece in pieces {
            let FormatPiece::Placeholder { argument, spec } = piece else {
                continue;
            };
            let Some(value) = arguments.get(argument + 1) else {
                continue;
            };
            let ty = self
                .expression_types
                .get(value)
                .cloned()
                .unwrap_or(Type::Unknown);
            let required = match spec.kind {
                FormatKind::Display => "Display",
                FormatKind::Debug => "Debug",
                FormatKind::Binary
                | FormatKind::Octal
                | FormatKind::LowerHex
                | FormatKind::UpperHex => {
                    if !matches!(
                        deref_type(&ty),
                        Type::Integer(_) | Type::IntegerVariable(_) | Type::Unknown
                    ) {
                        self.error(
                            format!(
                                "format type `{:?}` requires an integer, found `{ty}`",
                                spec.kind
                            ),
                            value.span(),
                        );
                    }
                    continue;
                }
                FormatKind::LowerExp | FormatKind::UpperExp => {
                    if !matches!(
                        deref_type(&ty),
                        Type::Float(_) | Type::FloatVariable(_) | Type::Unknown
                    ) {
                        self.error(
                            format!(
                                "format type `{:?}` requires a float, found `{ty}`",
                                spec.kind
                            ),
                            value.span(),
                        );
                    }
                    continue;
                }
            };
            if !self.implements(&ty, required) {
                self.error(
                    format!("type `{ty}` does not implement `{required}`"),
                    value.span(),
                );
            }
        }
    }

    fn implements(&self, ty: &Type, required: &str) -> bool {
        match deref_type(ty) {
            Type::Unknown | Type::Variable(_) | Type::Associated { .. } => true,
            Type::Unit
            | Type::Bool
            | Type::Integer(_)
            | Type::IntegerVariable(_)
            | Type::IntegerInference(_)
            | Type::Float(_)
            | Type::FloatVariable(_)
            | Type::FloatInference(_)
            | Type::Char
            | Type::String => true,
            Type::Tuple(elements) => {
                required == "Debug" && elements.iter().all(|ty| self.implements(ty, required))
            }
            Type::Array { element, .. } | Type::Option(element) => {
                required == "Debug" && self.implements(element, required)
            }
            Type::Result(ok, error) => {
                required == "Debug"
                    && self.implements(ok, required)
                    && self.implements(error, required)
            }
            Type::Named { name, arguments }
                if matches!(name.as_str(), "Vec" | "HashMap" | "HashSet" | "Range") =>
            {
                required == "Debug" && arguments.iter().all(|ty| self.implements(ty, required))
            }
            Type::Named { name, .. } if self.host_types.contains(name) => true,
            Type::Named { name, .. } if self.nominals.contains(name) => self
                .implementations
                .get(name)
                .is_some_and(|traits| traits.contains(required)),
            Type::Named { .. } => true,
            Type::Function { .. } => false,
            Type::Reference { .. } => unreachable!(),
        }
    }

    fn error(&mut self, message: String, span: Span) {
        self.diagnostics
            .push(AnalysisDiagnostic::error(message, span));
    }
}

fn deref_type(mut ty: &Type) -> &Type {
    while let Type::Reference { inner, .. } = ty {
        ty = inner;
    }
    ty
}
