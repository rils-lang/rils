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
        // Box is a compiler-provided heap indirection. Keep its nominal
        // declaration in the HIR even when the standard-library source is not
        // part of a standalone compilation unit.
        let box_id = type_definitions.len();
        types.insert("Box".to_owned(), box_id);
        type_definitions.push(HirTypeDefinition::Struct {
            name: "Box".to_owned(),
            opaque_native: rils_builtins::builtin("Box").is_some_and(|item| item.opaque_native),
            generic_parameters: vec![rils_frontend::ast::GenericParameter {
                is_const: false,
                name: "T".to_owned(),
                bounds: Vec::new(),
                span: Span::default(),
            }],
            fields: Vec::new(),
        });
        let rc_id = type_definitions.len();
        types.insert("Rc".to_owned(), rc_id);
        type_definitions.push(HirTypeDefinition::Struct {
            name: "Rc".to_owned(),
            opaque_native: rils_builtins::builtin("Rc").is_some_and(|item| item.opaque_native),
            generic_parameters: vec![rils_frontend::ast::GenericParameter {
                is_const: false,
                name: "T".to_owned(),
                bounds: Vec::new(),
                span: Span::default(),
            }],
            fields: Vec::new(),
        });
        let cell_id = type_definitions.len();
        types.insert("Cell".to_owned(), cell_id);
        type_definitions.push(HirTypeDefinition::Struct {
            name: "Cell".to_owned(),
            opaque_native: rils_builtins::builtin("Cell").is_some_and(|item| item.opaque_native),
            generic_parameters: vec![rils_frontend::ast::GenericParameter {
                is_const: false,
                name: "T".to_owned(),
                bounds: Vec::new(),
                span: Span::default(),
            }],
            fields: Vec::new(),
        });
        let ref_cell_id = type_definitions.len();
        types.insert("RefCell".to_owned(), ref_cell_id);
        type_definitions.push(HirTypeDefinition::Struct {
            name: "RefCell".to_owned(),
            opaque_native: rils_builtins::builtin("RefCell").is_some_and(|item| item.opaque_native),
            generic_parameters: vec![rils_frontend::ast::GenericParameter {
                is_const: false,
                name: "T".to_owned(),
                bounds: Vec::new(),
                span: Span::default(),
            }],
            fields: Vec::new(),
        });
        let deque_id = type_definitions.len();
        types.insert("VecDeque".to_owned(), deque_id);
        type_definitions.push(HirTypeDefinition::Struct {
            name: "VecDeque".to_owned(),
            opaque_native: rils_builtins::builtin("VecDeque")
                .is_some_and(|item| item.opaque_native),
            generic_parameters: vec![rils_frontend::ast::GenericParameter {
                is_const: false,
                name: "T".to_owned(),
                bounds: Vec::new(),
                span: Span::default(),
            }],
            fields: Vec::new(),
        });
        let heap_id = type_definitions.len();
        types.insert("BinaryHeap".to_owned(), heap_id);
        type_definitions.push(HirTypeDefinition::Struct {
            name: "BinaryHeap".to_owned(),
            opaque_native: rils_builtins::builtin("BinaryHeap")
                .is_some_and(|item| item.opaque_native),
            generic_parameters: vec![rils_frontend::ast::GenericParameter {
                is_const: false,
                name: "T".to_owned(),
                bounds: Vec::new(),
                span: Span::default(),
            }],
            fields: Vec::new(),
        });
        let tree_map_id = type_definitions.len();
        types.insert("BTreeMap".to_owned(), tree_map_id);
        type_definitions.push(HirTypeDefinition::Struct {
            name: "BTreeMap".to_owned(),
            opaque_native: rils_builtins::builtin("BTreeMap")
                .is_some_and(|item| item.opaque_native),
            generic_parameters: ["K", "V"]
                .into_iter()
                .map(|name| rils_frontend::ast::GenericParameter {
                    is_const: false,
                    name: name.to_owned(),
                    bounds: Vec::new(),
                    span: Span::default(),
                })
                .collect(),
            fields: Vec::new(),
        });
        let tree_set_id = type_definitions.len();
        types.insert("BTreeSet".to_owned(), tree_set_id);
        type_definitions.push(HirTypeDefinition::Struct {
            name: "BTreeSet".to_owned(),
            opaque_native: rils_builtins::builtin("BTreeSet")
                .is_some_and(|item| item.opaque_native),
            generic_parameters: vec![rils_frontend::ast::GenericParameter {
                is_const: false,
                name: "T".to_owned(),
                bounds: Vec::new(),
                span: Span::default(),
            }],
            fields: Vec::new(),
        });
        let weak_id = type_definitions.len();
        types.insert("Weak".to_owned(), weak_id);
        type_definitions.push(HirTypeDefinition::Struct {
            name: "Weak".to_owned(),
            opaque_native: rils_builtins::builtin("Weak").is_some_and(|item| item.opaque_native),
            generic_parameters: vec![rils_frontend::ast::GenericParameter {
                is_const: false,
                name: "T".to_owned(),
                bounds: Vec::new(),
                span: Span::default(),
            }],
            fields: Vec::new(),
        });
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
                        attributes,
                        generic_parameters,
                        fields,
                        ..
                    } => Some(HirTypeDefinition::Struct {
                        name: qualified_name(&unit.module_path, name),
                        opaque_native: rils_frontend::ast::has_compiler_internal_attribute(
                            attributes,
                        ),
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
        for unit in units {
            reject_unchecked_callable_bounds(&unit.program.statements)?;
        }
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
            &self.functions,
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
                    &self.functions,
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
        let mut trait_implementations = trait_implementations(&self.methods);
        for unit in units {
            collect_marker_trait_implementations(
                &unit.program.statements,
                &mut unit.module_path.clone(),
                unit.source,
                &mut trait_implementations,
            );
        }
        Ok(HirProgram {
            sources,
            functions: lowered,
            types: self.type_definitions,
            iterators: iterator_methods(&self.methods),
            trait_implementations,
            entry: 0,
        })
    }
}

