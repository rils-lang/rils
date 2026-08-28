use std::collections::{BTreeMap, HashMap};

use rils_host::HostContract;

use crate::{
    FrontendError, SourceId, Span, Type,
    analysis::{
        DocumentAnalysis, ExternalModuleExport, analyze_program_with_host_declarations,
        analyze_program_with_source_id_and_external_exports_and_host_types,
    },
    ast::{EnumVariant, Program, Stmt},
};

pub fn analyze_with_host(
    source: &str,
    host: &HostContract,
) -> Result<DocumentAnalysis, FrontendError> {
    let tokens = crate::lexer::lex(source).map_err(FrontendError::Lex)?;
    let mut program = crate::parser::parse(tokens).map_err(FrontendError::Parse)?;
    inject_host_enum_declarations(&mut program, host);
    let signatures = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    Ok(analyze_program_with_host_declarations(
        &program,
        &signatures,
        &host_types,
    ))
}

pub fn analyze_with_host_and_source_id_and_external_exports(
    source: &str,
    source_id: SourceId,
    host: &HostContract,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
) -> Result<DocumentAnalysis, FrontendError> {
    let tokens = crate::lexer::lex_with_source_id(source, source_id).map_err(FrontendError::Lex)?;
    let program = crate::parser::parse(tokens).map_err(FrontendError::Parse)?;
    Ok(
        analyze_program_with_host_and_source_id_and_external_exports(
            &program,
            source_id,
            host,
            external_exports,
        ),
    )
}

pub fn analyze_program_with_host_and_source_id_and_external_exports(
    program: &Program,
    source_id: SourceId,
    host: &HostContract,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
) -> DocumentAnalysis {
    let mut program = program.clone();
    inject_host_enum_declarations(&mut program, host);
    let signatures = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    analyze_program_with_source_id_and_external_exports_and_host_types(
        &program,
        source_id,
        &signatures,
        &host_types,
        external_exports,
    )
}

#[doc(hidden)]
pub fn inject_host_enum_declarations(program: &mut Program, host: &HostContract) {
    let mut root = HostEnumModule::default();
    let mut flag_types = Vec::new();
    for declaration in host.types() {
        let Some(definition) = declaration.enum_definition.as_ref() else {
            continue;
        };
        let mut segments = declaration.name.split("::").collect::<Vec<_>>();
        let Some(name) = segments.pop() else {
            continue;
        };
        let mut module = &mut root;
        for segment in segments {
            module = module.children.entry(segment.to_owned()).or_default();
        }
        module.enums.push((
            name.to_owned(),
            definition.variants.keys().cloned().collect(),
        ));
        if definition.flags {
            flag_types.push(declaration.name.clone());
        }
    }
    let mut declarations = host_enum_module_statements(root);
    declarations.extend(flag_types.into_iter().map(|name| Stmt::Impl {
        generic_parameters: Vec::new(),
        trait_name: Some("BitFlags".into()),
        target: Type::named(name),
        associated_types: Vec::new(),
        methods: Vec::new(),
        span: Span::default(),
    }));
    program.statements.splice(0..0, declarations);
}

#[derive(Default)]
struct HostEnumModule {
    enums: Vec<(String, Vec<String>)>,
    children: BTreeMap<String, HostEnumModule>,
}

fn host_enum_module_statements(module: HostEnumModule) -> Vec<Stmt> {
    let mut statements = module
        .enums
        .into_iter()
        .map(|(name, variants)| Stmt::Public {
            statement: Box::new(Stmt::Enum {
                attributes: Vec::new(),
                name: name.clone(),
                name_span: Span::default(),
                generic_parameters: Vec::new(),
                variants: variants
                    .into_iter()
                    .map(|name| EnumVariant::Unit {
                        name,
                        span: Span::default(),
                    })
                    .collect(),
                span: Span::default(),
            }),
            span: Span::default(),
        })
        .collect::<Vec<_>>();
    statements.extend(
        module
            .children
            .into_iter()
            .map(|(name, child)| Stmt::Public {
                statement: Box::new(Stmt::Module {
                    name: name.clone(),
                    name_span: Span::default(),
                    statements: Some(host_enum_module_statements(child)),
                    span: Span::default(),
                }),
                span: Span::default(),
            }),
    );
    statements
}
