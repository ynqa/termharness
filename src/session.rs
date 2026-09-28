use std::{
    io::Write,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use portable_pty::{Child, CommandBuilder, MasterPty};

use crate::{
    error::{self, Error, Result},
    screen::Screen,
};

pub struct Session {
    screen: Arc<Mutex<Screen>>,
    master: Box<dyn MasterPty + Send>,
    child: Option<Box<dyn Child + Send + Sync>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    output: Arc<Mutex<Vec<u8>>>,
    reader_thread: Option<JoinHandle<()>>,
}

impl Session {
    pub fn spawn(
        mut cmd: CommandBuilder,
        terminal_rows: usize,
        terminal_cols: usize,
        cursor_x: usize,
        cursor_y: usize,
    ) -> Result<Self> {
        cmd.env("TERM", "xterm-256color");
        let spawn_cmd = cmd.clone();

        let pty_pair = portable_pty::native_pty_system()
            .openpty(portable_pty::PtySize {
                rows: terminal_rows as u16,
                cols: terminal_cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| Error::OpenPty {
                message: err.to_string(),
            })?;

        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|err| Error::SpawnCommand {
                command: error::format_spawn_command(&spawn_cmd),
                message: err.to_string(),
            })?;
        drop(pty_pair.slave);

        let screen = Arc::new(Mutex::new(Screen::new_with_cursor(
            terminal_rows,
            terminal_cols,
            cursor_x,
            cursor_y,
        )));
        let master = pty_pair.master;
        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer = Arc::new(Mutex::new(master.take_writer().map_err(|err| {
            Error::TakeWriter {
                message: err.to_string(),
            }
        })?));

        let reader_thread = {
            let screen = screen.clone();
            let output = output.clone();
            let writer = writer.clone();
            let mut reader = master
                .try_clone_reader()
                .map_err(|err| Error::CloneReader {
                    message: err.to_string(),
                })?;

            thread::spawn(move || {
                let mut buf = [0u8; 1024];

                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let chunk = &buf[..n];

                            output
                                .lock()
                                .expect("failed to lock output")
                                .extend_from_slice(chunk);

                            let responses = {
                                let mut screen = screen.lock().expect("failed to lock screen");
                                screen.process_with_responses(chunk)
                            };
                            if !responses.is_empty() {
                                let mut writer = writer.lock().expect("failed to lock writer");
                                writer
                                    .write_all(&responses)
                                    .expect("failed to write terminal responses");
                                writer.flush().expect("failed to flush terminal responses");
                            }
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
            })
        };

        Ok(Self {
            screen,
            master,
            child: Some(child),
            writer,
            output,
            reader_thread: Some(reader_thread),
        })
    }

    /// Write user input to the pseudo terminal.
    pub fn write_input(&self, input: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().expect("failed to lock writer");
        writer.write_all(input).map_err(|err| Error::WriteInput {
            message: err.to_string(),
        })?;
        writer.flush().map_err(|err| Error::WriteInput {
            message: err.to_string(),
        })
    }

    /// Resizes the terminal. This sends a resize signal
    /// to the child process and updates the screen size.
    pub fn resize(&mut self, rows: usize, cols: usize) -> Result<()> {
        // SIGWINCH can cause immediate child output. Hold the screen lock until
        // both the PTY and the model have the new size before parsing that output.
        let mut screen = self.screen.lock().expect("failed to lock screen");
        self.master
            .resize(portable_pty::PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| Error::ResizePty {
                message: err.to_string(),
            })?;

        screen.resize(rows, cols);
        Ok(())
    }

    /// Scroll toward older output without sending input to the child process.
    /// Stops at the oldest retained line.
    pub fn scroll_up(&self, lines: u16) {
        self.screen
            .lock()
            .expect("failed to lock screen")
            .scroll_up(lines);
    }

    /// Scroll toward live output without sending input to the child process.
    /// Stops at the live screen.
    pub fn scroll_down(&self, lines: u16) {
        self.screen
            .lock()
            .expect("failed to lock screen")
            .scroll_down(lines);
    }

    /// Takes a snapshot of the current viewport, including scrolled-back output.
    /// Each string in the returned vector represents a line on the screen.
    pub fn screen_snapshot(&self) -> Vec<String> {
        self.screen
            .lock()
            .expect("failed to lock screen")
            .snapshot()
    }

    /// Return all bytes read from the pseudo terminal so far.
    pub fn output(&self) -> Vec<u8> {
        self.output.lock().expect("failed to lock output").clone()
    }

    /// Wait for the child process and reader thread to finish.
    pub fn wait(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            child.wait().map_err(|err| Error::WaitChild {
                message: err.to_string(),
            })?;
        }
        self.join_reader()
    }

    /// Terminate the child process and wait for all session resources.
    pub fn terminate(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let exited = child
                .try_wait()
                .map_err(|err| Error::WaitChild {
                    message: err.to_string(),
                })?
                .is_some();

            if !exited {
                child.kill().map_err(|err| Error::TerminateChild {
                    message: err.to_string(),
                })?;
                child.wait().map_err(|err| Error::WaitChild {
                    message: err.to_string(),
                })?;
            }
        }
        self.join_reader()
    }

    fn join_reader(&mut self) -> Result<()> {
        if let Some(reader_thread) = self.reader_thread.take() {
            reader_thread
                .join()
                .map_err(|_| Error::ReaderThreadPanicked)?;
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod session {
        use super::*;

        mod spawn {
            use super::*;

            #[test]
            fn reports_the_position_at_each_request_before_later_cursor_moves() -> Result<()> {
                let mut cmd = CommandBuilder::new("/bin/bash");
                cmd.arg("-lc");
                cmd.arg(
                    r#"stty -echo -icanon min 1 time 0
printf '\033[2;3H\033[6n\033[4;5H\033[6n\033[1;1H'
IFS= read -rsd R -t 2 first
IFS= read -rsd R -t 2 second
printf 'REPLIES:%sR%sR' "$first" "$second""#,
                );
                let mut session = Session::spawn(cmd, 10, 40, 0, 0)?;
                session.wait()?;
                let output = session.output();
                assert!(
                    String::from_utf8_lossy(&output).contains("REPLIES:\x1b[2;3R\x1b[4;5R"),
                    "unexpected replies: {:?}",
                    String::from_utf8_lossy(&output),
                );
                Ok(())
            }

            #[test]
            fn success() -> Result<()> {
                let mut cmd = CommandBuilder::new("echo");
                cmd.arg("Hello, world!");
                let mut session = Session::spawn(cmd, 24, 80, 0, 0)?;

                session.wait()?;

                let output = session.output();
                let output = String::from_utf8_lossy(&output);
                assert!(output.contains("Hello, world!"));
                Ok(())
            }

            #[test]
            fn sets_fixed_term_environment_variable() -> Result<()> {
                let mut cmd = CommandBuilder::new("/bin/bash");
                cmd.arg("-lc");
                cmd.arg("printf '%s' \"$TERM\"");
                cmd.env("TERM", "dumb");
                let mut session = Session::spawn(cmd, 24, 80, 0, 0)?;

                session.wait()?;

                assert!(
                    String::from_utf8_lossy(&session.output()).contains("xterm-256color"),
                    "session should override TERM",
                );
                Ok(())
            }

            #[test]
            fn responds_to_cursor_position_requests() -> Result<()> {
                let mut cmd = CommandBuilder::new("/bin/bash");
                cmd.arg("-lc");
                cmd.arg(r#"printf 'abc\033[6n'; IFS= read -rsd R pos; printf '%sR' "$pos""#);
                let mut session = Session::spawn(cmd, 24, 80, 0, 0)?;

                session.wait()?;

                let output = session.output();
                assert!(
                    String::from_utf8_lossy(&output).contains("\x1b[1;4R"),
                    "expected DSR response in output, got {:?}",
                    String::from_utf8_lossy(&output),
                );
                Ok(())
            }

            #[test]
            fn responds_from_custom_initial_cursor_position() -> Result<()> {
                let mut cmd = CommandBuilder::new("/bin/bash");
                cmd.arg("-lc");
                cmd.arg(r#"printf '\033[6n'; IFS= read -rsd R pos; printf '%sR' "$pos""#);
                let mut session = Session::spawn(cmd, 24, 80, 0, 23)?;

                session.wait()?;

                let output = session.output();
                assert!(
                    String::from_utf8_lossy(&output).contains("\x1b[24;1R"),
                    "expected DSR response in output, got {:?}",
                    String::from_utf8_lossy(&output),
                );
                Ok(())
            }

            #[test]
            fn responds_to_every_cursor_position_request() -> Result<()> {
                let mut cmd = CommandBuilder::new("/bin/bash");
                cmd.arg("-lc");
                cmd.arg(
                    r#"printf '\033[6n\033[6n'; IFS= read -rsd R first; IFS= read -rsd R second; printf '%sR%sR' "$first" "$second""#,
                );
                let mut session = Session::spawn(cmd, 24, 80, 0, 0)?;

                session.wait()?;

                let output = session.output();
                assert!(
                    String::from_utf8_lossy(&output).contains("\x1b[1;1R\x1b[1;1R"),
                    "expected two DSR responses in output, got {:?}",
                    String::from_utf8_lossy(&output),
                );
                Ok(())
            }
        }

        mod resize {
            use super::*;

            struct RedrawingPty {
                screen: Arc<Mutex<Screen>>,
                pending: Arc<Mutex<Vec<u8>>>,
            }

            impl MasterPty for RedrawingPty {
                fn resize(&self, size: portable_pty::PtySize) -> anyhow::Result<()> {
                    // Schedule child output immediately after its size notification.
                    // If the reader is blocked, deliver it after resize returns.
                    let output = format!("\x1b[{};1HNEW", size.rows);
                    match self.screen.try_lock() {
                        Ok(mut screen) => screen.process(output.as_bytes()),
                        Err(std::sync::TryLockError::WouldBlock) => {
                            self.pending
                                .lock()
                                .unwrap()
                                .extend_from_slice(output.as_bytes());
                        }
                        Err(error) => panic!("screen lock poisoned: {error}"),
                    }
                    Ok(())
                }

                fn get_size(&self) -> anyhow::Result<portable_pty::PtySize> {
                    unreachable!()
                }

                fn try_clone_reader(&self) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
                    unreachable!()
                }

                fn take_writer(&self) -> anyhow::Result<Box<dyn Write + Send>> {
                    unreachable!()
                }

                #[cfg(unix)]
                fn process_group_leader(&self) -> Option<i32> {
                    None
                }

                #[cfg(unix)]
                fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
                    None
                }

                #[cfg(unix)]
                fn tty_name(&self) -> Option<std::path::PathBuf> {
                    None
                }
            }

            #[test]
            fn child_redraw_after_notification_uses_the_new_screen_dimensions() -> Result<()> {
                let screen = Arc::new(Mutex::new(Screen::new(3, 8)));
                let pending = Arc::new(Mutex::new(Vec::new()));
                let mut session = Session {
                    screen: screen.clone(),
                    master: Box::new(RedrawingPty {
                        screen,
                        pending: pending.clone(),
                    }),
                    child: None,
                    writer: Arc::new(Mutex::new(Box::new(std::io::sink()))),
                    output: Arc::new(Mutex::new(Vec::new())),
                    reader_thread: None,
                };
                session.resize(6, 8)?;
                session
                    .screen
                    .lock()
                    .unwrap()
                    .process(&pending.lock().unwrap());
                assert_eq!(
                    session.screen_snapshot(),
                    vec![
                        "        ", "        ", "        ", "        ", "        ", "NEW     ",
                    ]
                );
                Ok(())
            }

            #[test]
            fn resize_reflows_wrapped_lines() {
                let mut screen = Screen::new(3, 8);
                screen.process(b"abcdefghij");

                assert_eq!(
                    screen
                        .snapshot_with_size(3, 8)
                        .expect("snapshot should succeed"),
                    vec![
                        "abcdefgh".to_string(),
                        "ij      ".to_string(),
                        "        ".to_string(),
                    ]
                );

                screen.resize(3, 6);

                assert_eq!(
                    screen
                        .snapshot_with_size(3, 6)
                        .expect("snapshot should succeed"),
                    vec![
                        "abcdef".to_string(),
                        "ghij  ".to_string(),
                        "      ".to_string(),
                    ]
                );
            }
        }
    }
}
