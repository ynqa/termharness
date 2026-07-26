use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use portable_pty::CommandBuilder;

use crate::{
    error::{Error, Result},
    scenario::ast::{ActionAst, InputAst, KeyAst, ScenarioAst},
    session::Session,
};

pub mod ast;
pub mod formatter;
pub mod parser;

pub type ActionResult = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>;
pub type Action = Arc<dyn Fn(&mut Session) -> ActionResult + Send + Sync>;

const SCREEN_POLL_INTERVAL: Duration = Duration::from_millis(10);
const OUTPUT_POLL_INTERVAL: Duration = Duration::from_millis(1);
const OUTPUT_ERROR_TAIL_BYTES: usize = 500;

/// Represent a test scenario consisting of multiple steps,
/// where each step has a label, a settle duration,
/// and an action to perform on a session.
pub struct Scenario {
    pub name: String,
    pub steps: Vec<Step>,
}

/// Represent a single step in a scenario,
/// with a label, settle duration, and action.
pub struct Step {
    pub label: String,
    /// Duration to wait after performing the action
    /// before proceeding to the next step.
    pub settle: Duration,
    pub action: Action,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Run {
    pub records: Vec<Record>,
}

/// Represent a record of a scenario step execution,
/// including the step label and the captured screen state.
#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub label: String,
    pub screen: Vec<String>,
}

/// Parse and execute a scenario document.
pub fn run_document(document: &str) -> Result<Run> {
    let scenario = parser::parse(document)?;
    run_ast(&scenario)
}

/// Execute a parsed scenario and verify every expected screen snapshot.
pub fn run_ast(scenario: &ScenarioAst) -> Result<Run> {
    let command = scenario_command(scenario)?;
    let mut session = Session::spawn(
        command,
        scenario.terminal.rows,
        scenario.terminal.cols,
        scenario.cursor.col - 1,
        scenario.cursor.row - 1,
    )?;

    let result = run_ast_with_session(scenario, &mut session);
    let terminate_result = session.terminate();

    match (result, terminate_result) {
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
        (Ok(run), Ok(())) => Ok(run),
    }
}

fn run_ast_with_session(scenario: &ScenarioAst, session: &mut Session) -> Result<Run> {
    let mut records = Vec::with_capacity(scenario.steps.len());

    for step in &scenario.steps {
        for action in &step.actions {
            match action {
                ActionAst::Input(input) => write_input(session, input)?,
                ActionAst::WaitOutput { text, timeout_ms } => wait_for_output(
                    session,
                    text,
                    Duration::from_millis(*timeout_ms),
                    &scenario.name,
                    &step.label,
                )?,
                ActionAst::WaitScreenLinePrefix { text, timeout_ms } => {
                    wait_for_screen_line_prefix(
                        session,
                        text,
                        Duration::from_millis(*timeout_ms),
                        &scenario.name,
                        &step.label,
                    )?
                }
                ActionAst::Resize(size) => session.resize(size.rows, size.cols)?,
            }
        }
        std::thread::sleep(Duration::from_millis(step.settle_ms));

        let actual = wait_for_screen(
            session,
            &step.expect,
            Duration::from_millis(step.expect_timeout_ms),
        );
        if actual != step.expect {
            return Err(Error::ScreenMismatch {
                scenario: scenario.name.clone(),
                step: step.label.clone(),
                expected: step.expect.clone(),
                actual,
            });
        }

        records.push(Record {
            label: step.label.clone(),
            screen: actual,
        });
    }

    Ok(Run { records })
}

fn wait_for_screen(session: &Session, expected: &[String], timeout: Duration) -> Vec<String> {
    let deadline = Instant::now() + timeout;
    loop {
        let actual = session.screen_snapshot();
        if actual == expected || Instant::now() >= deadline {
            return actual;
        }
        std::thread::sleep(SCREEN_POLL_INTERVAL);
    }
}

fn wait_for_output(
    session: &Session,
    expected: &str,
    timeout: Duration,
    scenario: &str,
    step: &str,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let expected_bytes = expected.as_bytes();

    loop {
        let output = session.output();
        if output
            .windows(expected_bytes.len())
            .any(|window| window == expected_bytes)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let tail = &output[output.len().saturating_sub(OUTPUT_ERROR_TAIL_BYTES)..];
            return Err(Error::OutputTimeout {
                scenario: scenario.to_string(),
                step: step.to_string(),
                expected: expected.to_string(),
                timeout_ms: timeout.as_millis() as u64,
                actual: String::from_utf8_lossy(tail).into_owned(),
            });
        }
        std::thread::sleep(OUTPUT_POLL_INTERVAL);
    }
}

