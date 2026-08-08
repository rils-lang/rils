use crate::types::{FunctionSignature, Type};

pub fn standard_function_signature(name: &str) -> Option<FunctionSignature> {
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
