//! Install declarations derived from the shared host contract.

use super::*;

impl Interpreter {
    pub(crate) fn register_host_contract_types(
        &mut self,
        contract: &rils_host::HostContract,
    ) -> Result<(), String> {
        let context = crate::runtime_builtins::NativeOwnedContext::from_host_contract(contract)?;
        let declarations = context
            .hosts
            .into_iter()
            .map(Value::HostType)
            .chain(context.enums.into_iter().map(Value::EnumType));
        let mut prepared = Vec::new();
        for declaration in declarations {
            let qualified = match &declaration {
                Value::HostType(definition) => &definition.name,
                Value::EnumType(definition) => &definition.name,
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
