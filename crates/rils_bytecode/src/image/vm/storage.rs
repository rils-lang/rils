//! Aggregate construction and declaration-driven owned storage conversion.

use super::*;

impl VirtualMachine<'_> {
    pub(super) fn execute_storage(
        &mut self,
        instruction: Instruction,
        span: Span,
    ) -> Result<(), BytecodeError> {
        match instruction {
            Instruction::ApplyStorage {
                destination,
                source,
                expected,
            } => {
                let expected = expected.substitute(&self.frame().type_bindings);
                let value = self.take_register(source, span)?;
                let value = self
                    .native_context
                    .storage()
                    .apply_declared(value, &expected)
                    .map_err(|message| BytecodeError::new(message, span))?;
                self.frame_mut().registers[destination] = Some(value);
            }
            Instruction::BuildTuple {
                destination,
                elements,
            } => {
                let values = self.take_registers(elements, span)?;
                self.frame_mut().registers[destination] =
                    Some(sequence_value(values, false, span)?);
            }
            Instruction::BuildArray {
                destination,
                elements,
            } => {
                let values = self.take_registers(elements, span)?;
                self.frame_mut().registers[destination] = Some(sequence_value(values, true, span)?);
            }
            Instruction::BuildRepeatArray {
                destination,
                value,
                count,
            } => {
                let value = self.take_register(value, span)?;
                let count = self.take_register(count, span)?;
                let Some(count) = count.as_usize() else {
                    return Err(BytecodeError::new("array repeat count must be usize", span));
                };
                if !value.is_copy() {
                    return Err(BytecodeError::new(
                        "array repeat syntax requires a Copy value",
                        span,
                    ));
                }
                let values = (0..count)
                    .map(|_| {
                        value
                            .clone_owned()
                            .map_err(|message| BytecodeError::new(message, span))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.frame_mut().registers[destination] = Some(sequence_value(values, true, span)?);
            }
            Instruction::BuildRange {
                destination,
                start,
                end,
            } => {
                let start = self.take_register(start, span)?;
                let end = self.take_register(end, span)?;
                let range = native_range(start, end)
                    .map_err(|message| BytecodeError::new(message, span))?;
                self.frame_mut().registers[destination] = Some(range);
            }
            Instruction::BuildOptionNone {
                destination,
                item_type,
            } => {
                let item_type = item_type.substitute(&self.frame().type_bindings);
                let value = self
                    .native_context
                    .none(&item_type)
                    .map_err(|message| BytecodeError::new(message, span))?;
                self.frame_mut().registers[destination] = Some(value);
            }
            Instruction::BuildOptionSome {
                destination,
                source,
                item_type,
            } => {
                let item_type = item_type.substitute(&self.frame().type_bindings);
                let value = self.take_register(source, span)?;
                let value = self
                    .native_context
                    .storage()
                    .construct_option(&Type::Option(Box::new(item_type)), Some(value))
                    .map_err(|message| BytecodeError::new(message, span))?;
                self.frame_mut().registers[destination] = Some(value);
            }
            Instruction::BuildResultOk {
                destination,
                source,
            } => {
                let value = self.take_register(source, span)?;
                let ok_type = Type::of_value(&value);
                self.frame_mut().registers[destination] = Some(Value::Result {
                    value: Ok(Rc::new(value)),
                    ok_type,
                    error_type: None,
                });
            }
            Instruction::BuildResultErr {
                destination,
                source,
            } => {
                let value = self.take_register(source, span)?;
                let error_type = Type::of_value(&value);
                self.frame_mut().registers[destination] = Some(Value::Result {
                    value: Err(Rc::new(value)),
                    ok_type: None,
                    error_type,
                });
            }
            _ => unreachable!("non-storage instruction"),
        }
        Ok(())
    }
}
