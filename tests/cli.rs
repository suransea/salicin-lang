use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
enum M1Expectation {
    RunWithExitCode(i32),
    CheckFailsContaining(&'static str),
}

struct M1PendingCase {
    relative_path: &'static str,
    expectation: M1Expectation,
}

const M1_PENDING_CASES: &[M1PendingCase] = &[
    M1PendingCase {
        relative_path: "m1_pending/pass/while_mutation.sali",
        expectation: M1Expectation::RunWithExitCode(42),
    },
    M1PendingCase {
        relative_path: "m1_pending/pass/loop_break_value.sali",
        expectation: M1Expectation::RunWithExitCode(42),
    },
    M1PendingCase {
        relative_path: "m1_pending/pass/fixed_array_index.sali",
        expectation: M1Expectation::RunWithExitCode(42),
    },
    M1PendingCase {
        relative_path: "m1_pending/fail/array_index_type.sali",
        expectation: M1Expectation::CheckFailsContaining("index"),
    },
    M1PendingCase {
        relative_path: "m1_pending/fail/array_length_mismatch.sali",
        expectation: M1Expectation::CheckFailsContaining("length"),
    },
];

fn salic() -> Command {
    Command::new(env!("CARGO_BIN_EXE_salic"))
}

fn fixture(kind: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(kind)
        .join(name)
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "salic-test-{}-{timestamp}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create test directory");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn help_and_version_identify_salic() {
    let help = salic().arg("--help").output().expect("run salic --help");
    assert!(help.status.success(), "{}", output_text(&help));
    assert!(String::from_utf8_lossy(&help.stdout).contains("salic build"));

    let version = salic()
        .arg("--version")
        .output()
        .expect("run salic --version");
    assert!(version.status.success(), "{}", output_text(&version));
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("salic "));
}

#[test]
fn emit_ir_and_check_cover_the_frontend() {
    let emitted = salic()
        .args(["emit-ir"])
        .arg(fixture("pass", "exit_42.sali"))
        .output()
        .expect("emit LLVM IR");
    assert!(emitted.status.success(), "{}", output_text(&emitted));
    let ir = String::from_utf8_lossy(&emitted.stdout);
    assert!(ir.contains("define i32 @main()"), "unexpected IR:\n{ir}");

    let checked = salic()
        .arg("check")
        .arg(fixture("pass", "condition.sali"))
        .output()
        .expect("check source");
    assert!(checked.status.success(), "{}", output_text(&checked));

    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/basics.sali");
    let checked_example = salic()
        .arg("check")
        .arg(example)
        .output()
        .expect("check documented example");
    assert!(
        checked_example.status.success(),
        "{}",
        output_text(&checked_example)
    );
}

#[test]
fn run_supports_grouped_calls_and_unit_main() {
    let curried = salic()
        .arg("run")
        .arg(fixture("pass", "curried_call.sali"))
        .output()
        .expect("run curried program");
    assert_eq!(curried.status.code(), Some(42), "{}", output_text(&curried));

    let unit = salic()
        .arg("run")
        .arg(fixture("pass", "unit_main.sali"))
        .output()
        .expect("run unit program");
    assert!(unit.status.success(), "{}", output_text(&unit));

    let unit_values = salic()
        .arg("run")
        .arg(fixture("pass", "unit_values.sali"))
        .output()
        .expect("run program with unit values");
    assert_eq!(
        unit_values.status.code(),
        Some(42),
        "{}",
        output_text(&unit_values)
    );

    let control_flow = salic()
        .arg("run")
        .arg(fixture("pass", "short_circuit_return.sali"))
        .output()
        .expect("run short-circuit control flow program");
    assert_eq!(
        control_flow.status.code(),
        Some(42),
        "{}",
        output_text(&control_flow)
    );
}

