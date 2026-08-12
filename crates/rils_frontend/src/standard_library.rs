use crate::types::{FunctionSignature, Type};

pub fn standard_function_signature(name: &str) -> Option<FunctionSignature> {
    if let Some(declaration) = rils_builtins::builtin_function(name) {
        let signature = declaration
            .signature
            .expect("function declaration has a signature");
        let return_type = resolve_type_pattern(signature.result);
        return Some(
            if signature.variadic || matches!(name, "std::io::print" | "std::io::println") {
                FunctionSignature::variadic(return_type)
            } else {
                FunctionSignature::fixed(
                    signature
                        .parameters
                        .iter()
                        .copied()
                        .map(resolve_type_pattern)
                        .collect(),
                    return_type,
                )
            },
        );
    }
    let error = Type::named("std::io::Error");
    let result = |ok| Type::Result(Box::new(ok), Box::new(error.clone()));
    let signature = match name {
        "std::io::print" | "std::io::println" => FunctionSignature::variadic(Type::Unit),
        "std::io::read_line" => FunctionSignature::fixed(Vec::new(), result(Type::String)),
        "std::io::write" | "std::io::write_line" => {
            FunctionSignature::fixed(vec![Type::Unknown], result(Type::Unit))
        }
        "std::io::flush" => FunctionSignature::fixed(Vec::new(), result(Type::Unit)),
        "std::fs::read_to_string" => {
            FunctionSignature::fixed(vec![Type::String], result(Type::String))
        }
        "std::fs::write" | "std::fs::append" => {
            FunctionSignature::fixed(vec![Type::String, Type::String], result(Type::Unit))
        }
        "std::fs::try_exists" => FunctionSignature::fixed(vec![Type::String], result(Type::Bool)),
        "std::fs::create_dir_all" | "std::fs::remove_file" | "std::fs::remove_dir" => {
            FunctionSignature::fixed(vec![Type::String], result(Type::Unit))
        }
        "std::fs::read_dir" => FunctionSignature::fixed(
            vec![Type::String],
            result(Type::Named {
                name: "Vec".into(),
                arguments: vec![Type::String],
            }),
        ),
        _ => return None,
    };
    Some(signature)
}

fn resolve_type_pattern(pattern: rils_builtins::TypePattern) -> Type {
    use rils_builtins::TypePattern;
    match pattern {
        TypePattern::SelfType | TypePattern::AnyInteger | TypePattern::Unknown => Type::Unknown,
        TypePattern::Generic(name) => Type::Variable(name.into()),
        TypePattern::Unit => Type::Unit,
        TypePattern::Bool => Type::Bool,
        TypePattern::String => Type::String,
        TypePattern::F32 => Type::Float(crate::types::FloatType::F32),
        TypePattern::F64 => Type::Float(crate::types::FloatType::F64),
        TypePattern::Usize => Type::USIZE,
        TypePattern::Named { path, arguments } => Type::Named {
            name: path.into(),
            arguments: arguments
                .iter()
                .copied()
                .map(resolve_type_pattern)
                .collect(),
        },
        TypePattern::Option(inner) => Type::Option(Box::new(resolve_type_pattern(*inner))),
        TypePattern::Result { ok, error } => Type::Result(
            Box::new(resolve_type_pattern(*ok)),
            Box::new(resolve_type_pattern(*error)),
        ),
        TypePattern::Tuple(elements) => {
            Type::Tuple(elements.iter().copied().map(resolve_type_pattern).collect())
        }
        TypePattern::Function { parameters, result } => Type::function(
            parameters
                .iter()
                .copied()
                .map(resolve_type_pattern)
                .collect(),
            resolve_type_pattern(*result),
        ),
        TypePattern::Reference { mutable, inner } => Type::Reference {
            mutable,
            inner: Box::new(resolve_type_pattern(*inner)),
        },
    }
}
