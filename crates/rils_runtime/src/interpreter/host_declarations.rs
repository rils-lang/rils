//! Install declarations derived from the shared host contract.

use super::*;

impl Interpreter {
    pub(crate) fn analyze_program(
        &self,
        program: &Program,
    ) -> rils_frontend::analysis::DocumentAnalysis {
        let environment = self.globals.borrow();
        let functions = environment.host_function_signatures();
        let types = environment
            .visible_host_definitions()
            .iter()
            .map(|definition| definition.name.clone())
            .collect();
        rils_frontend::analysis::analyze_program_with_host_declarations(program, &functions, &types)
    }

    pub(crate) fn register_host_contract_types(
        &mut self,
        contract: &rils_host::HostContract,
    ) -> Result<(), String> {
        let context = crate::runtime_builtins::NativeOwnedContext::from_host_contract(contract)?;
        let declarations = context
            .hosts
            .into_iter()
            .map(crate::value::Declaration::HostType)
            .chain(
                context
                    .enums
                    .into_iter()
                    .map(crate::value::Declaration::EnumType),
            );
        let mut prepared = Vec::new();
        for declaration in declarations {
            let qualified = match &declaration {
                crate::value::Declaration::HostType(definition) => &definition.name,
                crate::value::Declaration::EnumType(definition) => &definition.name,
                _ => unreachable!("host contract declaration"),
            };
            let mut path = qualified.split("::").map(str::to_owned).collect::<Vec<_>>();
            let name = path.pop().ok_or("host type requires a name")?;
            let environment = self.host_module_environment(&path)?;
            if environment.borrow().get(&name).is_some() {
                return Err(format!("name `{qualified}` is already registered"));
            }
            prepared.push((path, name, environment, declaration));
        }
        for (path, name, environment, declaration) in prepared {
            environment
                .borrow_mut()
                .define(name.clone(), declaration, false, None);
            if let Some(module) = self.host_module(&path)? {
                module.public.borrow_mut().insert(name);
            }
        }
        Ok(())
    }
}
