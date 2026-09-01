use std::collections::{HashMap, HashSet};

use crate::{
    analysis::AnalysisDiagnostic,
    ast::{Block, EnumVariant, Expr, Literal, MatchArm, Pattern, Program, Stmt},
    semantic::ExpressionTypes,
    source::Span,
    types::Type,
};

pub(crate) fn analyze(
    program: &Program,
    expression_types: ExpressionTypes<'_>,
    host_contract: Option<&rils_host::HostContract>,
) -> Vec<AnalysisDiagnostic> {
    Checker::new(program, expression_types, host_contract).run(program)
}

struct Checker<'a> {
    expression_types: ExpressionTypes<'a>,
    enums: HashMap<String, Vec<String>>,
    diagnostics: Vec<AnalysisDiagnostic>,
}

impl<'a> Checker<'a> {
    fn new(
        program: &Program,
        expression_types: ExpressionTypes<'a>,
        host_contract: Option<&rils_host::HostContract>,
    ) -> Self {
        let mut checker = Self {
            expression_types,
            enums: HashMap::new(),
            diagnostics: Vec::new(),
        };
        if let Some(host) = host_contract {
            for declaration in host.types() {
                if let Some(host_enum) = declaration.enum_definition.as_ref() {
                    checker.enums.insert(
                        declaration.name.clone(),
                        host_enum.variants.keys().cloned().collect(),
                    );
                }
            }
        }
        checker.collect_enums(&program.statements);
        checker
    }

    fn run(mut self, program: &Program) -> Vec<AnalysisDiagnostic> {
        self.statements(&program.statements);
        self.diagnostics
    }

    fn collect_enums(&mut self, statements: &[Stmt]) {
        for statement in statements {
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.collect_enums(statements),
                Stmt::Enum { name, variants, .. } => {
                    self.enums.insert(
                        name.clone(),
                        variants
                            .iter()
                            .map(|variant| match variant {
                                EnumVariant::Unit { name, .. }
                                | EnumVariant::Tuple { name, .. }
                                | EnumVariant::Record { name, .. } => name.clone(),
                            })
                            .collect(),
                    );
                }
                _ => {}
            }
        }
    }

    fn statements(&mut self, statements: &[Stmt]) -> bool {
        let mut reachable = true;
        for statement in statements {
            if !reachable {
                self.diagnostics.push(AnalysisDiagnostic::warning(
                    "unreachable statement",
                    statement_span(statement),
                ));
            }
            let falls_through = self.statement(statement);
            if reachable {
                reachable = falls_through;
            }
        }
        reachable
    }

    fn statement(&mut self, statement: &Stmt) -> bool {
        match statement {
            Stmt::Module {
                statements: Some(statements),
                ..
            } => {
                self.statements(statements);
                true
            }
            Stmt::Function {
                return_type, body, ..
            } => {
                self.function(return_type.as_ref(), body);
                true
            }
            Stmt::Impl { methods, .. } => {
                for method in methods {
                    self.function(method.return_type.as_ref(), &method.body);
                }
                true
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expression(condition);
                self.block(body);
                true
            }
            Stmt::Loop { body, .. } => {
                let can_break = block_contains_break(body);
                self.block(body);
                can_break
            }
            Stmt::For { iterable, body, .. } => {
                self.expression(iterable);
                self.block(body);
                true
            }
            Stmt::Return { value, .. } | Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
                false
            }
            Stmt::Continue { .. } => false,
            Stmt::Let { initializer, .. } => {
                self.expression(initializer);
                true
            }
            Stmt::Expr { expression, .. } => self.expression(expression),
            Stmt::Module {
                statements: None, ..
            }
            | Stmt::Use { .. }
            | Stmt::Struct { .. }
            | Stmt::Enum { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Trait { .. } => true,
        }
    }

    fn function(&mut self, return_type: Option<&Type>, body: &Block) {
        let falls_through = self.block(body);
        let Some(expected) = return_type else {
            return;
        };
        if !falls_through || matches!(expected, Type::Unit | Type::Unknown) {
            return;
        }
        if block_can_produce_unit(body) {
            self.diagnostics.push(AnalysisDiagnostic::error(
                format!("not all paths return `{expected}`"),
                body.span,
            ));
        }
    }

    fn block(&mut self, block: &Block) -> bool {
        self.statements(&block.statements)
    }

    fn expression(&mut self, expression: &Expr) -> bool {
        match expression {
            Expr::Member { object, .. }
            | Expr::Try {
                operand: object, ..
            }
            | Expr::Borrow { target: object, .. }
            | Expr::Unary {
                operand: object, ..
            }
            | Expr::Cast {
                operand: object, ..
            } => {
                self.expression(object);
                true
            }
            Expr::Index { object, index, .. }
            | Expr::Assign {
                target: object,
                value: index,
                ..
            }
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
                true
            }
            Expr::Tuple { elements, .. } | Expr::Array { elements, .. } => {
                for element in elements {
                    self.expression(element);
                }
                if let Expr::Array {
                    repeat: Some(repeat),
                    ..
                } = expression
                {
                    self.expression(repeat);
                }
                true
            }
            Expr::RecordLiteral { fields, .. } => {
                for field in fields {
                    self.expression(&field.value);
                }
                true
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                self.expression(callee);
                for argument in arguments {
                    self.expression(argument);
                }
                true
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expression(condition);
                let then_falls_through = self.block(then_branch);
                let else_falls_through = else_branch
                    .as_deref()
                    .is_none_or(|branch| self.expression(branch));
                then_falls_through || else_falls_through
            }
            Expr::Match { value, arms, .. } => {
                self.expression(value);
                let exhaustive = self.check_match(value, arms);
                let mut any_arm_falls_through = false;
                for arm in arms {
                    // Analyze every arm even after one falls through so diagnostics remain complete.
                    any_arm_falls_through =
                        self.expression(&arm.expression) || any_arm_falls_through;
                }
                !exhaustive || any_arm_falls_through
            }
            Expr::Block(block) => self.block(block),
            Expr::Literal { .. }
            | Expr::Variable { .. }
            | Expr::Path { .. }
            | Expr::QualifiedPath { .. } => true,
        }
    }

    fn check_match(&mut self, value: &Expr, arms: &[MatchArm]) -> bool {
        let ty = self
            .expression_types
            .get(value)
            .cloned()
            .unwrap_or(Type::Unknown);
        let domain = match_domain(&ty, &self.enums);
        let mut covered = HashSet::new();
        let mut catch_all = false;

        for arm in arms {
            let coverage = pattern_coverage(&arm.pattern, &ty, &self.enums);
            let domain_already_covered = domain
                .as_ref()
                .is_some_and(|domain| domain.iter().all(|key| covered.contains(key)));
            let unreachable = catch_all
                || domain_already_covered
                || (!coverage.is_empty() && coverage.iter().all(|key| covered.contains(key)));
            if unreachable {
                self.diagnostics.push(AnalysisDiagnostic::warning(
                    "unreachable match arm",
                    arm.pattern.span(),
                ));
                continue;
            }
            if coverage.is_empty() && is_irrefutable(&arm.pattern) {
                catch_all = true;
            } else {
                covered.extend(coverage);
            }
        }

        let exhaustive = catch_all
            || domain
                .as_ref()
                .is_some_and(|domain| domain.iter().all(|key| covered.contains(key)));
        if !exhaustive && let Some(domain) = domain {
            let missing = domain
                .into_iter()
                .filter(|key| !covered.contains(key))
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                self.diagnostics.push(AnalysisDiagnostic::error(
                    format!("non-exhaustive match; missing {}", missing.join(", ")),
                    value.span(),
                ));
            }
        }
        exhaustive
    }
}

