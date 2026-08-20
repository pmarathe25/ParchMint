//! Small runtime diagnostics for the desktop application.
//!
//! The logger is intentionally safe to call before the desktop graph has
//! discovered its application-data directory: events go to standard error
//! until [`configure_file`] installs the persistent log file. Callers should
//! record operation names and non-content identifiers, never document text.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, MutexGuard, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const LOG_FILE_NAME: &str = "parchmint-debug.log";
const MAX_LOG_BYTES: u64 = 1024 * 1024;
const MAX_TIMING_GROUPS: usize = 64;
const TIMING_SAMPLES_PER_REPORT: u64 = 64;

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

#[derive(Debug, Default)]
struct DiagnosticSink {
    file: Option<File>,
    path: Option<PathBuf>,
    bytes_written: u64,
}

impl DiagnosticSink {
    fn write_line(&mut self, line: &str, flush: bool) {
        if line.len() > usize::try_from(MAX_LOG_BYTES).unwrap_or(usize::MAX) {
            return;
        }
        let line_bytes = u64::try_from(line.len()).unwrap_or(u64::MAX);
        let Some(file) = self.file.as_mut() else {
            eprint!("{line}");
            return;
        };
        if self.bytes_written.saturating_add(line_bytes) > MAX_LOG_BYTES {
            if file.set_len(0).is_err() {
                return;
            }
            self.bytes_written = 0;
        }
        if file.write_all(line.as_bytes()).is_ok() {
            self.bytes_written = self.bytes_written.saturating_add(line_bytes);
            if flush {
                let _ = file.flush();
            }
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

/// Configures bounded diagnostics at `<data directory>/logs/parchmint-debug.log`.
///
/// On Unix and Windows, the final log path is opened without following a
/// symbolic link or reparse point. Rotation uses that same open descriptor.
///
/// Reconfiguring is supported for the production bootstrap's test seams. A
/// failure leaves the preceding sink untouched so diagnostics never prevent
/// ParchMint from starting.
pub fn configure_file(data_directory: impl AsRef<Path>) -> io::Result<PathBuf> {
    let directory = data_directory.as_ref().join("logs");
    fs::create_dir_all(&directory)?;
    let path = directory.join(LOG_FILE_NAME);
    let file = open_log_file(&path)?;
    let existing_bytes = opened_log_len(&file)?;
    let reset = existing_bytes >= MAX_LOG_BYTES;
    if reset {
        file.set_len(0)?;
    }
    let mut sink = lock_sink();
    sink.file = Some(file);
    sink.path = Some(path.clone());
    sink.bytes_written = if reset { 0 } else { existing_bytes };
    drop(sink);
    event(
        Level::Info,
        "diagnostics",
        "persistent debug logging configured",
        &[],
    );
    Ok(path)
}

fn open_log_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    set_no_follow(&mut options);
    options.open(path)
}

#[cfg(unix)]
fn set_no_follow(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;

    options.custom_flags(libc::O_NOFOLLOW);
}

#[cfg(windows)]
fn set_no_follow(options: &mut OpenOptions) {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

#[cfg(not(any(unix, windows)))]
fn set_no_follow(_options: &mut OpenOptions) {}

#[cfg(windows)]
fn opened_log_len(file: &File) -> io::Result<u64> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let metadata = file.metadata()?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "diagnostics log must not be a reparse point",
        ));
    }
    Ok(metadata.len())
}

#[cfg(not(windows))]
fn opened_log_len(file: &File) -> io::Result<u64> {
    file.metadata().map(|metadata| metadata.len())
}

/// The persistent debug-log path, if logging has been configured.
pub fn log_path() -> Option<PathBuf> {
    lock_sink().path.clone()
}

/// Writes one structured, line-oriented event. Failures are deliberately
/// ignored: diagnostics must not change application behavior.
pub fn event(level: Level, target: &str, message: &str, fields: &[(&str, &str)]) {
    write_event(level, target, message, fields, true);
}

