use super::*;

#[test]
fn resolves_local_definitions_and_references() {
    let source = "let value = 42; value";
    let analysis = analyze_with_source_id(source, SourceId::new(7), &HashMap::new()).unwrap();
    assert!(analysis.diagnostics.is_empty());
    let reference = analysis
        .symbols
        .iter()
        .find(|symbol| !symbol.is_definition && symbol.name == "value")
        .unwrap();
    assert_eq!(
        reference.definition_span,
        Some(Span::in_source(SourceId::new(7), 4, 9))
    );
    assert_eq!(
        reference.definition_id,
        Some(SymbolId {
            source: SourceId::new(7),
            local: 1,
        })
    );
}

#[test]
fn symbol_ids_distinguish_shadowed_bindings() {
    let source = "let value = 1; { let value = 2; value } value";
    let analysis = analyze_with_source_id(source, SourceId::new(11), &HashMap::new()).unwrap();
    let definitions = analysis
        .symbols
        .iter()
        .filter(|symbol| symbol.is_definition && symbol.name == "value")
        .collect::<Vec<_>>();
    let references = analysis
        .symbols
        .iter()
        .filter(|symbol| !symbol.is_definition && symbol.name == "value")
        .collect::<Vec<_>>();
    assert_eq!(definitions.len(), 2);
    assert_eq!(references.len(), 2);
    assert_ne!(definitions[0].symbol_id, definitions[1].symbol_id);
    assert_eq!(references[0].definition_id, definitions[1].symbol_id);
    assert_eq!(references[1].definition_id, definitions[0].symbol_id);
}

#[test]
fn reports_undefined_names_without_executing() {
    let analysis = analyze("if false { missing }").unwrap();
    assert_eq!(analysis.diagnostics.len(), 1);
    assert!(analysis.diagnostics[0].message.contains("missing"));
}

#[test]
fn exposes_host_function_symbols_with_signatures() {
    let functions = HashMap::from([(
        "unity_engine::math::add".to_owned(),
        FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
    )]);
    let analysis =
        analyze_with_host_functions("unity_engine::math::add(20, 22)", &functions).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let symbol = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.name == "add")
        .unwrap();
    assert_eq!(symbol.kind, SymbolKind::Function);
    assert_eq!(
        symbol.inferred_type,
        Some(functions.values().next().unwrap().as_type())
    );
    assert_eq!(
        symbol.detail.as_deref(),
        Some("host fn unity_engine::math::add(i32, i32) -> i32")
    );
}

#[test]
fn pattern_bindings_are_scoped() {
    let analysis = analyze("match Some(1) { Some(value) => value, None => 0 }; value").unwrap();
    assert_eq!(analysis.diagnostics.len(), 1);
    assert!(analysis.diagnostics[0].message.contains("undefined"));
}

#[test]
fn analyzes_builtin_result_patterns_and_try_expressions() {
    let source = r#"
            fn source() -> Result<i32, string> { Ok(42) }
            fn forward() -> Result<i32, string> {
                let value = source()?;
                Ok(value)
            }
            match forward() { Ok(value) => value, Err(_) => 0 }
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn analyzes_standard_io_and_fs_paths() {
    let source = r#"
            fn load(path: string) -> Result<string, std::io::Error> {
                std::fs::read_to_string(path)
            }
            let loaded = std::fs::read_to_string("missing.txt");
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    assert!(analysis.inlay_hints.iter().any(|hint| {
        &source[hint.span.start..hint.span.end] == "loaded"
            && hint.label == ": Result<string, std::io::Error>"
    }));
}

#[test]
fn annotated_types_resolve_to_their_declarations() {
    let source = "struct Point { x: i32 }\nfn keep(value: Point) -> Point { value }\nlet p: Point = Point { x: 1 };";
    let analysis = analyze(source).unwrap();
    assert!(analysis.diagnostics.is_empty());
    let definition = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.is_definition && symbol.name == "Point")
        .unwrap()
        .span;
    let references = analysis
        .symbols
        .iter()
        .filter(|symbol| !symbol.is_definition && symbol.name == "Point")
        .collect::<Vec<_>>();
    assert_eq!(references.len(), 4);
    assert!(
        references
            .iter()
            .all(|reference| reference.definition_span == Some(definition))
    );
}

