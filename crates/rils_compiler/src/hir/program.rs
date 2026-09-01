use super::*;

pub(crate) fn lower_with_host(
    program: &Program,
    host: &HostContract,
    analysis: &rils_frontend::analysis::DocumentAnalysis,
    sources: Vec<SourceFile>,
    entry: Option<rils_frontend::DefId>,
) -> Result<HirProgram, CompileError> {
    let units = [ProgramUnit {
        module_path: Vec::new(),
        program,
        source: SourceId::UNKNOWN,
    }];
    ProgramLowerer::new(&units, host, analysis)?.lower(&units, sources, entry)
}

pub(crate) fn lower_project_with_host(
    syntax: &rils_frontend::ProjectSyntax,
    modules: &rils_frontend::ModuleGraph,
    host: &HostContract,
    analysis: &rils_frontend::analysis::DocumentAnalysis,
    sources: Vec<SourceFile>,
    entry: Option<rils_frontend::DefId>,
) -> Result<HirProgram, CompileError> {
    let root = syntax.root_program();
    let mut units = Vec::with_capacity(syntax.modules().len() + 1);
    if !root.statements.is_empty() {
        units.push(ProgramUnit {
            module_path: Vec::new(),
            program: &root,
            source: SourceId::UNKNOWN,
        });
    }
    units.extend(syntax.modules().filter_map(|(id, program)| {
        let module = modules.module(id)?;
        Some(ProgramUnit {
            module_path: module_path_segments(&module.path),
            program,
            source: module.source.unwrap_or(SourceId::UNKNOWN),
        })
    }));
    ProgramLowerer::new(&units, host, analysis)?.lower(&units, sources, entry)
}

struct ProgramUnit<'a> {
    module_path: Vec<String>,
    program: &'a Program,
    source: SourceId,
}