fn write_event(level: Level, target: &str, message: &str, fields: &[(&str, &str)], flush: bool) {
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

    lock_sink().write_line(&line, flush);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TimingAggregate {
    samples: u64,
    total_duration_us: u128,
    max_duration_us: u128,
}

impl TimingAggregate {
    fn record(&mut self, duration: Duration) -> Option<Self> {
        let duration_us = duration.as_micros();
        self.samples = self.samples.saturating_add(1);
        self.total_duration_us = self.total_duration_us.saturating_add(duration_us);
        self.max_duration_us = self.max_duration_us.max(duration_us);
        if self.samples < TIMING_SAMPLES_PER_REPORT {
            return None;
        }
        Some(std::mem::take(self))
    }
}

fn timing_aggregates() -> &'static Mutex<BTreeMap<(&'static str, &'static str), TimingAggregate>> {
    static TIMINGS: OnceLock<Mutex<BTreeMap<(&'static str, &'static str), TimingAggregate>>> =
        OnceLock::new();
    TIMINGS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Aggregates the duration of work measured at a blocking executor boundary.
///
/// One summary is written per sample window, without a synchronous flush. The
/// in-memory cardinality and persistent log are both bounded. Measurements are
/// best-effort and are not evidence that a performance requirement is met.
pub fn timing(operation: &'static str, context: &'static str, duration: Duration) {
    let aggregate = {
        let mut timings = timing_aggregates()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !timings.contains_key(&(operation, context)) && timings.len() >= MAX_TIMING_GROUPS {
            return;
        }
        timings
            .entry((operation, context))
            .or_default()
            .record(duration)
    };
    let Some(aggregate) = aggregate else {
        return;
    };
    let samples = aggregate.samples.to_string();
    let total_duration_us = aggregate.total_duration_us.to_string();
    let max_duration_us = aggregate.max_duration_us.to_string();
    write_event(
        Level::Info,
        "performance",
        "blocking operation aggregate",
        &[
            ("operation", operation),
            ("context", context),
            ("samples", &samples),
            ("total_duration_us", &total_duration_us),
            ("max_duration_us", &max_duration_us),
        ],
        false,
    );
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "parchmint-diagnostics-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[cfg(unix)]
    fn create_file_symlink(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).expect("create log symlink");
        true
    }

    #[cfg(windows)]
    fn create_file_symlink(target: &Path, link: &Path) -> bool {
        match std::os::windows::fs::symlink_file(target, link) {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => false,
            Err(error) => panic!("create log symlink: {error}"),
        }
    }

    #[test]
    fn event_fields_are_single_line_and_shell_readable() {
        let mut rendered = String::new();
        append_escaped(&mut rendered, "one two=three\nfour");
        assert_eq!(rendered, "one\\ two\\=three\\nfour");
    }

    #[test]
    fn timing_is_aggregated_before_it_is_reported() {
        let mut aggregate = TimingAggregate::default();
        for _ in 1..TIMING_SAMPLES_PER_REPORT {
            assert_eq!(aggregate.record(Duration::from_micros(3)), None);
        }
        assert_eq!(
            aggregate.record(Duration::from_micros(5)),
            Some(TimingAggregate {
                samples: TIMING_SAMPLES_PER_REPORT,
                total_duration_us: u128::from(TIMING_SAMPLES_PER_REPORT - 1) * 3 + 5,
                max_duration_us: 5,
            })
        );
        assert_eq!(aggregate, TimingAggregate::default());
    }

    #[test]
    fn diagnostic_sink_truncates_before_exceeding_its_size_bound() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("parchmint-diagnostics-{nonce}.log"));
        fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize]).expect("fill test log");
        let file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open test log");
        let mut sink = DiagnosticSink {
            file: Some(file),
            path: Some(path.clone()),
            bytes_written: MAX_LOG_BYTES,
        };
        let line = "next event\n";
        sink.write_line(line, false);

        assert_eq!(fs::read_to_string(&path).expect("read test log"), line);
        let _ = fs::remove_file(path);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn configure_file_refuses_a_symlink_without_truncating_its_target() {
        let directory = unique_test_directory("symlink");
        let logs = directory.join("logs");
        fs::create_dir_all(&logs).expect("create test logs directory");
        let sentinel = directory.join("sentinel");
        let sentinel_bytes = vec![b'x'; MAX_LOG_BYTES as usize];
        fs::write(&sentinel, &sentinel_bytes).expect("write sentinel");
        let log = logs.join(LOG_FILE_NAME);
        if !create_file_symlink(&sentinel, &log) {
            let _ = fs::remove_file(sentinel);
            let _ = fs::remove_dir_all(directory);
            return;
        }

        assert!(configure_file(&directory).is_err());
        assert_eq!(fs::read(&sentinel).expect("read sentinel"), sentinel_bytes);

        let _ = fs::remove_file(log);
        let _ = fs::remove_file(sentinel);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn configure_file_rotates_a_regular_file_through_its_open_descriptor() {
        let directory = unique_test_directory("rotation");
        let logs = directory.join("logs");
        fs::create_dir_all(&logs).expect("create test logs directory");
        let log = logs.join(LOG_FILE_NAME);
        fs::write(&log, vec![b'x'; MAX_LOG_BYTES as usize]).expect("fill test log");

        assert_eq!(configure_file(&directory).expect("configure log"), log);
        let contents = fs::read(&log).expect("read rotated log");
        assert!(contents.len() < MAX_LOG_BYTES as usize);
        assert!(!contents.contains(&b'x'));
        assert!(
            String::from_utf8(contents)
                .expect("diagnostics are UTF-8")
                .contains("persistent debug logging configured")
        );

        let mut sink = lock_sink();
        sink.file = None;
        sink.path = None;
        sink.bytes_written = 0;
        drop(sink);
        let _ = fs::remove_dir_all(directory);
    }
}
