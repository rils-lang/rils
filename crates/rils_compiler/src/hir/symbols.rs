//! Symbol discovery performed before expression lowering.

use std::collections::{HashMap, HashSet};

use crate::{
    ast::{Block, Parameter, Stmt},
    bytecode::CompileError,
    source::Span,
    types::Type,
};

use super::{FunctionId, HirIteratorMethods, HirTraitImplementation, HirTypeDefinition, TypeId};
pub(super) struct FunctionDeclaration<'a> {
    pub(super) name: &'a str,
    pub(super) name_span: Span,
    pub(super) qualified_name: String,
    pub(super) parameters: &'a [Parameter],
    pub(super) body: &'a Block,
    pub(super) span: Span,
    pub(super) exported: bool,
    pub(super) self_type: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) struct MethodInfo {
    pub(super) function: FunctionId,
    pub(super) receiver: Option<ReceiverMode>,
    pub(super) source: crate::source::SourceId,
}

#[derive(Clone, Copy)]
pub(super) enum ReceiverMode {
    Owned,
    Reference { mutable: bool },
}

pub(super) fn function_declaration(statement: &Stmt) -> Option<FunctionDeclaration<'_>> {
    let exported = matches!(statement, Stmt::Public { .. });
    let statement = match statement {
        Stmt::Public { statement, .. } => statement.as_ref(),
        statement => statement,
    };
    let Stmt::Function {
        name,
        name_span,
        parameters,
        body,
        span,
        ..
    } = statement
    else {
        return None;
    };
    Some(FunctionDeclaration {
        name,
        name_span: *name_span,
        qualified_name: name.clone(),
        parameters,
        body,
        span: *span,
        exported,
        self_type: None,
    })
}

pub(super) fn unwrapped_statement(statement: &Stmt) -> &Stmt {
    match statement {
        Stmt::Public { statement, .. } => statement.as_ref(),
        statement => statement,
    }
}

pub(super) fn collect_nested_symbols(
    statements: &[Stmt],
    prefix: &mut Vec<String>,
    functions: &mut HashMap<String, FunctionId>,
    types: &mut HashMap<String, TypeId>,
    type_definitions: &mut Vec<HirTypeDefinition>,
) -> Result<(), CompileError> {
    for statement in statements {
        let Stmt::Module {
            name,
            statements: Some(module_statements),
            ..
        } = unwrapped_statement(statement)
        else {
            continue;
        };
        prefix.push(name.clone());
        for statement in module_statements {
            let statement = unwrapped_statement(statement);
            match statement {
                Stmt::Function { name, span, .. } => {
                    let qualified = qualified_name(prefix, name);
                    let next_id = functions.values().copied().max().unwrap_or(0) + 1;
                    if functions.insert(qualified.clone(), next_id).is_some() {
                        return Err(CompileError::unsupported(
                            format!("duplicate function `{qualified}`"),
                            *span,
                        ));
                    }
                    functions.entry(name.clone()).or_insert(next_id);
                }
                Stmt::Struct {
                    name,
                    generic_parameters,
                    fields,
                    ..
                } => {
                    let qualified = qualified_name(prefix, name);
                    let id = type_definitions.len();
                    types.insert(qualified.clone(), id);
                    types.entry(name.clone()).or_insert(id);
                    type_definitions.push(HirTypeDefinition::Struct {
                        name: qualified,
                        generic_parameters: generic_parameters.clone(),
                        fields: fields.clone(),
                    });
                }
                Stmt::Enum {
                    name,
                    generic_parameters,
                    variants,
                    ..
                } => {
                    let qualified = qualified_name(prefix, name);
                    let id = type_definitions.len();
                    types.insert(qualified.clone(), id);
                    types.entry(name.clone()).or_insert(id);
                    type_definitions.push(HirTypeDefinition::Enum {
                        name: qualified,
                        generic_parameters: generic_parameters.clone(),
                        variants: variants.clone(),
                    });
                }
                _ => {}
            }
        }
        collect_nested_symbols(
            module_statements,
            prefix,
            functions,
            types,
            type_definitions,
        )?;
        prefix.pop();
    }
    Ok(())
}

