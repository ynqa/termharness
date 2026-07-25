const CURSOR_POSITION_REQUEST: &[u8] = b"\x1b[6n"; // ESC [ 6 n
pub const CURSOR_POSITION_REQUEST_LEN: usize = CURSOR_POSITION_REQUEST.len();

/// Searches for the cursor position request escape sequence in the given buffer
/// and returns its starting index if found.
pub fn find_cursor_position_request(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(CURSOR_POSITION_REQUEST_LEN)
        .position(|window| window == CURSOR_POSITION_REQUEST)
}

/// Count cursor position request escape sequences in the given buffer.
pub fn cursor_position_request_count(buffer: &[u8]) -> usize {
    buffer
        .windows(CURSOR_POSITION_REQUEST_LEN)
        .filter(|window| *window == CURSOR_POSITION_REQUEST)
        .count()
}

/// Generates a cursor position response escape sequence for the given row and column.
pub fn cursor_position_response(row: usize, col: usize) -> String {
    format!("\x1b[{row};{col}R")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_cursor_position_requests() {
        assert_eq!(cursor_position_request_count(b"\x1b[6ntext\x1b[6n"), 2);
    }
}
