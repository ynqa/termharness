use termharness::{error::Result, scenario};

/// A raw-mode receiver prints every incoming byte as hex. This checks the
/// actual PTY transport, including delimiters, exact line endings, action order,
/// and the absence of an implicit Enter. The final `!` is a boundary sentinel.
fn assert_received(actions: &str, expected: &[u8]) -> Result<()> {
    let hex: String = expected.iter().map(|byte| format!("{byte:02x}")).collect();
    let cols = hex.len().max(5);
    let ready = format!("{:<cols$}", "ready");
    let blank = " ".repeat(cols);
    let document = format!(
        r#"Scenario "paste bytes"
Command "zsh"
Arg "-fc"
Arg "stty raw -echo; printf 'ready\r\n'; dd bs=1 count={} 2>/dev/null | od -An -v -tx1 | tr -d ' \n'; sleep 1"
Terminal rows 2 cols {cols}
Cursor row 1 col 1

Step "wait for the raw receiver"
WaitFrontendLineStartsWith "ready" timeout 5000ms
Settle 0ms
Expect:
  r00 |{ready}|
  r01 |{blank}|

Step "send paste and inspect every byte"
{actions}
Settle 0ms
Expect:
  r00 |{ready}|
  r01 |{hex}|
"#,
        expected.len(),
    );
    scenario::run_document(&document)?;
    Ok(())
}

#[test]
fn sends_multiline_paste_between_bracketed_paste_delimiters() -> Result<()> {
    assert_received(
        "Paste \"echo one\\necho two\"\nInput \"!\"",
        b"\x1b[200~echo one\necho two\x1b[201~!",
    )
}

#[test]
fn preserves_mixed_line_endings_unicode_quotes_tabs_and_backslashes() -> Result<()> {
    assert_received(
        concat!(
            r#"Paste "日本語\r\n\rnext\n\t\"quote\"\\""#,
            "\nInput \"!\""
        ),
        "\x1b[200~日本語\r\n\rnext\n\t\"quote\"\\\x1b[201~!".as_bytes(),
    )
}

#[test]
fn supports_empty_paste_and_keeps_adjacent_actions_in_order() -> Result<()> {
    assert_received(
        "Input \"A\"\nPaste \"\"\nPaste \"B\"\nInput enter\nInput \"!\"",
        b"A\x1b[200~\x1b[201~\x1b[200~B\x1b[201~\r!",
    )
}
