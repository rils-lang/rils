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
                    Some(Value::HostFunction(function)) => {
                        if let Some(signature) = &function.signature {
                            signatures.insert(path, signature.clone());
                        }
                    }
                    Some(Value::Module(module)) => {
                        let identity = Rc::as_ptr(module) as usize;
                        if active.insert(identity) {
                            collect(&module.members.borrow(), &path, active, signatures);
                            active.remove(&identity);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut signatures = HashMap::new();
        collect(self, "", &mut HashSet::new(), &mut signatures);
        signatures
    }
}
