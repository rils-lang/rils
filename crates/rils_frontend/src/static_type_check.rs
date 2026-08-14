use std::collections::{HashMap, HashSet};

use crate::{
    analysis::AnalysisDiagnostic,
    ast::{BinaryOp, Block, Expr, Program, Stmt, UnaryOp},
    source::Span,
    types::{Type, merge_types},
};

pub(crate) fn analyze(
    program: &Program,
    expression_types: &HashMap<Span, Type>,
) -> Vec<AnalysisDiagnostic> {
    Checker::new(program, expression_types).run(program)
}

#[derive(Clone)]
struct Alias {
    parameters: Vec<String>,
    target: Type,
}

struct Checker<'a> {
    expression_types: &'a HashMap<Span, Type>,
    aliases: HashMap<String, Alias>,
    return_types: Vec<Option<Type>>,
    self_types: Vec<Option<Type>>,
    diagnostics: Vec<AnalysisDiagnostic>,
}

impl<'a> Checker<'a> {
    fn new(program: &Program, expression_types: &'a HashMap<Span, Type>) -> Self {
        let mut checker = Self {
            expression_types,
            aliases: HashMap::new(),
            return_types: Vec::new(),
            self_types: Vec::new(),
            diagnostics: Vec::new(),
        };
        checker.collect_aliases(&program.statements);
        checker
    }

    fn run(mut self, program: &Program) -> Vec<AnalysisDiagnostic> {
        self.statements(&program.statements);
        self.diagnostics
    }

