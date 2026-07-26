use std::{iter::Peekable, str::Lines};

use thiserror::Error;
use unicode_width::UnicodeWidthStr;

use super::ast::{ActionAst, CursorAst, InputAst, KeyAst, ScenarioAst, StepAst, TerminalAst};

pub type Result<T> = std::result::Result<T, Error>;
const DEFAULT_EXPECT_TIMEOUT_MS: u64 = 2_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("failed to parse scenario at line {line}: {message}")]
    Parse { line: usize, message: String },
}

/// Parse a scenario document into its AST representation.
///
/// Constraints:
/// - Declarations are order-dependent.
pub fn parse(input: &str) -> crate::error::Result<ScenarioAst> {
    Parser::new(input).parse()
}

pub struct Parser<'a> {
    lines: Peekable<std::iter::Enumerate<Lines<'a>>>,
    current_line: usize,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            lines: input.lines().enumerate().peekable(),
            current_line: 0,
        }
    }

    pub fn parse(mut self) -> crate::error::Result<ScenarioAst> {
        self.parse_scenario().map_err(crate::error::Error::from)
    }

    fn parse_scenario(&mut self) -> Result<ScenarioAst> {
        let name = self.parse_named_string("Scenario")?;
        let command = self.parse_named_string("Command")?;
        let mut args = Vec::new();
        let mut env = Vec::new();
        loop {
            match self.peek_line() {
                Some(line) if line.starts_with("Arg ") => {
                    args.push(self.parse_named_string("Arg")?);
                }
                Some(line) if line.starts_with("Env ") => {
                    env.push(self.parse_env()?);
                }
                _ => break,
            }
        }
        let terminal = self.parse_terminal_declaration("Terminal")?;
        let cursor = if matches!(self.peek_line(), Some(line) if line.starts_with("Cursor ")) {
            self.parse_cursor(terminal)?
        } else {
            CursorAst {
                row: terminal.rows,
                col: 1,
            }
        };
        let steps = self.parse_steps(terminal)?;

        Ok(ScenarioAst {
            name,
            command,
            args,
            env,
            terminal,
            cursor,
            steps,
        })
    }

    fn parse_named_string(&mut self, keyword: &str) -> Result<String> {
        let line = self.next_line()?;
        let rest = line.strip_prefix(keyword).ok_or_else(|| {
            self.error_at_current_line(format!("expected `{keyword}` declaration"))
        })?;
        if !rest.starts_with(char::is_whitespace) {
            return Err(self.error_at_current_line(format!("expected `{keyword}` declaration")));
        }
        let value = rest.trim_start();
        Self::parse_quoted_string(value).ok_or_else(|| {
            self.error_at_current_line(format!("expected quoted value for `{keyword}`"))
        })
    }

    fn next_line(&mut self) -> Result<&'a str> {
        match self.lines.next() {
            Some((index, line)) => {
                self.current_line = index + 1;
                Ok(line)
            }
            None => Err(self.error_at_next_line("unexpected end of input")),
        }
    }

    fn parse_env(&mut self) -> Result<(String, String)> {
        let line = self.next_line()?;
        let declaration = line
            .strip_prefix("Env ")
            .ok_or_else(|| self.error_at_current_line("expected `Env` declaration"))?;
        let (name, value) = declaration
            .split_once(char::is_whitespace)
            .ok_or_else(|| self.error_at_current_line("expected `Env <name> \"<value>\"`"))?;
        if name.is_empty() || name.contains('=') {
            return Err(self.error_at_current_line("invalid environment variable name"));
        }
        let value = Self::parse_quoted_string(value.trim_start())
            .ok_or_else(|| self.error_at_current_line("expected quoted environment value"))?;
        Ok((name.to_string(), value))
    }

    fn parse_terminal_declaration(&mut self, keyword: &str) -> Result<TerminalAst> {
        let line = self.next_line()?;
        let rest = line.strip_prefix(keyword).ok_or_else(|| {
            self.error_at_current_line(format!("expected `{keyword}` declaration"))
        })?;
        let mut parts = rest.split_whitespace();

        let rows_label = parts.next().ok_or_else(|| {
            self.error_at_current_line(format!(
                "expected `rows <value> cols <value>` after `{keyword}`"
            ))
        })?;
        if rows_label != "rows" {
            return Err(self.error_at_current_line(format!("expected `rows` after `{keyword}`")));
        }

        let rows = parts
            .next()
            .ok_or_else(|| self.error_at_current_line("expected terminal rows value"))?
            .parse()
            .map_err(|_| self.error_at_current_line("expected terminal rows to be an integer"))?;

        let cols_label = parts
            .next()
            .ok_or_else(|| self.error_at_current_line("expected `cols` after terminal rows"))?;
        if cols_label != "cols" {
            return Err(self.error_at_current_line("expected `cols` after terminal rows"));
        }

        let cols = parts
            .next()
            .ok_or_else(|| self.error_at_current_line("expected terminal cols value"))?
            .parse()
            .map_err(|_| self.error_at_current_line("expected terminal cols to be an integer"))?;

        if parts.next().is_some() {
            return Err(self.error_at_current_line(format!(
                "unexpected trailing tokens in `{keyword}` declaration"
            )));
        }
        if rows == 0 || cols == 0 {
            return Err(self.error_at_current_line("terminal dimensions must be greater than zero"));
        }
        if rows > usize::from(u16::MAX) || cols > usize::from(u16::MAX) {
            return Err(self.error_at_current_line("terminal dimensions must fit in a u16"));
        }

        Ok(TerminalAst { rows, cols })
    }

    fn parse_cursor(&mut self, terminal: TerminalAst) -> Result<CursorAst> {
        let line = self.next_line()?;
        let rest = line
            .strip_prefix("Cursor")
            .ok_or_else(|| self.error_at_current_line("expected `Cursor` declaration"))?;
        let mut parts = rest.split_whitespace();

        if parts.next() != Some("row") {
            return Err(self.error_at_current_line("expected `row` after `Cursor`"));
        }
        let row = parts
            .next()
            .ok_or_else(|| self.error_at_current_line("expected cursor row value"))?
            .parse::<usize>()
            .map_err(|_| self.error_at_current_line("expected cursor row to be an integer"))?;

        if parts.next() != Some("col") {
            return Err(self.error_at_current_line("expected `col` after cursor row"));
        }
        let col = parts
            .next()
            .ok_or_else(|| self.error_at_current_line("expected cursor column value"))?
            .parse::<usize>()
            .map_err(|_| self.error_at_current_line("expected cursor column to be an integer"))?;

        if parts.next().is_some() {
            return Err(
                self.error_at_current_line("unexpected trailing tokens in `Cursor` declaration")
            );
        }
        if !(1..=terminal.rows).contains(&row) || !(1..=terminal.cols).contains(&col) {
            return Err(self.error_at_current_line(format!(
                "cursor position row {row} col {col} is outside terminal size {}x{}",
                terminal.rows, terminal.cols
            )));
        }

        Ok(CursorAst { row, col })
    }

    fn parse_steps(&mut self, mut terminal: TerminalAst) -> Result<Vec<StepAst>> {
        let mut steps = Vec::new();

        loop {
            self.skip_blank_lines();
            if self.peek_line().is_none() {
                break;
            }

            let label = self.parse_named_string("Step")?;
            let actions = self.parse_actions(&mut terminal)?;
            let settle_ms = self.parse_settle()?;
            let (expect_timeout_ms, expect) = self.parse_expect(terminal)?;

            steps.push(StepAst {
                label,
                actions,
                settle_ms,
                expect_timeout_ms,
                expect,
            });
        }

        Ok(steps)
    }

    fn parse_actions(&mut self, terminal: &mut TerminalAst) -> Result<Vec<ActionAst>> {
        let mut actions = Vec::new();

        loop {
            let action = match self.peek_line() {
                Some(line) if line.starts_with("Input ") => ActionAst::Input(self.parse_input()?),
                Some(line) if line.starts_with("WaitPtyOutputContains ") => {
                    self.parse_wait_pty_output_contains()?
                }
                Some(line) if line.starts_with("WaitScreenLineStartsWith ") => {
                    self.parse_wait_screen_line_starts_with()?
                }
                Some(line) if line.starts_with("Resize ") => {
                    *terminal = self.parse_terminal_declaration("Resize")?;
                    ActionAst::Resize(*terminal)
                }
                _ => break,
            };
            actions.push(action);
        }

        Ok(actions)
    }

    fn parse_input(&mut self) -> Result<InputAst> {
        let line = self.next_line()?;
        let value = line
            .strip_prefix("Input ")
            .ok_or_else(|| self.error_at_current_line("expected `Input` declaration"))?;

        if value.starts_with('"') {
            return Self::parse_quoted_string(value)
                .map(InputAst::Text)
                .ok_or_else(|| self.error_at_current_line("expected quoted input text"));
        }

        let mut parts = value.split_whitespace();
        let key = match parts.next() {
            Some("left") => KeyAst::Left,
            Some("right") => KeyAst::Right,
            Some("up") => KeyAst::Up,
            Some("down") => KeyAst::Down,
            Some("enter") => KeyAst::Enter,
            Some("backspace") => KeyAst::Backspace,
            Some("tab") => KeyAst::Tab,
            Some("escape") => KeyAst::Escape,
            Some(key) => {
                return Err(self.error_at_current_line(format!("unsupported input key `{key}`")));
            }
            None => return Err(self.error_at_current_line("expected input text or key")),
        };

        let count = match parts.next() {
            Some(count) => count
                .parse::<u16>()
                .map_err(|_| self.error_at_current_line("expected key count to be a u16"))?,
            None => 1,
        };
        if count == 0 {
            return Err(self.error_at_current_line("key count must be greater than zero"));
        }
        if parts.next().is_some() {
            return Err(self.error_at_current_line("unexpected trailing tokens in `Input`"));
        }

        Ok(InputAst::Key { key, count })
    }

    fn parse_wait_pty_output_contains(&mut self) -> Result<ActionAst> {
        let line = self.next_line()?;
        let value = line.strip_prefix("WaitPtyOutputContains ").ok_or_else(|| {
            self.error_at_current_line("expected `WaitPtyOutputContains` declaration")
        })?;
        let (text, rest) = Self::parse_leading_quoted_string(value)
            .ok_or_else(|| self.error_at_current_line("expected quoted output text"))?;
        if text.is_empty() {
            return Err(self.error_at_current_line("wait output text must not be empty"));
        }

        let mut parts = rest.split_whitespace();
        if parts.next() != Some("timeout") {
            return Err(
                self.error_at_current_line("expected `timeout <milliseconds>ms` after output text")
            );
        }
        let timeout_ms = parts
            .next()
            .and_then(|value| value.strip_suffix("ms"))
            .ok_or_else(|| self.error_at_current_line("expected output timeout in milliseconds"))?
            .parse::<u64>()
            .map_err(|_| self.error_at_current_line("expected output timeout to be an integer"))?;
        if parts.next().is_some() {
            return Err(
                self.error_at_current_line("unexpected trailing tokens in `WaitPtyOutputContains`")
            );
        }

        Ok(ActionAst::WaitPtyOutputContains {
            text: text.to_string(),
            timeout_ms,
        })
    }

    fn parse_wait_screen_line_starts_with(&mut self) -> Result<ActionAst> {
        let line = self.next_line()?;
        let value = line
            .strip_prefix("WaitScreenLineStartsWith ")
            .ok_or_else(|| {
                self.error_at_current_line("expected `WaitScreenLineStartsWith` declaration")
            })?;
        let (text, rest) = Self::parse_leading_quoted_string(value)
            .ok_or_else(|| self.error_at_current_line("expected quoted screen line prefix"))?;
        if text.is_empty() {
            return Err(self.error_at_current_line("screen line prefix must not be empty"));
        }

        let mut parts = rest.split_whitespace();
        if parts.next() != Some("timeout") {
            return Err(self.error_at_current_line(
                "expected `timeout <milliseconds>ms` after screen line prefix",
            ));
        }
        let timeout_ms = parts
            .next()
            .and_then(|value| value.strip_suffix("ms"))
            .ok_or_else(|| self.error_at_current_line("expected screen timeout in milliseconds"))?
            .parse::<u64>()
            .map_err(|_| self.error_at_current_line("expected screen timeout to be an integer"))?;
        if parts.next().is_some() {
            return Err(self.error_at_current_line(
                "unexpected trailing tokens in `WaitScreenLineStartsWith`",
            ));
        }

        Ok(ActionAst::WaitScreenLineStartsWith {
            text: text.to_string(),
            timeout_ms,
        })
    }

    fn parse_settle(&mut self) -> Result<u64> {
        let line = self.next_line()?;
        let value = line
            .strip_prefix("Settle ")
            .and_then(|value| value.strip_suffix("ms"))
            .ok_or_else(|| self.error_at_current_line("expected `Settle <milliseconds>ms`"))?;
        value
            .parse()
            .map_err(|_| self.error_at_current_line("expected settle duration to be an integer"))
    }

    fn parse_expect(&mut self, terminal: TerminalAst) -> Result<(u64, Vec<String>)> {
        let line = self.next_line()?;
        let timeout_ms = if line == "Expect:" {
            DEFAULT_EXPECT_TIMEOUT_MS
        } else {
            line.strip_prefix("Expect timeout ")
                .and_then(|value| value.strip_suffix("ms:"))
                .ok_or_else(|| {
                    self.error_at_current_line(
                        "expected `Expect:` or `Expect timeout <milliseconds>ms:`",
                    )
                })?
                .parse::<u64>()
                .map_err(|_| self.error_at_current_line("expected timeout to be an integer"))?
        };

        let mut expected = Vec::with_capacity(terminal.rows);
        for row in 0..terminal.rows {
            let line = self.next_line()?;
            let rest = line
                .strip_prefix("  r")
                .ok_or_else(|| self.error_at_current_line("expected screen row"))?;
            let (row_label, content) = rest
                .split_once(" |")
                .ok_or_else(|| self.error_at_current_line("expected `rNN |content|`"))?;
            let parsed_row = row_label
                .parse::<usize>()
                .map_err(|_| self.error_at_current_line("expected numeric screen row"))?;
            if parsed_row != row {
                return Err(self.error_at_current_line(format!(
                    "expected screen row r{row:02}, found r{parsed_row:02}"
                )));
            }

            let content = content
                .strip_suffix('|')
                .ok_or_else(|| self.error_at_current_line("expected closing `|` for screen row"))?;
            let width = content.width();
            if width != terminal.cols {
                return Err(self.error_at_current_line(format!(
                    "expected screen row width {}, found {width}",
                    terminal.cols
                )));
            }
            expected.push(content.replace('·', " "));
        }

        Ok((timeout_ms, expected))
    }

    fn peek_line(&mut self) -> Option<&'a str> {
        self.lines.peek().map(|(_, line)| *line)
    }

    fn skip_blank_lines(&mut self) {
        while matches!(self.peek_line(), Some(line) if line.trim().is_empty()) {
            let _ = self.lines.next();
        }
    }

    fn parse_quoted_string(input: &str) -> Option<String> {
        let content = input.strip_prefix('"')?.strip_suffix('"')?;
        if content.contains('"') {
            return None;
        }
        Some(content.to_string())
    }

    fn parse_leading_quoted_string(input: &str) -> Option<(&str, &str)> {
        let input = input.strip_prefix('"')?;
        let closing_quote = input.find('"')?;
        let (content, rest) = input.split_at(closing_quote);
        Some((content, rest.strip_prefix('"')?.trim_start()))
    }

    fn error_at_current_line(&mut self, message: impl Into<String>) -> Error {
        Error::Parse {
            line: self.current_line.max(1),
            message: message.into(),
        }
    }

    fn error_at_next_line(&mut self, message: impl Into<String>) -> Error {
        let line = self
            .lines
            .peek()
            .map(|(index, _)| index + 1)
            .unwrap_or(self.current_line + 1);
        Error::Parse {
            line,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    mod parse_scenario {
        use super::*;

        #[test]
        fn parses_header_into_scenario_ast() {
            let input = indoc! {r#"
                Scenario "mid_buffer_insert_wrap"
                Command "zsh"
                Arg "-fi"
                Env PS1 "❯❯ "
                Terminal rows 10 cols 40
                Cursor row 5 col 2
            "#};

            assert_eq!(
                Parser::new(input)
                    .parse_scenario()
                    .expect("header should parse successfully"),
                ScenarioAst {
                    name: "mid_buffer_insert_wrap".to_string(),
                    command: "zsh".to_string(),
                    args: vec!["-fi".to_string()],
                    env: vec![("PS1".to_string(), "❯❯ ".to_string())],
                    terminal: TerminalAst { rows: 10, cols: 40 },
                    cursor: CursorAst { row: 5, col: 2 },
                    steps: Vec::new(),
                }
            );
        }

        #[test]
        fn parses_steps_into_scenario_ast() {
            let input = indoc! {r#"
                Scenario "typing"
                Command "zsh"
                Arg "-fi"
                Env PS1 "❯❯ "
                Terminal rows 2 cols 5

                Step "spawn"
                Settle 300ms
                Expect:
                  r00 |·····|
                  r01 |❯❯···|

                Step "type"
                Input "hi"
                Settle 100ms
                Expect:
                  r00 |·····|
                  r01 |❯❯·hi|

                Step "left"
                Input left 2
                Settle 50ms
                Expect:
                  r00 |·····|
                  r01 |❯❯·hi|

                Step "resize"
                Resize rows 2 cols 4
                Settle 50ms
                Expect:
                  r00 |····|
                  r01 |❯❯·h|
            "#};

            let scenario = Parser::new(input)
                .parse_scenario()
                .expect("scenario should parse");

            assert_eq!(scenario.steps.len(), 4);
            assert_eq!(scenario.cursor, CursorAst { row: 2, col: 1 });
            assert!(scenario.steps[0].actions.is_empty());
            assert_eq!(scenario.steps[0].settle_ms, 300);
            assert_eq!(scenario.steps[0].expect_timeout_ms, 2_000);
            assert_eq!(
                scenario.steps[1].actions,
                vec![ActionAst::Input(InputAst::Text("hi".to_string()))]
            );
            assert_eq!(
                scenario.steps[2].actions,
                vec![ActionAst::Input(InputAst::Key {
                    key: KeyAst::Left,
                    count: 2,
                })]
            );
            assert_eq!(
                scenario.steps[3].actions,
                vec![ActionAst::Resize(TerminalAst { rows: 2, cols: 4 })]
            );
            assert_eq!(scenario.steps[1].expect, vec!["     ", "❯❯ hi"]);
            assert_eq!(scenario.steps[3].expect, vec!["    ", "❯❯ h"]);
        }

        #[test]
        fn parses_multiple_actions_and_expect_timeout() {
            let input = indoc! {r#"
                Scenario "action sequence"
                Command "true"
                Terminal rows 1 cols 4

                Step "race output and resize"
                WaitScreenLineStartsWith "❯❯" timeout 1000ms
                Input "go"
                WaitPtyOutputContains "ready" timeout 1000ms
                Resize rows 1 cols 5
                Resize rows 1 cols 6
                Settle 300ms
                Expect timeout 0ms:
                  r00 |······|
            "#};

            let scenario = Parser::new(input)
                .parse_scenario()
                .expect("action sequence should parse");
            let step = &scenario.steps[0];

            assert_eq!(
                step.actions,
                vec![
                    ActionAst::WaitScreenLineStartsWith {
                        text: "❯❯".to_string(),
                        timeout_ms: 1_000,
                    },
                    ActionAst::Input(InputAst::Text("go".to_string())),
                    ActionAst::WaitPtyOutputContains {
                        text: "ready".to_string(),
                        timeout_ms: 1_000,
                    },
                    ActionAst::Resize(TerminalAst { rows: 1, cols: 5 }),
                    ActionAst::Resize(TerminalAst { rows: 1, cols: 6 }),
                ]
            );
            assert_eq!(step.settle_ms, 300);
            assert_eq!(step.expect_timeout_ms, 0);
            assert_eq!(step.expect, vec!["      "]);
        }

        #[test]
        fn parses_mid_buffer_insert_wrap_example() {
            let scenario =
                Parser::new(include_str!("../../examples/mid_buffer_insert_wrap.zsh.th"))
                    .parse_scenario()
                    .expect("example scenario should parse");

            assert_eq!(scenario.name, "mid_buffer_insert_wrap");
            assert_eq!(scenario.command, "zsh");
            assert_eq!(scenario.args, vec!["-fi"]);
            assert_eq!(
                scenario.env,
                vec![
                    ("PS1".to_string(), "❯❯ ".to_string()),
                    ("RPS1".to_string(), String::new()),
                    ("PROMPT_EOL_MARK".to_string(), String::new()),
                ]
            );
            assert_eq!(scenario.terminal, TerminalAst { rows: 10, cols: 40 });
            assert_eq!(scenario.cursor, CursorAst { row: 10, col: 1 });
            assert_eq!(scenario.steps.len(), 4);
            assert_eq!(
                scenario.steps[2].actions,
                vec![ActionAst::Input(InputAst::Key {
                    key: KeyAst::Left,
                    count: 36,
                })]
            );
        }
    }
}
