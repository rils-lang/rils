use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use rils_syntax::ast::{Attribute, Stmt};
use syn::{Error, Ident, LitStr, Token, parse::Parse, parse_macro_input};

use crate::builtin_ids;

struct Input {
    config_path: LitStr,
    directory: LitStr,
    visibility: syn::Visibility,
    builtins: Ident,
    modules: Ident,
    sources: Ident,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let config_path = input.parse()?;
        input.parse::<Token![;]>()?;
        let directory = input.parse()?;
        input.parse::<Token![;]>()?;
        let visibility = input.parse()?;
        input.parse::<Token![const]>()?;
        let builtins = input.parse()?;
        input.parse::<Token![,]>()?;
        let modules = input.parse()?;
        input.parse::<Token![,]>()?;
        let sources = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(Self {
            config_path,
            directory,
            visibility,
            builtins,
            modules,
            sources,
        })
    }
}

struct SourceFile {
    relative: String,
    absolute: PathBuf,
    program: rils_syntax::ast::Program,
}

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);
    match expand_input(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand_input(input: Input) -> syn::Result<proc_macro2::TokenStream> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| Error::new(input.directory.span(), "CARGO_MANIFEST_DIR is unavailable"))?;
    let directory = manifest.join(input.directory.value());
    let mut paths = Vec::new();
    discover_rils_files(&directory, &mut paths).map_err(|error| {
        Error::new(
            input.directory.span(),
            format!("failed to discover `{}`: {error}", directory.display()),
        )
    })?;
    paths.sort();
    if paths.is_empty() {
        return Err(Error::new(
            input.directory.span(),
            "the built-in stdlib directory contains no .rils files",
        ));
    }

    let mut files = Vec::with_capacity(paths.len());
    for absolute in paths {
        let relative = absolute
            .strip_prefix(&manifest)
            .map_err(|_| Error::new(input.directory.span(), "stdlib file is outside its crate"))?
            .to_string_lossy()
            .replace('\\', "/");
        let source = fs::read_to_string(&absolute).map_err(|error| {
            Error::new(
                input.directory.span(),
                format!("failed to read `{}`: {error}", absolute.display()),
            )
        })?;
        let tokens = rils_syntax::lex(&source)
            .map_err(|error| Error::new(input.directory.span(), error.message))?;
        let program = rils_syntax::parser::parse_builtin_declarations(tokens)
            .map_err(|error| Error::new(input.directory.span(), error.message))?;
        files.push(SourceFile {
            relative,
            absolute,
            program,
        });
    }

    let (_, configured) = builtin_ids::load(&input.config_path.value())
        .map_err(|error| Error::new(input.config_path.span(), error))?;
    let mut declarations = Vec::new();
    let mut declaration_items = Vec::new();
    let mut module_members = BTreeMap::<String, BTreeSet<String>>::new();
    let mut tracked_sources = Vec::new();
    let mut source_entries = Vec::new();
    let config_path = input.config_path.clone();

    for (index, file) in files.iter().enumerate() {
        let absolute = LitStr::new(&file.absolute.to_string_lossy(), input.directory.span());
        tracked_sources.push(quote!(
            const _: &str = include_str!(#absolute);
        ));
        let path = Path::new(&file.relative);
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let relative_literal = LitStr::new(&file.relative, input.directory.span());
        let source_module = source_module(path);
        if stem == "integer" || stem == "float" {
            source_entries.push(source_entry(
                &file.relative,
                &source_module,
                quote!(Numeric),
                input.directory.span(),
            ));
            let family = if stem == "integer" {
                format_ident!("Integer")
            } else {
                format_ident!("Float")
            };
            let prefix = LitStr::new(&format!("core::{stem}"), input.directory.span());
            let intrinsics = format_ident!("{}_INTRINSICS", stem.to_ascii_uppercase());
            let constants = format_ident!("{}_CONSTANTS", stem.to_ascii_uppercase());
            declarations.push(quote! {
                rils_builtins_macros::builtin_numeric_file! {
                    #config_path;
                    #relative_literal;
                    complete #prefix;
                    family #family;
                    pub const #intrinsics, #constants;
                }
            });
            continue;
        }

        if stem == "modules" {
            collect_module_tree(&file.program.statements, "", &mut module_members);
            source_entries.push(source_entry(
                &file.relative,
                "",
                quote!(ModuleTree),
                input.directory.span(),
            ));
        }

        let name = format_ident!("__STDLIB_DECLARATIONS_{index}");
        if is_catalog(&file.program.statements) {
            if stem != "modules" {
                source_entries.push(source_entry(
                    &file.relative,
                    &source_module,
                    quote!(Catalog),
                    input.directory.span(),
                ));
            }
            let prefix = catalog_prefix(path);
            let backend = if prefix.starts_with("std::") {
                let capability = LitStr::new(&prefix, input.directory.span());
                quote!(Host(#capability))
            } else if stem == "prelude" {
                quote!(Runtime)
            } else {
                quote!(Metadata)
            };
            let prefix_literal = LitStr::new(&prefix, input.directory.span());
            declarations.push(quote! {
                rils_builtins_macros::builtin_catalog_file! {
                    #relative_literal;
                    prefix #prefix_literal;
                    backend #backend;
                    const #name;
                }
            });
            let count = catalog_declaration_count(&file.program.statements);
            declaration_items.extend((0..count).map(|item| quote!(#name[#item])));
            collect_catalog_exports(&file.program.statements, &prefix, &mut module_members);
        } else {
            source_entries.push(source_entry(
                &file.relative,
                &source_module,
                quote!(Type),
                input.directory.span(),
            ));
            let (prefix, declared_ids) = infer_builtin_prefix(file, &configured)?;
            let configured_ids = configured
                .keys()
                .filter(|path| direct_member(path, &prefix).is_some())
                .cloned()
                .collect::<BTreeSet<_>>();
            let completeness = if !configured_ids.is_empty() && configured_ids == declared_ids {
                quote!(complete)
            } else {
                quote!(partial)
            };
            let backend = if declared_ids.is_empty() {
                quote!(Metadata)
            } else {
                quote!(Runtime)
            };
            let kind = primary_declaration_name(&file.program.statements)
                .filter(|name| *name == "Array")
                .map(|_| quote!(kind Primitive;));
            let prefix_literal = LitStr::new(&prefix, input.directory.span());
            declarations.push(quote! {
                rils_builtins_macros::builtin_file! {
                    #config_path;
                    #relative_literal;
                    #completeness #prefix_literal;
                    #kind
                    backend #backend;
                    const #name;
                }
            });
            declaration_items.push(quote!(#name));
            collect_type_exports(file, &mut module_members);
        }
    }

    let module_entries = module_members.iter().map(|(path, members)| {
        let path = LitStr::new(path, input.directory.span());
        let members = members
            .iter()
            .map(|member| LitStr::new(member, input.directory.span()));
        quote!(BuiltinModule { path: #path, members: &[#(#members),*] })
    });
    let visibility = input.visibility;
    let builtins = input.builtins;
    let modules = input.modules;
    let sources = input.sources;
    Ok(quote! {
        #(#tracked_sources)*
        #(#declarations)*
        #visibility const #builtins: &[BuiltinDeclaration] = &[#(#declaration_items),*];
        #visibility const #modules: &[BuiltinModule] = &[#(#module_entries),*];
        #visibility const #sources: &[BuiltinSource] = &[#(#source_entries),*];
    })
}

fn source_module(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    value
        .strip_prefix("stdlib/")
        .unwrap_or(&value)
        .trim_end_matches(".rils")
        .replace('/', "::")
}

fn source_entry(
    path: &str,
    module: &str,
    kind: proc_macro2::TokenStream,
    span: proc_macro2::Span,
) -> proc_macro2::TokenStream {
    let path = LitStr::new(path, span);
    let module = LitStr::new(module, span);
    quote!(BuiltinSource {
        path: #path,
        module: #module,
        kind: BuiltinSourceKind::#kind,
    })
}

fn discover_rils_files(directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            discover_rils_files(&path, files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "rils")
        {
            files.push(path);
        }
    }
    Ok(())
}

fn public_inner(statement: &Stmt) -> &Stmt {
    match statement {
        Stmt::Public { statement, .. } => public_inner(statement),
        other => other,
    }
}

fn is_catalog(statements: &[Stmt]) -> bool {
    statements.iter().all(|statement| {
        matches!(
            public_inner(statement),
            Stmt::Module { .. } | Stmt::Function { .. }
        )
    })
}

fn catalog_prefix(path: &Path) -> String {
    let components = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let stdlib = components
        .iter()
        .position(|component| component == "stdlib");
    let Some(stdlib) = stdlib else {
        return String::new();
    };
    let relative = &components[stdlib + 1..];
    match relative {
        [file] if file == "prelude.rils" || file == "modules.rils" => String::new(),
        [module, file] => format!("{module}::{}", file.trim_end_matches(".rils")),
        _ => String::new(),
    }
}

fn catalog_declaration_count(statements: &[Stmt]) -> usize {
    statements
        .iter()
        .filter(|statement| {
            matches!(
                public_inner(statement),
                Stmt::Module { .. } | Stmt::Function { .. }
            )
        })
        .count()
}

fn infer_builtin_prefix(
    file: &SourceFile,
    configured: &builtin_ids::Members,
) -> syn::Result<(String, BTreeSet<String>)> {
    let mut explicit = BTreeSet::new();
    let mut method_names = Vec::new();
    for statement in &file.program.statements {
        match public_inner(statement) {
            Stmt::Impl { methods, .. } => {
                for method in methods {
                    collect_method_path(
                        &method.name,
                        &method.attributes,
                        &mut explicit,
                        &mut method_names,
                    )?;
                }
            }
            Stmt::Trait { methods, .. } => {
                for method in methods {
                    collect_method_path(
                        &method.name,
                        &method.attributes,
                        &mut explicit,
                        &mut method_names,
                    )?;
                }
            }
            _ => {}
        }
    }
    let mut scores = BTreeMap::<String, usize>::new();
    for name in &method_names {
        for path in configured.keys() {
            if path
                .rsplit_once("::")
                .is_some_and(|(_, member)| member == name)
            {
                let prefix = path.rsplit_once("::").unwrap().0;
                *scores.entry(prefix.to_owned()).or_default() += 1;
            }
        }
    }
    let fallback = file
        .relative
        .strip_prefix("stdlib/")
        .unwrap_or(&file.relative)
        .trim_end_matches(".rils")
        .replace('/', "::");
    let explicit_prefix = explicit
        .iter()
        .filter_map(|path| path.rsplit_once("::").map(|(prefix, _)| prefix))
        .next()
        .map(str::to_owned);
    let prefix = if scores.contains_key(&fallback) {
        fallback
    } else {
        scores
            .into_iter()
            .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
            .map(|(prefix, _)| prefix)
            .or(explicit_prefix)
            .unwrap_or(fallback)
    };
    let mut declared = explicit;
    for name in method_names {
        let path = format!("{prefix}::{name}");
        if configured.contains_key(&path) {
            declared.insert(path);
        }
    }
    Ok((prefix, declared))
}

fn collect_method_path(
    name: &str,
    attributes: &[Attribute],
    explicit: &mut BTreeSet<String>,
    defaults: &mut Vec<String>,
) -> syn::Result<()> {
    if let Some(attribute) = attributes
        .iter()
        .find(|attribute| attribute.path.as_slice() == ["runtime"])
    {
        let [path] = attribute.arguments.as_slice() else {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!("runtime attribute on `{name}` requires one path"),
            ));
        };
        explicit.insert(path.join("::"));
    } else if !attributes
        .iter()
        .any(|attribute| attribute.path.as_slice() == ["metadata"])
    {
        defaults.push(name.to_owned());
    }
    Ok(())
}

fn direct_member<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)?
        .strip_prefix("::")
        .filter(|name| !name.contains("::"))
}

fn primary_declaration_name(statements: &[Stmt]) -> Option<&str> {
    statements
        .iter()
        .find_map(|statement| match public_inner(statement) {
            Stmt::Enum { name, .. } | Stmt::Struct { name, .. } | Stmt::Trait { name, .. } => {
                Some(name.as_str())
            }
            Stmt::Impl {
                target: rils_syntax::Type::String,
                ..
            } => Some("string"),
            _ => None,
        })
}

fn collect_module_tree(
    statements: &[Stmt],
    parent: &str,
    modules: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for statement in statements {
        match public_inner(statement) {
            Stmt::Module {
                name, statements, ..
            } => {
                modules
                    .entry(parent.to_owned())
                    .or_default()
                    .insert(name.clone());
                let path = if parent.is_empty() {
                    name.clone()
                } else {
                    format!("{parent}::{name}")
                };
                if let Some(statements) = statements {
                    collect_module_tree(statements, &path, modules);
                }
            }
            Stmt::Use { imports, .. } => {
                for import in imports {
                    if let Some(name) = import.binding_name() {
                        modules
                            .entry(parent.to_owned())
                            .or_default()
                            .insert(name.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_catalog_exports(
    statements: &[Stmt],
    prefix: &str,
    modules: &mut BTreeMap<String, BTreeSet<String>>,
) {
    if prefix.is_empty() {
        return;
    }
    for statement in statements {
        if let Stmt::Function { name, .. } = public_inner(statement) {
            modules
                .entry(prefix.to_owned())
                .or_default()
                .insert(name.clone());
        }
    }
}

fn collect_type_exports(file: &SourceFile, modules: &mut BTreeMap<String, BTreeSet<String>>) {
    let module = file
        .relative
        .strip_prefix("stdlib/")
        .unwrap_or(&file.relative)
        .trim_end_matches(".rils")
        .replace('/', "::");
    for statement in &file.program.statements {
        match public_inner(statement) {
            Stmt::Enum { name, variants, .. } => {
                let members = modules.entry(module.clone()).or_default();
                members.insert(name.clone());
                for variant in variants {
                    members.insert(match variant {
                        rils_syntax::ast::EnumVariant::Unit { name, .. }
                        | rils_syntax::ast::EnumVariant::Tuple { name, .. }
                        | rils_syntax::ast::EnumVariant::Record { name, .. } => name.clone(),
                    });
                }
            }
            Stmt::Struct { name, .. } | Stmt::Trait { name, .. } => {
                modules
                    .entry(module.clone())
                    .or_default()
                    .insert(name.clone());
            }
            _ => {}
        }
    }
}
