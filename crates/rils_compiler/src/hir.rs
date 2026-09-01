use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    HostContract, HostFunctionDeclaration, HostReceiver,
    ast::{Block, EnumVariant, Expr, Literal, Pattern, Program, Stmt, UnaryOp},
    bytecode::CompileError,
    source::{SourceFile, SourceId, Span},
    types::{FunctionSignature, Type},
};

mod combinators;
mod expression;
mod function;
mod helpers;
mod imports;
mod ir;
mod iterator_defaults;
mod literals;
mod program;
mod symbols;

use imports::*;
pub use ir::*;
use literals::*;
pub(crate) use program::{lower_project_with_host, lower_with_host};
use symbols::*;

fn overload_score(host: &HostContract, expected: &[Type], actual: &[Type]) -> Option<usize> {
    expected
        .iter()
        .zip(actual)
        .try_fold(0usize, |score, (expected, actual)| {
            if expected == actual {
                return Some(score);
            }
            if matches!(
                actual,
                Type::Unknown | Type::IntegerVariable(_) | Type::FloatVariable(_)
            ) {
                return Some(score + 100);
            }
            match (expected, actual) {
                (
                    Type::Named {
                        name: expected,
                        arguments: expected_arguments,
                    },
                    Type::Named {
                        name: actual,
                        arguments: actual_arguments,
                    },
                ) if expected_arguments.is_empty()
                    && actual_arguments.is_empty()
                    && host.is_type_assignable(expected, actual) =>
                {
                    Some(score + host.type_assignment_distance(expected, actual)?)
                }
                _ => None,
            }
        })
}

fn format_host_candidates(candidates: &[HostFunctionDeclaration]) -> String {
    candidates
        .iter()
        .map(|candidate| {
            format!(
                "  {}({}) -> {}",
                candidate.name,
                candidate
                    .signature
                    .parameters
                    .as_ref()
                    .expect("host signatures are fixed")
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                candidate.signature.return_type
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_host_use_aliases(
    statements: &[Stmt],
    prefix: &mut Vec<String>,
    functions: &mut HashMap<String, Vec<HostFunctionDeclaration>>,
) {
    for statement in statements {
        match statement {
            Stmt::Use { imports, .. } => {
                for import in imports {
                    let candidates = use_resolution_candidates(prefix, &import.path);
                    if import.kind == crate::ast::UseImportKind::Glob {
                        let declarations = functions
                            .iter()
                            .filter_map(|(name, declaration)| {
                                let member = candidates
                                    .iter()
                                    .find_map(|candidate| immediate_path_member(name, candidate))?;
                                Some((member.to_owned(), declaration.clone()))
                            })
                            .collect::<Vec<_>>();
                        for (name, declaration) in declarations {
                            functions.insert(qualified_name(prefix, &name), declaration);
                        }
                        continue;
                    }
                    let declarations = candidates
                        .iter()
                        .find_map(|candidate| functions.get(candidate))
                        .cloned();
                    if let Some(declarations) = declarations {
                        functions.insert(
                            qualified_name(prefix, import.binding_name().expect("single import")),
                            declarations,
                        );
                    }
                }
            }
            Stmt::Module {
                name,
                statements: Some(module_statements),
                ..
            } => {
                prefix.push(name.clone());
                collect_host_use_aliases(module_statements, prefix, functions);
                prefix.pop();
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
struct GeneratedFunctions {
    next_id: Rc<Cell<FunctionId>>,
    functions: Rc<RefCell<Vec<(FunctionId, HirFunction)>>>,
}

struct FunctionLowerer<'a> {
    types: &'a HashMap<String, TypeId>,
    type_definitions: &'a [HirTypeDefinition],
    host_functions: &'a HashMap<String, Vec<HostFunctionDeclaration>>,
    host_methods: &'a HashMap<String, Vec<HostFunctionDeclaration>>,
    host_contract: &'a HostContract,
    expression_ids: &'a rils_frontend::semantic::ExpressionIdentityMap,
    typeck_results: &'a rils_frontend::semantic::TypeckResults,
    resolved_definitions: &'a HashMap<rils_frontend::DefId, MethodInfo>,
    namespace: String,
    self_type: Option<String>,
    scopes: Vec<HashMap<String, LocalId>>,
    mutable: Vec<bool>,
    in_function: bool,
    capture_count: usize,
    generated: GeneratedFunctions,
    captured: HashSet<LocalId>,
}
