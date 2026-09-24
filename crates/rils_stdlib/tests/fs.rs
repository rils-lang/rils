use rils_stdlib::stdlib::{fs, io::Error, result::Result, string::String};

fn ok<T>(result: Result<T, Error>) -> T {
    match result {
        Result::Ok(value) => value,
        Result::Err(error) => panic!("{}", error.message),
    }
}

#[test]
fn filesystem_functions_use_rust_bodies() {
    let root = std::env::temp_dir().join(format!("rils-stdlib-fs-{}", std::process::id()));
    let nested = root.join("nested");
    let file = nested.join("note.txt");
    let path =
        |value: &std::path::Path| String::from(value.to_str().expect("UTF-8 test path").to_owned());

    ok(fs::create_dir_all(path(&nested)));
    ok(fs::write(path(&file), String::from("hello".to_owned())));
    ok(fs::append(path(&file), String::from(" world".to_owned())));
    let contents: std::string::String = ok(fs::read_to_string(path(&file))).into();
    assert_eq!(contents, "hello world");
    assert!(ok(fs::try_exists(path(&file))));
    assert_eq!(ok(fs::read_dir(path(&nested))).len(), 1);
    ok(fs::remove_file(path(&file)));
    assert!(!ok(fs::try_exists(path(&file))));
    ok(fs::remove_dir(path(&nested)));
    ok(fs::remove_dir(path(&root)));

    match fs::read_to_string(path(&file)) {
        Result::Err(error) => assert_eq!(error.kind, std::io::ErrorKind::NotFound),
        Result::Ok(_) => panic!("missing file unexpectedly read"),
    }
}
