use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[path = "interpreter/builtin_methods/iterator.rs"]
mod builtin_iterator;
mod builtin_methods;
#[path = "interpreter/builtin_methods/option_result.rs"]
mod builtin_option_result;
mod builtins;
mod call;
mod construction;
mod evaluation;
mod execution;
mod formatting;
mod operators;
mod pattern;
mod place;
mod traits;
mod type_check;

use builtins::*;
use pattern::*;
use traits::*;
use type_check::*;

use crate::{
    ast::{
        AssociatedType, BinaryOp, Block, EnumVariant, Expr, GenericParameter, Literal, LogicalOp,
        NamedField, Parameter, Pattern, Program, Stmt, TraitMethod, UnaryOp,
    },
    environment::{AccessError, AssignError, Environment, EnvironmentRef},
    source::Span,
    types::{FunctionSignature, Type, merge_types},
    value::{
        BoundMethod, BuiltinBoundMethod, BuiltinFunction, BuiltinMethod, BuiltinType, EnumInstance,
        EnumPayload, EnumType, FieldSlot, HashMapValue, HashSetValue, HostBoundMethod,
        HostFunction, HostFunctionHandler, HostObject, HostType, ModuleValue, NativeFunction,
        RangeValue, ReferenceValue, SequenceIteratorValue, SequenceValue, StructInstance,
        StructType, TraitMethodSelector, TraitType, TypeAliasType, UserFunction, Value,
        VariantConstructor, enum_variant_name,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeError {
    pub message: String,
    pub span: Span,
    pub stack: Vec<String>,
}

impl RuntimeError {
    fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            stack: Vec::new(),
        }
    }
}

#[derive(Clone)]
enum Flow {
    Value(Value),
    Return(Value),
    Break(Value),
    Continue,
}

const TRY_RETURN_SIGNAL: &str = "#rils_try_return";

