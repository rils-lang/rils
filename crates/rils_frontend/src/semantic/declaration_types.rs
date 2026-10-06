//! Declaration types retain their defining module when used by later stages.

use std::collections::{HashMap, HashSet};

use crate::{
    Type,
    ast::{Program, Stmt, UseImportKind},
};

mod visibility;

/// Owned sums need both branch types even when only one branch is present.
/// Indexed aggregates carry these declarations recursively; references do not.
pub fn requires_storage_declaration(ty: &Type) -> bool {
    match ty {
        Type::Named { arguments, .. } => arguments
            .iter()
            .all(crate::standard_library::is_concrete_native_type),
        Type::Option(_) | Type::Result(_, _) => true,
        Type::Tuple(types) => types.iter().any(requires_storage_declaration),
        Type::Array { element, .. } | Type::ArrayParameter { element, .. } => {
            requires_storage_declaration(element)
        }
        _ => false,
    }
}

#[derive(Clone, Debug)]
struct Alias {
    parameters: Vec<String>,
    target: Type,
    module: Vec<String>,
}

/// Shared resolution of nominal declaration paths and transparent aliases.
/// Generic variables remain variables; this table never invents a layout.
#[derive(Clone, Debug, Default)]
pub struct DeclarationTypeResolver {
    copy_types: crate::copy_types::CopyTypes,
    types: HashSet<String>,
    exposed_types: HashMap<String, String>,
    aliases: HashMap<String, Alias>,
    imports: HashMap<String, Vec<(String, String)>>,
    globs: HashMap<String, Vec<String>>,
    private_paths: HashMap<String, Vec<String>>,
}

impl DeclarationTypeResolver {
    pub fn copy_types(&self) -> &crate::copy_types::CopyTypes {
        &self.copy_types
    }
    /// Host nominal types use the same canonical identities as source types.
    pub fn extend_host_contract(&mut self, host: &rils_host::HostContract) {
        self.types
            .extend(host.types().map(|declaration| declaration.name.clone()));
    }

    /// Whether a canonical path denotes a source type declaration.
    pub fn is_declared_type(&self, name: &str) -> bool {
        self.types.contains(name) || self.exposed_types.contains_key(name)
    }

    pub(crate) fn extend_exports(&mut self, exports: &crate::exports::ExportTable) {
        for (module, declarations) in exports {
            for export in declarations {
                if matches!(
                    export.kind,
                    crate::analysis::SymbolKind::Type | crate::analysis::SymbolKind::Trait
                ) {
                    let original = exports
                        .get(&export.module_path)
                        .and_then(|values| {
                            values.iter().find(|candidate| {
                                candidate.span == export.span && candidate.kind == export.kind
                            })
                        })
                        .unwrap_or(export);
                    let canonical = crate::exports::join_path(&export.module_path, &original.name);
                    self.types.insert(canonical.clone());
                    self.exposed_types
                        .insert(crate::exports::join_path(module, &export.name), canonical);
                }
            }
        }
    }