    fn collect_aliases(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let statement = match statement {
                Stmt::Public { statement, .. } => statement.as_ref(),
                statement => statement,
            };
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.collect_aliases(statements),
                Stmt::TypeAlias {
                    name,
                    generic_parameters,
                    target,
                    ..
                } => {
                    self.aliases.insert(
                        name.clone(),
                        Alias {
                            parameters: generic_parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            target: target.clone(),
                        },
                    );
                }
                _ => {}
            }
        }
    }

    fn statements(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Public { statement, .. } => self.statement(statement),
            Stmt::Module {
                statements: Some(statements),
                ..
            } => self.statements(statements),
            Stmt::Let {
                type_annotation: Some(expected),
                initializer,
                ..
            } => {
                self.expression(initializer);
                self.expect(
                    expected,
                    self.ty(initializer),
                    initializer.span(),
                    "initializer",
                );
            }
            Stmt::Let { initializer, .. } => self.expression(initializer),
            Stmt::Function {
                return_type, body, ..
            } => self.function(return_type.as_ref(), body, None),
            Stmt::Impl {
                target, methods, ..
            } => {
                for method in methods {
                    self.function(
                        method.return_type.as_ref(),
                        &method.body,
                        Some(target.clone()),
                    );
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expression(condition);
                self.expect_bool(condition, "while condition");
                self.block(body);
            }
            Stmt::Loop { body, .. } => self.block(body),
            Stmt::For { iterable, body, .. } => {
                self.expression(iterable);
                self.block(body);
            }
            Stmt::Return { value, span } => {
                if let Some(value) = value {
                    self.expression(value);
                }
                if let Some(Some(expected)) = self.return_types.last().cloned() {
                    let actual = value.as_ref().map_or(Type::Unit, |value| self.ty(value));
                    self.expect(&expected, actual, *span, "return value");
                }
            }
            Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            Stmt::Expr { expression, .. } => self.expression(expression),
            Stmt::Module {
                statements: None, ..
            }
            | Stmt::Use { .. }
            | Stmt::Struct { .. }
            | Stmt::Enum { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Trait { .. }
            | Stmt::Continue { .. } => {}
        }
    }

    fn function(&mut self, return_type: Option<&Type>, body: &Block, self_type: Option<Type>) {
        self.return_types.push(return_type.cloned());
        self.self_types.push(self_type);
        self.block(body);
        if let Some(expected) = return_type
            && let Some(Stmt::Expr {
                expression,
                terminated: false,
            }) = body.statements.last()
        {
            self.expect(
                expected,
                self.ty(expression),
                expression.span(),
                "function result",
            );
        }
        self.self_types.pop();
        self.return_types.pop();
    }

    fn block(&mut self, block: &Block) {
        self.statements(&block.statements);
    }

    fn expression(&mut self, expression: &Expr) {
        match expression {
            Expr::Member { object, .. }
            | Expr::Borrow { target: object, .. }
            | Expr::Try {
                operand: object, ..
            } => self.expression(object),
            Expr::Index { object, index, .. } => {
                self.expression(object);
                self.expression(index);
                self.expect(
                    &Type::USIZE,
                    self.ty(index),
                    index.span(),
                    "collection index",
                );
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
                for pair in elements.windows(2) {
                    self.expect(
                        &self.ty(&pair[0]),
                        self.ty(&pair[1]),
                        pair[1].span(),
                        "array element",
                    );
                }
                if let Some(repeat) = repeat {
                    self.expression(repeat);
                    self.expect(
                        &Type::USIZE,
                        self.ty(repeat),
                        repeat.span(),
                        "array repeat count",
                    );
                }
            }
            Expr::RecordLiteral { fields, .. } => {
                for (_, value) in fields {
                    self.expression(value);
                }
            }
            Expr::Assign {
                target,
                value,
                span,
            } => {
                self.expression(target);
                self.expression(value);
                self.expect(&self.ty(target), self.ty(value), *span, "assigned value");
            }
            Expr::Unary {
                operator,
                operand,
                span,
            } => {
                self.expression(operand);
                match operator {
                    UnaryOp::Not => self.expect_bool(operand, "operand of `!`"),
                    UnaryOp::Negate => {
                        let ty = self.ty(operand);
                        let signed = matches!(
                            ty,
                            Type::Integer(integer) if integer.is_signed()
                        ) || matches!(ty, Type::Float(_));
                        if !signed && !matches!(ty, Type::Unknown | Type::Variable(_)) {
                            self.diagnostic(
                                format!("operand of unary `-` must be numeric, found `{ty}`"),
                                *span,
                            );
                        }
                    }
                    UnaryOp::Dereference => {
                        let ty = self.ty(operand);
                        if !matches!(
                            ty,
                            Type::Reference { .. } | Type::Unknown | Type::Variable(_)
                        ) {
                            self.diagnostic(
                                format!("cannot dereference value of type `{ty}`"),
                                *span,
                            );
                        }
                    }
                }
            }
            Expr::Cast {
                operand,
                target,
                span,
            } => {
                self.expression(operand);
                let source = self.ty(operand);
                match (&source, target) {
                    (Type::Integer(source), Type::Integer(target))
                        if source.can_cast_losslessly_to(*target) => {}
                    (Type::Unknown | Type::IntegerVariable(_), Type::Integer(_)) => {}
                    _ => self.diagnostic(
                        format!("cannot losslessly cast `{source}` to `{target}`"),
                        *span,
                    ),
                }
            }
            Expr::Binary {
                left,
                operator,
                right,
                ..
            } => {
                self.expression(left);
                self.expression(right);
                let left_type = self.ty(left);
                let right_type = self.ty(right);
                if !binary_compatible(*operator, &left_type, &right_type) {
                    self.diagnostic(
                        format!("binary operator cannot combine `{left_type}` and `{right_type}`"),
                        right.span(),
                    );
                }
            }
            Expr::Logical { left, right, .. } => {
                self.expression(left);
                self.expression(right);
                self.expect_bool(left, "logical operand");
                self.expect_bool(right, "logical operand");
            }
            Expr::Range { start, end, .. } => {
                self.expression(start);
                self.expression(end);
                let start_type = self.ty(start);
                let end_type = self.ty(end);
                if !start_type.is_integer() || start_type != end_type {
                    self.diagnostic(
                        format!(
                            "range bounds must have the same integer type, found `{start_type}` and `{end_type}`"
                        ),
                        start.span().merge(end.span()),
                    );
                }
            }
            Expr::Call {
                callee,
                arguments,
                span,
            } => {
                self.expression(callee);
                for argument in arguments {
                    self.expression(argument);
                }
                if let Expr::Member { object, name, .. } = callee.as_ref()
                    && rils_builtins::integer_method(name).is_some()
                {
                    let receiver = self.ty(object);
                    if !matches!(
                        receiver,
                        Type::Integer(_) | Type::IntegerVariable(_) | Type::Unknown
                    ) && rils_builtins::float_method(name).is_none()
                    {
                        self.diagnostic(
                            format!("integer intrinsic `{name}` is not available on `{receiver}`"),
                            *span,
                        );
                        return;
                    }
                }
                if let Expr::Member { object, name, .. } = callee.as_ref()
                    && rils_builtins::float_method(name).is_some()
                {
                    let receiver = self.ty(object);
                    if !matches!(
                        receiver,
                        Type::Float(_) | Type::FloatVariable(_) | Type::Unknown
                    ) && rils_builtins::integer_method(name).is_none()
                    {
                        self.diagnostic(
                            format!("float intrinsic `{name}` is not available on `{receiver}`"),
                            *span,
                        );
                        return;
                    }
                }
                if self.check_builtin_member_call(callee, arguments, *span) {
                    return;
                }
                if self.check_builtin_call(callee, arguments, *span) {
                    return;
                }
                let Type::Function {
                    parameters: Some(parameters),
                    ..
                } = self.ty(callee)
                else {
                    return;
                };
                if parameters.len() != arguments.len() {
                    self.diagnostic(
                        format!(
                            "function expects {} arguments, found {}",
                            parameters.len(),
                            arguments.len()
                        ),
                        *span,
                    );
                    return;
                }
                for (expected, argument) in parameters.iter().zip(arguments) {
                    self.expect(expected, self.ty(argument), argument.span(), "argument");
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expression(condition);
                self.expect_bool(condition, "if condition");
                self.block(then_branch);
                if let Some(else_branch) = else_branch {
                    self.expression(else_branch);
                    if let Some(Stmt::Expr {
                        expression: then_value,
                        terminated: false,
                    }) = then_branch.statements.last()
                    {
                        self.expect(
                            &self.ty(then_value),
                            self.ty(else_branch),
                            else_branch.span(),
                            "if branch",
                        );
                    }
                }
            }
            Expr::Match { value, arms, .. } => {
                self.expression(value);
                let mut first = None;
                for arm in arms {
                    self.expression(&arm.expression);
                    if let Some(expected) = &first {
                        self.expect(
                            expected,
                            self.ty(&arm.expression),
                            arm.expression.span(),
                            "match arm",
                        );
                    } else {
                        first = Some(self.ty(&arm.expression));
                    }
                }
            }
            Expr::Block(block) => self.block(block),
            Expr::Literal { .. }
            | Expr::Variable { .. }
            | Expr::Path { .. }
            | Expr::QualifiedPath { .. } => {}
        }
    }

    fn check_builtin_call(&mut self, callee: &Expr, arguments: &[Expr], span: Span) -> bool {
        let Expr::Variable { name, .. } = callee else {
            return false;
        };
        let Some(signature) = rils_builtins::builtin_function(name).and_then(|item| item.signature)
        else {
            return false;
        };
        if signature.variadic {
            return true;
        }
        let arity = signature.parameters.len();
        if arguments.len() != arity {
            self.diagnostic(
                format!(
                    "function expects {arity} arguments, found {}",
                    arguments.len()
                ),
                span,
            );
            return true;
        }
        if name == "unwrap_or" {
            let container = self.ty(&arguments[0]);
            let expected = match container {
                Type::Option(inner) | Type::Result(inner, _) => Some(*inner),
                Type::Unknown | Type::Variable(_) => None,
                actual => {
                    self.diagnostic(
                        format!("argument expects `Option<T>` or `Result<T, E>`, found `{actual}`"),
                        arguments[0].span(),
                    );
                    None
                }
            };
            if let Some(expected) = expected {
                self.expect(
                    &expected,
                    self.ty(&arguments[1]),
                    arguments[1].span(),
                    "default argument",
                );
            }
        }
        true
    }

    fn check_builtin_member_call(&mut self, callee: &Expr, arguments: &[Expr], span: Span) -> bool {
        let Expr::Member { object, name, .. } = callee else {
            return false;
        };
        if crate::standard_library::builtin_receiver_mode(&self.ty(object), name).is_none() {
            return false;
        }
        let Type::Function {
            parameters: Some(parameters),
            ..
        } = self.ty(callee)
        else {
            return false;
        };
        if parameters.len() != arguments.len() {
            self.diagnostic(
                format!(
                    "method expects {} arguments, found {}",
                    parameters.len(),
                    arguments.len()
                ),
                span,
            );
            return true;
        }
        for (expected, argument) in parameters.iter().zip(arguments) {
            self.expect(expected, self.ty(argument), argument.span(), "argument");
        }
        true
    }

    fn expect_bool(&mut self, expression: &Expr, subject: &str) {
        self.expect(&Type::Bool, self.ty(expression), expression.span(), subject);
    }

    fn expect(&mut self, expected: &Type, actual: Type, span: Span, subject: &str) {
        let expected = self.expand(expected, &mut HashSet::new());
        let actual = self.expand(&actual, &mut HashSet::new());
        if merge_types(&expected, &actual).is_none() {
            self.diagnostic(
                format!("{subject} expects `{expected}`, found `{actual}`"),
                span,
            );
        }
    }

    fn expand(&self, ty: &Type, visiting: &mut HashSet<String>) -> Type {
        let Type::Named { name, arguments } = ty else {
            return ty.clone();
        };
        if name == "Self"
            && arguments.is_empty()
            && let Some(Some(self_type)) = self.self_types.last()
        {
            return self_type.clone();
        }
        let Some(alias) = self.aliases.get(name) else {
            return ty.clone();
        };
        if !visiting.insert(name.clone()) {
            return ty.clone();
        }
        let substitutions = alias
            .parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        let expanded = self.expand(&alias.target.substitute(&substitutions), visiting);
        visiting.remove(name);
        expanded
    }

    fn ty(&self, expression: &Expr) -> Type {
        self.expression_types
            .get(&expression.span())
            .cloned()
            .unwrap_or(Type::Unknown)
    }

    fn diagnostic(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(AnalysisDiagnostic::error(message, span));
    }
}

fn binary_compatible(operator: BinaryOp, left: &Type, right: &Type) -> bool {
    if matches!(left, Type::Unknown | Type::Variable(_))
        || matches!(right, Type::Unknown | Type::Variable(_))
        || matches!(operator, BinaryOp::Equal | BinaryOp::NotEqual)
    {
        return true;
    }
    (left == right && left.is_numeric())
        || (operator == BinaryOp::Add && left == &Type::String && right == &Type::String)
}
