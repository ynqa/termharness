# termharness

Terminal application test harness backed by a pseudo-terminal and an ANSI screen model.

## Example

TermHarness scenarios are plain-text documents that use the `.th` file extension.
Each scenario defines the command to run, the terminal dimensions, user actions,
and the expected screen contents.

```text
Scenario "cursor_resize"
Command "zsh"
Arg "-fi"
Env PS1 "❯❯ "
Env RPS1 ""
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
```