#[test]
fn m1_struct_programs_run_with_expected_result() {
    for name in [
        "struct_fields.sali",
        "struct_mutation.sali",
        "positional_constructor.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 struct fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_struct_errors_report_their_cause() {
    for (name, expected) in [
        ("unknown_field.sali", "unknown field"),
        ("constructor_missing_field.sali", "missing field"),
        ("constructor_duplicate_field.sali", "duplicate field"),
        ("constructor_mixed_arguments.sali", "mixed"),
        ("immutable_field_assignment.sali", "immutable"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 struct fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_match_and_partial_programs_run_with_expected_result() {
    for name in [
        "enum_match.sali",
        "nested_match.sali",
        "match_guard.sali",
        "partial_application.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 match or partial-application fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_match_and_partial_errors_report_their_cause() {
    for (name, expected) in [
        ("non_exhaustive_match.sali", "exhaustive"),
        ("pattern_type_mismatch.sali", "pattern"),
        ("partial_application_escape.sali", "escape"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 match or partial-application fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_ownership_programs_run_with_expected_result() {
    for name in [
        "shared_borrow_call.sali",
        "mut_borrow_field_update.sali",
        "explicit_move_i32_once.sali",
        "borrow_released_after_complete_call.sali",
        "borrowed_unit_is_abi_erased.sali",
        "branch_move_does_not_pollute_sibling.sali",
        "disjoint_mut_field_borrows.sali",
        "inferred_copy_i32.sali",
        "move_then_return_preserves_other_branch.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 ownership fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_ownership_errors_report_their_cause() {
    for (name, expected) in [
        ("use_after_move.sali", &["moved"][..]),
        ("use_after_explicit_move_i32.sali", &["moved"][..]),
        (
            "copy_non_copy.sali",
            &["requires `Copy`", "does not implement Copy"][..],
        ),
        (
            "double_mut_borrow.sali",
            &["mutable borrow", "already borrowed"][..],
        ),
        ("borrow_move_conflict.sali", &["move", "borrowed"][..]),
        (
            "same_field_mut_borrow_conflict.sali",
            &["mutable borrow", "already borrowed"][..],
        ),
        ("use_after_inferred_move.sali", &["moved"][..]),
        ("possibly_moved_after_branch.sali", &["possibly moved"][..]),
        ("both_branches_move.sali", &["moved"][..]),
        ("short_circuit_possibly_moves.sali", &["possibly moved"][..]),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 ownership fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        for fragment in expected {
            assert!(
                stderr.contains(fragment),
                "{name} did not report `{fragment}`:\n{}",
                output_text(&output)
            );
        }
        assert!(
            !stderr.contains("not supported"),
            "{name} reached a placeholder diagnostic:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_local_closure_programs_run_with_expected_result() {
    for name in [
        "capturing_closure.sali",
        "closure_shared_repeat.sali",
        "closure_capture_parameter.sali",
        "closure_mut_capture.sali",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run M1 closure fixture");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_local_closure_errors_report_their_cause() {
    for (name, expected) in [
        ("closure_escape_return.sali", "escape"),
        ("closure_partial_application.sali", "curried closures"),
        ("closure_fnmut_immutable.sali", "FnMut"),
        ("closure_capture_borrow_conflict.sali", "borrowed"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 closure fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn shorthand_builds_a_native_executable() {
    let temporary = TestDirectory::new();
    let executable = temporary.join("mutation");
    let built = salic()
        .arg(fixture("pass", "block_mutation.sali"))
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("build source");
    assert!(built.status.success(), "{}", output_text(&built));
    assert!(executable.is_file());

    let status = Command::new(executable)
        .status()
        .expect("run native executable");
    assert_eq!(status.code(), Some(42));
}

#[test]
fn source_errors_fail_check_without_creating_output() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fail");
    let mut fixtures: Vec<_> = fs::read_dir(directory)
        .expect("read failure fixtures")
        .map(|entry| entry.expect("read directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sali")
        })
        .collect();
    fixtures.sort();

    for path in fixtures {
        let name = path.file_name().unwrap().to_string_lossy();
        let output = salic()
            .arg("check")
            .arg(&path)
            .output()
            .expect("check invalid source");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            !output.stderr.is_empty(),
            "{name} produced no diagnostic output"
        );
    }
}

#[test]
fn every_pass_fixture_checks_successfully() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pass");
    let mut fixtures: Vec<_> = fs::read_dir(directory)
        .expect("read passing fixtures")
        .map(|entry| entry.expect("read directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sali")
        })
        .collect();
    fixtures.sort();

    for path in fixtures {
        let output = salic()
            .arg("check")
            .arg(&path)
            .output()
            .expect("check valid source");
        assert!(
            output.status.success(),
            "{} failed:\n{}",
            path.display(),
            output_text(&output)
        );
    }
}

#[test]
fn m1_pending_acceptance_matrix_is_complete() {
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths = HashSet::new();

    for case in M1_PENDING_CASES {
        assert!(
            paths.insert(case.relative_path),
            "duplicate M1 fixture in acceptance matrix: {}",
            case.relative_path
        );
        assert!(
            fixture_root.join(case.relative_path).is_file(),
            "missing M1 fixture: {}",
            case.relative_path
        );

        match case.expectation {
            M1Expectation::RunWithExitCode(code) => {
                assert_eq!(
                    code, 42,
                    "M1 pass fixtures use exit code 42 as their oracle"
                )
            }
            M1Expectation::CheckFailsContaining(fragment) => assert!(
                !fragment.is_empty(),
                "M1 failure fixture needs a diagnostic oracle: {}",
                case.relative_path
            ),
        }
    }

    let pending_files = fs::read_dir(fixture_root.join("m1_pending/pass"))
        .expect("read pending M1 pass fixtures")
        .chain(
            fs::read_dir(fixture_root.join("m1_pending/fail"))
                .expect("read pending M1 failure fixtures"),
        )
        .map(|entry| entry.expect("read pending M1 fixture").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sali")
        })
        .count();
    assert_eq!(
        pending_files,
        M1_PENDING_CASES.len(),
        "every pending M1 fixture must appear in the acceptance matrix"
    );
}

#[test]
fn output_must_not_overwrite_the_source() {
    let temporary = TestDirectory::new();
    let source = temporary.join("keep.sali");
    let original = b"let main(): i32 = 0\n";
    fs::write(&source, original).expect("write source fixture");

    for command in ["build", "emit-ir"] {
        let output = salic()
            .arg(command)
            .arg(&source)
            .arg("-o")
            .arg(&source)
            .output()
            .expect("run salic");
        assert_eq!(output.status.code(), Some(2), "{}", output_text(&output));
        assert_eq!(fs::read(&source).expect("read preserved source"), original);
    }
}