fn reject_unchecked_callable_bounds(statements: &[Stmt]) -> Result<(), CompileError> {
    for statement in statements {
        match statement {
            Stmt::Function {
                generic_parameters,
                body,
                ..
            } => {
                reject_callable_parameters(generic_parameters)?;
                reject_unchecked_callable_bounds(&body.statements)?;
            }
            Stmt::Impl {
                generic_parameters,
                methods,
                ..
            } => {
                reject_callable_parameters(generic_parameters)?;
                for method in methods {
                    reject_callable_parameters(&method.generic_parameters)?;
                    reject_unchecked_callable_bounds(&method.body.statements)?;
                }
            }
            Stmt::Struct {
                generic_parameters, ..
            }
            | Stmt::Enum {
                generic_parameters, ..
            }
            | Stmt::TypeAlias {
                generic_parameters, ..
            } => {
                reject_callable_parameters(generic_parameters)?;
            }
            Stmt::Module {
                statements: Some(children),
                ..
            } => {
                reject_unchecked_callable_bounds(children)?;
            }
            Stmt::While { body, .. } | Stmt::Loop { body, .. } | Stmt::For { body, .. } => {
                reject_unchecked_callable_bounds(&body.statements)?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn reject_callable_parameters(
    parameters: &[rils_frontend::ast::GenericParameter],
) -> Result<(), CompileError> {
    for parameter in parameters {
        if parameter.bounds.iter().any(|bound| {
            matches!(bound, Type::Named { name, .. } if rils_stdlib::stdlib::ops::callable_trait_kind(name).is_some())
        }) {
            return Err(CompileError::unsupported(
                "callable trait bounds are not yet checked by the bytecode backend",
                parameter.span,
            ));
        }
    }
    Ok(())
}
