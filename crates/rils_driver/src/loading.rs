use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use rils_frontend::{
    Span,
    ast::{Program, Stmt},
    macros::NativeMacroDefinition,
};
use rils_project::{Project, ProjectError};

use crate::{DriverError, ProjectSources};

pub fn discover_entry_project(path: &Path) -> Result<Project, ProjectError> {
    Project::discover_configured(path, None)?
        .map(Ok)
        .unwrap_or_else(|| Project::for_legacy_entry(path))
}

fn load_external_modules(
    statements: &mut [Stmt],
    base: &Path,
    native_macros: &[NativeMacroDefinition],
    loading: &mut HashSet<PathBuf>,
    sources: &mut ProjectSources,
) -> Result<(), DriverError> {
    for statement in statements {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_mut(),
            statement => statement,
        };
        let Stmt::Module {
            name, statements, ..
        } = statement
        else {
            continue;
        };
        if let Some(statements) = statements {
            load_external_modules(statements, base, native_macros, loading, sources)?;
            continue;
        }

        let flat = base.join(format!("{name}.rils"));
        let nested = base.join(name.as_str()).join("mod.rils");
        let path = if flat.is_file() {
            flat
        } else if nested.is_file() {
            nested
        } else {
            return Err(message(format!(
                "cannot find module `{name}`; expected `{}` or `{}`",
                flat.display(),
                nested.display()
            )));
        };
        let canonical = path
            .canonicalize()
            .map_err(|error| load_error(&path, error))?;
        if !loading.insert(canonical.clone()) {
            return Err(message(format!(
                "cyclic module load detected at `{}`",
                path.display()
            )));
        }
        let source = fs::read_to_string(&path).map_err(|error| load_error(&path, error))?;
        let source_id = sources.register_source(&path, &source);
        let mut module = sources.parse(source_id, native_macros)?;
        load_external_modules(
            &mut module.statements,
            path.parent().unwrap_or(base),
            native_macros,
            loading,
            sources,
        )?;
        loading.remove(&canonical);
        *statements = Some(module.statements);
    }
    Ok(())
}

pub fn load_file_modules(
    program: &mut Program,
    entry_path: &Path,
    project: &Project,
    native_macros: &[NativeMacroDefinition],
    sources: &mut ProjectSources,
    require_entry: bool,
) -> Result<(), DriverError> {
    if project.manifest_path().is_none() {
        let base = entry_path.parent().unwrap_or_else(|| Path::new("."));
        let mut loading = HashSet::new();
        if let Ok(canonical) = entry_path.canonicalize() {
            loading.insert(canonical);
        }
        load_external_modules(
            &mut program.statements,
            base,
            native_macros,
            &mut loading,
            sources,
        )?;
        sources.push_root_program(program.clone());
        return Ok(());
    }

    let entry = project.module_for_file(entry_path);
    let entry_source = sources.source_id(entry_path);
    let entry_is_prelude = project.prelude().is_some_and(|prelude_path| {
        prelude_path == entry_path
            || entry_path.canonicalize().is_ok_and(|entry_path| {
                prelude_path
                    .canonicalize()
                    .is_ok_and(|path| path == entry_path)
            })
    });
    if entry.is_none() && !entry_is_prelude {
        return Err(message(format!(
            "entry script `{}` is outside the src roots configured by `{}`",
            entry_path.display(),
            project.manifest_path().unwrap().display()
        )));
    }
    let entry_statements = if require_entry {
        prepare_project_entry(std::mem::take(&mut program.statements))?
    } else {
        reject_external_module_declarations(&program.statements)?;
        std::mem::take(&mut program.statements)
    };
    let mut entry_program = program.clone();
    entry_program.statements = entry_statements;
    if entry_is_prelude {
        sources.push_root_program(entry_program.clone());
    } else if let Some(prelude_path) = project.prelude() {
        let source =
            fs::read_to_string(prelude_path).map_err(|error| load_error(prelude_path, error))?;
        let source_id = sources.register_source(prelude_path, &source);
        let prelude = sources.parse(source_id, native_macros)?;
        reject_external_module_declarations(&prelude.statements)?;
        sources.push_root_program(prelude);
    }
    for dependency in project.dependencies() {
        let Some(prelude_path) = dependency.prelude.as_deref() else {
            continue;
        };
        let source =
            fs::read_to_string(prelude_path).map_err(|error| load_error(prelude_path, error))?;
        let source_id = sources.register_source(prelude_path, &source);
        let prelude = sources.parse(source_id, native_macros)?;
        reject_external_module_declarations(&prelude.statements)?;
        sources.push_root_program(prelude);
    }
    for file in project.modules() {
        let file_source = sources
            .source_id(&file.path)
            .expect("project modules were registered before loading");
        let module_program = if entry_source == Some(file_source) {
            entry_program.clone()
        } else {
            let source =
                fs::read_to_string(&file.path).map_err(|error| load_error(&file.path, error))?;
            let source_id = sources.register_source(&file.path, &source);
            let program = sources.parse(source_id, native_macros)?;
            reject_external_module_declarations(&program.statements)?;
            program
        };
        sources.set_module_program(file_source, module_program);
    }
    Ok(())
}

fn prepare_project_entry(statements: Vec<Stmt>) -> Result<Vec<Stmt>, DriverError> {
    reject_external_module_declarations(&statements)?;
    let mut found = false;
    let mut prepared = Vec::with_capacity(statements.len());
    for statement in statements {
        match statement {
            Stmt::Function {
                ref name,
                ref parameters,
                span,
                ..
            } if name == "main" => {
                if found {
                    return Err(DriverError::message(
                        "project entry contains more than one `fn main()`",
                        span,
                    ));
                }
                if !parameters.is_empty() {
                    return Err(DriverError::message(
                        "project entry `fn main()` must not have parameters",
                        span,
                    ));
                }
                found = true;
                prepared.push(Stmt::Public {
                    statement: Box::new(statement),
                    span,
                });
            }
            Stmt::Public { statement, span } => {
                if let Stmt::Function {
                    name,
                    parameters,
                    span: function_span,
                    ..
                } = statement.as_ref()
                    && name == "main"
                {
                    if found {
                        return Err(DriverError::message(
                            "project entry contains more than one `fn main()`",
                            *function_span,
                        ));
                    }
                    if !parameters.is_empty() {
                        return Err(DriverError::message(
                            "project entry `fn main()` must not have parameters",
                            *function_span,
                        ));
                    }
                    found = true;
                }
                prepared.push(Stmt::Public { statement, span });
            }
            statement => prepared.push(statement),
        }
    }
    if !found {
        return Err(message(
            "a rils.toml project entry must define a zero-parameter `fn main()`".into(),
        ));
    }
    Ok(prepared)
}

fn reject_external_module_declarations(statements: &[Stmt]) -> Result<(), DriverError> {
    for statement in statements {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        if let Stmt::Module {
            name,
            statements: None,
            span,
            ..
        } = statement
        {
            return Err(DriverError::message(
                format!(
                    "external `mod {name};` declarations are not used in rils.toml projects; reference the module with `use` or a qualified path"
                ),
                *span,
            ));
        }
        if let Stmt::Module {
            statements: Some(statements),
            ..
        } = statement
        {
            reject_external_module_declarations(statements)?;
        }
    }
    Ok(())
}

fn load_error(path: &Path, error: impl std::fmt::Display) -> DriverError {
    message(format!(
        "failed to load module `{}`: {error}",
        path.display()
    ))
}

fn message(message: String) -> DriverError {
    DriverError::message(message, Span::default())
}