pub(super) fn collect_nested_function_declarations<'a>(
    statements: &'a [Stmt],
    prefix: &mut Vec<String>,
    functions: &HashMap<String, FunctionId>,
    declarations: &mut Vec<(FunctionId, FunctionDeclaration<'a>)>,
) {
    for statement in statements {
        let Stmt::Module {
            name,
            statements: Some(module_statements),
            ..
        } = unwrapped_statement(statement)
        else {
            continue;
        };
        prefix.push(name.clone());
        for statement in module_statements {
            if let Some(mut declaration) = function_declaration(statement) {
                declaration.qualified_name = qualified_name(prefix, declaration.name);
                declarations.push((functions[&declaration.qualified_name], declaration));
            }
        }
        collect_nested_function_declarations(module_statements, prefix, functions, declarations);
        prefix.pop();
    }
}

pub(super) fn collect_use_aliases(
    statements: &[Stmt],
    prefix: &mut Vec<String>,
    functions: &mut HashMap<String, FunctionId>,
    types: &mut HashMap<String, TypeId>,
    public_symbols: &HashSet<String>,
) {
    for statement in statements {
        match unwrapped_statement(statement) {
            Stmt::Use { imports, .. } => {
                for import in imports {
                    let candidates = use_resolution_candidates(prefix, &import.path);
                    if import.kind == crate::ast::UseImportKind::Glob {
                        let function_members = functions
                            .iter()
                            .filter_map(|(name, id)| {
                                if !public_symbols.contains(name) {
                                    return None;
                                }
                                let member = candidates
                                    .iter()
                                    .find_map(|candidate| immediate_path_member(name, candidate))?;
                                Some((member.to_owned(), *id))
                            })
                            .collect::<Vec<_>>();
                        let type_members = types
                            .iter()
                            .filter_map(|(name, id)| {
                                if !public_symbols.contains(name) {
                                    return None;
                                }
                                let member = candidates
                                    .iter()
                                    .find_map(|candidate| immediate_path_member(name, candidate))?;
                                Some((member.to_owned(), *id))
                            })
                            .collect::<Vec<_>>();
                        for (name, id) in function_members {
                            functions.insert(qualified_name(prefix, &name), id);
                        }
                        for (name, id) in type_members {
                            types.insert(qualified_name(prefix, &name), id);
                        }
                        let public_modules = public_symbols
                            .iter()
                            .filter(|symbol| {
                                candidates.iter().any(|candidate| {
                                    immediate_path_member(symbol, candidate).is_some()
                                }) && functions.keys().chain(types.keys()).any(|name| {
                                    name.strip_prefix(symbol.as_str())
                                        .is_some_and(|rest| rest.starts_with("::"))
                                })
                            })
                            .cloned()
                            .collect::<Vec<_>>();
                        for module in public_modules {
                            let name = module.rsplit("::").next().expect("module name");
                            let destination = qualified_name(prefix, name);
                            alias_public_module_members(
                                functions,
                                &module,
                                &destination,
                                public_symbols,
                            );
                            alias_public_module_members(
                                types,
                                &module,
                                &destination,
                                public_symbols,
                            );
                        }
                        continue;
                    }
                    let alias = import.binding_name().expect("single import");
                    if let Some(id) = candidates
                        .iter()
                        .find_map(|candidate| functions.get(candidate))
                        .copied()
                    {
                        functions.insert(qualified_name(prefix, alias), id);
                    }
                    if let Some(id) = candidates
                        .iter()
                        .find_map(|candidate| types.get(candidate))
                        .copied()
                    {
                        types.insert(qualified_name(prefix, alias), id);
                    }
                    for module in candidates
                        .iter()
                        .filter(|candidate| public_symbols.contains(*candidate))
                    {
                        let destination = qualified_name(prefix, alias);
                        alias_public_module_members(
                            functions,
                            module,
                            &destination,
                            public_symbols,
                        );
                        alias_public_module_members(types, module, &destination, public_symbols);
                    }
                }
            }
            Stmt::Module {
                name,
                statements: Some(module_statements),
                ..
            } => {
                prefix.push(name.clone());
                collect_use_aliases(module_statements, prefix, functions, types, public_symbols);
                prefix.pop();
            }
            _ => {}
        }
    }
}

pub(super) fn collect_public_symbols(
    statements: &[Stmt],
    prefix: &mut Vec<String>,
    output: &mut HashSet<String>,
) {
    for statement in statements {
        let (statement, public) = match statement {
            Stmt::Public { statement, .. } => (statement.as_ref(), true),
            statement => (statement, false),
        };
        if public {
            match statement {
                Stmt::Function { name, .. }
                | Stmt::Struct { name, .. }
                | Stmt::Enum { name, .. }
                | Stmt::TypeAlias { name, .. }
                | Stmt::Module { name, .. } => {
                    output.insert(qualified_name(prefix, name));
                }
                Stmt::Use { imports, .. } => {
                    for import in imports {
                        if let Some(name) = import.binding_name() {
                            output.insert(qualified_name(prefix, name));
                        }
                    }
                }
                _ => {}
            }
        }
        if let Stmt::Module {
            name,
            statements: Some(statements),
            ..
        } = statement
        {
            prefix.push(name.clone());
            collect_public_symbols(statements, prefix, output);
            prefix.pop();
        }
    }
}

