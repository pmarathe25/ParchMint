//! Small, dependency-free runtime diagnostics for the desktop application.
//!
//! The logger is intentionally safe to call before the desktop graph has
//! discovered its application-data directory: events go to standard error
//! until [`configure_file`] installs the persistent log file. Callers should
//! record operation names and non-content identifiers, never document text.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, MutexGuard, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const LOG_FILE_NAME: &str = "parchmint-debug.log";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Trace,
    Info,
    Warn,
    Error,
}

impl Level {
    const fn label(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Debug)]
struct DiagnosticSink {
    file: Option<File>,
    path: Option<PathBuf>,
}

impl Default for DiagnosticSink {
    fn default() -> Self {
        Self {
            file: None,
            path: None,
        }
    }
}

fn sink() -> &'static Mutex<DiagnosticSink> {
    static SINK: OnceLock<Mutex<DiagnosticSink>> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(DiagnosticSink::default()))
}

fn next_sequence() -> u64 {
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

fn lock_sink() -> MutexGuard<'static, DiagnosticSink> {
    sink()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Configures append-only diagnostics at `<data directory>/logs/parchmint-debug.log`.
///
/// Reconfiguring is supported for the production bootstrap's test seams. A
/// failure leaves the preceding sink untouched so diagnostics never prevent
/// ParchMint from starting.
pub fn configure_file(data_directory: impl AsRef<Path>) -> io::Result<PathBuf> {
    let directory = data_directory.as_ref().join("logs");
    fs::create_dir_all(&directory)?;
    let path = directory.join(LOG_FILE_NAME);
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let mut sink = lock_sink();
    sink.file = Some(file);
    sink.path = Some(path.clone());
    drop(sink);
    event(
        Level::Info,
        "diagnostics",
        "persistent debug logging configured",
        &[],
    );
    Ok(path)
}

/// The persistent debug-log path, if logging has been configured.
pub fn log_path() -> Option<PathBuf> {
    lock_sink().path.clone()
}

/// Writes one structured, line-oriented event. Failures are deliberately
/// ignored: diagnostics must not change application behavior.
pub fn event(level: Level, target: &str, message: &str, fields: &[(&str, &str)]) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let sequence = next_sequence();
    let mut line = format!(
        "{timestamp} #{sequence} [{}] {target}: {message}",
        level.label()
    );
    for (key, value) in fields {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        append_escaped(&mut line, value);
    }
    line.push('\n');

    let mut sink = lock_sink();
    if let Some(file) = sink.file.as_mut() {
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    } else {
        eprint!("{line}");
    }
}

fn append_escaped(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ' ' | '=' => {
                output.push('\\');
                output.push(character);
            }
            _ => output.push(character),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_fields_are_single_line_and_shell_readable() {
        let mut rendered = String::new();
        append_escaped(&mut rendered, "one two=three\nfour");
        assert_eq!(rendered, "one\\ two\\=three\\nfour");
    }
}
