//! Source type access remains separate from internal declaration identities.

use super::*;

impl Checker<'_> {
    pub(super) fn check_type_path_visibility(&mut self, name: &str, span: Span) {
        if let Some(path) = self
            .declaration_types
            .inaccessible_type_path(name, &self.module_path)
        {
            self.diagnostic(format!("type path `{path}` is private"), span);
        }
    }

    fn check_type_visibility(&mut self, ty: &Type, span: Span) {
        if let Some(path) = self
            .declaration_types
            .inaccessible_type(ty, &self.module_path)
        {
            self.diagnostic(format!("type path `{path}` is private"), span);
        }
    }

    pub(super) fn check_statement_type_visibility(&mut self, statement: &Stmt) {
        let mut types = Vec::new();
        match statement {
            Stmt::Let {
                type_annotation: Some(ty),
                span,
                ..
            }
            | Stmt::TypeAlias {
                target: ty, span, ..
            } => types.push((ty, *span)),
            Stmt::Function {
                parameters,
                return_type,
                span,
                ..
            } => {
                types.extend(parameters.iter().filter_map(|parameter| {
                    parameter
                        .type_annotation
                        .as_ref()
                        .map(|ty| (ty, parameter.span))
                }));
                types.extend(return_type.iter().map(|ty| (ty, *span)));
            }
            Stmt::Struct { fields, .. } => types.extend(
                fields
                    .iter()
                    .map(|field| (&field.type_annotation, field.span)),
            ),
            Stmt::Enum { variants, .. } => {
                for variant in variants {
                    match variant {
                        crate::ast::EnumVariant::Tuple { fields, span, .. } => {
                            types.extend(fields.iter().map(|ty| (ty, *span)))
                        }
                        crate::ast::EnumVariant::Record { fields, .. } => types.extend(
                            fields
                                .iter()
                                .map(|field| (&field.type_annotation, field.span)),
                        ),
                        _ => {}
                    }
                }
            }
            Stmt::Impl {
                target,
                methods,
                associated_types,
                span,
                ..
            } => {
                types.push((target, *span));
                for method in methods {
                    types.extend(method.parameters.iter().filter_map(|parameter| {
                        parameter
                            .type_annotation
                            .as_ref()
                            .map(|ty| (ty, parameter.span))
                    }));
                    types.extend(method.return_type.iter().map(|ty| (ty, method.span)));
                }
                types.extend(
                    associated_types
                        .iter()
                        .filter_map(|item| item.value.as_ref().map(|ty| (ty, item.span))),
                );
            }
            Stmt::Trait {
                methods,
                associated_types,
                ..
            } => {
                for method in methods {
                    types.extend(method.parameters.iter().filter_map(|parameter| {
                        parameter
                            .type_annotation
                            .as_ref()
                            .map(|ty| (ty, parameter.span))
                    }));
                    types.extend(method.return_type.iter().map(|ty| (ty, method.span)));
                }
                types.extend(
                    associated_types
                        .iter()
                        .filter_map(|item| item.value.as_ref().map(|ty| (ty, item.span))),
                );
            }
            _ => {}
        }
        for (ty, span) in types {
            self.check_type_visibility(ty, span);
        }
    }
}
