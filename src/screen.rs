use alacritty_terminal::{
    Term,
    event::{Event, EventListener},
    grid::Scroll,
    index::{Column, Line, Point},
    term::{Config, cell::Flags, test::TermSize},
    vte::ansi::Processor,
};
use std::sync::{Arc, Mutex};
use unicode_width::UnicodeWidthStr;

use crate::error::{Error, Result};

#[derive(Clone, Default)]
struct TerminalResponses(Arc<Mutex<Vec<u8>>>);

impl EventListener for TerminalResponses {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(response) = event {
            self.0
                .lock()
                .expect("failed to lock terminal responses")
                .extend_from_slice(response.as_bytes());
        }
    }
}

/// Pad the given content to fit within the specified number of columns.
/// e.g. if the content is "Hello" and cols is 10, the result will be "Hello     ".
fn pad_to_cols(cols: usize, content: &str) -> Result<String> {
    let width = content.width();
    if width > cols {
        return Err(Error::ContentExceedsColumn {
            column: cols,
            width,
        });
    }

    let mut line = String::from(content);
    line.push_str(&" ".repeat(cols - width));
    Ok(line)
}

/// A simple screen that can be used for testing.
pub struct Screen {
    /// ANSI parser for processing input.
    parser: Processor,
    terminal: Term<TerminalResponses>,
    responses: TerminalResponses,
}

impl Screen {
    /// Create a new screen with the given size.
    pub fn new(rows: usize, cols: usize) -> Self {
        let size = TermSize::new(cols, rows);
        let responses = TerminalResponses::default();
        Self {
            parser: Processor::new(),
            terminal: Term::new(Config::default(), &size, responses.clone()),
            responses,
        }
    }

    /// Create a new screen with the given size and cursor position.
    pub fn new_with_cursor(rows: usize, cols: usize, cursor_x: usize, cursor_y: usize) -> Self {
        let mut screen = Self::new(rows, cols);
        screen.set_cursor_position(cursor_x, cursor_y);
        screen
    }

    /// Process bytes as terminal input and update the screen state.
    pub fn process(&mut self, bytes: &[u8]) {
        let _ = self.process_with_responses(bytes);
    }

    /// Capture replies at the point each query is parsed, before later output moves the cursor.
    pub(crate) fn process_with_responses(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.parser.advance(&mut self.terminal, bytes);
        std::mem::take(
            &mut *self
                .responses
                .0
                .lock()
                .expect("failed to lock terminal responses"),
        )
    }

    /// Get the current cursor position as (row, column).
    pub fn cursor_position(&self) -> (usize, usize) {
        let point = self.terminal.grid().cursor.point;
        let row = usize::try_from(point.line.0).expect("cursor row should be non-negative");
        let col = point.column.0;
        (row, col)
    }

    fn set_cursor_position(&mut self, cursor_x: usize, cursor_y: usize) {
        let cursor = &mut self.terminal.grid_mut().cursor;
        cursor.point = Point::new(Line(i32::from(cursor_y as u16)), Column(cursor_x));
        // Clear any pending auto-wrap because the cursor position was set explicitly.
        cursor.input_needs_wrap = false;
    }

