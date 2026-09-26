use indoc::indoc;
use termharness::{error::Error, scenario};

#[test]
fn runs_wait_backend_line_starts_with_and_resizes_in_one_step() -> Result<(), Error> {
    let run = scenario::run_document(indoc! {r#"
        Scenario "wait output and resize sequence"
        Command "zsh"
        Arg "-fc"
        Arg "stty -echo; print -r -- armed; read value; print -rn -- ready:$value; sleep 1"
        Terminal rows 3 cols 12

        Step "race output and resize"
        WaitFrontendLineStartsWith "armed" timeout 1000ms
        Input "hello"
        Input enter
        WaitBackendLineStartsWith "ready:hello" timeout 1000ms
        Resize rows 3 cols 10
        Resize rows 3 cols 12
        Settle 10ms
        Expect timeout 0ms:
          r00 |············|
          r01 |armed·······|
          r02 |ready:hello·|
    "#})?;

    assert_eq!(run.records.len(), 1);
    Ok(())
}

#[test]
fn reports_wait_backend_line_starts_with_timeout() {
    let error = scenario::run_document(indoc! {r#"
        Scenario "wait output timeout"
        Command "true"
        Terminal rows 1 cols 1

        Step "wait"
        WaitBackendLineStartsWith "missing" timeout 0ms
        Settle 0ms
        Expect timeout 0ms:
          r00 |·|
    "#})
    .expect_err("missing output should time out");

    assert!(matches!(
        error,
        Error::BackendLineStartsWithTimeout {
            expected,
            timeout_ms: 0,
            ..
        } if expected == "missing"
    ));
}

#[test]
fn reports_wait_frontend_line_starts_with_timeout() {
    let error = scenario::run_document(indoc! {r#"
        Scenario "wait screen timeout"
        Command "true"
        Terminal rows 1 cols 1

        Step "wait"
        WaitFrontendLineStartsWith "missing" timeout 0ms
        Settle 0ms
        Expect timeout 0ms:
          r00 |·|
    "#})
    .expect_err("missing screen prefix should time out");

    assert!(matches!(
        error,
        Error::FrontendLineStartsWithTimeout {
            expected,
            timeout_ms: 0,
            ..
        } if expected == "missing"
    ));
}

#[test]
fn does_not_match_text_inside_a_backend_line() {
    let error = scenario::run_document(indoc! {r#"
        Scenario "backend line prefix"
        Command "zsh"
        Arg "-fc"
        Arg "stty -echo; print -rn -- not-ready; read value"
        Terminal rows 1 cols 12

        Step "reject a substring"
        WaitFrontendLineStartsWith "not-ready" timeout 1000ms
        WaitBackendLineStartsWith "ready" timeout 0ms
        Settle 0ms
        Expect timeout 0ms:
          r00 |not-ready···|
    "#})
    .expect_err("a substring inside a backend line must not match");

    assert!(matches!(
        error,
        Error::BackendLineStartsWithTimeout {
            expected,
            actual,
            timeout_ms: 0,
            ..
        } if expected == "ready" && actual.contains("not-ready")
    ));
}
