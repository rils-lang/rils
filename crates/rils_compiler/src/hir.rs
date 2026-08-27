use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    ast::{Block, Expr, Literal, Pattern, Program, Stmt, UnaryOp},
    bytecode::CompileError,
    host::{HostContract, HostFunctionDeclaration},
    source::{SourceFile, Span},
    types::{FunctionSignature, Type},
};

mod combinators;
mod imports;
mod ir;
mod iterator_defaults;
mod symbols;

use imports::*;
pub use ir::*;
use symbols::*;

fn overload_score(host: &HostContract, expected: &[Type], actual: &[Type]) -> Option<usize> {
    expected
        .iter()
        .zip(actual)
        .try_fold(0usize, |score, (expected, actual)| {
            if expected == actual {
                return Some(score);
            }
            if matches!(
                actual,
                Type::Unknown | Type::IntegerVariable(_) | Type::FloatVariable(_)
            ) {
                return Some(score + 100);
            }
            match (expected, actual) {
                (
                    Type::Named {
                        name: expected,
                        arguments: expected_arguments,
                    },
                    Type::Named {
                        name: actual,
                        arguments: actual_arguments,
                    },
                ) if expected_arguments.is_empty()
                    && actual_arguments.is_empty()
                    && host.is_type_assignable(expected, actual) =>
                {
                    Some(score + host.type_assignment_distance(expected, actual)?)
                }
                _ => None,
            }
        })
}

