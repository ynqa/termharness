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

/// A single scenario step with an optional action and expected screen snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepAst {
    pub label: String,
    pub action: Option<ActionAst>,
    pub settle_ms: u64,
    pub expect: Vec<String>,
}

/// An action performed by a scenario step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionAst {
    Input(InputAst),
    Resize(TerminalAst),
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
