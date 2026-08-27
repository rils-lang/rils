rils_builtins_macros::builtin_id_declarations!("tests/fixtures/builtin_ids.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypePattern {
    Generic(&'static str),
    Unit,
    Bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuiltinMemberKind {
    Variant,
    Method,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuiltinKind {
    Enum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuiltinBackend {
    Runtime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReceiverMode {
    Owned,
    Shared,
    Mutable,
}

#[derive(Clone, Copy, Debug)]
struct BuiltinSignature {
    parameters: &'static [TypePattern],
    result: TypePattern,
    variadic: bool,
}

#[derive(Clone, Copy, Debug)]
struct BuiltinMember {
    name: &'static str,
    kind: BuiltinMemberKind,
    signature: Option<BuiltinSignature>,
    value_type: Option<TypePattern>,
    receiver: Option<ReceiverMode>,
    builtin_id: Option<BuiltinId>,
    runtime_import: Option<&'static str>,
    required: bool,
    type_parameters: &'static [&'static str],
    documentation: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct BuiltinDeclaration {
    path: &'static str,
    kind: BuiltinKind,
    type_parameters: &'static [&'static str],
    members: &'static [BuiltinMember],
    signature: Option<BuiltinSignature>,
    backend: BuiltinBackend,
    documentation: &'static str,
}

rils_builtins_macros::builtin_file! {
    "tests/fixtures/builtin_ids.toml";
    "tests/fixtures/builtin_file.rils";
    complete "core::fixture";
    backend Runtime;
    const FIXTURE_BUILTIN;
}

#[test]
fn rils_source_generates_variants_methods_signatures_docs_and_ids() {
    assert_eq!(FIXTURE_BUILTIN.path, "Fixture");
    assert_eq!(FIXTURE_BUILTIN.kind, BuiltinKind::Enum);
    assert_eq!(FIXTURE_BUILTIN.type_parameters, &["T"]);
    assert_eq!(FIXTURE_BUILTIN.backend, BuiltinBackend::Runtime);
    assert_eq!(FIXTURE_BUILTIN.documentation, "");
    assert!(FIXTURE_BUILTIN.signature.is_none());

    let [empty, value, owned, shared, mutable] = FIXTURE_BUILTIN.members else {
        panic!("expected all fixture members");
    };
    assert!(
        FIXTURE_BUILTIN
            .members
            .iter()
            .all(|member| member.runtime_import.is_none())
    );

    assert_eq!(empty.kind, BuiltinMemberKind::Variant);
    assert_eq!(empty.value_type, Some(TypePattern::Unit));
    assert_eq!(empty.documentation, "No value is present.");
    assert_eq!(value.value_type, Some(TypePattern::Generic("T")));

    assert_eq!(owned.receiver, Some(ReceiverMode::Owned));
    assert!(owned.required);
    assert_eq!(owned.builtin_id, Some(BuiltinId::FixtureOwned));
    assert_eq!(
        owned.signature.expect("owned signature").result,
        TypePattern::Generic("T")
    );
    assert_eq!(shared.receiver, Some(ReceiverMode::Shared));
    assert_eq!(shared.builtin_id, Some(BuiltinId::FixtureShared));
    assert_eq!(mutable.receiver, Some(ReceiverMode::Mutable));
    assert_eq!(mutable.builtin_id, Some(BuiltinId::FixtureMutable));
    let signature = mutable.signature.expect("mutable signature");
    assert_eq!(signature.parameters, &[TypePattern::Generic("T")]);
    assert_eq!(signature.result, TypePattern::Unit);
    assert!(!signature.variadic);
    assert!(mutable.type_parameters.is_empty());
    assert!(mutable.value_type.is_none());
    assert_eq!(mutable.name, "mutable");
    assert_eq!(mutable.documentation, "Replaces the fixture value.");
}
