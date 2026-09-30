use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use rils_syntax::ast::Stmt;
use syn::{Error, Ident, LitStr, Token, parse::Parse, parse_macro_input};

struct Input {
    directory: LitStr,
    visibility: syn::Visibility,
    builtins: Ident,
    modules: Ident,
    sources: Ident,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
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

    let mut declarations = Vec::new();
    let mut declaration_items = Vec::new();
    let mut module_members = BTreeMap::<String, BTreeSet<String>>::new();
    let mut tracked_sources = Vec::new();
    let mut source_entries = Vec::new();

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
        if !is_catalog(&file.program.statements) {
            return Err(Error::new(
                input.directory.span(),
                format!(
                    "`{}` must contain only module and function declarations",
                    file.relative
                ),
            ));
        }
        {
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
            let export_module = if stem == "prelude" {
                &source_module
            } else {
                &prefix
            };
            collect_catalog_exports(&file.program.statements, export_module, &mut module_members);
        }
    }

    declaration_items.push(quote!(crate::native_definitions::option::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::result::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::string::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::clone::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::copy::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::default::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::eq::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::hash::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::bit_flags::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::range::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::vec_deque::DECLARATION));
    declaration_items.push(quote!(
        crate::native_definitions::vec_deque_into_iter::DECLARATION
    ));
    declaration_items.push(quote!(crate::native_definitions::binary_heap::DECLARATION));
    declaration_items.push(quote!(
        crate::native_definitions::binary_heap_into_iter::DECLARATION
    ));
    declaration_items.push(quote!(crate::native_definitions::btree_map::DECLARATION));
    declaration_items.push(quote!(
        crate::native_definitions::btree_map_into_iter::DECLARATION
    ));
    declaration_items.push(quote!(crate::native_definitions::btree_set::DECLARATION));
    declaration_items.push(quote!(
        crate::native_definitions::btree_set_into_iter::DECLARATION
    ));
    declaration_items.push(quote!(crate::native_definitions::hash_map::DECLARATION));
    declaration_items.push(quote!(
        crate::native_definitions::hash_map_into_iter::DECLARATION
    ));
    declaration_items.push(quote!(crate::native_definitions::hash_set::DECLARATION));
    declaration_items.push(quote!(
        crate::native_definitions::hash_set_into_iter::DECLARATION
    ));
    declaration_items.push(quote!(crate::native_definitions::vec::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::iter::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::iterator::DECLARATION));
    declaration_items.push(quote!(
        crate::native_definitions::into_iterator::DECLARATION
    ));
    declaration_items.push(quote!(crate::native_definitions::function::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::function_mut::DECLARATION));
    declaration_items.push(quote!(
        crate::native_definitions::function_once::DECLARATION
    ));
    declaration_items.push(quote!(crate::native_definitions::format_error::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::debug::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::display::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::formatter::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::boxed::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::io_error::DECLARATION));
    declaration_items.push(quote!(
        crate::native_definitions::io_error_kind::DECLARATION
    ));
    declaration_items.push(quote!(crate::native_definitions::rc::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::weak::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::cell::DECLARATION));
    declaration_items.push(quote!(crate::native_definitions::ref_cell::DECLARATION));
    declaration_items.extend(
        (0usize..8).map(|index| quote!(crate::native_definitions::fs::DECLARATIONS[#index])),
    );
    declaration_items.extend(
        (0usize..3).map(
            |index| quote!(crate::native_definitions::callable_functions::DECLARATIONS[#index]),
        ),
    );
    declaration_items.extend(
        (0usize..6)
            .map(|index| quote!(crate::native_definitions::io_functions::DECLARATIONS[#index])),
    );

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
    let relative = value
        .strip_prefix("stdlib/")
        .unwrap_or(&value)
        .trim_end_matches(".rils");
    let segments = relative.split('/').collect::<Vec<_>>();
    if segments.len() > 2 {
        segments[..segments.len() - 1].join("::")
    } else {
        relative.replace('/', "::")
    }
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

fn is_catalog(statements: &[Stmt]) -> bool {
    statements
        .iter()
        .all(|statement| matches!(statement, Stmt::Module { .. } | Stmt::Function { .. }))
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
        .filter(|statement| matches!(statement, Stmt::Module { .. } | Stmt::Function { .. }))
        .count()
}

fn collect_module_tree(
    statements: &[Stmt],
    parent: &str,
    modules: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for statement in statements {
        match statement {
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
        if let Stmt::Function { name, .. } = statement {
            modules
                .entry(prefix.to_owned())
                .or_default()
                .insert(name.clone());
        }
    }
}
