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
    types::Type,
};

mod combinators;
mod imports;
mod ir;
mod iterator_defaults;
mod symbols;

use imports::*;
pub use ir::*;
use symbols::*;

fn collect_host_use_aliases(
    statements: &[Stmt],
    prefix: &mut Vec<String>,
    functions: &mut HashMap<String, HostFunctionDeclaration>,
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
                    let declaration = candidates
                        .iter()
                        .find_map(|candidate| functions.get(candidate))
                        .cloned();
                    if let Some(declaration) = declaration {
                        functions.insert(
                            qualified_name(prefix, import.binding_name().expect("single import")),
                            declaration,
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
    expression_types: &HashMap<Span, Type>,
    sources: Vec<SourceFile>,
) -> Result<HirProgram, CompileError> {
    ProgramLowerer::new(program, host, expression_types)?.lower(program, sources)
}

struct ProgramLowerer {
    functions: HashMap<String, FunctionId>,
    methods: HashMap<String, MethodInfo>,
    method_names: HashMap<String, Option<MethodInfo>>,
    types: HashMap<String, TypeId>,
    type_definitions: Vec<HirTypeDefinition>,
    host_functions: HashMap<String, HostFunctionDeclaration>,
    expression_types: HashMap<Span, Type>,
}

impl ProgramLowerer {
    fn new(
        program: &Program,
        host: &HostContract,
        expression_types: &HashMap<Span, Type>,
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
        let mut method_names = HashMap::new();
        let mut next_method_id = functions.values().copied().max().unwrap_or(0) + 1;
        collect_method_symbols(
            &program.statements,
            &mut Vec::new(),
            &mut next_method_id,
            &mut methods,
            &mut method_names,
        );
        let mut public_symbols = HashSet::new();
        collect_public_symbols(&program.statements, &mut Vec::new(), &mut public_symbols);
        collect_use_aliases(
            &program.statements,
            &mut Vec::new(),
            &mut functions,
            &mut types,
            &public_symbols,
        );
        let mut host_functions = host
            .functions()
            .map(|function| (function.name.clone(), function.clone()))
            .collect::<HashMap<_, _>>();
        collect_host_use_aliases(&program.statements, &mut Vec::new(), &mut host_functions);
        Ok(Self {
            functions,
            methods,
            method_names,
            types,
            type_definitions,
            host_functions,
            expression_types: expression_types.clone(),
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
                &self.functions,
                &self.methods,
                &self.method_names,
                &self.types,
                &self.host_functions,
                &self.expression_types,
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
                    &self.functions,
                    &self.methods,
                    &self.method_names,
                    &self.types,
                    &self.host_functions,
                    &self.expression_types,
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
    functions: &'a HashMap<String, FunctionId>,
    methods: &'a HashMap<String, MethodInfo>,
    method_names: &'a HashMap<String, Option<MethodInfo>>,
    types: &'a HashMap<String, TypeId>,
    host_functions: &'a HashMap<String, HostFunctionDeclaration>,
    expression_types: &'a HashMap<Span, Type>,
    namespace: String,
    scopes: Vec<HashMap<String, LocalId>>,
    mutable: Vec<bool>,
    in_function: bool,
    capture_count: usize,
    generated: GeneratedFunctions,
    captured: HashSet<LocalId>,
}

impl<'a> FunctionLowerer<'a> {
    fn new(
        functions: &'a HashMap<String, FunctionId>,
        methods: &'a HashMap<String, MethodInfo>,
        method_names: &'a HashMap<String, Option<MethodInfo>>,
        types: &'a HashMap<String, TypeId>,
        host_functions: &'a HashMap<String, HostFunctionDeclaration>,
        expression_types: &'a HashMap<Span, Type>,
        generated: GeneratedFunctions,
    ) -> Self {
        Self {
            functions,
            methods,
            method_names,
            types,
            host_functions,
            expression_types,
            namespace: String::new(),
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
                    self.functions,
                    self.methods,
                    self.method_names,
                    self.types,
                    self.host_functions,
                    self.expression_types,
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
                    qualified_name,
                    parameters,
                    body,
                    span: *span,
                    exported: false,
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
                } else if let Some(function) = self.function_id(name) {
                    Ok(HirExpression::Function {
                        function,
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
                if let Some(function) = self.function_id(&segments.join("::")) {
                    return Ok(HirExpression::Function {
                        function,
                        span: *span,
                    });
                }
                let (type_id, variant) = self.enum_variant_path(segments, *span)?;
                Ok(HirExpression::ConstructUnitVariant {
                    type_id,
                    variant,
                    span: *span,
                })
            }
            Expr::QualifiedPath {
                target,
                trait_name,
                member,
                span,
            } => {
                let target_name = nominal_type_name(target).ok_or_else(|| {
                    CompileError::unsupported("UFCS target must be a nominal type", *span)
                })?;
                let key = method_key(
                    &self.scoped_name(target_name),
                    Some(&self.scoped_name(trait_name)),
                    member,
                );
                let method = self.methods.get(&key).ok_or_else(|| {
                    CompileError::unsupported(format!("unknown trait method `{key}`"), *span)
                })?;
                Ok(HirExpression::Function {
                    function: method.function,
                    span: *span,
                })
            }
            Expr::Member { object, name, span }
                if self
                    .method_names
                    .get(name)
                    .and_then(|method| *method)
                    .is_some() =>
            {
                let method = self.method_names[name].expect("guarded method");
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
                if let Expr::QualifiedPath {
                    target,
                    trait_name,
                    member,
                    ..
                } = callee.as_ref()
                {
                    let target_name = nominal_type_name(target).ok_or_else(|| {
                        CompileError::unsupported("UFCS target must be a nominal type", *span)
                    })?;
                    let target_name = self.scoped_name(target_name);
                    let trait_name = self.scoped_name(trait_name);
                    let key = method_key(&target_name, Some(&trait_name), member);
                    let method = self.methods.get(&key).copied().ok_or_else(|| {
                        CompileError::unsupported(format!("unknown trait method `{key}`"), *span)
                    })?;
                    return Ok(HirExpression::Call {
                        function: method.function,
                        arguments: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                if let Expr::Path { segments, .. } = callee.as_ref() {
                    let raw_key = segments.join("::");
                    if let [type_name, member] = segments.as_slice()
                        && let Some(target) = crate::types::IntegerType::from_name(type_name)
                        && let Some(intrinsic) = rils_builtins::integer_associated_function(member)
                    {
                        return Ok(HirExpression::CallIntrinsic {
                            intrinsic: intrinsic.id,
                            target: Some(target),
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    let key = self.anchored_name(&raw_key);
                    if let Some((name, signature)) = collection_import_signature(&key) {
                        return Ok(HirExpression::CallImport {
                            name: name.into(),
                            signature,
                            capability: "core".into(),
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    if let Some(signature) =
                        rils_frontend::standard_library::standard_function_signature(&key)
                    {
                        let capability = if key.starts_with("std::fs::") {
                            "std::fs"
                        } else {
                            "std::io"
                        };
                        return Ok(HirExpression::CallImport {
                            name: key,
                            signature,
                            capability: capability.into(),
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    if let Some(function) = self.host_function(&key).cloned() {
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
                    if let Some(function) = self.function_id(&key) {
                        return Ok(HirExpression::Call {
                            function,
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    if let Some(method) = self.symbol_id(self.methods, &key) {
                        return Ok(HirExpression::Call {
                            function: method.function,
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    let (type_id, variant) = self.enum_variant_path(segments, *span)?;
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
                    let intrinsic = self.expression_types.get(&object.span()).and_then(
                        |receiver| match receiver {
                            Type::Integer(_) | Type::IntegerVariable(_) => {
                                rils_builtins::integer_method(name)
                            }
                            Type::Float(_) | Type::FloatVariable(_) => {
                                rils_builtins::float_method(name)
                            }
                            _ => None,
                        },
                    );
                    if self.method_names.get(name).is_none()
                        && let Some(intrinsic) = intrinsic
                    {
                        let mut lowered = Vec::with_capacity(arguments.len() + 1);
                        lowered.push(self.expression(object)?);
                        lowered.extend(
                            arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<Vec<_>, _>>()?,
                        );
                        return Ok(HirExpression::CallIntrinsic {
                            intrinsic: intrinsic.id,
                            target: None,
                            arguments: lowered,
                            span: *span,
                        });
                    }
                    if self.method_names.get(name).is_none() {
                        let owner = self
                            .expression_types
                            .get(&object.span())
                            .and_then(rils_frontend::standard_library::builtin_owner_name);
                        if name == "into_iter"
                            && arguments.is_empty()
                            && matches!(owner, Some("Array" | "Vec" | "Range" | "Iterator"))
                        {
                            return Ok(HirExpression::IntoIterator {
                                value: Box::new(self.expression(object)?),
                                span: *span,
                            });
                        }
                        if let Some(expression) =
                            self.builtin_combinator(owner, name, object, arguments, *span)?
                        {
                            return Ok(expression);
                        }
                        if let Some((import_name, signature, receiver)) =
                            builtin_method_import(owner, name)
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
                            return Ok(HirExpression::CallImport {
                                name: import_name.into(),
                                signature,
                                capability: "core".into(),
                                arguments: lowered,
                                span: *span,
                            });
                        }
                    }
                    if let Some(host) = self.host_method(name).cloned() {
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
                    let method = self
                        .method_names
                        .get(name)
                        .and_then(|value| *value)
                        .ok_or_else(|| {
                            CompileError::unsupported(
                                format!("bytecode method `{name}` is ambiguous or unavailable"),
                                *span,
                            )
                        })?;
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
                if let Expr::Variable { name, .. } = callee.as_ref()
                    && self.lookup(name).is_none()
                    && let Some(function) = self.host_function(name).cloned()
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
                if let Expr::Variable { name, .. } = callee.as_ref()
                    && self.lookup(name).is_none()
                    && let Some(function) = self.function_id(name)
                {
                    return Ok(HirExpression::Call {
                        function,
                        arguments: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                if let Expr::Variable { name, .. } = callee.as_ref()
                    && self.lookup(name).is_none()
                    && let Some((import_name, signature, capability)) = native_macro_import(name)
                {
                    return Ok(HirExpression::CallImport {
                        name: import_name.into(),
                        signature,
                        capability: capability.into(),
                        arguments: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                if let Expr::Variable { name, .. } = callee.as_ref()
                    && self.lookup(name).is_none()
                    && let Some(signature) = core_import_signature(name)
                {
                    return Ok(HirExpression::CallImport {
                        name: name.clone(),
                        signature,
                        capability: "core".into(),
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
                        .map(|(name, value)| Ok((name.clone(), self.expression(value)?)))
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

    fn function_id(&self, name: &str) -> Option<FunctionId> {
        self.symbol_id(self.functions, name)
    }

    fn host_function(&self, name: &str) -> Option<&HostFunctionDeclaration> {
        let name = self.anchored_name(name);
        if !self.namespace.is_empty() {
            let relative = format!("{}::{name}", self.namespace);
            if let Some(function) = self.host_functions.get(&relative) {
                return Some(function);
            }
        }
        self.host_functions.get(&name)
    }

    fn host_method(&self, name: &str) -> Option<&HostFunctionDeclaration> {
        let mut matches = self.host_functions.values().filter(|function| {
            function.receiver.is_some()
                && function
                    .name
                    .rsplit_once("::")
                    .is_some_and(|(_, method)| method == name)
        });
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    fn scoped_name(&self, name: &str) -> String {
        let anchored = self.anchored_name(name);
        if anchored != name {
            return anchored;
        }
        if self.namespace.is_empty() || name.contains("::") {
            name.to_string()
        } else {
            format!("{}::{name}", self.namespace)
        }
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
