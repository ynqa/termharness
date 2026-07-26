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

A step can contain multiple actions. Actions run in the order written without
an implicit delay or screen assertion between them. This makes timing-sensitive
interactions reproducible in a scenario document.

```text
Step "type and resize while rendering"
WaitScreenLinePrefix "❯❯" timeout 1000ms
Input "Terminal prompts should remain stable when the window shrinks and expands again"
WaitOutput "Terminal" timeout 1000ms
Resize rows 10 cols 59
Resize rows 10 cols 58
Resize rows 10 cols 57
Resize rows 10 cols 58
Resize rows 10 cols 59
Resize rows 10 cols 60
Settle 300ms
Expect timeout 0ms:
  r00 |····························································|
  r01 |····························································|
  r02 |····························································|
  r03 |····························································|
  r04 |····························································|
  r05 |····························································|
  r06 |Build·completed·successfully.·······························|
  r07 |Hi!·························································|
  r08 |❯❯·Terminal·prompts·should·remain·stable·when·the·window·shr|
  r09 |inks·and·expands·again······································|
```

`WaitOutput` polls the raw PTY output and proceeds as soon as the requested
byte sequence appears. `WaitScreenLinePrefix` polls the emulated screen until
a visible line starts with the requested text. Both use a one-millisecond poll
interval and fail when their declared timeout expires.

`Settle` is an unconditional delay after all actions in the step. `Expect`
then compares the screen. Plain `Expect:` retains the default two-second match
grace period. `Expect timeout <milliseconds>ms:` overrides it; use
`Expect timeout 0ms:` for an immediate assertion after `Settle`.