fn block_can_produce_unit(block: &Block) -> bool {
    let Some(last) = block.statements.last() else {
        return true;
    };
    match last {
        Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. } => false,
        Stmt::Expr {
            expression,
            terminated: false,
        } => expression_can_produce_unit(expression),
        _ => true,
    }
}

fn expression_can_produce_unit(expression: &Expr) -> bool {
    match expression {
        Expr::Literal {
            value: Literal::Unit,
            ..
        }
        | Expr::Assign { .. } => true,
        Expr::If {
            then_branch,
            else_branch,
            ..
        } => {
            else_branch.is_none()
                || block_can_produce_unit(then_branch)
                || else_branch
                    .as_deref()
                    .is_some_and(expression_can_produce_unit)
        }
        Expr::Match { arms, .. } => arms
            .iter()
            .any(|arm| expression_can_produce_unit(&arm.expression)),
        Expr::Block(block) => block_can_produce_unit(block),
        _ => false,
    }
}

fn match_domain(ty: &Type, enums: &HashMap<String, Vec<String>>) -> Option<Vec<String>> {
    match ty {
        Type::Bool => Some(vec!["true".into(), "false".into()]),
        Type::Option(inner) => Some(
            variant_domain("Some", inner, enums)
                .into_iter()
                .chain(["None".into()])
                .collect(),
        ),
        Type::Result(ok, error) => Some(
            variant_domain("Ok", ok, enums)
                .into_iter()
                .chain(variant_domain("Err", error, enums))
                .collect(),
        ),
        Type::Named { name, .. } => enums.get(name).cloned(),
        _ => None,
    }
}

fn variant_domain(prefix: &str, inner: &Type, enums: &HashMap<String, Vec<String>>) -> Vec<String> {
    match_domain(inner, enums).map_or_else(
        || vec![prefix.into()],
        |domain| {
            domain
                .into_iter()
                .map(|key| format!("{prefix}({key})"))
                .collect()
        },
    )
}