fn format_host_candidates(candidates: &[HostFunctionDeclaration]) -> String {
    candidates
        .iter()
        .map(|candidate| {
            format!(
                "  {}({}) -> {}",
                candidate.name,
                candidate
                    .signature
                    .parameters
                    .as_ref()
                    .expect("host signatures are fixed")
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                candidate.signature.return_type
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_host_use_aliases(
    statements: &[Stmt],
    prefix: &mut Vec<String>,
    functions: &mut HashMap<String, Vec<HostFunctionDeclaration>>,
) {
    for statement in statements {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        match statement {
            Stmt::Use { imports, .. } => {
                for import in imports {
                    let candidates = use_resolution_candidates(prefix, &import.path);
                    if import.kind == crate::ast::UseImportKind::Glob {
                        let declarations = functions
                            .iter()
                            .filter_map(|(name, declaration)| {
                                let member = candidates
                                    .iter()
                                    .find_map(|candidate| immediate_path_member(name, candidate))?;
                                Some((member.to_owned(), declaration.clone()))
                            })
                            .collect::<Vec<_>>();
                        for (name, declaration) in declarations {
                            functions.insert(qualified_name(prefix, &name), declaration);
                        }
                        continue;
                    }
                    let declarations = candidates
                        .iter()
                        .find_map(|candidate| functions.get(candidate))
                        .cloned();
                    if let Some(declarations) = declarations {
                        functions.insert(
                            qualified_name(prefix, import.binding_name().expect("single import")),
                            declarations,
                        );
                    }
                }
            }
            Stmt::Module {
                name,
                statements: Some(module_statements),
                ..
            } => {
                prefix.push(name.clone());
                collect_host_use_aliases(module_statements, prefix, functions);
                prefix.pop();
            }
            _ => {}
        }
    }
}

pub(crate) fn lower_with_host(
    program: &Program,
    host: &HostContract,
    analysis: &rils_frontend::analysis::DocumentAnalysis,
    sources: Vec<SourceFile>,
) -> Result<HirProgram, CompileError> {
    ProgramLowerer::new(program, host, analysis)?.lower(program, sources)
}

struct ProgramLowerer {
    functions: HashMap<String, FunctionId>,
    methods: HashMap<String, MethodInfo>,
    types: HashMap<String, TypeId>,
    type_definitions: Vec<HirTypeDefinition>,
    host_functions: HashMap<String, Vec<HostFunctionDeclaration>>,
    host_methods: HashMap<String, Vec<HostFunctionDeclaration>>,
    host_contract: HostContract,
    expression_types: HashMap<Span, Type>,
    typeck_results: rils_frontend::semantic::TypeckResults,
    resolved_definitions: HashMap<rils_frontend::DefId, MethodInfo>,
}

impl ProgramLowerer {
    fn new(
        program: &Program,
        host: &HostContract,
        analysis: &rils_frontend::analysis::DocumentAnalysis,
    ) -> Result<Self, CompileError> {
        let mut functions = HashMap::new();
        let mut types = HashMap::new();
        let mut type_definitions = Vec::new();
        for statement in &program.statements {
            if let Some(declaration) = function_declaration(statement) {
                let id = functions.len() + 1;
                if functions.insert(declaration.name.to_string(), id).is_some() {
                    return Err(CompileError::unsupported(
                        format!("duplicate function `{}`", declaration.name),
                        declaration.span,
                    ));
                }
            }
            let statement = match statement {
                Stmt::Public { statement, .. } => statement.as_ref(),
                statement => statement,
            };
            let definition = match statement {
                Stmt::Struct {
                    name,
                    generic_parameters,
                    fields,
                    ..
                } => Some(HirTypeDefinition::Struct {
                    name: name.clone(),
                    generic_parameters: generic_parameters.clone(),
                    fields: fields.clone(),
                }),
                Stmt::Enum {
                    name,
                    generic_parameters,
                    variants,
                    ..
                } => Some(HirTypeDefinition::Enum {
                    name: name.clone(),
                    generic_parameters: generic_parameters.clone(),
                    variants: variants.clone(),
                }),
                _ => None,
            };
            if let Some(definition) = definition {
                let name = match &definition {
                    HirTypeDefinition::Struct { name, .. }
                    | HirTypeDefinition::Enum { name, .. } => name.clone(),
                };
                types.insert(name, type_definitions.len());
                type_definitions.push(definition);
            }
        }
        collect_nested_symbols(
            &program.statements,
            &mut Vec::new(),
            &mut functions,
            &mut types,
            &mut type_definitions,
        )?;

        let mut methods = HashMap::new();
        let mut next_method_id = functions.values().copied().max().unwrap_or(0) + 1;
        collect_method_symbols(
            &program.statements,
            &mut Vec::new(),
            &mut next_method_id,
            &mut methods,
        );
        let mut declarations = program
            .statements
            .iter()
            .filter_map(function_declaration)
            .map(|declaration| (functions[&declaration.qualified_name], declaration))
            .collect::<Vec<_>>();
        collect_nested_function_declarations(
            &program.statements,
            &mut Vec::new(),
            &functions,
            &mut declarations,
        );
        collect_method_declarations(
            &program.statements,
            &mut Vec::new(),
            &methods,
            &mut declarations,
        );
        let method_by_function = methods
            .values()
            .map(|method| (method.function, *method))
            .collect::<HashMap<_, _>>();
        let resolved_definitions = declarations
            .iter()
            .filter_map(|(function, declaration)| {
                let definition = analysis.def_map.resolution(declaration.name_span)?;
                let callable = method_by_function
                    .get(function)
                    .copied()
                    .unwrap_or(MethodInfo {
                        function: *function,
                        receiver: None,
                        source: declaration.name_span.source,
                    });
                Some((definition, callable))
            })
            .collect();
        let mut public_symbols = HashSet::new();
        collect_public_symbols(&program.statements, &mut Vec::new(), &mut public_symbols);
        collect_use_aliases(
            &program.statements,
            &mut Vec::new(),
            &mut functions,
            &mut types,
            &public_symbols,
        );
        let mut host_functions = host.function_overloads();
        collect_host_use_aliases(&program.statements, &mut Vec::new(), &mut host_functions);
        let host_methods = host.method_function_overloads();
        Ok(Self {
            functions,
            methods,
            types,
            type_definitions,
            host_functions,
            host_methods,
            host_contract: host.clone(),
            expression_types: analysis.expression_types.clone(),
            typeck_results: analysis.typeck_results.clone(),
            resolved_definitions,
        })
    }

    fn lower(
        self,
        program: &Program,
        sources: Vec<SourceFile>,
    ) -> Result<HirProgram, CompileError> {
        let generated = GeneratedFunctions {
            next_id: Rc::new(Cell::new(
                self.methods
                    .values()
                    .map(|method| method.function)
                    .chain(self.functions.values().copied())
                    .max()
                    .unwrap_or(0)
                    + 1,
            )),
            functions: Rc::new(RefCell::new(Vec::new())),
        };
        let mut lowered = Vec::with_capacity(self.functions.len() + 1);
        let entry_statements = program
            .statements
            .iter()
            .filter(|statement| !is_compile_time_declaration(statement))
            .collect::<Vec<_>>();
        lowered.push(
            FunctionLowerer::new(
                &self.types,
                &self.host_functions,
                &self.host_methods,
                &self.host_contract,
                &self.expression_types,
                &self.typeck_results,
                &self.resolved_definitions,
                generated.clone(),
            )
            .lower_entry(&entry_statements)?,
        );

        let mut declarations = program
            .statements
            .iter()
            .filter_map(function_declaration)
            .map(|declaration| (self.functions[&declaration.qualified_name], declaration))
            .collect::<Vec<_>>();
        collect_nested_function_declarations(
            &program.statements,
            &mut Vec::new(),
            &self.functions,
            &mut declarations,
        );
        collect_method_declarations(
            &program.statements,
            &mut Vec::new(),
            &self.methods,
            &mut declarations,
        );
        declarations.sort_by_key(|(id, _)| *id);
        for (_, declaration) in declarations {
            lowered.push(
                FunctionLowerer::new(
                    &self.types,
                    &self.host_functions,
                    &self.host_methods,
                    &self.host_contract,
                    &self.expression_types,
                    &self.typeck_results,
                    &self.resolved_definitions,
                    generated.clone(),
                )
                .lower_function(declaration)?,
            );
        }
        let mut generated_functions = generated.functions.borrow_mut();
        generated_functions.sort_by_key(|(id, _)| *id);
        lowered.extend(generated_functions.drain(..).map(|(_, function)| function));
        Ok(HirProgram {
            sources,
            functions: lowered,
            types: self.type_definitions,
            iterators: iterator_methods(&self.methods),
            trait_implementations: trait_implementations(&self.methods),
            entry: 0,
        })
    }
}

#[derive(Clone)]
struct GeneratedFunctions {
    next_id: Rc<Cell<FunctionId>>,
    functions: Rc<RefCell<Vec<(FunctionId, HirFunction)>>>,
}

struct FunctionLowerer<'a> {
    types: &'a HashMap<String, TypeId>,
    host_functions: &'a HashMap<String, Vec<HostFunctionDeclaration>>,
    host_methods: &'a HashMap<String, Vec<HostFunctionDeclaration>>,
    host_contract: &'a HostContract,
    expression_types: &'a HashMap<Span, Type>,
    typeck_results: &'a rils_frontend::semantic::TypeckResults,
    resolved_definitions: &'a HashMap<rils_frontend::DefId, MethodInfo>,
    namespace: String,
    self_type: Option<String>,
    scopes: Vec<HashMap<String, LocalId>>,
    mutable: Vec<bool>,
    in_function: bool,
    capture_count: usize,
    generated: GeneratedFunctions,
    captured: HashSet<LocalId>,
}

impl<'a> FunctionLowerer<'a> {
    #[allow(
        clippy::too_many_arguments,
        reason = "the lowerer borrows one immutable table per compiler identity domain"
    )]
    fn new(
        types: &'a HashMap<String, TypeId>,
        host_functions: &'a HashMap<String, Vec<HostFunctionDeclaration>>,
        host_methods: &'a HashMap<String, Vec<HostFunctionDeclaration>>,
        host_contract: &'a HostContract,
        expression_types: &'a HashMap<Span, Type>,
        typeck_results: &'a rils_frontend::semantic::TypeckResults,
        resolved_definitions: &'a HashMap<rils_frontend::DefId, MethodInfo>,
        generated: GeneratedFunctions,
    ) -> Self {
        Self {
            types,
            host_functions,
            host_methods,
            host_contract,
            expression_types,
            typeck_results,
            resolved_definitions,
            namespace: String::new(),
            self_type: None,
            scopes: vec![HashMap::new()],
            mutable: Vec::new(),
            in_function: false,
            capture_count: 0,
            generated,
            captured: HashSet::new(),
        }
    }

    fn lower_entry(mut self, statements: &[&Stmt]) -> Result<HirFunction, CompileError> {
        let statements = statements
            .iter()
            .map(|statement| self.statement(statement))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HirFunction {
            name: "<script>".into(),
            exported: false,
            parameter_count: 0,
            capture_count: 0,
            local_count: self.mutable.len(),
            local_mutability: self.mutable,
            statements,
            span: Span::default(),
        })
    }

    fn lower_function(
        mut self,
        declaration: FunctionDeclaration<'_>,
    ) -> Result<HirFunction, CompileError> {
        self.in_function = true;
        self.namespace = declaration
            .qualified_name
            .rsplit_once("::")
            .map_or_else(String::new, |(namespace, _)| namespace.to_string());
        self.self_type = declaration.self_type;
        for parameter in declaration.parameters {
            let local = self.mutable.len();
            self.mutable.push(parameter.mutable);
            self.scopes[0].insert(parameter.name.clone(), local);
        }
        let statements = self.statements(&declaration.body.statements)?;
        Ok(HirFunction {
            name: declaration.qualified_name,
            exported: declaration.exported,
            parameter_count: declaration.parameters.len(),
            capture_count: self.capture_count,
            local_count: self.mutable.len(),
            local_mutability: self.mutable,
            statements,
            span: declaration.span,
        })
    }

    fn statements(&mut self, statements: &[Stmt]) -> Result<Vec<HirStatement>, CompileError> {
        statements
            .iter()
            .map(|statement| self.statement(statement))
            .collect()
    }

    fn statement(&mut self, statement: &Stmt) -> Result<HirStatement, CompileError> {
        match statement {
            Stmt::Public { statement, .. } => self.statement(statement),
            Stmt::Let {
                name,
                mutable,
                initializer,
                span,
                ..
            } => {
                let initializer = self.expression(initializer)?;
                let local = self.mutable.len();
                self.mutable.push(*mutable);
                self.scopes.last_mut().unwrap().insert(name.clone(), local);
                Ok(HirStatement::Let {
                    local,
                    initializer,
                    span: *span,
                })
            }
            Stmt::While {
                condition,
                body,
                span,
            } => Ok(HirStatement::While {
                condition: self.expression(condition)?,
                body: self.block_statements(body)?,
                span: *span,
            }),
            Stmt::Loop { body, span } => Ok(HirStatement::Loop {
                body: self.block_statements(body)?,
                span: *span,
            }),
            Stmt::For {
                binding,
                iterable,
                body,
                span,
                ..
            } => {
                let iterable = self.expression(iterable)?;
                let local = self.mutable.len();
                self.mutable.push(false);
                self.scopes.push(HashMap::new());
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(binding.clone(), local);
                let body = self.statements(&body.statements);
                self.scopes.pop();
                Ok(HirStatement::For {
                    binding: local,
                    iterable,
                    body: body?,
                    span: *span,
                })
            }
            Stmt::Return { value, span } if self.in_function => Ok(HirStatement::Return {
                value: value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?,
                span: *span,
            }),
            Stmt::Break { value, span } => Ok(HirStatement::Break {
                value: value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?,
                span: *span,
            }),
            Stmt::Continue { span } => Ok(HirStatement::Continue { span: *span }),
            Stmt::Expr {
                expression,
                terminated,
            } => Ok(HirStatement::Expression {
                expression: self.expression(expression)?,
                terminated: *terminated,
                span: expression.span(),
            }),
            Stmt::Function {
                name,
                parameters,
                body,
                span,
                ..
            } => {
                let local = self.mutable.len();
                self.mutable.push(false);
                self.scopes.last_mut().unwrap().insert(name.clone(), local);

                let mut visible = HashMap::new();
                for scope in &self.scopes {
                    visible.extend(scope.iter().map(|(name, local)| (name.clone(), *local)));
                }
                let mut captured = visible.into_iter().collect::<Vec<_>>();
                captured.sort_by_key(|(_, local)| *local);
                let captures = captured.iter().map(|(_, local)| *local).collect::<Vec<_>>();
                self.captured.extend(captures.iter().copied());

                let function = self.generated.next_id.get();
                self.generated.next_id.set(function + 1);
                let mut child = FunctionLowerer::new(
                    self.types,
                    self.host_functions,
                    self.host_methods,
                    self.host_contract,
                    self.expression_types,
                    self.typeck_results,
                    self.resolved_definitions,
                    self.generated.clone(),
                );
                child.in_function = true;
                child.capture_count = captures.len();
                child.mutable = captures.iter().map(|local| self.mutable[*local]).collect();
                child.scopes[0] = captured
                    .into_iter()
                    .enumerate()
                    .map(|(capture, (name, _))| (name, capture))
                    .collect();
                let qualified_name = format!("{}${}@{}", self.namespace, name, span.start);
                let lowered = child.lower_function(FunctionDeclaration {
                    name,
                    name_span: *span,
                    qualified_name,
                    parameters,
                    body,
                    span: *span,
                    exported: false,
                    self_type: self.self_type.clone(),
                })?;
                self.generated
                    .functions
                    .borrow_mut()
                    .push((function, lowered));
                Ok(HirStatement::DefineFunction {
                    local,
                    function,
                    captures,
                    span: *span,
                })
            }
            unsupported => Err(CompileError::unsupported(
                "this declaration is not supported by the bytecode backend yet",
                statement_span(unsupported),
            )),
        }
    }

    fn expression(&mut self, expression: &Expr) -> Result<HirExpression, CompileError> {
        match expression {
            Expr::Literal { value, span } => Ok(HirExpression::Literal {
                value: lower_literal(value),
                span: *span,
            }),
            Expr::Variable { name, span } if name == "None" => {
                Ok(HirExpression::OptionNone { span: *span })
            }
            Expr::Variable { name, span } => {
                if let Some(local) = self.lookup(name) {
                    Ok(HirExpression::Local { local, span: *span })
                } else if let Some(callable) = self.resolved_value(*span) {
                    Ok(HirExpression::Function {
                        function: callable.function,
                        span: *span,
                    })
                } else {
                    Err(CompileError::unsupported(
                        format!("bytecode backend cannot resolve non-local value `{name}`"),
                        *span,
                    ))
                }
            }
            Expr::Path { segments, span } => {
                let segments = self.resolve_self_path(segments);
                if let [type_name, member] = segments.as_slice()
                    && let Some(target) = crate::types::IntegerType::from_name(type_name)
                    && let Some(constant) = rils_builtins::integer_constant(member)
                {
                    return Ok(HirExpression::Literal {
                        value: integer_constant_literal(target, constant.id),
                        span: *span,
                    });
                }
                if let [type_name, member] = segments.as_slice()
                    && let Some(target) = crate::types::FloatType::from_name(type_name)
                    && let Some(constant) = rils_builtins::float_constant(member)
                {
                    return Ok(HirExpression::Literal {
                        value: float_constant_literal(target, constant.id),
                        span: *span,
                    });
                }
                if let Some(callable) = self.resolved_value(*span) {
                    return Ok(HirExpression::Function {
                        function: callable.function,
                        span: *span,
                    });
                }
                let (type_id, variant) = self.enum_variant_path(&segments, *span)?;
                Ok(HirExpression::ConstructUnitVariant {
                    type_id,
                    variant,
                    span: *span,
                })
            }
            Expr::QualifiedPath { span, .. } => {
                let method = self.resolved_value(*span).ok_or_else(|| {
                    CompileError::unsupported(
                        "semantic analysis did not resolve UFCS function value",
                        *span,
                    )
                })?;
                Ok(HirExpression::Function {
                    function: method.function,
                    span: *span,
                })
            }
            Expr::Member { object, name, span } if self.resolved_value(*span).is_some() => {
                let method = self.resolved_value(*span).expect("guarded method value");
                let receiver = method.receiver.ok_or_else(|| {
                    CompileError::unsupported(
                        format!("associated function `{name}` cannot be bound to a receiver"),
                        *span,
                    )
                })?;
                Ok(HirExpression::BindMethod {
                    function: method.function,
                    receiver: Box::new(self.method_receiver(object, receiver)?),
                    span: *span,
                })
            }
            Expr::Index { span, .. } | Expr::Member { span, .. } => Ok(HirExpression::Place {
                place: self.place(expression)?,
                span: *span,
            }),
            Expr::Assign {
                target,
                value,
                span,
            } => match target.as_ref() {
                Expr::Variable { name, .. } => {
                    let local = self.lookup(name).ok_or_else(|| {
                        CompileError::unsupported(format!("unknown local `{name}`"), target.span())
                    })?;
                    if !self.mutable[local] {
                        return Err(CompileError::unsupported(
                            format!("cannot assign to immutable local `{name}`"),
                            *span,
                        ));
                    }
                    Ok(HirExpression::Assign {
                        local,
                        value: Box::new(self.expression(value)?),
                        span: *span,
                    })
                }
                Expr::Index { .. } | Expr::Member { .. } => Ok(HirExpression::AssignPlace {
                    place: self.place(target)?,
                    value: Box::new(self.expression(value)?),
                    span: *span,
                }),
                Expr::Unary {
                    operator: UnaryOp::Dereference,
                    operand,
                    ..
                } => Ok(HirExpression::AssignDereference {
                    reference: Box::new(self.expression(operand)?),
                    value: Box::new(self.expression(value)?),
                    span: *span,
                }),
                _ => Err(CompileError::unsupported(
                    "assignment place is not supported by the bytecode backend yet",
                    *span,
                )),
            },
            Expr::Unary {
                operator,
                operand,
                span,
            } => Ok(HirExpression::Unary {
                operator: *operator,
                operand: Box::new(self.expression(operand)?),
                span: *span,
            }),
            Expr::Cast {
                operand,
                target,
                span,
            } => {
                let crate::types::Type::Integer(target) = target else {
                    return Err(CompileError::unsupported(
                        "`as` currently supports concrete integer target types only",
                        *span,
                    ));
                };
                Ok(HirExpression::Cast {
                    operand: Box::new(self.expression(operand)?),
                    target: *target,
                    span: *span,
                })
            }
            Expr::Borrow {
                mutable,
                target,
                span,
            } => match target.as_ref() {
                Expr::Variable { name, .. } => {
                    let local = self.lookup(name).ok_or_else(|| {
                        CompileError::unsupported(format!("unknown local `{name}`"), target.span())
                    })?;
                    Ok(HirExpression::BorrowLocal {
                        local,
                        mutable: *mutable,
                        span: *span,
                    })
                }
                Expr::Index { .. } | Expr::Member { .. } => Ok(HirExpression::BorrowPlace {
                    place: self.place(target)?,
                    mutable: *mutable,
                    span: *span,
                }),
                Expr::Unary {
                    operator: UnaryOp::Dereference,
                    operand,
                    ..
                } => Ok(HirExpression::Reborrow {
                    reference: Box::new(self.expression(operand)?),
                    mutable: *mutable,
                    span: *span,
                }),
                _ => Err(CompileError::unsupported(
                    "borrow place is not supported by the bytecode backend yet",
                    *span,
                )),
            },
            Expr::Binary {
                left,
                operator,
                right,
                span,
            } => Ok(HirExpression::Binary {
                left: Box::new(self.expression(left)?),
                operator: *operator,
                right: Box::new(self.expression(right)?),
                integer: self
                    .expression_types
                    .get(&left.span())
                    .and_then(|value| match value {
                        Type::Integer(integer) => Some(*integer),
                        _ => None,
                    }),
                span: *span,
            }),
            Expr::Logical {
                left,
                operator,
                right,
                span,
            } => Ok(HirExpression::Logical {
                left: Box::new(self.expression(left)?),
                operator: *operator,
                right: Box::new(self.expression(right)?),
                span: *span,
            }),
            Expr::Call {
                callee,
                arguments,
                span,
            } => {
                if let Some((name, signature, capability)) = self.resolved_import(*span) {
                    return Ok(HirExpression::CallImport {
                        name: name.to_owned(),
                        signature: signature.clone(),
                        capability: capability.to_owned(),
                        arguments: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                if let Expr::QualifiedPath {
                    target,
                    trait_name,
                    member,
                    ..
                } = callee.as_ref()
                {
                    if trait_name == "Default" && member == "default" {
                        if !arguments.is_empty() {
                            return Err(CompileError::unsupported(
                                "Default::default takes no arguments",
                                *span,
                            ));
                        }
                        if let Some(value) = builtin_default_hir(target, *span)? {
                            return Ok(value);
                        }
                    }
                    if let Some(callable) = self.resolved_definition(*span) {
                        return Ok(HirExpression::Call {
                            function: callable.function,
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    return Err(CompileError::unsupported(
                        format!(
                            "semantic analysis did not resolve UFCS call `<{target} as {trait_name}>::{member}`"
                        ),
                        *span,
                    ));
                }
                if let Expr::Path { segments, .. } = callee.as_ref() {
                    let segments = self.resolve_self_path(segments);
                    if let Some(callable) = self.resolved_definition(*span) {
                        return Ok(HirExpression::Call {
                            function: callable.function,
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    if let [type_name, _] = segments.as_slice()
                        && let Some(target) = crate::types::IntegerType::from_name(type_name)
                        && let Some(intrinsic) =
                            self.resolved_builtin(*span)
                                .and_then(|(id, kind, receiver)| {
                                    (kind == rils_frontend::semantic::BuiltinCallKind::Intrinsic
                                        && receiver.is_none())
                                    .then_some(id)
                                })
                    {
                        return Ok(HirExpression::CallIntrinsic {
                            intrinsic,
                            target: Some(target),
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    if let Some(host_name) = self.resolved_host(*span)
                        && let Some(function) = self.host_function(host_name, arguments, *span)?
                    {
                        return Ok(HirExpression::CallImport {
                            name: function.name,
                            signature: function.signature,
                            capability: function.capability,
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    let (type_id, variant) = self.enum_variant_path(&segments, *span)?;
                    return Ok(HirExpression::ConstructTupleVariant {
                        type_id,
                        variant,
                        fields: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                if let Expr::Member { object, name, .. } = callee.as_ref() {
                    if let Some(method) = self.resolved_definition(*span) {
                        let mut lowered = Vec::with_capacity(
                            arguments.len() + usize::from(method.receiver.is_some()),
                        );
                        if let Some(receiver) = method.receiver {
                            lowered.push(self.method_receiver(object, receiver)?);
                        }
                        lowered.extend(
                            arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<Vec<_>, _>>()?,
                        );
                        return Ok(HirExpression::Call {
                            function: method.function,
                            arguments: lowered,
                            span: *span,
                        });
                    }
                    let semantic_builtin =
                        self.typeck_results
                            .resolved_call_at(*span)
                            .and_then(|call| match call {
                                rils_frontend::semantic::ResolvedCall::Builtin {
                                    id,
                                    kind,
                                    receiver,
                                } => Some((*id, *kind, *receiver)),
                                _ => None,
                            });
                    let intrinsic = semantic_builtin
                        .filter(|(_, kind, _)| {
                            *kind == rils_frontend::semantic::BuiltinCallKind::Intrinsic
                        })
                        .map(|(id, _, _)| id);
                    if let Some(intrinsic) = intrinsic {
                        let mut lowered = Vec::with_capacity(arguments.len() + 1);
                        lowered.push(self.expression(object)?);
                        lowered.extend(
                            arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<Vec<_>, _>>()?,
                        );
                        return Ok(HirExpression::CallIntrinsic {
                            intrinsic,
                            target: None,
                            arguments: lowered,
                            span: *span,
                        });
                    }
                    if let Some((builtin, _, receiver)) = semantic_builtin.filter(|(_, kind, _)| {
                        *kind == rils_frontend::semantic::BuiltinCallKind::Runtime
                    }) {
                        if name == "into_iter"
                            && arguments.is_empty()
                            && matches!(
                                builtin,
                                rils_builtins::BuiltinId::SequenceIntoIter
                                    | rils_builtins::BuiltinId::RangeIntoIter
                                    | rils_builtins::BuiltinId::IteratorIntoIter
                            )
                        {
                            return Ok(HirExpression::IntoIterator {
                                value: Box::new(self.expression(object)?),
                                span: *span,
                            });
                        }
                        if let Some(expression) =
                            self.builtin_combinator(Some(builtin), name, object, arguments, *span)?
                        {
                            return Ok(expression);
                        }
                        if builtin.has_direct_runtime_call()
                            && let Some(receiver) = receiver.map(|receiver| match receiver {
                                rils_builtins::ReceiverMode::Owned => ReceiverMode::Owned,
                                rils_builtins::ReceiverMode::Shared => {
                                    ReceiverMode::Reference { mutable: false }
                                }
                                rils_builtins::ReceiverMode::Mutable => {
                                    ReceiverMode::Reference { mutable: true }
                                }
                            })
                        {
                            let receiver = self.method_receiver(object, receiver)?;
                            let mut lowered = Vec::with_capacity(arguments.len() + 1);
                            lowered.push(receiver);
                            lowered.extend(
                                arguments
                                    .iter()
                                    .map(|argument| self.expression(argument))
                                    .collect::<Result<Vec<_>, _>>()?,
                            );
                            return Ok(HirExpression::CallRuntime {
                                builtin,
                                arguments: lowered,
                                span: *span,
                            });
                        }
                    }
                    if self.resolved_host(*span).is_some()
                        && let Some(host) = self.host_method(object, name, arguments, *span)?
                    {
                        let receiver = match host.receiver {
                            Some(crate::host::HostReceiver::Value) => ReceiverMode::Owned,
                            Some(crate::host::HostReceiver::Ref) => {
                                ReceiverMode::Reference { mutable: false }
                            }
                            Some(crate::host::HostReceiver::RefMut) => {
                                ReceiverMode::Reference { mutable: true }
                            }
                            None => unreachable!("host_method only returns receiver methods"),
                        };
                        let mut lowered = Vec::with_capacity(arguments.len() + 1);
                        // Host ABI methods always receive the opaque handle by value.  The
                        // receiver mode still controls borrowing/ownership at the source
                        // level, but a `&self`/`&mut self` receiver must be dereferenced
                        // before crossing the import boundary (otherwise the VM passes a
                        // reference value and the host reports `expected HostHandle`).
                        let receiver_value = self.method_receiver(object, receiver)?;
                        lowered.push(match receiver {
                            ReceiverMode::Owned => receiver_value,
                            ReceiverMode::Reference { .. } => HirExpression::Unary {
                                operator: UnaryOp::Dereference,
                                operand: Box::new(receiver_value),
                                span: *span,
                            },
                        });
                        lowered.extend(
                            arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<Vec<_>, _>>()?,
                        );
                        return Ok(HirExpression::CallImport {
                            name: host.name.clone(),
                            signature: host.signature.clone(),
                            capability: host.capability.clone(),
                            arguments: lowered,
                            span: *span,
                        });
                    }
                    return Err(CompileError::unsupported(
                        format!(
                            "semantic analysis did not resolve method call `{name}` on `{}`",
                            self.expression_types
                                .get(&object.span())
                                .cloned()
                                .unwrap_or(Type::Unknown)
                        ),
                        *span,
                    ));
                }
                if let Expr::Variable { name, .. } = callee.as_ref()
                    && matches!(name.as_str(), "Some" | "Ok" | "Err")
                {
                    let [argument] = arguments.as_slice() else {
                        return Err(CompileError::unsupported(
                            format!("`{name}` expects exactly one argument"),
                            *span,
                        ));
                    };
                    let value = Box::new(self.expression(argument)?);
                    return Ok(match name.as_str() {
                        "Some" => HirExpression::OptionSome { value, span: *span },
                        "Ok" => HirExpression::ResultOk { value, span: *span },
                        "Err" => HirExpression::ResultErr { value, span: *span },
                        _ => unreachable!(),
                    });
                }
                if matches!(callee.as_ref(), Expr::Variable { .. })
                    && let Some(callable) = self.resolved_definition(*span)
                {
                    return Ok(HirExpression::Call {
                        function: callable.function,
                        arguments: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                if let Expr::Variable { name, .. } = callee.as_ref()
                    && self.lookup(name).is_none()
                    && let Some(host_name) = self.resolved_host(*span)
                    && let Some(function) = self.host_function(host_name, arguments, *span)?
                {
                    return Ok(HirExpression::CallImport {
                        name: function.name,
                        signature: function.signature,
                        capability: function.capability,
                        arguments: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                Ok(HirExpression::CallValue {
                    callee: Box::new(self.expression(callee)?),
                    arguments: arguments
                        .iter()
                        .map(|argument| self.expression(argument))
                        .collect::<Result<_, _>>()?,
                    span: *span,
                })
            }
            Expr::RecordLiteral { path, fields, span } => {
                let path = self.resolve_self_path(path);
                let (type_id, variant) = if path.len() >= 2 {
                    let enum_name = path[..path.len() - 1].join("::");
                    if let Some(type_id) = self.types.get(&enum_name) {
                        (*type_id, Some(path.last().unwrap().clone()))
                    } else {
                        (self.type_id(&path.join("::"), *span)?, None)
                    }
                } else {
                    (self.type_id(path.last().unwrap(), *span)?, None)
                };
                Ok(HirExpression::ConstructRecord {
                    type_id,
                    variant,
                    fields: fields
                        .iter()
                        .map(|field| Ok((field.name.clone(), self.expression(&field.value)?)))
                        .collect::<Result<_, CompileError>>()?,
                    span: *span,
                })
            }
            Expr::Try { operand, span } if self.in_function => Ok(HirExpression::Try {
                operand: Box::new(self.expression(operand)?),
                span: *span,
            }),
            Expr::Match { value, arms, span } => {
                let value = Box::new(self.expression(value)?);
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    self.scopes.push(HashMap::new());
                    let pattern = self.pattern(&arm.pattern)?;
                    let expression = self.expression(&arm.expression)?;
                    self.scopes.pop();
                    lowered_arms.push(HirMatchArm {
                        pattern,
                        expression,
                        span: arm.pattern.span(),
                    });
                }
                Ok(HirExpression::Match {
                    value,
                    arms: lowered_arms,
                    span: *span,
                })
            }
            Expr::Tuple { elements, span } => Ok(HirExpression::Tuple {
                elements: elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect::<Result<_, _>>()?,
                span: *span,
            }),
            Expr::Array {
                elements,
                repeat,
                span,
            } => Ok(HirExpression::Array {
                elements: elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect::<Result<_, _>>()?,
                repeat: repeat
                    .as_ref()
                    .map(|value| self.expression(value).map(Box::new))
                    .transpose()?,
                span: *span,
            }),
            Expr::Range { start, end, span } => Ok(HirExpression::Range {
                start: Box::new(self.expression(start)?),
                end: Box::new(self.expression(end)?),
                span: *span,
            }),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => Ok(HirExpression::If {
                condition: Box::new(self.expression(condition)?),
                then_branch: self.block_statements(then_branch)?,
                else_branch: else_branch
                    .as_ref()
                    .map(|branch| self.expression(branch).map(Box::new))
                    .transpose()?,
                span: *span,
            }),
            Expr::Block(block) => Ok(HirExpression::Block {
                statements: self.block_statements(block)?,
                span: block.span,
            }),
            _ => Err(CompileError::unsupported(
                "expression is not supported by the bytecode backend yet",
                expression.span(),
            )),
        }
    }

    fn block_statements(&mut self, block: &Block) -> Result<Vec<HirStatement>, CompileError> {
        let first_local = self.mutable.len();
        self.scopes.push(HashMap::new());
        let mut statements = self.statements(&block.statements)?;
        self.scopes.pop();
        for local in (first_local..self.mutable.len()).rev() {
            if self.captured.contains(&local) {
                continue;
            }
            statements.push(HirStatement::DropLocal {
                local,
                span: block.span,
            });
        }
        Ok(statements)
    }

    fn pattern(&mut self, pattern: &Pattern) -> Result<HirPattern, CompileError> {
        Ok(match pattern {
            Pattern::Wildcard { .. } => HirPattern::Wildcard,
            Pattern::Binding { name, .. } => {
                let local = self.mutable.len();
                self.mutable.push(false);
                self.scopes.last_mut().unwrap().insert(name.clone(), local);
                HirPattern::Binding(local)
            }
            Pattern::Literal { value, .. } => HirPattern::Literal(lower_literal(value)),
            Pattern::Some { inner, .. } => HirPattern::Some(Box::new(self.pattern(inner)?)),
            Pattern::None { .. } => HirPattern::None,
            Pattern::Ok { inner, .. } => HirPattern::Ok(Box::new(self.pattern(inner)?)),
            Pattern::Err { inner, .. } => HirPattern::Err(Box::new(self.pattern(inner)?)),
            Pattern::TupleVariant { path, fields, .. } => HirPattern::TupleVariant {
                path: path.clone(),
                fields: fields
                    .iter()
                    .map(|pattern| self.pattern(pattern))
                    .collect::<Result<_, _>>()?,
            },
            Pattern::Record { path, fields, .. } => HirPattern::Record {
                path: path.clone(),
                fields: fields
                    .iter()
                    .map(|(name, pattern)| Ok((name.clone(), self.pattern(pattern)?)))
                    .collect::<Result<_, CompileError>>()?,
            },
            Pattern::Path { path, .. } => HirPattern::Path(path.clone()),
        })
    }

    fn place(&mut self, expression: &Expr) -> Result<HirPlace, CompileError> {
        match expression {
            Expr::Variable { name, span } => Ok(HirPlace {
                local: self.lookup(name).ok_or_else(|| {
                    CompileError::unsupported(format!("unknown local `{name}`"), *span)
                })?,
                projections: Vec::new(),
            }),
            Expr::Member { object, name, span } => {
                let mut place = self.place(object)?;
                place.projections.push(match name.parse::<usize>() {
                    Ok(index) => HirProjection::Index(Box::new(HirExpression::Literal {
                        value: HirLiteral::Usize(index),
                        span: *span,
                    })),
                    Err(_) => HirProjection::Field(name.clone()),
                });
                Ok(place)
            }
            Expr::Index { object, index, .. } => {
                let mut place = self.place(object)?;
                place
                    .projections
                    .push(HirProjection::Index(Box::new(self.expression(index)?)));
                Ok(place)
            }
            Expr::Unary {
                operator: UnaryOp::Dereference,
                operand,
                ..
            } => self.place(operand),
            _ => Err(CompileError::unsupported(
                "place must be rooted in a local value",
                expression.span(),
            )),
        }
    }

    fn method_receiver(
        &mut self,
        expression: &Expr,
        receiver: ReceiverMode,
    ) -> Result<HirExpression, CompileError> {
        match receiver {
            ReceiverMode::Owned => self.expression(expression),
            ReceiverMode::Reference { mutable }
                if matches!(
                    self.expression_types.get(&expression.span()),
                    Some(Type::Reference { .. })
                ) =>
            {
                Ok(HirExpression::Reborrow {
                    reference: Box::new(self.expression(expression)?),
                    mutable,
                    span: expression.span(),
                })
            }
            ReceiverMode::Reference { mutable } => match expression {
                Expr::Variable { name, span } => Ok(HirExpression::BorrowLocal {
                    local: self.lookup(name).ok_or_else(|| {
                        CompileError::unsupported(format!("unknown local `{name}`"), *span)
                    })?,
                    mutable,
                    span: *span,
                }),
                Expr::Member { .. } | Expr::Index { .. } => Ok(HirExpression::BorrowPlace {
                    place: self.place(expression)?,
                    mutable,
                    span: expression.span(),
                }),
                _ => Ok(HirExpression::BorrowTemporary {
                    value: Box::new(self.expression(expression)?),
                    mutable,
                    span: expression.span(),
                }),
            },
        }
    }

    fn lookup(&self, name: &str) -> Option<LocalId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn type_id(&self, name: &str, span: Span) -> Result<TypeId, CompileError> {
        self.symbol_id(self.types, name).ok_or_else(|| {
            CompileError::unsupported(format!("unknown bytecode type `{name}`"), span)
        })
    }

    fn resolved_definition(&self, span: Span) -> Option<MethodInfo> {
        let rils_frontend::semantic::ResolvedCall::Definition(definition) =
            self.typeck_results.resolved_call_at(span)?
        else {
            return None;
        };
        self.resolved_definitions.get(definition).copied()
    }

    fn resolved_value(&self, span: Span) -> Option<MethodInfo> {
        let definition = self.typeck_results.resolved_value_at(span)?;
        self.resolved_definitions.get(&definition).copied()
    }

    fn resolved_builtin(
        &self,
        span: Span,
    ) -> Option<(
        rils_builtins::BuiltinId,
        rils_frontend::semantic::BuiltinCallKind,
        Option<rils_builtins::ReceiverMode>,
    )> {
        let rils_frontend::semantic::ResolvedCall::Builtin { id, kind, receiver } =
            self.typeck_results.resolved_call_at(span)?
        else {
            return None;
        };
        Some((*id, *kind, *receiver))
    }

    fn resolved_import(&self, span: Span) -> Option<(&str, &FunctionSignature, &str)> {
        let rils_frontend::semantic::ResolvedCall::Import {
            name,
            signature,
            capability,
        } = self.typeck_results.resolved_call_at(span)?
        else {
            return None;
        };
        Some((name, signature, capability))
    }

    fn resolved_host(&self, span: Span) -> Option<&str> {
        let rils_frontend::semantic::ResolvedCall::Host { path } =
            self.typeck_results.resolved_call_at(span)?
        else {
            return None;
        };
        Some(path)
    }

    fn host_function(
        &self,
        name: &str,
        arguments: &[Expr],
        span: Span,
    ) -> Result<Option<HostFunctionDeclaration>, CompileError> {
        self.host_function_candidates(name)
            .map(|functions| self.select_host_overload(functions, arguments, None, span))
            .transpose()
    }

    fn host_function_candidates(&self, name: &str) -> Option<&[HostFunctionDeclaration]> {
        let name = self.anchored_name(name);
        if !self.namespace.is_empty() {
            let relative = format!("{}::{name}", self.namespace);
            if let Some(functions) = self.host_functions.get(&relative) {
                return Some(functions);
            }
        }
        self.host_functions.get(&name).map(Vec::as_slice)
    }

    fn host_method(
        &self,
        object: &Expr,
        name: &str,
        call_arguments: &[Expr],
        span: Span,
    ) -> Result<Option<HostFunctionDeclaration>, CompileError> {
        let object_type = self.expression_type_for_overload(object)?;
        let Type::Named {
            name: receiver_type,
            arguments: type_arguments,
        } = &object_type
        else {
            return Ok(None);
        };
        if !type_arguments.is_empty() {
            return Ok(None);
        }
        self.host_methods
            .get(&format!("{receiver_type}::{name}"))
            .map(|functions| {
                self.select_host_overload(functions, call_arguments, Some(object), span)
            })
            .transpose()
    }

    fn select_host_overload(
        &self,
        candidates: &[HostFunctionDeclaration],
        arguments: &[Expr],
        receiver: Option<&Expr>,
        span: Span,
    ) -> Result<HostFunctionDeclaration, CompileError> {
        let actual_types = receiver
            .into_iter()
            .chain(arguments)
            .map(|argument| self.expression_type_for_overload(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let mut matches = candidates
            .iter()
            .filter_map(|candidate| {
                let expected = candidate.signature.parameters.as_ref()?;
                (expected.len() == actual_types.len())
                    .then(|| overload_score(self.host_contract, expected, &actual_types))?
                    .map(|score| (score, candidate))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|(score, candidate)| (*score, candidate.function_id));
        let name = candidates
            .first()
            .map(|candidate| candidate.name.as_str())
            .unwrap_or("<unknown>");
        let Some((best_score, best)) = matches.first().copied() else {
            return Err(CompileError::unsupported(
                format!(
                    "no host overload of `{name}` accepts ({})\navailable candidates:\n{}",
                    actual_types
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    format_host_candidates(candidates)
                ),
                span,
            ));
        };
        if matches
            .get(1)
            .is_some_and(|(score, _)| *score == best_score)
        {
            return Err(CompileError::unsupported(
                format!(
                    "ambiguous host call `{name}` for arguments ({})\ncandidates:\n{}\nadd explicit type annotations or casts to select one overload",
                    actual_types
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    format_host_candidates(
                        &matches
                            .iter()
                            .take_while(|(score, _)| *score == best_score)
                            .map(|(_, candidate)| (*candidate).clone())
                            .collect::<Vec<_>>()
                    )
                ),
                span,
            ));
        }
        Ok(best.clone())
    }

    fn expression_type_for_overload(&self, expression: &Expr) -> Result<Type, CompileError> {
        if let Expr::Call {
            callee,
            arguments,
            span,
        } = expression
        {
            let direct_name = match callee.as_ref() {
                Expr::Path { segments, .. } => Some(self.resolve_self_path(segments).join("::")),
                Expr::Variable { name, .. } if self.lookup(name).is_none() => Some(name.clone()),
                _ => None,
            };
            if let Some(name) = direct_name
                && let Some(candidates) = self.host_function_candidates(&name)
            {
                return self
                    .select_host_overload(candidates, arguments, None, *span)
                    .map(|function| function.signature.return_type);
            }
            if let Expr::Member { object, name, .. } = callee.as_ref() {
                let receiver_type = self.expression_type_for_overload(object)?;
                if let Type::Named {
                    name: receiver_type,
                    arguments: type_arguments,
                } = receiver_type
                    && type_arguments.is_empty()
                    && let Some(candidates) =
                        self.host_methods.get(&format!("{receiver_type}::{name}"))
                {
                    return self
                        .select_host_overload(candidates, arguments, Some(object), *span)
                        .map(|function| function.signature.return_type);
                }
            }
        }
        Ok(self
            .expression_types
            .get(&expression.span())
            .cloned()
            .unwrap_or(Type::Unknown))
    }

    fn symbol_id<T: Copy>(&self, symbols: &HashMap<String, T>, name: &str) -> Option<T> {
        let anchored = self.anchored_name(name);
        if anchored != name {
            return symbols.get(&anchored).copied();
        }
        if !self.namespace.is_empty() {
            let relative = format!("{}::{name}", self.namespace);
            if let Some(id) = symbols.get(&relative).copied() {
                return Some(id);
            }
        }
        symbols.get(name).copied()
    }

    fn anchored_name(&self, name: &str) -> String {
        let path = name.split("::").map(str::to_owned).collect::<Vec<_>>();
        let prefix = self
            .namespace
            .split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        resolve_anchored_path(&prefix, &path).unwrap_or_else(|| name.to_owned())
    }

    fn resolve_self_path(&self, path: &[String]) -> Vec<String> {
        if path.first().is_some_and(|segment| segment == "Self")
            && let Some(self_type) = &self.self_type
        {
            return self_type
                .split("::")
                .map(str::to_owned)
                .chain(path.iter().skip(1).cloned())
                .collect();
        }
        path.to_vec()
    }

    fn enum_variant_path(
        &self,
        path: &[String],
        span: Span,
    ) -> Result<(TypeId, String), CompileError> {
        if path.len() < 2 {
            return Err(CompileError::unsupported(
                "expected an enum variant path",
                span,
            ));
        }
        Ok((
            self.type_id(&path[..path.len() - 1].join("::"), span)?,
            path.last().unwrap().clone(),
        ))
    }
}

fn lower_literal(value: &Literal) -> HirLiteral {
    match value {
        Literal::Unit => HirLiteral::Unit,
        Literal::Bool(value) => HirLiteral::Bool(*value),
        Literal::I8(value) => HirLiteral::I8(*value),
        Literal::I16(value) => HirLiteral::I16(*value),
        Literal::I32(value) => HirLiteral::I32(*value),
        Literal::I64(value) => HirLiteral::I64(*value),
        Literal::I128(value) => HirLiteral::I128(*value),
        Literal::Isize(value) => HirLiteral::Isize(*value),
        Literal::U8(value) => HirLiteral::U8(*value),
        Literal::U16(value) => HirLiteral::U16(*value),
        Literal::U32(value) => HirLiteral::U32(*value),
        Literal::U64(value) => HirLiteral::U64(*value),
        Literal::U128(value) => HirLiteral::U128(*value),
        Literal::Usize(value) => HirLiteral::Usize(*value),
        Literal::F32(value) => HirLiteral::F32(*value),
        Literal::F64(value) => HirLiteral::F64(*value),
        Literal::Char(value) => HirLiteral::Char(*value),
        Literal::Integer(value) => HirLiteral::I32(
            i32::try_from(*value).expect("unresolved integer literal must fit the i32 default"),
        ),
        Literal::Float(value) => HirLiteral::F64(*value),
        Literal::String(value) => HirLiteral::String(value.clone()),
    }
}

fn builtin_default_hir(ty: &Type, span: Span) -> Result<Option<HirExpression>, CompileError> {
    use rils_frontend::default::DefaultPlan;

    let Some(plan) = rils_frontend::default::default_plan(ty) else {
        return Err(CompileError::unsupported(
            format!("type `{ty}` does not implement Default"),
            span,
        ));
    };
    fn lower(plan: &DefaultPlan, span: Span) -> Result<Option<HirExpression>, CompileError> {
        let literal = |value| HirExpression::Literal { value, span };
        Ok(Some(match plan {
            DefaultPlan::Unit => literal(HirLiteral::Unit),
            DefaultPlan::Bool => literal(HirLiteral::Bool(false)),
            DefaultPlan::Integer(crate::types::IntegerType::I8) => literal(HirLiteral::I8(0)),
            DefaultPlan::Integer(crate::types::IntegerType::I16) => literal(HirLiteral::I16(0)),
            DefaultPlan::Integer(crate::types::IntegerType::I32) => literal(HirLiteral::I32(0)),
            DefaultPlan::Integer(crate::types::IntegerType::I64) => literal(HirLiteral::I64(0)),
            DefaultPlan::Integer(crate::types::IntegerType::I128) => literal(HirLiteral::I128(0)),
            DefaultPlan::Integer(crate::types::IntegerType::Isize) => literal(HirLiteral::Isize(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U8) => literal(HirLiteral::U8(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U16) => literal(HirLiteral::U16(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U32) => literal(HirLiteral::U32(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U64) => literal(HirLiteral::U64(0)),
            DefaultPlan::Integer(crate::types::IntegerType::U128) => literal(HirLiteral::U128(0)),
            DefaultPlan::Integer(crate::types::IntegerType::Usize) => literal(HirLiteral::Usize(0)),
            DefaultPlan::Float(crate::types::FloatType::F32) => literal(HirLiteral::F32(0.0)),
            DefaultPlan::Float(crate::types::FloatType::F64) => literal(HirLiteral::F64(0.0)),
            DefaultPlan::Char => literal(HirLiteral::Char('\0')),
            DefaultPlan::String => literal(HirLiteral::String(String::new())),
            DefaultPlan::Tuple(elements) => HirExpression::Tuple {
                elements: elements
                    .iter()
                    .map(|element| {
                        lower(element, span)?.ok_or_else(|| {
                            CompileError::unsupported(
                                "nested type does not implement Default",
                                span,
                            )
                        })
                    })
                    .collect::<Result<_, _>>()?,
                span,
            },
            DefaultPlan::Array {
                element, length, ..
            } => HirExpression::Array {
                elements: (0..*length)
                    .map(|_| {
                        lower(element, span)?.ok_or_else(|| {
                            CompileError::unsupported(
                                "array element does not implement Default",
                                span,
                            )
                        })
                    })
                    .collect::<Result<_, _>>()?,
                repeat: None,
                span,
            },
            DefaultPlan::Option(_) => HirExpression::OptionNone { span },
            DefaultPlan::EmptyCollection { name, .. } => {
                let (name, signature) = collection_import_signature(&format!("{name}::new"))
                    .expect("default collection has a constructor import");
                HirExpression::CallImport {
                    name: name.into(),
                    signature,
                    capability: "core".into(),
                    arguments: Vec::new(),
                    span,
                }
            }
            DefaultPlan::TraitCall(_) => return Ok(None),
        }))
    }
    lower(&plan, span)
}

fn integer_constant_literal(
    target: crate::types::IntegerType,
    constant: rils_builtins::IntegerConstantId,
) -> HirLiteral {
    use crate::types::IntegerType::*;
    use rils_builtins::IntegerConstantId::*;
    if constant == Bits {
        return HirLiteral::U32(target.bits());
    }
    match (target, constant) {
        (I8, Min) => HirLiteral::I8(i8::MIN),
        (I8, Max) => HirLiteral::I8(i8::MAX),
        (I16, Min) => HirLiteral::I16(i16::MIN),
        (I16, Max) => HirLiteral::I16(i16::MAX),
        (I32, Min) => HirLiteral::I32(i32::MIN),
        (I32, Max) => HirLiteral::I32(i32::MAX),
        (I64, Min) => HirLiteral::I64(i64::MIN),
        (I64, Max) => HirLiteral::I64(i64::MAX),
        (I128, Min) => HirLiteral::I128(i128::MIN),
        (I128, Max) => HirLiteral::I128(i128::MAX),
        (Isize, Min) => HirLiteral::Isize(isize::MIN),
        (Isize, Max) => HirLiteral::Isize(isize::MAX),
        (U8, Min) => HirLiteral::U8(u8::MIN),
        (U8, Max) => HirLiteral::U8(u8::MAX),
        (U16, Min) => HirLiteral::U16(u16::MIN),
        (U16, Max) => HirLiteral::U16(u16::MAX),
        (U32, Min) => HirLiteral::U32(u32::MIN),
        (U32, Max) => HirLiteral::U32(u32::MAX),
        (U64, Min) => HirLiteral::U64(u64::MIN),
        (U64, Max) => HirLiteral::U64(u64::MAX),
        (U128, Min) => HirLiteral::U128(u128::MIN),
        (U128, Max) => HirLiteral::U128(u128::MAX),
        (Usize, Min) => HirLiteral::Usize(usize::MIN),
        (Usize, Max) => HirLiteral::Usize(usize::MAX),
        (_, Bits) => unreachable!(),
    }
}

fn float_constant_literal(
    target: crate::types::FloatType,
    constant: rils_builtins::FloatConstantId,
) -> HirLiteral {
    use crate::types::FloatType::*;
    use rils_builtins::FloatConstantId::*;
    match (target, constant) {
        (F32, Min) => HirLiteral::F32(f32::MIN),
        (F32, Max) => HirLiteral::F32(f32::MAX),
        (F32, Epsilon) => HirLiteral::F32(f32::EPSILON),
        (F32, MinPositive) => HirLiteral::F32(f32::MIN_POSITIVE),
        (F32, Nan) => HirLiteral::F32(f32::NAN),
        (F32, Infinity) => HirLiteral::F32(f32::INFINITY),
        (F32, NegInfinity) => HirLiteral::F32(f32::NEG_INFINITY),
        (F64, Min) => HirLiteral::F64(f64::MIN),
        (F64, Max) => HirLiteral::F64(f64::MAX),
        (F64, Epsilon) => HirLiteral::F64(f64::EPSILON),
        (F64, MinPositive) => HirLiteral::F64(f64::MIN_POSITIVE),
        (F64, Nan) => HirLiteral::F64(f64::NAN),
        (F64, Infinity) => HirLiteral::F64(f64::INFINITY),
        (F64, NegInfinity) => HirLiteral::F64(f64::NEG_INFINITY),
    }
}

fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Public { span, .. }
        | Stmt::Module { span, .. }
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
