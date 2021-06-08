use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::{env, path::PathBuf, process::Command};

#[test]
fn usage_error() {
    let mut command = Command::cargo_bin("atm").unwrap();
    command.assert().failure();
    command.assert().stderr(predicate::str::contains("Usage:"));
}

#[test]
fn correct_run() {
    let expected = r#"client,available,held,total,locked
1,1.5,0,1.5,false
2,2,0,2,false
"#;
    let manifest_path: PathBuf = env::var("CARGO_MANIFEST_DIR").unwrap().parse().unwrap();
    let test_file = manifest_path.join("tests").join("sample_input.csv");
    let mut command = Command::cargo_bin("atm").unwrap();
    command.arg(test_file.to_str().unwrap());
    command.assert().success();
    command.assert().stdout(predicate::eq(expected));
}
