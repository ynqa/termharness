# termharness

Terminal application test harness backed by a pseudo-terminal and an ANSI screen model.

## Example

```text
Scenario "typing"
Command "cat"
Terminal rows 2 cols 8

Step "type text"
Input "hello"
Settle 100ms
Expect:
  r00 |········|
  r01 |hello···|
```

## Syntax

### Scenario header

| Syntax | Purpose | Requirement / default |
| --- | --- | --- |
| `Scenario "name"` | Name the scenario. | Required; the first declaration in the document. |
| `Command "program"` | Choose the executable to run. | Required after `Scenario`; pass arguments separately with `Arg`. |
| `Arg "value"` | Append one command-line argument. | Optional between `Command` and `Terminal`; may be repeated and interleaved with `Env`. |
| `Env NAME "value"` | Set an environment variable for the process. | Optional between `Command` and `Terminal`; may be repeated and interleaved with `Arg`. |
| `Terminal rows <rows> cols <cols>` | Set the initial terminal dimensions. | Required after `Command` and any `Arg` / `Env` declarations; each dimension is 1–65535. |
| `Cursor row <row> col <col>` | Set the initial cursor position. | Optional immediately after `Terminal`; defaults to the last row, column 1. Coordinates are 1-based and must be inside the terminal. |

### Steps and actions

| Syntax | Purpose | Notes |
| --- | --- | --- |
| `Step "label"` | Start a step. | Follow with zero or more actions, then `Settle` and `Expect`. Actions run in the order written without implicit waits. |
| `Input "text"` | Send text to the application. | Does not append Enter. |
| `Input <key> [count]` | Send a special key one or more times. | Keys: `left`, `right`, `up`, `down`, `enter`, `backspace`, `tab`, `escape`. Count defaults to 1; range: 1–65535. |
| `WaitPtyOutputContains "text" timeout <ms>ms` | Wait until raw PTY output contains the text. | Text must be nonempty; timeout is required. Polls every millisecond and fails when the timeout expires. |
| `WaitScreenLineStartsWith "text" timeout <ms>ms` | Wait until a visible screen line starts with the text. | Text must be nonempty; timeout is required. Inspects the current viewport, polling every millisecond; fails when the timeout expires. |
| `Resize rows <rows> cols <cols>` | Resize the terminal. | Each dimension is 1–65535. Subsequent expectations use the new dimensions. |
| `Scroll up <lines>` | Move the viewport toward older output. | Line count is required; range: 1–65535. Stops at the oldest retained line. |
| `Scroll down <lines>` | Move the viewport toward the live screen. | Line count is required; range: 1–65535. Stops at the live screen. |
| `Settle <ms>ms` | Wait unconditionally after the step's actions. | Required; use `Settle 0ms` for no delay. |

### Expected screen

| Syntax | Purpose | Notes |
| --- | --- | --- |
| `Expect:` | Wait for the visible screen to match the following rows. | Required after `Settle`; defaults to a 2000 ms timeout. |
| `Expect timeout <ms>ms:` | Set an explicit timeout for the screen match. | Alternative to `Expect:`; `0ms` checks immediately. |
| `  r00 \|content\|` | Declare one expected screen row. | Indent with exactly two spaces. Include every row in order, starting at `r00`; each row's display width must equal the terminal's column count. |
| `·` | Represent a space inside an expected row. | Literal spaces are also accepted. |

### Notation

| Notation | Meaning |
| --- | --- |
| `.th` | Plain-text scenario file defining the command, terminal dimensions, actions, and expected screen contents. |
| `<value>` | A placeholder to replace; do not write the angle brackets. |
| `[count]` | An optional argument; do not write the square brackets. |
| `"text"` | A literal double-quoted string on one line. Escape sequences are not interpreted, and embedded double quotes are unsupported. |
| `<ms>ms` | A nonnegative integer duration in milliseconds, such as `100ms`. |
| Blank lines | Allowed before and between steps, and after the final step. Do not insert them within the header or a step. |