    pub fn from_programs<'a>(
        programs: impl IntoIterator<Item = (&'a [String], &'a Program)>,
    ) -> Self {
        let mut result = Self::default();
        let programs = programs.into_iter().collect::<Vec<_>>();
        let mut exports = crate::exports::ExportTable::new();
        for (module, program) in &programs {
            result.collect(&program.statements, module);
            crate::exports::collect_exports(program, module, None, true, &mut exports);
        }
        crate::exports::resolve_reexports(&mut exports, programs.iter().copied());
        result.extend_exports(&exports);
        result.copy_types = crate::copy_types::collect(&programs, &result);
        result
    }

    fn collect(&mut self, statements: &[Stmt], module: &[String]) {
        let namespace = module.join("::");
        for statement in statements {
            if statement
                .visibility()
                .is_some_and(|visibility| !visibility.is_public())
            {
                let name = match statement {
                    Stmt::Struct { name, .. }
                    | Stmt::Enum { name, .. }
                    | Stmt::Trait { name, .. }
                    | Stmt::TypeAlias { name, .. }
                    | Stmt::Module { name, .. } => Some(name),
                    _ => None,
                };
                if let Some(name) = name {
                    self.private_paths
                        .insert(crate::exports::join_path(&namespace, name), module.to_vec());
                }
            }
            match statement {
                Stmt::Struct { name, .. } | Stmt::Enum { name, .. } | Stmt::Trait { name, .. } => {
                    self.types
                        .insert(crate::exports::join_path(&namespace, name));
                }
                Stmt::TypeAlias {
                    name,
                    generic_parameters,
                    target,
                    ..
                } => {
                    let name = crate::exports::join_path(&namespace, name);
                    self.types.insert(name.clone());
                    self.aliases.insert(
                        name,
                        Alias {
                            parameters: generic_parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            target: target.clone(),
                            module: module.to_vec(),
                        },
                    );
                }
                Stmt::Use { imports, .. } => {
                    for import in imports {
                        let path = import.path.join("::");
                        match import.kind {
                            UseImportKind::Single => {
                                self.imports.entry(namespace.clone()).or_default().push((
                                    import.binding_name().expect("single import").to_owned(),
                                    path,
                                ))
                            }
                            UseImportKind::Glob => {
                                self.globs.entry(namespace.clone()).or_default().push(path)
                            }
                        }
                    }
                }
                Stmt::Module {
                    name,
                    statements: Some(children),
                    ..
                } => {
                    let mut child = module.to_vec();
                    child.push(name.clone());
                    self.collect(children, &child);
                }
                _ => {}
            }
        }
    }

    pub fn resolve(&self, ty: &Type, module: &[String]) -> Type {
        self.expand(ty, module, &mut HashSet::new())
    }

    pub(crate) fn display(&self, ty: &Type) -> String {
        self.display_text(&ty.to_string())
    }

    pub(crate) fn display_text(&self, text: &str) -> String {
        let mut output = String::new();
        let mut name = String::new();
        let flush = |name: &mut String, output: &mut String| {
            if self.types.contains(name) {
                output.push_str(name.rsplit("::").next().unwrap_or(name));
            } else {
                output.push_str(name);
            }
            name.clear();
        };
        for ch in text.chars() {
            if ch.is_alphanumeric() || matches!(ch, '_' | ':') {
                name.push(ch);
            } else {
                flush(&mut name, &mut output);
                output.push(ch);
            }
        }
        flush(&mut name, &mut output);
        output
    }

    fn path(
        &self,
        name: &str,
        module: &[String],
        visiting: &mut HashSet<String>,
    ) -> Option<String> {
        let segments = name.split("::").map(str::to_owned).collect::<Vec<_>>();
        let candidates = crate::exports::module_candidates(module, &segments);
        let anchored = matches!(
            segments.first().map(String::as_str),
            Some("crate" | "self" | "super")
        );
        for candidate in candidates.iter().take(1) {
            if let Some(canonical) = self.exposed_types.get(candidate) {
                return Some(canonical.clone());
            }
            if self.types.contains(candidate) {
                return Some(candidate.clone());
            }
        }
        if anchored {
            return None;
        }
        let key = format!("{}|{name}", module.join("::"));
        if !visiting.insert(key.clone()) {
            return None;
        }
        let mut candidates = HashSet::new();
        if let Some(imports) = self.imports.get(&module.join("::")) {
            for (binding, target) in imports {
                if segments.first() == Some(binding) {
                    let import_key = format!("import:{}:{binding}", module.join("::"));
                    if !visiting.insert(import_key.clone()) {
                        continue;
                    }
                    let suffix = segments
                        .iter()
                        .skip(1)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("::");
                    let target = if suffix.is_empty() {
                        target.clone()
                    } else {
                        format!("{target}::{suffix}")
                    };
                    if let Some(path) = self.path(&target, module, visiting) {
                        candidates.insert(path);
                    }
                    visiting.remove(&import_key);
                }
            }
        }
        if candidates.is_empty()
            && segments.len() == 1
            && let Some(globs) = self.globs.get(&module.join("::"))
        {
            for glob in globs {
                if let Some(path) = self.path(&format!("{glob}::{name}"), module, visiting) {
                    candidates.insert(path);
                }
            }
        }
        visiting.remove(&key);
        if candidates.len() == 1 {
            return candidates.into_iter().next();
        }
        if candidates.is_empty()
            && !module.is_empty()
            && !matches!(
                segments.first().map(String::as_str),
                Some("crate" | "self" | "super")
            )
        {
            return self.path(name, &module[..module.len() - 1], visiting);
        }
        None
    }

    fn expand(&self, ty: &Type, module: &[String], visiting: &mut HashSet<String>) -> Type {
        let child =
            |ty: &Type, visiting: &mut HashSet<String>| Box::new(self.expand(ty, module, visiting));
        let children = |types: &[Type], visiting: &mut HashSet<String>| {
            types
                .iter()
                .map(|ty| self.expand(ty, module, visiting))
                .collect()
        };
        match ty {
            Type::Named { name, arguments } => {
                let arguments: Vec<Type> = children(arguments, visiting);
                let name = self
                    .path(name, module, &mut HashSet::new())
                    .unwrap_or_else(|| name.clone());
                if let Some(alias) = self.aliases.get(&name)
                    && alias.parameters.len() == arguments.len()
                    && visiting.insert(name.clone())
                {
                    // Resolve the alias body at its declaration before substituting
                    // caller arguments, which already carry their own identities.
                    let target = self.expand(&alias.target, &alias.module, visiting);
                    let substitutions = alias.parameters.iter().cloned().zip(arguments).collect();
                    visiting.remove(&name);
                    return target.substitute(&substitutions);
                }
                Type::Named { name, arguments }
            }
            Type::Option(item) => Type::Option(child(item, visiting)),
            Type::Result(ok, error) => Type::Result(child(ok, visiting), child(error, visiting)),
            Type::Tuple(items) => Type::Tuple(children(items, visiting)),
            Type::Slice(item) => Type::Slice(child(item, visiting)),
            Type::Array { element, length } => Type::Array {
                element: child(element, visiting),
                length: *length,
            },
            Type::ArrayParameter { element, length } => Type::ArrayParameter {
                element: child(element, visiting),
                length: length.clone(),
            },
            Type::Reference { mutable, inner } => Type::Reference {
                mutable: *mutable,
                inner: child(inner, visiting),
            },
            Type::Function {
                parameters,
                return_type,
            } => Type::Function {
                parameters: parameters.as_ref().map(|types| children(types, visiting)),
                return_type: child(return_type, visiting),
            },
            Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => Type::Associated {
                base: child(base, visiting),
                trait_name: trait_name.clone(),
                name: name.clone(),
                arguments: children(arguments, visiting),
            },
            Type::BoundVariable { name, bounds } => Type::BoundVariable {
                name: name.clone(),
                bounds: children(bounds, visiting),
            },
            _ => ty.clone(),
        }
    }
}