pub struct Interpreter {
    globals: EnvironmentRef,
    steps: usize,
    limits: crate::ExecutionLimits,
    function_depth: usize,
    pending_return: Option<Value>,
    pending_loop_flow: Option<Flow>,
    output_handler: Rc<crate::OutputHandler>,
    host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
    semantic_expression_ids: Option<rils_frontend::semantic::ExpressionIdentityMap>,
    typeck_results: Option<rils_frontend::TypeckResults>,
    frontend_semantics_verified: bool,
    frontend_verified_trait_impls: HashSet<rils_frontend::ImplId>,
    frontend_impl_ids: HashMap<Span, rils_frontend::ImplId>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        let globals = Environment::global();
        install_builtins(&globals);
        Self {
            globals,
            steps: 0,
            limits: crate::ExecutionLimits::default(),
            function_depth: 0,
            pending_return: None,
            pending_loop_flow: None,
            output_handler: crate::output::default_output_handler(),
            host_value_formatter: None,
            semantic_expression_ids: None,
            typeck_results: None,
            frontend_semantics_verified: false,
            frontend_verified_trait_impls: HashSet::new(),
            frontend_impl_ids: HashMap::new(),
        }
    }

    pub fn set_max_steps(&mut self, max_steps: usize) {
        self.limits.max_steps = max_steps;
    }

    pub fn set_max_call_depth(&mut self, max_call_depth: usize) {
        self.limits.max_call_depth = max_call_depth;
    }

    pub fn set_execution_limits(&mut self, limits: crate::ExecutionLimits) {
        self.limits = limits;
    }

    pub(crate) fn set_output_handler(&mut self, handler: Rc<crate::OutputHandler>) {
        self.output_handler = handler;
    }

    pub(crate) fn set_host_value_formatter(
        &mut self,
        formatter: Option<Rc<crate::HostValueFormatter>>,
    ) {
        self.host_value_formatter = formatter;
    }

    pub(crate) fn register_native_function(
        &mut self,
        binding_name: &'static str,
        name: &'static str,
        min_arity: usize,
        max_arity: usize,
        function: fn(&[Value]) -> Result<Value, String>,
    ) -> Result<(), String> {
        if self.globals.borrow().get(binding_name).is_some() {
            return Err(format!(
                "native function `{binding_name}` is already registered"
            ));
        }
        self.globals.borrow_mut().define(
            binding_name,
            Value::NativeFunction(NativeFunction {
                binding_name,
                name,
                min_arity,
                max_arity,
                signature: None,
                function,
            }),
            false,
            None,
        );
        Ok(())
    }

    pub(crate) fn register_host_module(&mut self, path: &[String]) -> Result<(), String> {
        self.host_module_environment(path).map(|_| ())
    }

    pub(crate) fn register_host_function(
        &mut self,
        module_path: &[String],
        name: String,
        min_arity: usize,
        max_arity: usize,
        signature: Option<FunctionSignature>,
        function: Rc<HostFunctionHandler>,
    ) -> Result<(), String> {
        if min_arity > max_arity {
            return Err("host function minimum arity cannot exceed maximum arity".into());
        }
        let environment = self.host_module_environment(module_path)?;
        if environment.borrow().get(&name).is_some() {
            return Err(format!("name `{name}` is already registered"));
        }
        environment.borrow_mut().define(
            name.clone(),
            Value::HostFunction(Rc::new(HostFunction {
                name: name.clone(),
                min_arity,
                max_arity,
                signature,
                function,
            })),
            false,
            None,
        );
        if let Some(module) = self.host_module(module_path)? {
            module.public.borrow_mut().insert(name);
        }
        Ok(())
    }

    pub(crate) fn register_host_type(
        &mut self,
        module_path: &[String],
        name: String,
    ) -> Result<Rc<HostType>, String> {
        let environment = self.host_module_environment(module_path)?;
        if environment.borrow().get(&name).is_some() {
            return Err(format!("name `{name}` is already registered"));
        }
        let definition = Rc::new(HostType {
            name: name.clone(),
            base_types: Default::default(),
            copy: false,
            methods: RefCell::new(Default::default()),
        });
        environment.borrow_mut().define(
            name.clone(),
            Value::HostType(definition.clone()),
            false,
            None,
        );
        if let Some(module) = self.host_module(module_path)? {
            module.public.borrow_mut().insert(name);
        }
        Ok(definition)
    }

    fn host_module_environment(&mut self, path: &[String]) -> Result<EnvironmentRef, String> {
        let mut environment = self.globals.clone();
        let mut parent_module: Option<Rc<ModuleValue>> = None;
        for segment in path {
            let existing = environment.borrow().get(segment);
            let module = match existing {
                Some(Value::Module(module)) => module,
                Some(value) => {
                    return Err(format!(
                        "cannot register module `{segment}` over {}",
                        value.type_name()
                    ));
                }
                None => {
                    let members = Environment::module_child(environment.clone());
                    let module = Rc::new(ModuleValue {
                        name: segment.clone(),
                        members,
                        public: RefCell::new(Default::default()),
                    });
                    environment.borrow_mut().define(
                        segment.clone(),
                        Value::Module(module.clone()),
                        false,
                        None,
                    );
                    if let Some(parent) = &parent_module {
                        parent.public.borrow_mut().insert(segment.clone());
                    }
                    module
                }
            };
            environment = module.members.clone();
            parent_module = Some(module);
        }
        Ok(environment)
    }

    fn host_module(&self, path: &[String]) -> Result<Option<Rc<ModuleValue>>, String> {
        let mut environment = self.globals.clone();
        let mut current = None;
        for segment in path {
            let Some(Value::Module(module)) = environment.borrow().get(segment) else {
                return Err(format!("module `{segment}` is not registered"));
            };
            environment = module.members.clone();
            current = Some(module);
        }
        Ok(current)
    }

    pub(crate) fn execute_with_analysis(
        &mut self,
        program: &Program,
        analysis: &rils_frontend::analysis::DocumentAnalysis,
    ) -> Result<Value, RuntimeError> {
        self.semantic_expression_ids =
            Some(rils_frontend::semantic::ExpressionIdentityMap::allocate(
                program,
                crate::SourceId::UNKNOWN,
            ));
        self.typeck_results = Some(analysis.typeck_results.clone());
        let result = self.execute_inner(program);
        self.semantic_expression_ids = None;
        self.typeck_results = None;
        result
    }

    pub(crate) fn execute_project_with_analysis(
        &mut self,
        syntax: &rils_frontend::ProjectSyntax,
        graph: &rils_frontend::ModuleGraph,
        analysis: &rils_frontend::analysis::DocumentAnalysis,
        entry: rils_frontend::DefId,
    ) -> Result<Value, RuntimeError> {
        let mut expression_ids = rils_frontend::semantic::ExpressionIdentityMap::default();
        for program in syntax.roots() {
            expression_ids.extend(rils_frontend::semantic::ExpressionIdentityMap::allocate(
                program,
                crate::SourceId::UNKNOWN,
            ));
        }
        for (module, program) in syntax.modules() {
            let source = graph
                .module(module)
                .and_then(|module| module.source)
                .unwrap_or(crate::SourceId::UNKNOWN);
            expression_ids.extend(rils_frontend::semantic::ExpressionIdentityMap::allocate(
                program, source,
            ));
        }
        self.semantic_expression_ids = Some(expression_ids);
        self.typeck_results = Some(analysis.typeck_results.clone());
        let previous_semantics_verified =
            std::mem::replace(&mut self.frontend_semantics_verified, true);
        let previous_verified_trait_impls = std::mem::replace(
            &mut self.frontend_verified_trait_impls,
            analysis.verified_trait_impls.iter().copied().collect(),
        );
        let previous_impl_ids = std::mem::replace(
            &mut self.frontend_impl_ids,
            analysis.def_map.impls().collect(),
        );

        let result = (|| {
            self.steps = 0;
            self.pending_return = None;
            self.pending_loop_flow = None;
            let mut environments = HashMap::new();
            for module in graph.modules() {
                if module.path.is_empty() {
                    environments.insert(module.id, self.globals.clone());
                    continue;
                }
                let parent = module
                    .parent
                    .and_then(|parent| environments.get(&parent))
                    .cloned()
                    .ok_or_else(|| {
                        RuntimeError::new(
                            format!("module `{}` has no initialized parent", module.path),
                            Span::default(),
                        )
                    })?;
                let name = module
                    .path
                    .rsplit("::")
                    .next()
                    .expect("non-root module has a name")
                    .to_owned();
                let members = Environment::module_child(parent.clone());
                parent.borrow_mut().define(
                    name.clone(),
                    Value::Module(Rc::new(ModuleValue {
                        name,
                        members: members.clone(),
                        public: RefCell::new(HashSet::new()),
                    })),
                    false,
                    None,
                );
                environments.insert(module.id, members);
            }

            for program in syntax.roots() {
                self.execute_statements(&program.statements, self.globals.clone())?;
            }
            for (module, program) in syntax.modules() {
                let environment = environments.get(&module).cloned().ok_or_else(|| {
                    RuntimeError::new("project module has no runtime environment", Span::default())
                })?;
                self.execute_statements(&program.statements, environment.clone())?;
                let mut public = HashSet::new();
                for statement in &program.statements {
                    public.extend(execution::public_names(statement, &environment)?);
                }
                let module_data = graph.module(module).expect("syntax module is in graph");
                let parent = module_data
                    .parent
                    .and_then(|parent| environments.get(&parent))
                    .expect("module parent environment exists");
                let name = module_data
                    .path
                    .rsplit("::")
                    .next()
                    .expect("module has a name");
                if let Some(Value::Module(module)) = parent.borrow().get(name) {
                    *module.public.borrow_mut() = public;
                }
            }

            let definition = analysis.def_map.definition(entry).ok_or_else(|| {
                RuntimeError::new("project entry definition is missing", Span::default())
            })?;
            let module_path = match definition.container.as_ref() {
                Some(rils_frontend::SymbolContainer::Module(path)) => path.as_str(),
                _ => "",
            };
            let environment = graph
                .module_by_path(module_path)
                .and_then(|module| environments.get(&module.id))
                .cloned()
                .unwrap_or_else(|| self.globals.clone());
            let main = environment.borrow().get(&definition.name).ok_or_else(|| {
                RuntimeError::new("project entry function is not initialized", definition.span)
            })?;
            self.call(main, &[], definition.span)
        })();

        self.semantic_expression_ids = None;
        self.typeck_results = None;
        self.frontend_semantics_verified = previous_semantics_verified;
        self.frontend_verified_trait_impls = previous_verified_trait_impls;
        self.frontend_impl_ids = previous_impl_ids;
        result
    }

    fn execute_inner(&mut self, program: &Program) -> Result<Value, RuntimeError> {
        self.steps = 0;
        self.pending_return = None;
        self.pending_loop_flow = None;
        let globals = self.globals.clone();
        match self.execute_statements(&program.statements, globals)? {
            Flow::Value(value) => Ok(value),
            Flow::Return(_) => Err(RuntimeError::new(
                "`return` can only be used inside a function",
                Span::default(),
            )),
            Flow::Break(_) | Flow::Continue => Err(RuntimeError::new(
                "loop control escaped its loop",
                Span::default(),
            )),
        }
    }
}
