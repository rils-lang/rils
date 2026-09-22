//! Recursive nominal type checks shared by the analyzer and compiler.
use std::collections::{HashMap, HashSet};

use rils_syntax::{
    ast::{NamedField, Program, Stmt},
    types::Type,
};

use crate::analysis::AnalysisDiagnostic;

/// Reject inline recursive structs. Heap-backed handles and containers have a
/// fixed-size representation, so they terminate the inline size calculation.
pub(crate) fn analyze(program: &Program) -> Vec<AnalysisDiagnostic> {
    let mut fields = HashMap::<String, Vec<&NamedField>>::new();
    collect_structs(&program.statements, &mut fields);
    let mut diagnostics = Vec::new();
    for (name, declarations) in &fields {
        for field in declarations {
            if reaches_inline_cycle(&field.type_annotation, name, &fields, &mut HashSet::new()) {
                diagnostics.push(AnalysisDiagnostic::error(
                    format!("recursive field `{}` has infinite inline size; use `Box<T>` or another heap-backed container for indirection", field.name),
                    field.span,
                ));
            }
        }
    }
    diagnostics
}

fn collect_structs<'a>(statements: &'a [Stmt], fields: &mut HashMap<String, Vec<&'a NamedField>>) {
    for statement in statements {
        match statement {
            Stmt::Struct {
                name,
                fields: members,
                ..
            } => {
                fields.insert(name.clone(), members.iter().collect());
            }
            Stmt::Module {
                statements: Some(children),
                ..
            } => collect_structs(children, fields),
            _ => {}
        }
    }
}

fn reaches_inline_cycle(
    ty: &Type,
    root: &str,
    fields: &HashMap<String, Vec<&NamedField>>,
    visiting: &mut HashSet<String>,
) -> bool {
    match ty {
        Type::Named { name, .. } if is_heap_indirected(name) => false,
        Type::Named { name, .. } => {
            if name == root || !visiting.insert(name.clone()) {
                return name == root;
            }
            let result = fields.get(name).is_some_and(|members| {
                members.iter().any(|field| {
                    reaches_inline_cycle(&field.type_annotation, root, fields, visiting)
                })
            });
            visiting.remove(name);
            result
        }
        Type::Option(inner) => reaches_inline_cycle(inner, root, fields, visiting),
        // References have a fixed-size pointer representation. Direct
        // reference fields are checked separately by the ownership rules.
        Type::Reference { .. } => false,
        Type::Result(ok, error) => {
            reaches_inline_cycle(ok, root, fields, visiting)
                || reaches_inline_cycle(error, root, fields, visiting)
        }
        Type::Array { element, .. } => reaches_inline_cycle(element, root, fields, visiting),
        Type::Tuple(elements) => elements
            .iter()
            .any(|element| reaches_inline_cycle(element, root, fields, visiting)),
        _ => false,
    }
}

fn is_heap_indirected(name: &str) -> bool {
    let name = name.rsplit("::").next().unwrap_or(name);
    matches!(
        name,
        "Box" | "Rc" | "Weak" | "RefCell" | "Vec" | "HashMap" | "HashSet" | "SequenceIterator"
    )
}

#[cfg(test)]
mod tests {
    use super::super::analysis;

    #[test]
    fn rejects_inline_recursive_structs() {
        let result = analysis::analyze("struct Node { next: Option<Node> }");
        assert!(
            result
                .unwrap()
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("infinite inline size"))
        );
    }

    #[test]
    fn accepts_box_indirected_recursive_structs() {
        let result = analysis::analyze(
            "struct Node { next: Option<Box<Node>> }\nlet value: Node = Node { next: None };",
        );
        assert!(result.unwrap().diagnostics.is_empty());
    }

    #[test]
    fn accepts_vec_indirected_recursive_structs() {
        let result = analysis::analyze("struct Node { children: Vec<Node> }");
        assert!(result.unwrap().diagnostics.is_empty());
    }

    #[test]
    fn accepts_reference_counted_indirected_recursive_structs() {
        for source in [
            "struct Node { next: Option<Rc<Node>> }",
            "struct Node { next: Option<Weak<Node>> }",
        ] {
            let result = analysis::analyze(source);
            assert!(result.unwrap().diagnostics.is_empty(), "{source}");
        }
    }

    #[test]
    fn accepts_map_indirected_recursive_structs() {
        let result = analysis::analyze("struct Node { children: HashMap<string, Node> }");
        assert!(result.unwrap().diagnostics.is_empty());
    }

    #[test]
    fn still_rejects_inline_array_recursive_structs() {
        let result = analysis::analyze("struct Node { children: [Node; 1] }");
        assert!(
            result
                .unwrap()
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("infinite inline size"))
        );
    }
}