fn module_path_segments(path: &str) -> Vec<String> {
    path.split("::")
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

struct ProgramLowerer {
    functions: HashMap<String, FunctionId>,
    methods: HashMap<String, MethodInfo>,
    types: HashMap<String, TypeId>,
    type_definitions: Vec<HirTypeDefinition>,
    host_functions: HashMap<String, Vec<HostFunctionDeclaration>>,
    host_methods: HashMap<String, Vec<HostFunctionDeclaration>>,
    host_contract: HostContract,
    expression_ids: rils_frontend::semantic::ExpressionIdentityMap,
    typeck_results: rils_frontend::semantic::TypeckResults,
    resolved_definitions: HashMap<rils_frontend::DefId, MethodInfo>,
}

impl ProgramLowerer {
    fn new(
        units: &[ProgramUnit<'_>],
        host: &HostContract,
        analysis: &rils_frontend::analysis::DocumentAnalysis,
    ) -> Result<Self, CompileError> {
        let mut functions = HashMap::new();
        let mut types = HashMap::new();
        let mut type_definitions = Vec::new();
        for declaration in host.types() {
            let Some(host_enum) = declaration.enum_definition.as_ref() else {
                continue;
            };
            let id = type_definitions.len();
            types.insert(declaration.name.clone(), id);
            if let Some(short_name) = declaration.name.rsplit("::").next() {
                types.entry(short_name.to_owned()).or_insert(id);
            }
            type_definitions.push(HirTypeDefinition::Enum {
                name: declaration.name.clone(),
                generic_parameters: Vec::new(),
                variants: host_enum
                    .variants
                    .keys()
                    .cloned()
                    .map(|name| EnumVariant::Unit {
                        name,
                        span: Span::default(),
                    })
                    .collect(),
            });
        }
        for unit in units {
            for statement in &unit.program.statements {
                if let Some(declaration) = function_declaration(statement) {
                    let qualified = qualified_name(&unit.module_path, declaration.name);
                    let id = functions.values().copied().max().unwrap_or(0) + 1;
                    if functions.insert(qualified.clone(), id).is_some() {
                        return Err(CompileError::unsupported(
                            format!("duplicate function `{qualified}`"),
                            declaration.span,
                        ));
                    }
                    functions.entry(declaration.name.to_string()).or_insert(id);
                }
                let definition = match statement {
                    Stmt::Struct {
                        name,
                        generic_parameters,
                        fields,
                        ..
                    } => Some(HirTypeDefinition::Struct {
                        name: qualified_name(&unit.module_path, name),
                        generic_parameters: generic_parameters.clone(),
                        fields: fields.clone(),
                    }),
                    Stmt::Enum {
                        name,
                        generic_parameters,
                        variants,
                        ..
                    } => Some(HirTypeDefinition::Enum {
                        name: qualified_name(&unit.module_path, name),
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
                    let id = type_definitions.len();
                    types.insert(name, id);
                    if let Stmt::Struct { name, .. } | Stmt::Enum { name, .. } = statement {
                        types.entry(name.clone()).or_insert(id);
                    }
                    type_definitions.push(definition);
                }
            }
            collect_nested_symbols(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                &mut functions,
                &mut types,
                &mut type_definitions,
            )?;
        }

        let mut methods = HashMap::new();
        let mut next_method_id = functions.values().copied().max().unwrap_or(0) + 1;
        for unit in units {
            collect_method_symbols(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                &mut next_method_id,
                &mut methods,
            );
        }
        let mut declarations = Vec::new();
        for unit in units {
            declarations.extend(unit.program.statements.iter().filter_map(|statement| {
                let mut declaration = function_declaration(statement)?;
                declaration.qualified_name = qualified_name(&unit.module_path, declaration.name);
                Some((functions[&declaration.qualified_name], declaration))
            }));
            collect_nested_function_declarations(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                &functions,
                &mut declarations,
            );
            collect_method_declarations(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                &methods,
                &mut declarations,
            );
        }
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
        for unit in units {
            collect_public_symbols(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                &mut public_symbols,
            );
        }
        for unit in units {
            collect_use_aliases(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                &mut functions,
                &mut types,
                &public_symbols,
            );
        }
        let mut host_functions = host.function_overloads();
        for unit in units {
            collect_host_use_aliases(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                &mut host_functions,
            );
        }
        let host_methods = host.method_function_overloads();
        let mut expression_ids = rils_frontend::semantic::ExpressionIdentityMap::default();
        for unit in units {
            expression_ids.extend(rils_frontend::semantic::ExpressionIdentityMap::allocate(
                unit.program,
                unit.source,
            ));
        }
        Ok(Self {
            functions,
            methods,
            types,
            type_definitions,
            host_functions,
            host_methods,
            host_contract: host.clone(),
            expression_ids,
            typeck_results: analysis.typeck_results.clone(),
            resolved_definitions,
        })
    }

    fn lower(
        self,
        units: &[ProgramUnit<'_>],
        sources: Vec<SourceFile>,
        entry: Option<rils_frontend::DefId>,
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
        let entry_statements = units
            .iter()
            .filter(|unit| unit.module_path.is_empty())
            .flat_map(|unit| &unit.program.statements)
            .filter(|statement| !is_compile_time_declaration(statement))
            .collect::<Vec<_>>();
        let mut entry_function = FunctionLowerer::new(
            &self.types,
            &self.type_definitions,
            &self.host_functions,
            &self.host_methods,
            &self.host_contract,
            &self.expression_ids,
            &self.typeck_results,
            &self.resolved_definitions,
            generated.clone(),
        )
        .lower_entry(&entry_statements)?;
        if let Some(entry) = entry {
            let function = self
                .resolved_definitions
                .get(&entry)
                .ok_or_else(|| {
                    CompileError::new(
                        "project entry has no lowered function identity",
                        Span::default(),
                    )
                })?
                .function;
            entry_function.statements.push(HirStatement::Expression {
                expression: HirExpression::Call {
                    function,
                    arguments: Vec::new(),
                    span: Span::default(),
                },
                terminated: false,
                span: Span::default(),
            });
        }
        lowered.push(entry_function);

        let mut declarations = Vec::new();
        for unit in units {
            declarations.extend(unit.program.statements.iter().filter_map(|statement| {
                let mut declaration = function_declaration(statement)?;
                declaration.qualified_name = qualified_name(&unit.module_path, declaration.name);
                Some((self.functions[&declaration.qualified_name], declaration))
            }));
            collect_nested_function_declarations(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                &self.functions,
                &mut declarations,
            );
            collect_method_declarations(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                &self.methods,
                &mut declarations,
            );
        }
        declarations.sort_by_key(|(id, _)| *id);
        for (_, declaration) in declarations {
            lowered.push(
                FunctionLowerer::new(
                    &self.types,
                    &self.type_definitions,
                    &self.host_functions,
                    &self.host_methods,
                    &self.host_contract,
                    &self.expression_ids,
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
