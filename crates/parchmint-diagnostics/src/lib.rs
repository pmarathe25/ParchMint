//! Small runtime diagnostics for the desktop application.
//!
//! The logger is intentionally safe to call before the desktop graph has
//! discovered its application-data directory: enabled builds send events to
//! standard error until [`configure_file`] installs the persistent log file.
//! Release builds disable diagnostics unless the `capture` feature is enabled.
//! Callers should record operation names and non-content identifiers, never
//! document text.

#![cfg_attr(
    not(any(debug_assertions, feature = "capture")),
    allow(dead_code, unused_imports)
)]

use std::time::Duration;

#[cfg(any(debug_assertions, feature = "capture"))]
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, MutexGuard, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(any(debug_assertions, feature = "capture"))]
const LOG_FILE_NAME: &str = "parchmint-debug.log";
#[cfg(any(debug_assertions, feature = "capture"))]
const MAX_LOG_BYTES: u64 = 1024 * 1024;
#[cfg(any(debug_assertions, feature = "capture"))]
const MAX_TIMING_GROUPS: usize = 64;
#[cfg(any(debug_assertions, feature = "capture"))]
const TIMING_SAMPLES_PER_REPORT: u64 = 64;
#[cfg(any(debug_assertions, feature = "capture"))]
const MAX_BLOCKING_WORKER_GROUPS: usize = 64;
#[cfg(any(debug_assertions, feature = "capture"))]
const BLOCKING_WORKER_SAMPLES_PER_REPORT: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Trace,
    Info,
    Warn,
    Error,
}

/// A structured diagnostic event retained by debug/test captures.
///
/// The event contains operation metadata only; callers must not put document
/// content in diagnostic messages or fields.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(debug_assertions, feature = "capture"))]
pub struct DiagnosticEvent {
    pub sequence: u64,
    pub timestamp_millis: u128,
    pub level: Level,
    pub target: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

#[cfg(any(debug_assertions, feature = "capture"))]
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

#[cfg(any(debug_assertions, feature = "capture"))]
#[derive(Debug, Default)]
struct DiagnosticSink {
    file: Option<File>,
    path: Option<PathBuf>,
    bytes_written: u64,
}

#[cfg(any(debug_assertions, feature = "capture"))]
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

#[cfg(any(debug_assertions, feature = "capture"))]
fn sink() -> &'static Mutex<DiagnosticSink> {
    static SINK: OnceLock<Mutex<DiagnosticSink>> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(DiagnosticSink::default()))
}

#[cfg(any(debug_assertions, feature = "capture"))]
fn next_sequence() -> u64 {
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

#[cfg(any(debug_assertions, feature = "capture"))]
fn captured_events() -> &'static Mutex<Vec<DiagnosticEvent>> {
    static EVENTS: OnceLock<Mutex<Vec<DiagnosticEvent>>> = OnceLock::new();
    EVENTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Removes and returns events collected by a debug/test diagnostics capture.
///
/// Release builds without the `capture` feature return an empty vector and do
/// not retain diagnostic events.
#[cfg(any(debug_assertions, feature = "capture"))]
pub fn take_captured_events() -> Vec<DiagnosticEvent> {
    captured_events()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .drain(..)
        .collect()
}

#[cfg(any(debug_assertions, feature = "capture"))]
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
#[cfg(any(debug_assertions, feature = "capture"))]
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

#[cfg(any(debug_assertions, feature = "capture"))]
fn open_log_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    // `append` alone grants append-data access on Windows, which is not
    // sufficient for `set_len` during bounded-log rotation.
    options.create(true).write(true).append(true);
    set_no_follow(&mut options);
    options.open(path)
}

#[cfg(all(any(debug_assertions, feature = "capture"), unix))]
fn set_no_follow(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;

    options.custom_flags(libc::O_NOFOLLOW);
}

#[cfg(all(any(debug_assertions, feature = "capture"), windows))]
fn set_no_follow(options: &mut OpenOptions) {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

#[cfg(all(any(debug_assertions, feature = "capture"), not(any(unix, windows))))]
fn set_no_follow(_options: &mut OpenOptions) {}

#[cfg(all(any(debug_assertions, feature = "capture"), windows))]
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

#[cfg(all(any(debug_assertions, feature = "capture"), not(windows)))]
fn opened_log_len(file: &File) -> io::Result<u64> {
    file.metadata().map(|metadata| metadata.len())
}

/// The persistent debug-log path, if logging has been configured.
#[cfg(any(debug_assertions, feature = "capture"))]
pub fn log_path() -> Option<PathBuf> {
    lock_sink().path.clone()
}

/// Writes one structured, line-oriented event. Failures are deliberately
/// ignored: diagnostics must not change application behavior.
#[cfg(any(debug_assertions, feature = "capture"))]
pub fn event(level: Level, target: &str, message: &str, fields: &[(&str, &str)]) {
    write_event(level, target, message, fields, true);
}

#[cfg(not(any(debug_assertions, feature = "capture")))]
#[inline(always)]
pub fn event(_level: Level, _target: &str, _message: &str, _fields: &[(&str, &str)]) {}

#[cfg(any(debug_assertions, feature = "capture"))]
fn capture_event(
    sequence: u64,
    level: Level,
    target: &str,
    message: &str,
    fields: &[(&str, &str)],
) {
    let event = DiagnosticEvent {
        sequence,
        timestamp_millis: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis()),
        level,
        target: target.to_owned(),
        message: message.to_owned(),
        fields: fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    };
    captured_events()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(event);
}

