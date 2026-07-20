//! Explicit, local-only diagnostics export with a deliberately small schema.

use parchmint_storage::atomic_write;
use std::path::Path;

/// Privacy-preserving facts useful for support. Project paths and document
/// content are not accepted by this type, which makes accidental inclusion
/// impossible at this boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticsSnapshot {
    /// Whether a project was open at export time.
    pub project_open: bool,
    /// Count only; project node names are never collected.
    pub node_count: usize,
    /// Non-content workspace parse warning, when present.
    pub workspace_warning: Option<String>,
    /// Non-content disposable-index warning, when present.
    pub index_warning: Option<String>,
}

/// Writes a deterministic plain-text support report only to a user-selected path.
pub fn export_diagnostics(
    destination: &Path,
    snapshot: &DiagnosticsSnapshot,
) -> Result<(), DiagnosticsError> {
    if destination.as_os_str().is_empty() || destination.is_dir() {
        return Err(DiagnosticsError::InvalidDestination);
    }
    let sanitize = |value: Option<&str>| {
        value.map_or_else(|| "none".into(), |text| text.replace(['\r', '\n'], " "))
    };
    let report = format!(
        "ParchMint diagnostics\nversion={}\nos={}\narch={}\nproject_open={}\nnode_count={}\nworkspace_warning={}\nindex_warning={}\nnetwork_upload=disabled\ncontent_included=false\nproject_paths_included=false\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        snapshot.project_open,
        snapshot.node_count,
        sanitize(snapshot.workspace_warning.as_deref()),
        sanitize(snapshot.index_warning.as_deref()),
    );
    atomic_write(destination, report.as_bytes()).map_err(DiagnosticsError::Write)
}

/// Failure to create the explicitly requested local report.
#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    /// Destination is empty or names a directory.
    #[error("choose a diagnostics filename, not a directory")]
    InvalidDestination,
    /// Atomic local replacement failed.
    #[error("diagnostics could not be written: {0}")]
    Write(parchmint_storage::AtomicWriteError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_local_explicit_and_contains_no_content_or_path_field() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("diagnostics.txt");
        export_diagnostics(
            &destination,
            &DiagnosticsSnapshot {
                project_open: true,
                node_count: 42,
                workspace_warning: Some("line one\nline two".into()),
                index_warning: None,
            },
        )
        .unwrap();
        let report = std::fs::read_to_string(destination).unwrap();
        assert!(report.contains("node_count=42"));
        assert!(report.contains("network_upload=disabled"));
        assert!(report.contains("project_paths_included=false"));
        assert!(!report.contains("line one\nline two"));
    }
}