#[test]
fn resolves_ufcs_methods_and_place_expressions() {
    let source = r#"
            trait Value { fn value(&self) -> i32; }
            struct Number { inner: i32 }
            impl Value for Number { fn value(&self) -> i32 { self.inner } }

            let mut number = Number { inner: 1 };
            number.inner = 42;
            let index = 0;
            number[index];
            Value::value(&number);
            <Number as Value>::value(&number);
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );

    let definition = analysis
        .symbols
        .iter()
        .find(|symbol| {
            symbol.is_definition && symbol.kind == SymbolKind::Method && symbol.name == "value"
        })
        .expect("trait method definition");
    let ufcs_references = analysis
        .symbols
        .iter()
        .filter(|symbol| {
            !symbol.is_definition
                && symbol.kind == SymbolKind::Method
                && symbol.name == "value"
                && symbol.definition_span == Some(definition.span)
        })
        .count();
    assert_eq!(ufcs_references, 2);

    assert!(
        analysis
            .symbols
            .iter()
            .any(|symbol| !symbol.is_definition && symbol.name == "index")
    );
}

#[test]
fn classifies_called_members_as_methods_and_other_members_as_fields() {
    let source = r#"
            struct Factory { value: i32 }
            impl Factory {
                fn make(&self) -> i32 { self.value }
            }
            let factory = Factory { value: 42 };
            factory.make();
            factory.value;
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );

    assert!(analysis.symbols.iter().any(|symbol| {
        !symbol.is_definition && symbol.name == "make" && symbol.kind == SymbolKind::Method
    }));
    assert!(analysis.symbols.iter().any(|symbol| {
        !symbol.is_definition && symbol.name == "value" && symbol.kind == SymbolKind::Field
    }));
}

#[test]
fn describes_type_aliases_with_recursively_expanded_targets() {
    let source = r#"
            struct Box<T> { value: T }
            type ValueBox<T> = Box<T>;
            type IntBox = ValueBox<i32>;
            fn consume(value: IntBox) {}
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );

    for expected in [
        "type ValueBox<T> = Box<T>",
        "type ValueBox<i32> = Box<i32>",
        "type IntBox = Box<i32>",
    ] {
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.detail.as_deref() == Some(expected)),
            "missing {expected:?}: {:?}",
            analysis.symbols
        );
    }
}

#[test]
fn infers_function_let_and_pattern_binding_types() {
    let source = "
            fn answer() { 42 }
            fn identity<T>(input: T) { input }
            fn make_getter() {
                fn get() { 42 }
                get
            }
            let result = answer();
            let copied = identity(result);
            let getter = make_getter();
            let nested = getter();
            let maybe = Some(result);
            match maybe { Some(value) => value, None => 0 }
            struct Point { x: i32 }
            let point = Point { x: 1 };
            match point { Point { x } => x }
        ";
    let analysis = analyze(source).unwrap();
    for expected in [": i32", ": Option<i32>", " -> i32"] {
        assert!(
            analysis
                .inlay_hints
                .iter()
                .any(|hint| hint.label == expected),
            "missing hint {expected:?}: {:?}",
            analysis.inlay_hints
        );
    }
    let answer = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.is_definition && symbol.name == "answer")
        .unwrap();
    assert_eq!(
        answer.inferred_type,
        Some(Type::function(Vec::new(), Type::I32))
    );
    let copied = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.is_definition && symbol.name == "copied")
        .unwrap();
    assert_eq!(copied.inferred_type, Some(Type::I32));
    let getter = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.is_definition && symbol.name == "getter")
        .unwrap();
    assert_eq!(
        getter.inferred_type,
        Some(Type::function(Vec::new(), Type::I32))
    );
    let nested = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.is_definition && symbol.name == "nested")
        .unwrap();
    assert_eq!(nested.inferred_type, Some(Type::I32));
    assert!(
        analysis.inlay_hints.iter().any(|hint| {
            &source[hint.span.start..hint.span.end] == "x" && hint.label == ": i32"
        })
    );
}

#[test]
fn resolves_macro_definitions_and_invocations() {
    let source = "macro twice($value) { $value + $value } twice!(21)";
    let analysis = analyze(source).unwrap();
    let definition = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.is_definition && symbol.kind == SymbolKind::Macro)
        .unwrap();
    let reference = analysis
        .symbols
        .iter()
        .find(|symbol| !symbol.is_definition && symbol.kind == SymbolKind::Macro)
        .unwrap();
    assert_eq!(reference.definition_span, Some(definition.span));
}

#[test]
fn analyzes_modules_imports_and_builtin_namespaces() {
    let source = r#"
            mod math { pub fn answer() -> i32 { 42 } }
            use math::answer;
            let value = answer();
            std::io::println(value);
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    assert!(analysis.symbols.iter().any(|symbol| {
        symbol.is_definition && symbol.name == "math" && symbol.kind == SymbolKind::Module
    }));
    assert!(
        analysis
            .symbols
            .iter()
            .any(|symbol| { symbol.is_definition && symbol.name == "answer" })
    );
}