#[cfg(any(debug_assertions, feature = "capture"))]
fn write_event(level: Level, target: &str, message: &str, fields: &[(&str, &str)], flush: bool) {
    write_event_at(next_sequence(), level, target, message, fields, flush);
}

#[cfg(any(debug_assertions, feature = "capture"))]
fn write_event_at(
    sequence: u64,
    level: Level,
    target: &str,
    message: &str,
    fields: &[(&str, &str)],
    flush: bool,
) {
    capture_event(sequence, level, target, message, fields);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
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
#[cfg(any(debug_assertions, feature = "capture"))]
struct TimingAggregate {
    samples: u64,
    total_duration_us: u128,
    max_duration_us: u128,
}

#[cfg(any(debug_assertions, feature = "capture"))]
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

#[cfg(any(debug_assertions, feature = "capture"))]
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
#[cfg(any(debug_assertions, feature = "capture"))]
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

#[cfg(not(any(debug_assertions, feature = "capture")))]
#[inline(always)]
pub fn timing(_operation: &'static str, _context: &'static str, _duration: Duration) {}

/// Whether a completed blocking worker could publish its result to the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerDelivery {
    Accepted,
    Dropped,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg(any(debug_assertions, feature = "capture"))]
struct BlockingWorkerAggregate {
    samples: u64,
    accepted: u64,
    dropped: u64,
    total_duration_us: u128,
    max_duration_us: u128,
    active_concurrency: u64,
    peak_concurrency: u64,
}

#[cfg(any(debug_assertions, feature = "capture"))]
impl BlockingWorkerAggregate {
    fn record(
        &mut self,
        duration: Duration,
        delivery: WorkerDelivery,
        active: u64,
    ) -> Option<Self> {
        self.samples = self.samples.saturating_add(1);
        match delivery {
            WorkerDelivery::Accepted => self.accepted = self.accepted.saturating_add(1),
            WorkerDelivery::Dropped => self.dropped = self.dropped.saturating_add(1),
        }
        let duration_us = duration.as_micros();
        self.total_duration_us = self.total_duration_us.saturating_add(duration_us);
        self.max_duration_us = self.max_duration_us.max(duration_us);
        self.active_concurrency = active;
        self.peak_concurrency = self.peak_concurrency.max(active);
        if self.samples < BLOCKING_WORKER_SAMPLES_PER_REPORT {
            return None;
        }
        Some(std::mem::take(self))
    }
}

#[derive(Default)]
#[cfg(any(debug_assertions, feature = "capture"))]
struct BlockingWorkerState {
    active: BTreeMap<&'static str, u64>,
    aggregates: BTreeMap<&'static str, BlockingWorkerAggregate>,
}

#[cfg(any(debug_assertions, feature = "capture"))]
fn blocking_workers() -> &'static Mutex<BlockingWorkerState> {
    static WORKERS: OnceLock<Mutex<BlockingWorkerState>> = OnceLock::new();
    WORKERS.get_or_init(|| Mutex::new(BlockingWorkerState::default()))
}

/// Measures one detached blocking worker without changing how it is scheduled.
///
/// The guard counts active and peak concurrency per static operation name. Call
/// [`BlockingWorkerActivity::complete`] after attempting result delivery. If a
/// worker unwinds before that point, dropping the guard records a dropped
/// delivery. Reports are emitted only after a bounded sample window.
#[cfg(any(debug_assertions, feature = "capture"))]
pub struct BlockingWorkerActivity {
    operation: &'static str,
    started: std::time::Instant,
    tracked: bool,
    completed: bool,
}

#[cfg(not(any(debug_assertions, feature = "capture")))]
pub struct BlockingWorkerActivity;

/// Starts passive diagnostics for a blocking worker.
#[cfg(any(debug_assertions, feature = "capture"))]
pub fn blocking_worker(operation: &'static str) -> BlockingWorkerActivity {
    let tracked = {
        let mut workers = blocking_workers()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !workers.active.contains_key(operation)
            && workers.active.len() >= MAX_BLOCKING_WORKER_GROUPS
        {
            false
        } else {
            let active = {
                let active = workers.active.entry(operation).or_default();
                *active = active.saturating_add(1);
                *active
            };
            let aggregate = workers.aggregates.entry(operation).or_default();
            aggregate.peak_concurrency = aggregate.peak_concurrency.max(active);
            true
        }
    };
    BlockingWorkerActivity {
        operation,
        started: std::time::Instant::now(),
        tracked,
        completed: false,
    }
}