fn alias_public_module_members<T: Copy>(
    symbols: &mut HashMap<String, T>,
    source: &str,
    destination: &str,
    public_symbols: &HashSet<String>,
) {
    let prefix = format!("{source}::");
    let aliases = symbols
        .iter()
        .filter_map(|(name, id)| {
            let remainder = name.strip_prefix(&prefix)?;
            public_symbols
                .contains(name)
                .then(|| (format!("{destination}::{remainder}"), *id))
        })
        .collect::<Vec<_>>();
    symbols.extend(aliases);
}

pub(super) fn resolve_anchored_path(prefix: &[String], path: &[String]) -> Option<String> {
    let first = path.first()?.as_str();
    if !matches!(first, "crate" | "self" | "super") {
        return None;
    }
    let mut output = match first {
        "crate" => Vec::new(),
        "self" => prefix.to_vec(),
        "super" => {
            let mut output = prefix.to_vec();
            output.pop()?;
            output
        }
        _ => unreachable!(),
    };
    for segment in path.iter().skip(1) {
        match segment.as_str() {
            "crate" => output.clear(),
            "self" => {}
            "super" => {
                output.pop()?;
            }
            _ => output.push(segment.clone()),
        }
    }
    Some(output.join("::"))
}

pub(super) fn use_resolution_candidates(prefix: &[String], path: &[String]) -> Vec<String> {
    let absolute = path.join("::");
    let mut candidates = Vec::new();
    if let Some(anchored) = resolve_anchored_path(prefix, path) {
        candidates.push(anchored);
    } else {
        if !prefix.is_empty() {
            candidates.push(format!("{}::{absolute}", prefix.join("::")));
        }
        candidates.push(absolute);
    }
    candidates
}

pub(super) fn immediate_path_member<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let member = if prefix.is_empty() {
        name
    } else {
        name.strip_prefix(prefix)?.strip_prefix("::")?
    };
    (!member.is_empty() && !member.contains("::")).then_some(member)
}

pub(super) fn collect_method_symbols(
    statements: &[Stmt],
    prefix: &mut Vec<String>,
    next_function_id: &mut FunctionId,
    methods: &mut HashMap<String, MethodInfo>,
    method_names: &mut HashMap<String, Option<MethodInfo>>,
) {
    for statement in statements {
        match unwrapped_statement(statement) {
            Stmt::Impl {
                target,
                trait_name,
                methods: definitions,
                span,
                ..
            } => {
                let Some(target_name) = qualified_type_name(prefix, target) else {
                    continue;
                };
                let trait_name = trait_name
                    .as_deref()
                    .map(|name| qualify_symbol(prefix, name));
                for method in definitions {
                    let info = MethodInfo {
                        function: *next_function_id,
                        receiver: method.parameters.first().and_then(receiver_mode),
                        source: span.source,
                    };
                    *next_function_id += 1;
                    methods.insert(
                        method_key(&target_name, trait_name.as_deref(), &method.name),
                        info,
                    );
                    method_names
                        .entry(method.name.clone())
                        .and_modify(|value| *value = None)
                        .or_insert(Some(info));
                }
            }
            Stmt::Module {
                name,
                statements: Some(module_statements),
                ..
            } => {
                prefix.push(name.clone());
                collect_method_symbols(
                    module_statements,
                    prefix,
                    next_function_id,
                    methods,
                    method_names,
                );
                prefix.pop();
            }
            _ => {}
        }
    }
}

pub(super) fn collect_method_declarations<'a>(
    statements: &'a [Stmt],
    prefix: &mut Vec<String>,
    methods: &HashMap<String, MethodInfo>,
    declarations: &mut Vec<(FunctionId, FunctionDeclaration<'a>)>,
) {
    for statement in statements {
        match unwrapped_statement(statement) {
            Stmt::Impl {
                target,
                trait_name,
                methods: definitions,
                ..
            } => {
                let Some(target_name) = qualified_type_name(prefix, target) else {
                    continue;
                };
                let trait_name = trait_name
                    .as_deref()
                    .map(|name| qualify_symbol(prefix, name));
                for method in definitions {
                    let key = method_key(&target_name, trait_name.as_deref(), &method.name);
                    let info = methods[&key];
                    declarations.push((
                        info.function,
                        FunctionDeclaration {
                            name: &method.name,
                            name_span: method.name_span,
                            qualified_name: qualified_name(prefix, &method.name),
                            parameters: &method.parameters,
                            body: &method.body,
                            span: method.span,
                            exported: false,
                            self_type: Some(target_name.clone()),
                        },
                    ));
                }
            }
            Stmt::Module {
                name,
                statements: Some(module_statements),
                ..
            } => {
                prefix.push(name.clone());
                collect_method_declarations(module_statements, prefix, methods, declarations);
                prefix.pop();
            }
            _ => {}
        }
    }
}