fn wait_for_screen_line_prefix(
    session: &Session,
    expected: &str,
    timeout: Duration,
    scenario: &str,
    step: &str,
) -> Result<()> {
    let deadline = Instant::now() + timeout;

    loop {
        let screen = session.screen_snapshot();
        if screen.iter().any(|line| line.starts_with(expected)) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::ScreenLinePrefixTimeout {
                scenario: scenario.to_string(),
                step: step.to_string(),
                expected: expected.to_string(),
                timeout_ms: timeout.as_millis() as u64,
                actual: screen,
            });
        }
        std::thread::sleep(OUTPUT_POLL_INTERVAL);
    }
}

fn scenario_command(scenario: &ScenarioAst) -> Result<CommandBuilder> {
    let command = &scenario.command;
    let program = if command.starts_with("CARGO_BIN_EXE_") {
        resolve_cargo_binary(command)?
    } else {
        command.as_str().into()
    };

    let mut builder = CommandBuilder::new(program);
    for arg in &scenario.args {
        builder.arg(arg);
    }
    for (name, value) in &scenario.env {
        builder.env(name, value);
    }
    Ok(builder)
}

fn resolve_cargo_binary(variable: &str) -> Result<std::ffi::OsString> {
    if let Some(path) = std::env::var_os(variable) {
        return Ok(path);
    }

    let binary_name = variable
        .strip_prefix("CARGO_BIN_EXE_")
        .expect("caller should pass a Cargo binary variable");
    if let Ok(test_executable) = std::env::current_exe()
        && let Some(test_directory) = test_executable.parent()
    {
        let profile_directory = if test_directory
            .file_name()
            .is_some_and(|name| name == "deps")
        {
            test_directory.parent()
        } else {
            Some(test_directory)
        };
        if let Some(profile_directory) = profile_directory {
            let candidate =
                profile_directory.join(format!("{binary_name}{}", std::env::consts::EXE_SUFFIX));
            if candidate.is_file() {
                return Ok(candidate.into_os_string());
            }
        }
    }

    Err(Error::CargoBinaryNotFound {
        name: variable.to_string(),
    })
}

fn write_input(session: &Session, input: &InputAst) -> Result<()> {
    match input {
        InputAst::Text(text) => session.write_input(text.as_bytes()),
        InputAst::Key { key, count } => {
            let sequence = key_sequence(*key);
            let mut input = Vec::with_capacity(sequence.len() * usize::from(*count));
            for _ in 0..*count {
                input.extend_from_slice(sequence);
            }
            session.write_input(&input)
        }
    }
}

fn key_sequence(key: KeyAst) -> &'static [u8] {
    match key {
        KeyAst::Left => b"\x1b[D",
        KeyAst::Right => b"\x1b[C",
        KeyAst::Up => b"\x1b[A",
        KeyAst::Down => b"\x1b[B",
        KeyAst::Enter => b"\r",
        KeyAst::Backspace => b"\x7f",
        KeyAst::Tab => b"\t",
        KeyAst::Escape => b"\x1b",
    }
}

impl Scenario {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    /// Add a step to the scenario with the given label, settle duration, and action.
    pub fn step<F, S>(mut self, label: S, settle: Duration, action: F) -> Self
    where
        F: Fn(&mut Session) -> ActionResult + Send + Sync + 'static,
        S: Into<String>,
    {
        self.steps.push(Step {
            label: label.into(),
            settle,
            action: Arc::new(action),
        });
        self
    }

    /// Run the scenario by executing each step's action on the provided session,
    /// waiting for the specified settle duration after each step, and recording the screen state.
    pub fn run(&self, session: &mut Session) -> Result<Run> {
        let mut records = Vec::with_capacity(self.steps.len());

        for step in &self.steps {
            (step.action)(session).map_err(|source| Error::ScenarioAction { source })?;
            std::thread::sleep(step.settle);

            records.push(Record {
                label: step.label.clone(),
                screen: session.screen_snapshot(),
            });
        }

        Ok(Run { records })
    }
}
