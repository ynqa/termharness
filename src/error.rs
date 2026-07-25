use portable_pty::CommandBuilder;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot pad content to column {column}: display width {width} exceeds target")]
    ContentExceedsColumn { column: usize, width: usize },
    #[error("failed to open pseudo terminal: {message}")]
    OpenPty { message: String },
    #[error("failed to resize pseudo terminal: {message}")]
    ResizePty { message: String },
    #[error("failed to spawn command `{command}` in pseudo terminal: {message}")]
    SpawnCommand { command: String, message: String },
    #[error("failed to take pseudo terminal writer: {message}")]
    TakeWriter { message: String },
    #[error("failed to clone pseudo terminal reader: {message}")]
    CloneReader { message: String },
    #[error("failed to write input to pseudo terminal: {message}")]
    WriteInput { message: String },
    #[error("failed to wait for child process: {message}")]
    WaitChild { message: String },
    #[error("failed to terminate child process: {message}")]
    TerminateChild { message: String },
    #[error("pseudo terminal reader thread panicked")]
    ReaderThreadPanicked,
    #[error("could not resolve Cargo binary for scenario command `{name}`")]
    CargoBinaryNotFound { name: String },
    #[error(
        "scenario `{scenario}` step `{step}` screen did not match\nexpected: {expected:?}\n  actual: {actual:?}"
    )]
    ScreenMismatch {
        scenario: String,
        step: String,
        expected: Vec<String>,
        actual: Vec<String>,
    },
    #[error(transparent)]
    ScenarioParse(#[from] crate::scenario::parser::Error),
    #[error("failed to execute scenario action: {source}")]
    ScenarioAction {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

pub fn format_spawn_command(cmd: &CommandBuilder) -> String {
    cmd.as_unix_command_line().unwrap_or_else(|_| {
        cmd.get_argv()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    mod format_spawn_command {
        use super::*;

        #[test]
        fn formats_command_line() {
            let mut cmd = CommandBuilder::new("echo");
            cmd.arg("hello world");
            assert_eq!(format_spawn_command(&cmd), "echo 'hello world'");
        }
    }
}