#[test]
fn reports_non_exhaustive_builtin_and_enum_matches() {
    let source = r#"
            enum State { Ready, Waiting(i32), Failed { code: i32 } }
            let state = State::Ready;
            match state { State::Ready => 1, State::Waiting(_) => 2 };
            match Some(1) { Some(value) => value };
            match true { true => 1 };
        "#;
    let analysis = analyze(source).unwrap();
    let messages = analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    assert!(messages.iter().any(|message| message.contains("Failed")));
    assert!(messages.iter().any(|message| message.contains("None")));
    assert!(messages.iter().any(|message| message.contains("false")));
}

#[test]
fn accepts_exhaustive_matches_and_reports_unreachable_arms() {
    let source = r#"
            match Some(1) {
                Some(value) => value,
                None => 0,
                _ => -1,
                Some(_) => -2,
            };
        "#;
    let analysis = analyze(source).unwrap();
    let unreachable = analysis
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message == "unreachable match arm")
        .count();
    assert_eq!(unreachable, 2, "{:?}", analysis.diagnostics);
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("non-exhaustive"))
    );
}

#[test]
fn reports_missing_return_paths_and_unreachable_statements() {
    let source = r#"
            fn incomplete(flag: bool) -> i32 {
                if flag { return 1; }
            }
            fn complete(flag: bool) -> i32 {
                if flag { return 1; } else { return 2; }
                3
            }
            loop {
                break;
                4;
            }
        "#;
    let analysis = analyze(source).unwrap();
    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.contains("not all paths return"))
            .count(),
        1,
        "{:?}",
        analysis.diagnostics
    );
    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message == "unreachable statement")
            .count(),
        2,
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn accepts_diverging_loops_and_finds_duplicate_literals() {
    let source = r#"
            fn forever() -> i32 { loop {} }
            match 1 { 1 => 1, 1 => 2, _ => 3 };
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("not all paths return")),
        "{:?}",
        analysis.diagnostics
    );
    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message == "unreachable match arm")
            .count(),
        1,
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn reports_moves_and_merges_definite_branch_moves() {
    let source = r#"
            fn moved(flag: bool) {
                let first = "first";
                let owner = first;
                first;

                let second = "second";
                if flag { let left = second; } else { let right = second; }
                second;

                let third = "third";
                if flag { let maybe = third; }
                third;
            }
        "#;
    let analysis = analyze(source).unwrap();
    let moved = analysis
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message.contains("use of moved value"))
        .collect::<Vec<_>>();
    assert_eq!(moved.len(), 2, "{:?}", analysis.diagnostics);
    assert!(
        moved
            .iter()
            .any(|diagnostic| diagnostic.message.contains("first"))
    );
    assert!(
        moved
            .iter()
            .any(|diagnostic| diagnostic.message.contains("second"))
    );
}

#[test]
fn preserves_copy_values_and_allows_multiple_mutable_references() {
    let source = r#"
            fn valid() -> i32 {
                let value = 21;
                let copied = value;
                let mut target = value + copied;
                {
                    let first = &mut target;
                    let second = &mut target;
                    *first = *second;
                }
                target
            }
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn reports_mutability_borrow_and_reference_escape_errors() {
    let source = r#"
            fn invalid_return(value: &i32) { value }
            fn invalid_local() {
                let immutable = 1;
                immutable = 2;
                let writable = &mut immutable;

                let text = "hello";
                {
                    let reference = &text;
                    let moved = text;
                }

                let value = 1;
                let wrapped = Some(&value);
            }
        "#;
    let analysis = analyze(source).unwrap();
    for expected in [
        "cannot be returned",
        "cannot assign to immutable",
        "cannot mutably reference immutable",
        "while it is referenced",
        "cannot be stored inside owned values",
    ] {
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "missing {expected:?}: {:?}",
            analysis.diagnostics
        );
    }
}