#[cfg(not(any(debug_assertions, feature = "capture")))]
#[inline(always)]
pub fn blocking_worker(_operation: &'static str) -> BlockingWorkerActivity {
    BlockingWorkerActivity
}

#[cfg(any(debug_assertions, feature = "capture"))]
impl BlockingWorkerActivity {
    /// Records the worker duration and whether the receiver accepted its result.
    pub fn complete(mut self, delivery: WorkerDelivery) {
        self.finish(delivery);
    }

    fn finish(&mut self, delivery: WorkerDelivery) {
        if self.completed {
            return;
        }
        self.completed = true;
        if !self.tracked {
            return;
        }
        let aggregate = {
            let mut workers = blocking_workers()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let active = workers
                .active
                .get_mut(self.operation)
                .expect("tracked worker has an active count");
            *active = active.saturating_sub(1);
            let active = *active;
            workers
                .aggregates
                .get_mut(self.operation)
                .expect("tracked worker has an aggregate")
                .record(self.started.elapsed(), delivery, active)
        };
        let Some(aggregate) = aggregate else {
            return;
        };
        let samples = aggregate.samples.to_string();
        let accepted = aggregate.accepted.to_string();
        let dropped = aggregate.dropped.to_string();
        let total_duration_us = aggregate.total_duration_us.to_string();
        let max_duration_us = aggregate.max_duration_us.to_string();
        let active_concurrency = aggregate.active_concurrency.to_string();
        let peak_concurrency = aggregate.peak_concurrency.to_string();
        write_event(
            Level::Info,
            "performance",
            "blocking worker aggregate",
            &[
                ("operation", self.operation),
                ("samples", &samples),
                ("accepted", &accepted),
                ("dropped", &dropped),
                ("total_duration_us", &total_duration_us),
                ("max_duration_us", &max_duration_us),
                ("active_concurrency", &active_concurrency),
                ("peak_concurrency", &peak_concurrency),
            ],
            false,
        );
    }
}

#[cfg(not(any(debug_assertions, feature = "capture")))]
impl BlockingWorkerActivity {
    #[inline(always)]
    pub fn complete(self, _delivery: WorkerDelivery) {}
}

#[cfg(any(debug_assertions, feature = "capture"))]
impl Drop for BlockingWorkerActivity {
    fn drop(&mut self) {
        self.finish(WorkerDelivery::Dropped);
    }
}

#[cfg(any(debug_assertions, feature = "capture"))]
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
    fn event_capture_preserves_structured_metadata() {
        let _ = take_captured_events();
        event(
            Level::Info,
            "test.target",
            "operation complete",
            &[("id", "42")],
        );
        let events = take_captured_events();
        let event = events
            .iter()
            .find(|event| event.target == "test.target")
            .expect("captured event");
        assert_eq!(event.message, "operation complete");
        assert_eq!(event.fields.get("id"), Some(&"42".to_owned()));
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
    fn blocking_worker_aggregate_reports_and_resets_a_bounded_window() {
        let mut aggregate = BlockingWorkerAggregate::default();
        for _ in 1..BLOCKING_WORKER_SAMPLES_PER_REPORT {
            assert_eq!(
                aggregate.record(Duration::from_micros(3), WorkerDelivery::Accepted, 2),
                None
            );
        }
        assert_eq!(
            aggregate.record(Duration::from_micros(5), WorkerDelivery::Dropped, 1),
            Some(BlockingWorkerAggregate {
                samples: BLOCKING_WORKER_SAMPLES_PER_REPORT,
                accepted: BLOCKING_WORKER_SAMPLES_PER_REPORT - 1,
                dropped: 1,
                total_duration_us: u128::from(BLOCKING_WORKER_SAMPLES_PER_REPORT - 1) * 3 + 5,
                max_duration_us: 5,
                active_concurrency: 1,
                peak_concurrency: 2,
            })
        );
        assert_eq!(aggregate, BlockingWorkerAggregate::default());
    }

    #[test]
    fn diagnostic_sink_truncates_before_exceeding_its_size_bound() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("parchmint-diagnostics-{nonce}.log"));
        fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize]).expect("fill test log");
        let file = open_log_file(&path).expect("open test log");
        let mut sink = DiagnosticSink {
            file: Some(file),
            path: Some(path.clone()),
            bytes_written: MAX_LOG_BYTES,
        };
        let line = "next event\n";
        sink.write_line(line, false);

        assert!(
            fs::read_to_string(&path).expect("read test log") == line,
            "bounded sink should retain only the post-rotation line"
        );
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
