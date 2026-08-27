#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypePattern {
    SelfType,
    Generic(&'static str),
    AnyInteger,
    Unknown,
    Unit,
    Bool,
    Char,
    String,
    F32,
    F64,
    U32,
    U8,
    Usize,
    Named {
        path: &'static str,
        arguments: &'static [TypePattern],
    },
    Option(&'static TypePattern),
    Result {
        ok: &'static TypePattern,
        error: &'static TypePattern,
    },
    Tuple(&'static [TypePattern]),
    Function {
        parameters: &'static [TypePattern],
        result: &'static TypePattern,
    },
    Reference {
        mutable: bool,
        inner: &'static TypePattern,
    },
}

const UNKNOWN: TypePattern = rils_builtins_macros::type_pattern!(_);
const CALLBACK: TypePattern = rils_builtins_macros::type_pattern!(fn(&mut T, usize) -> Option<U>);
const IO_RESULT: TypePattern =
    rils_builtins_macros::type_pattern!(Result<Vec<string>, std::io::Error>);
const ITERATOR: TypePattern = rils_builtins_macros::type_pattern!(Iterator<(usize, T)>);

#[test]
fn type_pattern_macro_covers_nested_rust_style_types() {
    assert_eq!(UNKNOWN, TypePattern::Unknown);
    assert_eq!(
        CALLBACK,
        TypePattern::Function {
            parameters: &[
                TypePattern::Reference {
                    mutable: true,
                    inner: &TypePattern::Generic("T"),
                },
                TypePattern::Usize,
            ],
            result: &TypePattern::Option(&TypePattern::Generic("U")),
        }
    );
    assert_eq!(
        IO_RESULT,
        TypePattern::Result {
            ok: &TypePattern::Named {
                path: "Vec",
                arguments: &[TypePattern::String],
            },
            error: &TypePattern::Named {
                path: "std::io::Error",
                arguments: &[],
            },
        }
    );
    assert_eq!(
        ITERATOR,
        TypePattern::Named {
            path: "SequenceIterator",
            arguments: &[TypePattern::Tuple(&[
                TypePattern::Usize,
                TypePattern::Generic("T"),
            ])],
        }
    );
}

#[test]
fn type_pattern_macro_maps_scalar_and_special_types() {
    assert_eq!(
        rils_builtins_macros::type_pattern!(Self),
        TypePattern::SelfType
    );
    assert_eq!(
        rils_builtins_macros::type_pattern!(integer),
        TypePattern::AnyInteger
    );
    assert_eq!(rils_builtins_macros::type_pattern!(()), TypePattern::Unit);
    assert_eq!(rils_builtins_macros::type_pattern!(bool), TypePattern::Bool);
    assert_eq!(rils_builtins_macros::type_pattern!(char), TypePattern::Char);
    assert_eq!(rils_builtins_macros::type_pattern!(f32), TypePattern::F32);
    assert_eq!(rils_builtins_macros::type_pattern!(f64), TypePattern::F64);
    assert_eq!(rils_builtins_macros::type_pattern!(u32), TypePattern::U32);
    assert_eq!(rils_builtins_macros::type_pattern!(u8), TypePattern::U8);
}
