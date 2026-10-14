//! Read registered host signatures without materializing runtime payloads.
use super::*;
use crate::FunctionSignature;
use std::collections::HashSet;

impl Environment {
    pub fn host_function_signatures(&self) -> HashMap<String, FunctionSignature> {
        fn collect(
            environment: &Environment,
            prefix: &str,
            active: &mut HashSet<usize>,
            signatures: &mut HashMap<String, FunctionSignature>,
        ) {
            for (name, slot) in &environment.values {
                let slot = slot.borrow();
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}::{name}")
                };
                match slot.value.as_ref() {
                    Some(value) if value.as_callable::<crate::value::HostFunction>().is_some() => {
                        let function = value.as_callable::<crate::value::HostFunction>().unwrap();
                        if let Some(signature) = &function.signature {
                            signatures.insert(path, signature.clone());
                        }
                    }
                    _ => {}
                }
            }
            for (name, declaration) in &environment.names {
                if let Declaration::Module(module) = declaration {
                    let identity = Rc::as_ptr(module) as usize;
                    if active.insert(identity) {
                        let path = if prefix.is_empty() {
                            name.clone()
                        } else {
                            format!("{prefix}::{name}")
                        };
                        collect(&module.members.borrow(), &path, active, signatures);
                        active.remove(&identity);
                    }
                }
            }
        }
        let mut signatures = HashMap::new();
        collect(self, "", &mut HashSet::new(), &mut signatures);
        signatures
    }
}
