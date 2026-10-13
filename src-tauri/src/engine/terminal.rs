//! Integrated terminal: a managed pseudo-terminal (PTY) per shell session.
//!
//! Like [`watcher`](super::watcher), this engine module is allowed to touch Tauri types
//! (`AppHandle` + `Emitter`): the blocking read loop streams raw output back to the webview
//! as `pty-output-<id>` events, and emits `pty-exit-<id>` once when the shell ends.
//!
//! Threading model (mirrors how `notify` runs its own thread): the only blocking work — the
//! PTY read — runs on a **dedicated `std::thread`**, never on the tokio runtime, and never
//! touches an `AppState` lock, so it can't deadlock the command handlers. A [`PtySession`]'s
//! [`Drop`] kills the child and lets the reader thread wind down on its own (it hits EOF once
//! the child dies), which is the process + file-descriptor cleanup guarantee.

use std::io::{Read, Write};
use std::thread::JoinHandle;

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter};

/// Event name for a session's output stream (payload: raw bytes as `number[]`).
pub fn output_event(id: &str) -> String {
    format!("pty-output-{id}")
}

/// Event name fired once when a session's shell exits (payload: `null`).
pub fn exit_event(id: &str) -> String {
    format!("pty-exit-{id}")
}

/// The user's preferred shell, honoring `$SHELL` with a sensible platform fallback.
#[cfg(unix)]
fn default_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if std::path::Path::new("/bin/bash").exists() {
                "/bin/bash".to_string()
            } else {
                "/bin/sh".to_string()
            }
        })
}

/// Windows: honor an explicit `$SHELL` (e.g. Git Bash), otherwise PowerShell (ConPTY).
#[cfg(windows)]
fn default_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "powershell.exe".to_string())
}

/// A live PTY + its shell child. Owns every resource for one terminal tab; dropping it
/// terminates the shell and closes the pty's file descriptors.
pub struct PtySession {
    /// The repo this session was spawned under — used to reap it when that repo closes.
    pub repo: String,
    /// The pty master; resizing happens here. Dropping it closes the pty.
    master: Box<dyn MasterPty + Send>,
    /// The shell's stdin.
    writer: Box<dyn Write + Send>,
    /// The shell process, kept so we can kill it on cleanup.
    child: Box<dyn Child + Send + Sync>,
    /// The blocking read-loop thread. Detached (not joined) on drop — see module docs.
    reader: Option<JoinHandle<()>>,
}

/// Spawn the user's shell on a new PTY sized `cols`×`rows`, with `repo` as the working
/// directory. Streams output to `pty-output-<id>` and fires `pty-exit-<id>` on exit.
pub fn spawn(
    app: &AppHandle,
    id: String,
    repo: &str,
    cols: u16,
    rows: u16,
) -> Result<PtySession, String> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("failed to open pty: {e}"))?;

    let mut cmd = CommandBuilder::new(default_shell());
    cmd.cwd(repo);
    // Advertise a capable terminal so colour + TUI programs (vim, htop) behave.
    cmd.env("TERM", "xterm-256color");

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("failed to spawn shell: {e}"))?;
    // Drop the slave handle so the master sees EOF once the child exits (needed cross-platform).
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("failed to clone pty reader: {e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("failed to take pty writer: {e}"))?;

    // Dedicated OS thread for the blocking read loop (never the tokio runtime).
    let reader_handle = {
        let app = app.clone();
        let output = output_event(&id);
        let exit = exit_event(&id);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,              // EOF: shell closed the pty
                    Ok(n) => {
                        // Emit raw bytes so multi-byte UTF-8 split across reads can't corrupt.
                        let _ = app.emit(&output, buf[..n].to_vec());
                    }
                    Err(_) => break, // pty gone (killed/closed)
                }
            }
            let _ = app.emit::<Option<()>>(&exit, None);
        })
    };

    Ok(PtySession {
        repo: repo.to_string(),
        master: pair.master,
        writer,
        child,
        reader: Some(reader_handle),
    })
}

impl PtySession {
    /// Write keystrokes (UTF-8 text) to the shell's stdin.
    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(data)
            .and_then(|_| self.writer.flush())
            .map_err(|e| format!("failed to write to pty: {e}"))
    }

    /// Resize the pty to `cols`×`rows` (drives the shell's `SIGWINCH`/reflow).
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to resize pty: {e}"))
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        // Terminate the shell; dropping `writer`/`master` right after closes the pty, so the
        // reader thread hits EOF and exits on its own. We intentionally do NOT join it: a
        // grandchild that inherited the slave fd could keep the master open indefinitely, and
        // blocking here would stall tab close / app exit. Dropping the handle detaches it.
        let _ = self.child.kill();
        self.reader.take();
    }
}
