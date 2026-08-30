use crate::{
    ast::{LogicalOp, UnaryOp},
    bytecode::CompileError,
    hir::{
        HirExpression, HirFunction, HirLiteral, HirMatchArm, HirPlace, HirProgram, HirProjection,
        HirStatement, LocalId,
    },
    source::Span,
};

mod control_flow;
mod ir;

pub use ir::*;

pub(crate) fn lower(program: HirProgram) -> Result<MirProgram, CompileError> {
    let functions = program
        .functions
        .into_iter()
        .map(|function| Builder::new(function.local_count).function(function))
        .collect::<Result<_, _>>()?;
    Ok(MirProgram {
        sources: program.sources,
        functions,
        types: program.types,
        iterators: program.iterators,
        trait_implementations: program.trait_implementations,
        entry: program.entry,
    })
}

struct LoopContext {
    continue_block: BlockId,
    break_block: BlockId,
    result: Register,
}

struct Builder {
    blocks: Vec<BasicBlock>,
    current: BlockId,
    constants: Vec<HirLiteral>,
    register_count: usize,
    local_count: usize,
    loops: Vec<LoopContext>,
}

impl Builder {
    fn new(local_count: usize) -> Self {
        Self {
            blocks: vec![BasicBlock {
                instructions: Vec::new(),
                terminator: None,
            }],
            current: 0,
            constants: Vec::new(),
            register_count: 0,
            local_count,
            loops: Vec::new(),
        }
    }

    fn function(mut self, function: HirFunction) -> Result<MirFunction, CompileError> {
        let result = self.statements(&function.statements)?;
        if self.is_open() {
            self.terminate(MirTerminator::Return(result), function.span);
        }
        Ok(MirFunction {
            name: function.name,
            exported: function.exported,
            blocks: self.blocks,
            constants: self.constants,
            register_count: self.register_count,
            local_count: self.local_count,
            local_mutability: function.local_mutability,
            parameter_count: function.parameter_count,
            capture_count: function.capture_count,
            span: function.span,
        })
    }

    fn statements(&mut self, statements: &[HirStatement]) -> Result<Register, CompileError> {
        let mut result = self.unit(Span::default());
        for statement in statements {
            if !self.is_open() {
                break;
            }
            let value = self.statement(statement)?;
            if !matches!(statement, HirStatement::DropLocal { .. }) {
                result = value;
            }
        }
        Ok(result)
    }

    fn statement(&mut self, statement: &HirStatement) -> Result<Register, CompileError> {
        match statement {
            HirStatement::DefineFunction {
                local,
                function,
                captures,
                span,
            } => {
                let destination = self.register();
                self.emit(
                    MirInstruction::CreateClosure {
                        destination,
                        function: *function,
                        captures: captures.clone(),
                    },
                    *span,
                );
                self.emit(
                    MirInstruction::InitLocal {
                        local: *local,
                        source: destination,
                    },
                    *span,
                );
                Ok(self.unit(*span))
            }
            HirStatement::Let {
                local,
                initializer,
                span,
            } => {
                let value = self.expression(initializer)?;
                self.emit(
                    MirInstruction::InitLocal {
                        local: *local,
                        source: value,
                    },
                    *span,
                );
                Ok(self.unit(*span))
            }
            HirStatement::While {
                condition,
                body,
                span,
            } => self.while_statement(condition, body, *span),
            HirStatement::Loop { body, span } => self.loop_statement(body, *span),
            HirStatement::For {
                binding,
                iterable,
                body,
                span,
            } => self.for_statement(*binding, iterable, body, *span),
            HirStatement::Return { value, span } => {
                let value = value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?
                    .unwrap_or_else(|| self.unit(*span));
                self.terminate(MirTerminator::Return(value), *span);
                Ok(value)
            }
            HirStatement::Break { value, span } => {
                let Some(context) = self.loops.last() else {
                    return Err(CompileError::unsupported("`break` outside a loop", *span));
                };
                let result = context.result;
                let break_block = context.break_block;
                let value = value
                    .as_ref()
                    .map(|value| self.expression(value))
                    .transpose()?
                    .unwrap_or_else(|| self.unit(*span));
                self.emit(
                    MirInstruction::Move {
                        destination: result,
                        source: value,
                    },
                    *span,
                );
                self.terminate(MirTerminator::Goto(break_block), *span);
                Ok(result)
            }
            HirStatement::Continue { span } => {
                let Some(context) = self.loops.last() else {
                    return Err(CompileError::unsupported(
                        "`continue` outside a loop",
                        *span,
                    ));
                };
                let continue_block = context.continue_block;
                self.terminate(MirTerminator::Goto(continue_block), *span);
                Ok(self.unit(*span))
            }
            HirStatement::DropLocal { local, span } => {
                self.emit(MirInstruction::DropLocal { local: *local }, *span);
                Ok(self.unit(*span))
            }
            HirStatement::Expression {
                expression,
                terminated,
                span,
            } => {
                let value = self.expression(expression)?;
                Ok(if *terminated { self.unit(*span) } else { value })
            }
        }
    }

