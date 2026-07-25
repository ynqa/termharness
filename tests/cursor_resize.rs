use indoc::indoc;
use termharness::{error::Result, scenario};

#[test]
fn applies_initial_cursor_and_resize_from_document() -> Result<()> {
    let run = scenario::run_document(indoc! {r#"
        Scenario "cursor_resize"
        Command "zsh"
        Arg "-fi"
        Env PS1 "❯❯ "
        Env RPS1 ""
        Env PROMPT_EOL_MARK ""
        Terminal rows 4 cols 8
        Cursor row 2 col 1

        Step "spawn"
        Settle 300ms
        Expect:
          r00 |········|
          r01 |❯❯······|
          r02 |········|
          r03 |········|

        Step "resize"
        Resize rows 4 cols 6
        Settle 100ms
        Expect:
          r00 |······|
          r01 |❯❯····|
          r02 |······|
          r03 |······|
    "#})?;

    assert_eq!(run.records.len(), 2);
    Ok(())
}

#[test]
fn runs_a_non_zsh_command() -> Result<()> {
    let run = scenario::run_document(indoc! {r#"
        Scenario "arbitrary_command"
        Command "true"
        Terminal rows 1 cols 3

        Step "spawn"
        Settle 0ms
        Expect:
          r00 |···|
    "#})?;

    assert_eq!(run.records.len(), 1);
    Ok(())
}