#[test]
fn releases_local_and_temporary_borrows() {
    let source = r#"
            fn inspect(value: &string) -> i32 { 1 }
            fn valid() {
                let first = "first";
                inspect(&first);
                let moved_first = first;

                let second = "second";
                { let reference = &second; }
                let moved_second = second;
            }
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn tracks_partial_field_moves_and_field_reinitialization() {
    let source = r#"
            struct Message { text: string, code: i32 }
            fn invalid() {
                let message = Message { text: "hello", code: 1 };
                let text = message.text;
                message.text;
                message;
            }
            fn valid() {
                let mut message = Message { text: "hello", code: 1 };
                let text = message.text;
                message.text = "again";
                let restored = message;
            }
        "#;
    let analysis = analyze(source).unwrap();
    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.contains("moved place"))
            .count(),
        1,
        "{:?}",
        analysis.diagnostics
    );
    assert_eq!(
        analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message.contains("partially moved"))
            .count(),
        1,
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn reports_basic_static_type_mismatches() {
    let source = r#"
            fn takes_int(value: i32) -> i32 { value }
            fn wrong_return() -> i32 { "wrong" }
            fn explicit_wrong() -> i32 { return "wrong"; }

            let annotated: string = 42;
            let mut assigned: i32 = 1;
            assigned = "wrong";
            takes_int("wrong");
            if 1 { 1 } else { 2 };
            let mixed = if true { 1 } else { "wrong" };
            let array = [1, "wrong"];
            let range = 0..false;
            unwrap_or(Some(1), "wrong");
        "#;
    let analysis = analyze(source).unwrap();
    for expected in [
        "function result expects `i32`",
        "return value expects `i32`",
        "initializer expects `string`",
        "assigned value expects `i32`",
        "argument expects `i32`",
        "if condition expects `bool`",
        "if branch expects `i32`",
        "array element expects `i32`",
        "range bounds must have the same integer type",
        "default argument expects `i32`",
    ] {
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "missing {expected:?}: {:?}",
            analysis.diagnostics
        );
    }
}

#[test]
fn accepts_aliases_generics_and_concrete_numeric_operators() {
    let source = r#"
            type Count = i32;
            fn identity<T>(value: T) -> T { value }
            fn convert(value: f64) -> f64 { value }
            let count: Count = 42;
            let text = identity("text");
            let number = convert(1.5);
            let sum = 1 + 2;
            if true { count } else { 0 };
        "#;
    let analysis = analyze(source).unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn tracks_owned_and_borrowed_method_receivers() {
    let source = r#"
            struct Message { text: string }
            impl Message {
                fn read(&self) -> i32 { 1 }
                fn replace(&mut self, text: string) { *self = Message { text: text } }
                fn consume(self) -> i32 { 1 }
            }

            fn invalid_move() {
                let message = Message { text: "hello" };
                message.read();
                message.consume();
                message;
            }

            fn invalid_mutability() {
                let message = Message { text: "hello" };
                message.replace("next");
            }

            fn invalid_argument() {
                let mut message = Message { text: "hello" };
                message.replace(42);
            }

            fn valid() {
                let mut message = Message { text: "hello" };
                message.read();
                message.replace("next");
                message.read();
            }
        "#;
    let analysis = analyze(source).unwrap();
    for expected in [
        "use of moved value `message`",
        "cannot mutably reference immutable variable `message`",
        "argument expects `string`",
    ] {
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "missing {expected:?}: {:?}",
            analysis.diagnostics
        );
    }
    assert_eq!(analysis.diagnostics.len(), 3, "{:?}", analysis.diagnostics);
}

#[test]
fn infers_generic_record_literals_in_return_positions() {
    let analysis = analyze(
        r#"
            struct Pair<T, U> { first: T, second: U }
            impl<T, U> Pair<T, U> {
                fn swap(self) -> Pair<U, T> {
                    Pair { first: self.second, second: self.first }
                }
            }
        "#,
    )
    .unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn accepts_exhaustive_nested_option_matches() {
    let analysis = analyze(
        r#"
            fn describe(value: Option<Option<i32>>) -> i32 {
                match value {
                    Some(Some(number)) => number,
                    Some(None) => 0,
                    None => -1,
                }
            }
        "#,
    )
    .unwrap();
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn merges_moves_from_all_loop_break_paths() {
    let source = r#"
            fn definite() {
                let text = "owned";
                loop {
                    let consumed = text;
                    break;
                }
                text;
            }

            fn conditional(flag: bool) {
                let text = "owned";
                loop {
                    if flag {
                        let consumed = text;
                        break;
                    } else {
                        break;
                    }
                }
                text;
            }
        "#;
    let analysis = analyze(source).unwrap();
    let moved = analysis
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message.contains("use of moved value `text`"))
        .count();
    assert_eq!(moved, 1, "{:?}", analysis.diagnostics);
}

#[test]
fn reports_integer_casts_that_can_lose_information() {
    let accepted = analyze("let index = 1_i32; index as usize").unwrap();
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );

    let rejected = analyze("let value = 1usize; value as i32").unwrap();
    assert!(rejected.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cannot losslessly cast `usize` to `i32`")
    }));
}

#[test]
fn types_integer_intrinsics_and_rejects_them_on_other_values() {
    let accepted = analyze(
        "let value: i32 = 1; let checked = value.checked_add(2i32); i16::try_from(1usize);",
    )
    .unwrap();
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );

    let rejected = analyze(r#""text".checked_add("other")"#).unwrap();
    assert!(rejected.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("integer intrinsic `checked_add` is not available on `string`")
    }));
}
