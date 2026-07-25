use alacritty_terminal::{
    Term,
    event::VoidListener,
    index::{Column, Line, Point},
    term::{Config, cell::Flags, test::TermSize},
    vte::ansi::Processor,
};
use unicode_width::UnicodeWidthStr;

use crate::error::{Error, Result};

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
    terminal: Term<VoidListener>,
}

impl Screen {
    /// Create a new screen with the given size.
    pub fn new(rows: usize, cols: usize) -> Self {
        let size = TermSize::new(cols, rows);
        Self {
            parser: Processor::new(),
            terminal: Term::new(Config::default(), &size, VoidListener),
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
        self.parser.advance(&mut self.terminal, bytes);
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

    /// Create a snapshot of the current visible screen content.
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
