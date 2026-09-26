/// A parsed scenario document before it is lowered into an executable scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioAst {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub terminal: TerminalAst,
    pub cursor: CursorAst,
    pub steps: Vec<StepAst>,
}

/// Terminal dimensions declared in the scenario header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalAst {
    pub rows: usize,
    pub cols: usize,
}

/// Initial cursor position declared in the scenario header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorAst {
    /// 1-based terminal row.
    pub row: usize,
    /// 1-based terminal column.
    pub col: usize,
}

/// A single scenario step with ordered actions and an expected screen snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepAst {
    pub label: String,
    pub actions: Vec<ActionAst>,
    pub settle_ms: u64,
    pub expect_timeout_ms: u64,
    pub expect: Vec<String>,
}

/// An action performed by a scenario step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionAst {
    Input(InputAst),
    /// Wait for a line prefix in raw PTY output, splitting lines at LF bytes.
    WaitBackendLineStartsWith {
        text: String,
        timeout_ms: u64,
    },
    /// Wait for a line prefix in the emulated terminal viewport.
    WaitFrontendLineStartsWith {
        text: String,
        timeout_ms: u64,
    },
    Resize(TerminalAst),
    Scroll {
        direction: ScrollDirection,
        lines: u16,
    },
}

/// Direction of viewport movement through terminal scrollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
}

/// User input represented in the scenario document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAst {
    Text(String),
    Key { key: KeyAst, count: u16 },
}

/// Special keys supported by the scenario document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAst {
    Left,
    Right,
    Up,
    Down,
    Enter,
    Backspace,
    Tab,
    Escape,
}