pub(super) fn qualified_type_name(prefix: &[String], ty: &Type) -> Option<String> {
    match ty {
        Type::Named { name, .. } => Some(qualify_symbol(prefix, name)),
        _ => None,
    }
}

pub(super) fn qualify_symbol(prefix: &[String], name: &str) -> String {
    if prefix.is_empty() || name.contains("::") {
        name.to_string()
    } else {
        qualified_name(prefix, name)
    }
}

pub(super) fn qualified_name(prefix: &[String], name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", prefix.join("::"))
    }
}

pub(super) fn nominal_type_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Named { name, .. } => Some(name),
        _ => None,
    }
}

pub(super) fn receiver_mode(parameter: &Parameter) -> Option<ReceiverMode> {
    if parameter.name != "self" {
        return None;
    }
    Some(match parameter.type_annotation.as_ref() {
        Some(Type::Reference { mutable, .. }) => ReceiverMode::Reference { mutable: *mutable },
        _ => ReceiverMode::Owned,
    })
}

pub(super) fn method_key(target: &str, trait_name: Option<&str>, method: &str) -> String {
    match trait_name {
        Some(trait_name) => format!("<{target} as {trait_name}>::{method}"),
        None => format!("{target}::{method}"),
    }
}

pub(super) fn iterator_methods(
    methods: &HashMap<String, MethodInfo>,
) -> HashMap<String, HirIteratorMethods> {
    let mut iterators = HashMap::<String, HirIteratorMethods>::new();
    for (key, method) in methods {
        let Some(inner) = key.strip_prefix('<') else {
            continue;
        };
        let Some((implementation, method_name)) = inner.rsplit_once(">::") else {
            continue;
        };
        let Some((target, trait_name)) = implementation.rsplit_once(" as ") else {
            continue;
        };
        let trait_name = trait_name.rsplit("::").next().unwrap_or(trait_name);
        let entry = iterators.entry(target.to_string()).or_default();
        match (trait_name, method_name) {
            ("Iterator", "next") => entry.next = Some(method.function),
            ("IntoIterator", "into_iter") => entry.into_iter = Some(method.function),
            _ => {}
        }
    }
    iterators.retain(|_, methods| methods.next.is_some() || methods.into_iter.is_some());
    iterators
}

pub(super) fn trait_implementations(
    methods: &HashMap<String, MethodInfo>,
) -> Vec<HirTraitImplementation> {
    let mut implementations =
        HashMap::<(String, String), (crate::source::SourceId, HashMap<String, FunctionId>)>::new();
    for (key, method) in methods {
        let Some(inner) = key.strip_prefix('<') else {
            continue;
        };
        let Some((implementation, method_name)) = inner.rsplit_once(">::") else {
            continue;
        };
        let Some((target, trait_name)) = implementation.rsplit_once(" as ") else {
            continue;
        };
        implementations
            .entry((target.to_string(), trait_name.to_string()))
            .or_insert_with(|| (method.source, HashMap::new()))
            .1
            .insert(method_name.to_string(), method.function);
    }
    let mut implementations = implementations
        .into_iter()
        .map(
            |((target, trait_name), (source, methods))| HirTraitImplementation {
                target,
                trait_name,
                source,
                methods,
            },
        )
        .collect::<Vec<_>>();
    implementations.sort_by(|left, right| {
        (&left.trait_name, &left.target).cmp(&(&right.trait_name, &right.target))
    });
    implementations
}

pub(super) fn is_compile_time_declaration(statement: &Stmt) -> bool {
    let statement = match statement {
        Stmt::Public { statement, .. } => statement.as_ref(),
        statement => statement,
    };
    matches!(
        statement,
        Stmt::Function { .. }
            | Stmt::Struct { .. }
            | Stmt::Enum { .. }
            | Stmt::Impl { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Trait { .. }
            | Stmt::Module { .. }
            | Stmt::Use { .. }
    )
}
