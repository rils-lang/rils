use super::*;

impl<'a> FunctionLowerer<'a> {
    #[allow(
        clippy::too_many_arguments,
        reason = "the lowerer borrows one immutable table per compiler identity domain"
    )]
    pub(super) fn new(
        types: &'a HashMap<String, TypeId>,
        type_definitions: &'a [HirTypeDefinition],
        host_functions: &'a HashMap<String, Vec<HostFunctionDeclaration>>,
        host_methods: &'a HashMap<String, Vec<HostFunctionDeclaration>>,
        host_contract: &'a HostContract,
        expression_ids: &'a rils_frontend::semantic::ExpressionIdentityMap,
        typeck_results: &'a rils_frontend::semantic::TypeckResults,
        resolved_definitions: &'a HashMap<rils_frontend::DefId, MethodInfo>,
        generated: GeneratedFunctions,
    ) -> Self {
        Self {
            types,
            type_definitions,
            host_functions,
            host_methods,
            host_contract,
            expression_ids,
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

    pub(super) fn lower_entry(mut self, statements: &[&Stmt]) -> Result<HirFunction, CompileError> {
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

    pub(super) fn lower_function(
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

    pub(super) fn statements(
        &mut self,
        statements: &[Stmt],
    ) -> Result<Vec<HirStatement>, CompileError> {
        statements
            .iter()
            .map(|statement| self.statement(statement))
            .collect()
    }

    pub(super) fn statement(&mut self, statement: &Stmt) -> Result<HirStatement, CompileError> {
        match statement {
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
                    self.type_definitions,
                    self.host_functions,
                    self.host_methods,
                    self.host_contract,
                    self.expression_ids,
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
}
