use indoc::indoc;
use termharness::{error::Error, scenario};

#[test]
fn runs_wait_pty_output_contains_and_resizes_in_one_step() -> Result<(), Error> {
    let run = scenario::run_document(indoc! {r#"
        Scenario "wait output and resize sequence"
        Command "zsh"
        Arg "-fc"
        Arg "stty -echo; print -r -- armed; read value; print -rn -- ready:$value; sleep 1"
        Terminal rows 3 cols 12

        Step "race output and resize"
        WaitScreenLineStartsWith "armed" timeout 1000ms
        Input "hello"
        Input enter
        WaitPtyOutputContains "ready:hello" timeout 1000ms
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
fn reports_wait_pty_output_contains_timeout() {
    let error = scenario::run_document(indoc! {r#"
        Scenario "wait output timeout"
        Command "true"
        Terminal rows 1 cols 1

        Step "wait"
        WaitPtyOutputContains "missing" timeout 0ms
        Settle 0ms
        Expect timeout 0ms:
          r00 |·|
    "#})
    .expect_err("missing output should time out");

    assert!(matches!(
        error,
        Error::PtyOutputContainsTimeout {
            expected,
            timeout_ms: 0,
            ..
        } if expected == "missing"
    ));
}

#[test]
fn reports_wait_screen_line_starts_with_timeout() {
    let error = scenario::run_document(indoc! {r#"
        Scenario "wait screen timeout"
        Command "true"
        Terminal rows 1 cols 1

        Step "wait"
        WaitScreenLineStartsWith "missing" timeout 0ms
        Settle 0ms
        Expect timeout 0ms:
          r00 |·|
    "#})
    .expect_err("missing screen prefix should time out");

    assert!(matches!(
        error,
        Error::ScreenLineStartsWithTimeout {
            expected,
            timeout_ms: 0,
            ..
        } if expected == "missing"
    ));
}