    /// Resize the screen to the given number of rows and columns.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let size = TermSize::new(cols, rows);
        self.terminal.resize(size);
    }

    /// Move the viewport toward older output, clamping at the oldest retained line.
    /// This does not move the application's cursor or change terminal contents.
    pub fn scroll_up(&mut self, lines: u16) {
        self.terminal
            .scroll_display(Scroll::Delta(i32::from(lines)));
    }

    /// Move the viewport toward live output, clamping at the live screen.
    /// This does not move the application's cursor or change terminal contents.
    pub fn scroll_down(&mut self, lines: u16) {
        self.terminal
            .scroll_display(Scroll::Delta(-i32::from(lines)));
    }

    /// Create a snapshot of the current visible screen content, including scrollback
    /// when the viewport has been scrolled.
    pub fn snapshot(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current_line = None;

        // display_iter() yields visible screen cells in row-major order.
        // For a single row, cells arrive column by column (for example: 01, 02, 03, ...),
        // so start a new String whenever the row changes.
        for indexed in self.terminal.grid().display_iter() {
            if current_line != Some(indexed.point.line.0) {
                lines.push(String::new());
                current_line = Some(indexed.point.line.0);
            }

            let line = lines
                .last_mut()
                .expect("display iterator should yield rows");

            // Wide characters like 'あ' occupy two terminal cells.
            // The trailing cell is marked as WIDE_CHAR_SPACER, so skip it here
            // and emit only the leading cell's character.
            if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            line.push(indexed.cell.c);

            // Reconstruct graphemes that are stored as a base character plus
            // zero-width codepoints, for example 'e' + combining accent => 'é'.
            if let Some(zerowidth) = indexed.cell.zerowidth() {
                for ch in zerowidth {
                    line.push(*ch);
                }
            }
        }

        lines
    }

    /// Create a snapshot of the current screen content resized to the given dimensions.
    pub fn snapshot_with_size(&self, rows: usize, cols: usize) -> Result<Vec<String>> {
        let mut lines = self.snapshot();
        lines.resize(rows, String::new());
        lines
            .into_iter()
            .map(|line| pad_to_cols(cols, &line))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod process_with_responses {
        use super::*;

        #[test]
        fn replies_are_independent_of_read_boundaries() {
            let input = b"\x1b[2;3H\x1b[6n\x1b[4;5H\x1b[06n\x1b[1;1H";
            for boundary in 0..=input.len() {
                let mut screen = Screen::new(10, 40);
                let mut replies = screen.process_with_responses(&input[..boundary]);
                replies.extend(screen.process_with_responses(&input[boundary..]));
                assert_eq!(replies, b"\x1b[2;3R\x1b[4;5R", "split at {boundary}");
                assert_eq!(screen.cursor_position(), (0, 0));
            }
        }

        #[test]
        fn canceled_queries_do_not_emit_replies() {
            let mut screen = Screen::new(10, 40);
            assert!(screen.process_with_responses(b"\x1b[6\x18n").is_empty());
        }

        #[test]
        fn standalone_processing_does_not_retain_replies() {
            let mut screen = Screen::new(10, 40);
            screen.process(b"\x1b[6n");
            assert!(screen.process_with_responses(b"text").is_empty());
        }
    }

    mod scroll {
        use super::*;

        #[test]
        fn moves_in_both_directions_and_clamps_without_moving_the_cursor() {
            let mut screen = Screen::new(3, 3);
            screen.process(b"1\r\n2\r\n3\r\n4\r\n5");
            let cursor = screen.cursor_position();
            assert_eq!(screen.snapshot(), vec!["3  ", "4  ", "5  "]);

            screen.scroll_up(1);
            assert_eq!(screen.snapshot(), vec!["2  ", "3  ", "4  "]);
            assert_eq!(screen.cursor_position(), cursor);

            screen.scroll_up(u16::MAX);
            assert_eq!(screen.snapshot(), vec!["1  ", "2  ", "3  "]);
            assert_eq!(screen.cursor_position(), cursor);

            screen.scroll_down(1);
            assert_eq!(screen.snapshot(), vec!["2  ", "3  ", "4  "]);
            assert_eq!(screen.cursor_position(), cursor);

            screen.scroll_down(u16::MAX);
            assert_eq!(screen.snapshot(), vec!["3  ", "4  ", "5  "]);
            assert_eq!(screen.cursor_position(), cursor);
        }

        #[test]
        fn keeps_the_viewport_when_more_output_arrives() {
            let mut screen = Screen::new(3, 3);
            screen.process(b"1\r\n2\r\n3\r\n4\r\n5");
            screen.scroll_up(2);
            screen.process(b"\r\n6");
            assert_eq!(screen.snapshot(), vec!["1  ", "2  ", "3  "]);

            screen.scroll_down(u16::MAX);
            assert_eq!(screen.snapshot(), vec!["4  ", "5  ", "6  "]);
        }

        #[test]
        fn does_not_scroll_without_history() {
            let mut screen = Screen::new(2, 3);
            screen.process(b"abc");
            let before = screen.snapshot();
            screen.scroll_up(u16::MAX);
            assert_eq!(screen.snapshot(), before);
            screen.scroll_down(u16::MAX);
            assert_eq!(screen.snapshot(), before);
        }
    }

    mod snapshot {
        use super::*;

        #[test]
        fn uses_current_screen_size() {
            let mut screen = Screen::new(2, 5);
            screen.process("abc".as_bytes());

            let snapshot = screen.snapshot();
            assert_eq!(snapshot, vec!["abc  ", "     "]);
        }
    }

    mod snapshot_with_size {
        use super::*;

        #[test]
        fn empty_screen() -> Result<()> {
            let screen = Screen::new(3, 5);
            let snapshot = screen.snapshot_with_size(3, 5)?;
            assert_eq!(snapshot, vec!["     ", "     ", "     "]);
            Ok(())
        }

        #[test]
        fn ascii_text() -> Result<()> {
            let mut screen = Screen::new(2, 5);
            screen.process("abc".as_bytes());

            let snapshot = screen.snapshot_with_size(2, 5)?;
            assert_eq!(snapshot, vec!["abc  ", "     "]);
            Ok(())
        }

        #[test]
        fn combining_character() -> Result<()> {
            let mut screen = Screen::new(1, 4);
            screen.process("é".as_bytes());

            let snapshot = screen.snapshot_with_size(1, 4)?;
            assert_eq!(snapshot, vec!["é   "]);
            Ok(())
        }

        #[test]
        fn wide_character() -> Result<()> {
            let mut screen = Screen::new(1, 4);
            screen.process("あ".as_bytes());

            let snapshot = screen.snapshot_with_size(1, 4)?;
            assert_eq!(snapshot, vec!["あ  "]);
            Ok(())
        }

        #[test]
        fn emoji_with_skin_tone_modifier() -> Result<()> {
            let mut screen = Screen::new(1, 4);
            screen.process("👍🏻".as_bytes());

            let snapshot = screen.snapshot_with_size(1, 4)?;
            assert_eq!(snapshot, vec!["👍🏻  "]);
            Ok(())
        }

        #[test]
        fn shrinking_columns_reflows_wrapped_lines() -> Result<()> {
            let mut screen = Screen::new(3, 8);
            screen.process("abcdefghij".as_bytes());

            let before = screen.snapshot_with_size(3, 8)?;
            assert_eq!(before, vec!["abcdefgh", "ij      ", "        "]);

            screen.resize(3, 6);

            let snapshot = screen.snapshot_with_size(3, 6)?;
            assert_eq!(snapshot, vec!["abcdef", "ghij  ", "      "]);
            Ok(())
        }

        #[test]
        fn expanding_columns_reflows_wrapped_lines() -> Result<()> {
            let mut screen = Screen::new(3, 6);
            screen.process("abcdefghij".as_bytes());

            let before = screen.snapshot_with_size(3, 6)?;
            assert_eq!(before, vec!["abcdef", "ghij  ", "      "]);

            screen.resize(3, 8);

            let snapshot = screen.snapshot_with_size(3, 8)?;
            assert_eq!(snapshot, vec!["abcdefgh", "ij      ", "        "]);
            Ok(())
        }

        #[test]
        fn expanding_rows_adds_empty_lines() -> Result<()> {
            let mut screen = Screen::new(1, 5);
            screen.process("abc".as_bytes());

            let before = screen.snapshot_with_size(1, 5)?;
            assert_eq!(before, vec!["abc  "]);

            screen.resize(3, 5);

            let snapshot = screen.snapshot_with_size(3, 5)?;
            assert_eq!(snapshot, vec!["abc  ", "     ", "     "]);
            Ok(())
        }

        #[test]
        fn shrinking_rows_keeps_visible_bottom_lines() -> Result<()> {
            let mut screen = Screen::new(3, 5);
            screen.process("111112222233333".as_bytes());

            let before = screen.snapshot_with_size(3, 5)?;
            assert_eq!(before, vec!["11111", "22222", "33333"]);

            screen.resize(2, 5);

            let snapshot = screen.snapshot_with_size(2, 5)?;
            assert_eq!(snapshot, vec!["22222", "33333"]);
            Ok(())
        }

        #[test]
        fn shrinking_then_expanding_columns_roundtrips_ascii_content() -> Result<()> {
            let mut screen = Screen::new(3, 8);
            screen.process("abcdefghij".as_bytes());

            let before = screen.snapshot_with_size(3, 8)?;
            assert_eq!(before, vec!["abcdefgh", "ij      ", "        "]);

            screen.resize(3, 6);
            let shrunk = screen.snapshot_with_size(3, 6)?;
            assert_eq!(shrunk, vec!["abcdef", "ghij  ", "      "]);

            screen.resize(3, 8);

            let snapshot = screen.snapshot_with_size(3, 8)?;
            assert_eq!(snapshot, vec!["abcdefgh", "ij      ", "        "]);
            Ok(())
        }

        #[test]
        fn expanding_then_shrinking_rows_roundtrips_visible_lines() -> Result<()> {
            let mut screen = Screen::new(2, 5);
            screen.process("1111122222".as_bytes());

            let before = screen.snapshot_with_size(2, 5)?;
            assert_eq!(before, vec!["11111", "22222"]);

            screen.resize(3, 5);
            let expanded = screen.snapshot_with_size(3, 5)?;
            assert_eq!(expanded, vec!["11111", "22222", "     "]);

            screen.resize(2, 5);

            let snapshot = screen.snapshot_with_size(2, 5)?;
            assert_eq!(snapshot, vec!["11111", "22222"]);
            Ok(())
        }
    }
}
