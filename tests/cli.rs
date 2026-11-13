//! End-to-end tests: run the built binary on fixtures in `tests/data/`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

const HEADER: &str = "client,available,held,total,locked";

fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join(name)
}

/// Runs the binary with `args`, returning (status, stdout, stderr).
fn run_bin(args: &[&Path]) -> (ExitStatus, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_payments-engine"))
        .args(args)
        .output()
        .expect("failed to spawn binary");
    (
        out.status,
        String::from_utf8(out.stdout).expect("stdout is utf-8"),
        String::from_utf8(out.stderr).expect("stderr is utf-8"),
    )
}

fn run_fixture(name: &str) -> (ExitStatus, String, String) {
    run_bin(&[&data(name)])
}

/// Splits account CSV into its header line and its data rows, sorted so that
/// row order does not matter.
fn parse_accounts(csv: &str) -> (String, Vec<Vec<String>>) {
    let mut lines = csv.lines();
    let header = lines.next().unwrap_or_default().to_string();
    let mut rows: Vec<Vec<String>> = lines
        .filter(|l| !l.is_empty())
        .map(|l| l.split(',').map(str::to_string).collect())
        .collect();
    rows.sort();
    (header, rows)
}

/// Runs `<name>.csv` and compares stdout with `<name>.expected.csv`:
/// exact header, set-based data rows.
fn assert_matches_expected(name: &str) -> String {
    let (status, stdout, stderr) = run_fixture(&format!("{name}.csv"));
    assert!(status.success(), "exit status {status}, stderr: {stderr}");

    let expected = std::fs::read_to_string(data(&format!("{name}.expected.csv")))
        .expect("expected file exists");
    let (header, rows) = parse_accounts(&stdout);
    let (exp_header, exp_rows) = parse_accounts(&expected);
    assert_eq!(header, HEADER);
    assert_eq!(header, exp_header);
    assert_eq!(rows, exp_rows, "stdout was:\n{stdout}");
    stderr
}

fn row(fields: &[&str]) -> Vec<String> {
    fields.iter().map(|f| f.to_string()).collect()
}

#[test]
fn sample_from_spec() {
    let stderr = assert_matches_expected("sample");
    // client 2's withdrawal exceeds funds and is reported
    assert!(!stderr.is_empty());
}

#[test]
fn sample_rows_are_exact() {
    let (_, stdout, _) = run_fixture("sample.csv");
    let (_, rows) = parse_accounts(&stdout);
    assert_eq!(
        rows,
        vec![
            row(&["1", "1.5000", "0.0000", "1.5000", "false"]),
            row(&["2", "2.0000", "0.0000", "2.0000", "false"]),
        ]
    );
}

#[test]
fn empty_file_prints_header_only() {
    let (status, stdout, _) = run_fixture("empty.csv");
    assert!(status.success());
    assert_eq!(stdout, format!("{HEADER}\n"));
}

#[test]
fn header_only_file_prints_header_only() {
    let (status, stdout, _) = run_fixture("header_only.csv");
    assert!(status.success());
    assert_eq!(stdout, format!("{HEADER}\n"));
}

#[test]
fn dispute_lifecycle_ends_locked() {
    let stderr = assert_matches_expected("dispute_lifecycle");
    // the final deposit is rejected because the account is locked
    assert!(stderr.contains("locked"), "stderr: {stderr}");
}

#[test]
fn whitespace_is_trimmed_and_output_has_four_decimals() {
    let stderr = assert_matches_expected("whitespace_and_precision");
    assert!(
        stderr.is_empty(),
        "no row should be rejected, got: {stderr}"
    );
}

#[test]
fn invalid_rows_are_skipped_and_reported() {
    let stderr = assert_matches_expected("invalid_rows");
    // six bad rows, each reported on its own line
    assert_eq!(stderr.lines().count(), 6, "stderr: {stderr}");
}

#[test]
fn missing_argument_fails_with_usage() {
    let (status, stdout, stderr) = run_bin(&[]);
    assert!(!status.success());
    assert!(stdout.is_empty());
    assert!(stderr.contains("usage"), "stderr: {stderr}");
}

#[test]
fn nonexistent_file_fails() {
    let (status, stdout, stderr) = run_fixture("does_not_exist.csv");
    assert!(!status.success());
    assert!(stdout.is_empty());
    assert!(stderr.contains("cannot open"), "stderr: {stderr}");
}
