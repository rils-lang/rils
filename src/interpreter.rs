use std::{cell::RefCell, collections::HashMap, rc::Rc};

mod builtins;
mod call;
mod construction;
mod evaluation;
mod execution;
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
        EnumPayload, EnumType, FieldSlot, HostBoundMethod, HostFunction, HostFunctionHandler,
        HostType, ModuleValue, NativeFunction, RangeValue, ReferenceValue, SequenceIteratorValue,
        SequenceValue, StructInstance, StructType, TraitMethodSelector, TraitType, TypeAliasType,
        UserFunction, Value, VariantConstructor, enum_variant_name,
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
    max_steps: usize,
    function_depth: usize,
    pending_return: Option<Value>,
    pending_loop_flow: Option<Flow>,
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
            max_steps: 1_000_000,
            function_depth: 0,
            pending_return: None,
            pending_loop_flow: None,
        }
    }

    pub fn set_max_steps(&mut self, max_steps: usize) {
        self.max_steps = max_steps;
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

    pub fn execute(&mut self, program: &Program) -> Result<Value, RuntimeError> {
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
