use super::*;

impl<'a> VirtualMachine<'a> {
    pub(in crate::bytecode) fn new(
        module: &'a BytecodeModule,
        imports: Vec<Rc<BytecodeHostHandler>>,
        host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
        limits: crate::ExecutionLimits,
    ) -> Self {
        let entry = &module.functions[module.entry];
        Self {
            module,
            imports,
            host_value_formatter,
            frames: vec![Frame {
                function: module.entry,
                registers: vec![None; entry.register_count],
                locals: new_local_storage(entry),
                instruction: 0,
                return_action: ReturnAction::Complete,
            }],
            steps: 0,
            max_steps: limits.max_steps,
            max_call_depth: limits.max_call_depth,
            root_is_module_entry: true,
        }
    }

    pub(in crate::bytecode) fn new_call(
        module: &'a BytecodeModule,
        imports: Vec<Rc<BytecodeHostHandler>>,
        host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
        limits: crate::ExecutionLimits,
        function: usize,
        arguments: Vec<Value>,
    ) -> Result<Self, BytecodeError> {
        let callee = &module.functions[function];
        if callee.capture_count != 0 {
            return Err(BytecodeError::new(
                format!("function `{}` requires a closure environment", callee.name),
                callee.span,
            ));
        }
        if arguments.len() != callee.parameter_count {
            return Err(BytecodeError::new(
                format!(
                    "function `{}` expects {} arguments, found {}",
                    callee.name,
                    callee.parameter_count,
                    arguments.len()
                ),
                callee.span,
            ));
        }
        let locals = new_local_storage(callee);
        for (local, argument) in locals.iter().zip(arguments) {
            local.borrow_mut().initialize(argument);
        }
        Ok(Self {
            module,
            imports,
            host_value_formatter,
            frames: vec![Frame {
                function,
                registers: vec![None; callee.register_count],
                locals,
                instruction: 0,
                return_action: ReturnAction::Complete,
            }],
            steps: 0,
            max_steps: limits.max_steps,
            max_call_depth: limits.max_call_depth,
            root_is_module_entry: false,
        })
    }
}