    fn place(&mut self, place: &HirPlace) -> Result<MirPlace, CompileError> {
        let mut projections = Vec::with_capacity(place.projections.len());
        for projection in &place.projections {
            projections.push(match projection {
                HirProjection::Field(field) => MirProjection::Field(field.clone()),
                HirProjection::Index(index) => MirProjection::Index(self.expression(index)?),
            });
        }
        Ok(MirPlace {
            local: place.local,
            projections,
        })
    }

    fn expression(&mut self, expression: &HirExpression) -> Result<Register, CompileError> {
        match expression {
            HirExpression::Literal { value, span } => Ok(self.constant(value.clone(), *span)),
            HirExpression::Local { local, span } => {
                let destination = self.register();
                self.emit(
                    MirInstruction::TakeLocal {
                        destination,
                        local: *local,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Function { function, span } => {
                let destination = self.register();
                self.emit(
                    MirInstruction::LoadFunction {
                        destination,
                        function: *function,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::BindMethod {
                function,
                receiver,
                span,
            } => {
                let receiver = self.expression(receiver)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::BindMethod {
                        destination,
                        function: *function,
                        receiver,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::BorrowTemporary {
                value,
                mutable,
                span,
            } => {
                let source = self.expression(value)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::BorrowTemporary {
                        destination,
                        source,
                        mutable: *mutable,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Reborrow {
                reference,
                mutable,
                span,
            } => {
                let source = self.expression(reference)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::Reborrow {
                        destination,
                        source,
                        mutable: *mutable,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Place { place, span } => {
                let place = self.place(place)?;
                let destination = self.register();
                self.emit(MirInstruction::TakePlace { destination, place }, *span);
                Ok(destination)
            }
            HirExpression::Assign { local, value, span } => {
                let value = self.expression(value)?;
                self.emit(
                    MirInstruction::StoreLocal {
                        local: *local,
                        source: value,
                    },
                    *span,
                );
                Ok(self.unit(*span))
            }
            HirExpression::AssignPlace { place, value, span } => {
                let place = self.place(place)?;
                let source = self.expression(value)?;
                self.emit(MirInstruction::StorePlace { place, source }, *span);
                Ok(self.unit(*span))
            }
            HirExpression::AssignDereference {
                reference,
                value,
                span,
            } => {
                let reference = self.expression(reference)?;
                let value = self.expression(value)?;
                self.emit(
                    MirInstruction::StoreDereference {
                        reference,
                        source: value,
                    },
                    *span,
                );
                Ok(self.unit(*span))
            }
            HirExpression::BorrowLocal {
                local,
                mutable,
                span,
            } => {
                let destination = self.register();
                self.emit(
                    MirInstruction::BorrowLocal {
                        destination,
                        local: *local,
                        mutable: *mutable,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::BorrowPlace {
                place,
                mutable,
                span,
            } => {
                let place = self.place(place)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::BorrowPlace {
                        destination,
                        place,
                        mutable: *mutable,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Unary {
                operator,
                operand,
                span,
            } => {
                let operand = self.expression(operand)?;
                let destination = self.register();
                if *operator == UnaryOp::Dereference {
                    self.emit(
                        MirInstruction::Dereference {
                            destination,
                            source: operand,
                        },
                        *span,
                    );
                } else {
                    self.emit(
                        MirInstruction::Unary {
                            destination,
                            operator: *operator,
                            operand,
                        },
                        *span,
                    );
                }
                Ok(destination)
            }
            HirExpression::Cast {
                operand,
                target,
                span,
            } => {
                let source = self.expression(operand)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::Cast {
                        destination,
                        source,
                        target: *target,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Binary {
                left,
                operator,
                right,
                integer,
                span,
            } => {
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::Binary {
                        destination,
                        left,
                        operator: *operator,
                        right,
                        integer: *integer,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Logical {
                left,
                operator,
                right,
                span,
            } => self.logical_expression(left, *operator, right, *span),
            HirExpression::Call {
                function,
                arguments,
                span,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                let destination = self.register();
                self.emit(
                    MirInstruction::Call {
                        destination,
                        function: *function,
                        arguments,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::CallValue {
                callee,
                arguments,
                span,
            } => {
                let callee = self.expression(callee)?;
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                let destination = self.register();
                self.emit(
                    MirInstruction::CallValue {
                        destination,
                        callee,
                        arguments,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::CallImport {
                name,
                signature,
                capability,
                arguments,
                span,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                let destination = self.register();
                self.emit(
                    MirInstruction::CallImport {
                        destination,
                        name: name.clone(),
                        signature: signature.clone(),
                        capability: capability.clone(),
                        arguments,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::CallRuntime {
                builtin,
                arguments,
                span,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                let destination = self.register();
                self.emit(
                    MirInstruction::CallRuntime {
                        destination,
                        builtin: *builtin,
                        arguments,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::CallIntrinsic {
                intrinsic,
                target,
                arguments,
                span,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                let destination = self.register();
                self.emit(
                    MirInstruction::CallIntrinsic {
                        destination,
                        intrinsic: *intrinsic,
                        target: *target,
                        arguments,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::IntoIterator { value, span } => {
                let source = self.expression(value)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::IntoIterator {
                        destination,
                        source,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::ConstructRecord {
                type_id,
                variant,
                fields,
                span,
            } => {
                let fields = fields
                    .iter()
                    .map(|(name, value)| Ok((name.clone(), self.expression(value)?)))
                    .collect::<Result<_, CompileError>>()?;
                let destination = self.register();
                self.emit(
                    MirInstruction::ConstructRecord {
                        destination,
                        type_id: *type_id,
                        variant: variant.clone(),
                        fields,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::ConstructTupleVariant {
                type_id,
                variant,
                fields,
                span,
            } => {
                let fields = fields
                    .iter()
                    .map(|value| self.expression(value))
                    .collect::<Result<_, _>>()?;
                let destination = self.register();
                self.emit(
                    MirInstruction::ConstructTupleVariant {
                        destination,
                        type_id: *type_id,
                        variant: variant.clone(),
                        fields,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::ConstructUnitVariant {
                type_id,
                variant,
                span,
            } => {
                let destination = self.register();
                self.emit(
                    MirInstruction::ConstructUnitVariant {
                        destination,
                        type_id: *type_id,
                        variant: variant.clone(),
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Tuple { elements, span } => {
                let elements = elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect::<Result<Vec<_>, _>>()?;
                let destination = self.register();
                self.emit(
                    MirInstruction::BuildTuple {
                        destination,
                        elements,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Array {
                elements,
                repeat,
                span,
            } => {
                let elements = elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect::<Result<Vec<_>, _>>()?;
                let destination = self.register();
                if let Some(count) = repeat {
                    let count = self.expression(count)?;
                    self.emit(
                        MirInstruction::BuildRepeatArray {
                            destination,
                            value: elements[0],
                            count,
                        },
                        *span,
                    );
                } else {
                    self.emit(
                        MirInstruction::BuildArray {
                            destination,
                            elements,
                        },
                        *span,
                    );
                }
                Ok(destination)
            }
            HirExpression::Range { start, end, span } => {
                let start = self.expression(start)?;
                let end = self.expression(end)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::BuildRange {
                        destination,
                        start,
                        end,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::OptionNone { span } => {
                let destination = self.register();
                self.emit(MirInstruction::BuildOptionNone { destination }, *span);
                Ok(destination)
            }
            HirExpression::OptionSome { value, span } => {
                let source = self.expression(value)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::BuildOptionSome {
                        destination,
                        source,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::ResultOk { value, span } => {
                let source = self.expression(value)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::BuildResultOk {
                        destination,
                        source,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::ResultErr { value, span } => {
                let source = self.expression(value)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::BuildResultErr {
                        destination,
                        source,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Try { operand, span } => {
                let source = self.expression(operand)?;
                let destination = self.register();
                self.emit(
                    MirInstruction::TryResult {
                        destination,
                        source,
                    },
                    *span,
                );
                Ok(destination)
            }
            HirExpression::Match { value, arms, span } => self.match_expression(value, arms, *span),
            HirExpression::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => self.if_expression(condition, then_branch, else_branch.as_deref(), *span),
            HirExpression::Block { statements, span } => {
                if statements.is_empty() {
                    Ok(self.unit(*span))
                } else {
                    self.statements(statements)
                }
            }
        }
    }

    fn unit(&mut self, span: Span) -> Register {
        self.constant(HirLiteral::Unit, span)
    }

    fn constant(&mut self, value: HirLiteral, span: Span) -> Register {
        let constant = self.constants.len();
        self.constants.push(value);
        let destination = self.register();
        self.emit(
            MirInstruction::LoadConstant {
                destination,
                constant,
            },
            span,
        );
        destination
    }

    fn register(&mut self) -> Register {
        let register = self.register_count;
        self.register_count += 1;
        register
    }

    fn block(&mut self) -> BlockId {
        let block = self.blocks.len();
        self.blocks.push(BasicBlock {
            instructions: Vec::new(),
            terminator: None,
        });
        block
    }

    fn emit(&mut self, instruction: MirInstruction, span: Span) {
        if self.is_open() {
            self.blocks[self.current]
                .instructions
                .push(SpannedInstruction { instruction, span });
        }
    }

    fn terminate(&mut self, terminator: MirTerminator, span: Span) {
        if self.is_open() {
            self.blocks[self.current].terminator = Some(SpannedTerminator { terminator, span });
        }
    }

    fn is_open(&self) -> bool {
        self.blocks[self.current].terminator.is_none()
    }
}
