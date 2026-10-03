use std::io::Write;
use std::process::{Command, Stdio};
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_specqr"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn cli_modes() {
    let help = run(&["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout).unwrap().contains("Usage:"));
    assert_eq!(
        String::from_utf8(run(&["--package-version"]).stdout)
            .unwrap()
            .trim(),
        specqr::VERSION
    );
    let svg = run(&["HELLO", "--format", "svg"]);
    assert!(svg.status.success());
    assert!(svg.stdout.starts_with(b"<svg"));
    let png = run(&["--hex", "00ff80", "--format", "png"]);
    assert!(png.status.success());
    assert!(png.stdout.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
    let j = run(&["日本語", "--format", "json", "--mask", "3"]);
    assert!(j.status.success(), "{}", String::from_utf8_lossy(&j.stderr));
    let v = specqr::json::parse(std::str::from_utf8(&j.stdout).unwrap()).unwrap();
    assert_eq!(v.get("maskPattern").unwrap().as_u64(), Some(3));
    let estimate = run(&["HELLO", "--estimate"]);
    assert!(estimate.status.success());
    let v = specqr::json::parse(std::str::from_utf8(&estimate.stdout).unwrap()).unwrap();
    assert_eq!(v.get("ok").unwrap().as_bool(), Some(true));
    for args in [
        vec!["--mask", "8"],
        vec!["--version", "0"],
        vec!["--scale", "0"],
        vec!["--hex", "a"],
        vec!["--hex", "gg"],
        vec!["--unknown"],
        vec!["one", "two"],
        vec!["--eci", "1000000"],
        vec!["--text", "a", "--format", "invalid"],
        vec!["--input", "this-file-does-not-exist"],
    ] {
        assert!(!run(&args).status.success(), "{args:?}");
    }
}
#[test]
fn stdin_and_invalid_utf8() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_specqr"))
        .args(["--format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all("stdin 日本語".as_bytes())
        .unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    let mut child = Command::new(env!("CARGO_BIN_EXE_specqr"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&[255]).unwrap();
    assert!(!child.wait_with_output().unwrap().status.success());
}
#[test]
fn help_in_explicit_data_is_encoded() {
    for args in [
        vec!["--text", "--help", "--format", "json"],
        vec!["--format", "json", "--", "--help"],
    ] {
        let result = run(&args);
        assert!(result.status.success());
        let value = specqr::json::parse(std::str::from_utf8(&result.stdout).unwrap()).unwrap();
        assert!(value.get("matrix").is_some());
    }
}
#[cfg(unix)]
#[test]
fn non_utf8_argv_is_typed_error_not_panic() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let output = Command::new(env!("CARGO_BIN_EXE_specqr"))
        .arg("--text")
        .arg(OsStr::from_bytes(&[255]))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .starts_with("INVALID_INPUT:")
    );
}
