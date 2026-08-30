use super::*;

impl Builder {
    pub(super) fn logical_expression(
        &mut self,
        left: &HirExpression,
        operator: LogicalOp,
        right: &HirExpression,
        span: Span,
    ) -> Result<Register, CompileError> {
        let result = self.register();
        let left = self.expression(left)?;
        self.emit(
            MirInstruction::Move {
                destination: result,
                source: left,
            },
            span,
        );
        let right_block = self.block();
        let exit_block = self.block();
        let (then_block, else_block) = match operator {
            LogicalOp::And => (right_block, exit_block),
            LogicalOp::Or => (exit_block, right_block),
        };
        self.terminate(
            MirTerminator::Branch {
                condition: result,
                then_block,
                else_block,
            },
            span,
        );
        self.current = right_block;
        let right = self.expression(right)?;
        self.emit(
            MirInstruction::Move {
                destination: result,
                source: right,
            },
            span,
        );
        self.terminate(MirTerminator::Goto(exit_block), span);
        self.current = exit_block;
        Ok(result)
    }

    pub(super) fn match_expression(
        &mut self,
        value: &HirExpression,
        arms: &[HirMatchArm],
        span: Span,
    ) -> Result<Register, CompileError> {
        let value = self.expression(value)?;
        let result = self.register();
        let join_block = self.block();

        for arm in arms {
            let matched = self.register();
            self.emit(
                MirInstruction::MatchPattern {
                    destination: matched,
                    source: value,
                    pattern: arm.pattern.clone(),
                },
                arm.span,
            );
            let body_block = self.block();
            let next_block = self.block();
            self.terminate(
                MirTerminator::Branch {
                    condition: matched,
                    then_block: body_block,
                    else_block: next_block,
                },
                arm.span,
            );

            self.current = body_block;
            self.emit(
                MirInstruction::BindPattern {
                    source: value,
                    pattern: arm.pattern.clone(),
                },
                arm.span,
            );
            let arm_value = self.expression(&arm.expression)?;
            if self.is_open() {
                self.emit(
                    MirInstruction::Move {
                        destination: result,
                        source: arm_value,
                    },
                    arm.span,
                );
                self.terminate(MirTerminator::Goto(join_block), arm.span);
            }
            self.current = next_block;
        }

        self.terminate(MirTerminator::MatchFail, span);
        self.current = join_block;
        Ok(result)
    }

    pub(super) fn if_expression(
        &mut self,
        condition: &HirExpression,
        then_branch: &[HirStatement],
        else_branch: Option<&HirExpression>,
        span: Span,
    ) -> Result<Register, CompileError> {
        let condition = self.expression(condition)?;
        let then_block = self.block();
        let else_block = self.block();
        let join_block = self.block();
        let result = self.register();
        self.terminate(
            MirTerminator::Branch {
                condition,
                then_block,
                else_block,
            },
            span,
        );

        self.current = then_block;
        let then_value = self.statements(then_branch)?;
        if self.is_open() {
            self.emit(
                MirInstruction::Move {
                    destination: result,
                    source: then_value,
                },
                span,
            );
            self.terminate(MirTerminator::Goto(join_block), span);
        }

        self.current = else_block;
        let else_value = else_branch
            .map(|branch| self.expression(branch))
            .transpose()?
            .unwrap_or_else(|| self.unit(span));
        if self.is_open() {
            self.emit(
                MirInstruction::Move {
                    destination: result,
                    source: else_value,
                },
                span,
            );
            self.terminate(MirTerminator::Goto(join_block), span);
        }

        self.current = join_block;
        Ok(result)
    }

    pub(super) fn while_statement(
        &mut self,
        condition: &HirExpression,
        body: &[HirStatement],
        span: Span,
    ) -> Result<Register, CompileError> {
        let condition_block = self.block();
        let body_block = self.block();
        let normal_exit = self.block();
        let exit_block = self.block();
        let result = self.register();
        self.terminate(MirTerminator::Goto(condition_block), span);

        self.current = condition_block;
        let condition = self.expression(condition)?;
        self.terminate(
            MirTerminator::Branch {
                condition,
                then_block: body_block,
                else_block: normal_exit,
            },
            span,
        );

        self.current = normal_exit;
        let unit = self.unit(span);
        self.emit(
            MirInstruction::Move {
                destination: result,
                source: unit,
            },
            span,
        );
        self.terminate(MirTerminator::Goto(exit_block), span);

        self.loops.push(LoopContext {
            continue_block: condition_block,
            break_block: exit_block,
            result,
        });
        self.current = body_block;
        self.statements(body)?;
        if self.is_open() {
            self.terminate(MirTerminator::Goto(condition_block), span);
        }
        self.loops.pop();
        self.current = exit_block;
        Ok(result)
    }

    pub(super) fn for_statement(
        &mut self,
        binding: LocalId,
        iterable: &HirExpression,
        body: &[HirStatement],
        span: Span,
    ) -> Result<Register, CompileError> {
        let iterable = self.expression(iterable)?;
        let iterator = self.register();
        self.emit(
            MirInstruction::IntoIterator {
                destination: iterator,
                source: iterable,
            },
            span,
        );

        let next_block = self.block();
        let body_block = self.block();
        let normal_exit = self.block();
        let exit_block = self.block();
        let item = self.register();
        let result = self.register();
        self.terminate(MirTerminator::Goto(next_block), span);

        self.current = next_block;
        self.terminate(
            MirTerminator::IteratorNext {
                iterator,
                destination: item,
                some_block: body_block,
                none_block: normal_exit,
            },
            span,
        );

        self.current = normal_exit;
        let unit = self.unit(span);
        self.emit(
            MirInstruction::Move {
                destination: result,
                source: unit,
            },
            span,
        );
        self.terminate(MirTerminator::Goto(exit_block), span);

        self.loops.push(LoopContext {
            continue_block: next_block,
            break_block: exit_block,
            result,
        });
        self.current = body_block;
        self.emit(
            MirInstruction::InitLocal {
                local: binding,
                source: item,
            },
            span,
        );
        self.statements(body)?;
        if self.is_open() {
            self.terminate(MirTerminator::Goto(next_block), span);
        }
        self.loops.pop();
        self.current = exit_block;
        Ok(result)
    }

    pub(super) fn loop_statement(
        &mut self,
        body: &[HirStatement],
        span: Span,
    ) -> Result<Register, CompileError> {
        let body_block = self.block();
        let exit_block = self.block();
        let result = self.register();
        self.terminate(MirTerminator::Goto(body_block), span);
        self.loops.push(LoopContext {
            continue_block: body_block,
            break_block: exit_block,
            result,
        });
        self.current = body_block;
        self.statements(body)?;
        if self.is_open() {
            self.terminate(MirTerminator::Goto(body_block), span);
        }
        self.loops.pop();
        self.current = exit_block;
        Ok(result)
    }
}