fn pattern_coverage(
    pattern: &Pattern,
    ty: &Type,
    enums: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    match pattern {
        Pattern::Literal {
            value: Literal::Bool(value),
            ..
        } if matches!(ty, Type::Bool) => vec![value.to_string()],
        Pattern::Literal { value, .. } => literal_key(value).into_iter().collect(),
        Pattern::Some { inner, .. } => {
            let Type::Option(inner_type) = ty else {
                return Vec::new();
            };
            if is_irrefutable(inner) {
                return variant_domain("Some", inner_type, enums);
            }
            pattern_coverage(inner, inner_type, enums)
                .into_iter()
                .map(|key| format!("Some({key})"))
                .collect()
        }
        Pattern::None { .. } if matches!(ty, Type::Option(_)) => vec!["None".into()],
        Pattern::Ok { inner, .. } => {
            let Type::Result(ok_type, _) = ty else {
                return Vec::new();
            };
            if is_irrefutable(inner) {
                return variant_domain("Ok", ok_type, enums);
            }
            pattern_coverage(inner, ok_type, enums)
                .into_iter()
                .map(|key| format!("Ok({key})"))
                .collect()
        }
        Pattern::Err { inner, .. } => {
            let Type::Result(_, error_type) = ty else {
                return Vec::new();
            };
            if is_irrefutable(inner) {
                return variant_domain("Err", error_type, enums);
            }
            pattern_coverage(inner, error_type, enums)
                .into_iter()
                .map(|key| format!("Err({key})"))
                .collect()
        }
        Pattern::TupleVariant { path, fields, .. } if fields.iter().all(is_irrefutable) => {
            path.last().cloned().into_iter().collect()
        }
        Pattern::Record { path, fields, .. }
            if fields.iter().all(|(_, pattern)| is_irrefutable(pattern)) =>
        {
            path.last().cloned().into_iter().collect()
        }
        Pattern::Path { path, .. } => path.last().cloned().into_iter().collect(),
        _ => Vec::new(),
    }
}

fn literal_key(literal: &Literal) -> Option<String> {
    match literal {
        Literal::Unit => Some("literal:()".into()),
        Literal::Bool(value) => Some(format!("literal:{value}")),
        Literal::I8(value) => Some(format!("literal:i8:{value}")),
        Literal::I16(value) => Some(format!("literal:i16:{value}")),
        Literal::I32(value) => Some(format!("literal:i32:{value}")),
        Literal::I64(value) => Some(format!("literal:i64:{value}")),
        Literal::I128(value) => Some(format!("literal:i128:{value}")),
        Literal::Isize(value) => Some(format!("literal:isize:{value}")),
        Literal::U8(value) => Some(format!("literal:u8:{value}")),
        Literal::U16(value) => Some(format!("literal:u16:{value}")),
        Literal::U32(value) => Some(format!("literal:u32:{value}")),
        Literal::U64(value) => Some(format!("literal:u64:{value}")),
        Literal::U128(value) => Some(format!("literal:u128:{value}")),
        Literal::Usize(value) => Some(format!("literal:usize:{value}")),
        Literal::F32(value) => Some(format!("literal:f32:{:08x}", value.to_bits())),
        Literal::F64(value) => Some(format!("literal:f64:{:016x}", value.to_bits())),
        Literal::Char(value) => Some(format!("literal:char:{value:?}")),
        Literal::Integer(value) => Some(format!("literal:integer:{value}")),
        Literal::Float(value) => Some(format!("literal:f64:{:016x}", value.to_bits())),
        Literal::String(value) => Some(format!("literal:{value:?}")),
    }
}

fn block_contains_break(block: &Block) -> bool {
    block.statements.iter().any(statement_contains_break)
}

fn statement_contains_break(statement: &Stmt) -> bool {
    match statement {
        Stmt::Break { .. } => true,
        Stmt::Expr { expression, .. } => expression_contains_break(expression),
        Stmt::While { .. }
        | Stmt::Loop { .. }
        | Stmt::For { .. }
        | Stmt::Function { .. }
        | Stmt::Impl { .. }
        | Stmt::Trait { .. } => false,
        _ => false,
    }
}

fn expression_contains_break(expression: &Expr) -> bool {
    match expression {
        Expr::If {
            then_branch,
            else_branch,
            ..
        } => {
            block_contains_break(then_branch)
                || else_branch
                    .as_deref()
                    .is_some_and(expression_contains_break)
        }
        Expr::Match { arms, .. } => arms
            .iter()
            .any(|arm| expression_contains_break(&arm.expression)),
        Expr::Block(block) => block_contains_break(block),
        _ => false,
    }
}

fn is_irrefutable(pattern: &Pattern) -> bool {
    matches!(pattern, Pattern::Wildcard { .. } | Pattern::Binding { .. })
}

fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Module { span, .. }
        | Stmt::Use { span, .. }
        | Stmt::Let { span, .. }
        | Stmt::Function { span, .. }
        | Stmt::Struct { span, .. }
        | Stmt::Enum { span, .. }
        | Stmt::TypeAlias { span, .. }
        | Stmt::Impl { span, .. }
        | Stmt::Trait { span, .. }
        | Stmt::While { span, .. }
        | Stmt::Loop { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Break { span, .. }
        | Stmt::Continue { span, .. } => *span,
        Stmt::Expr { expression, .. } => expression.span(),
    }
}
