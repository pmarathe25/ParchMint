//! Filesystem primitives whose behavior is independent of project schemas.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use thiserror::Error;

/// Writes `contents` beside `destination`, flushes it, and atomically replaces
/// the destination when the platform filesystem supports atomic replacement.
pub fn atomic_write(destination: &Path, contents: &[u8]) -> Result<(), AtomicWriteError> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| AtomicWriteError::MissingParent(destination.to_owned()))?;
    fs::create_dir_all(parent).map_err(AtomicWriteError::PrepareDirectory)?;

    let mut temporary = NamedTempFile::new_in(parent).map_err(AtomicWriteError::CreateTemporary)?;
    temporary
        .write_all(contents)
        .map_err(AtomicWriteError::WriteTemporary)?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(AtomicWriteError::FlushTemporary)?;
    temporary
        .persist(destination)
        .map_err(|error| AtomicWriteError::Replace(error.error))?;
    sync_parent(parent)?;
    Ok(())
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), AtomicWriteError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(AtomicWriteError::FlushDirectory)
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), AtomicWriteError> {
    // Windows replacement durability is provided by tempfile's platform rename
    // plus the file flush. Directory handles require privileges unavailable to
    // normal desktop applications; this limitation is recorded in ADR-0006.
    Ok(())
}

/// A phase-specific atomic-write failure.
#[derive(Debug, Error)]
pub enum AtomicWriteError {
    /// Destination has no usable containing directory.
    #[error("atomic-write destination has no parent: {0}")]
    MissingParent(PathBuf),
    /// Parent directory could not be prepared.
    #[error("could not prepare destination directory: {0}")]
    PrepareDirectory(io::Error),
    /// Same-directory temporary file creation failed.
    #[error("could not create same-directory temporary file: {0}")]
    CreateTemporary(io::Error),
    /// Temporary content write failed.
    #[error("could not write temporary file: {0}")]
    WriteTemporary(io::Error),
    /// Temporary content could not be flushed.
    #[error("could not flush temporary file: {0}")]
    FlushTemporary(io::Error),
    /// Atomic replacement failed.
    #[error("could not atomically replace destination: {0}")]
    Replace(io::Error),
    /// Parent directory metadata could not be flushed.
    #[error("replacement succeeded but directory metadata flush failed: {0}")]
    FlushDirectory(io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_replaces_without_partial_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("document.md");
        atomic_write(&path, b"old state").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"old state");
        atomic_write(&path, b"complete new state").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"complete new state");
        let leftovers = fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        assert_eq!(leftovers.len(), 1);
    }

    #[test]
    fn failed_parent_is_reported_without_touching_destination() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        fs::write(&blocker, b"canonical").unwrap();
        let result = atomic_write(&blocker.join("document.md"), b"replacement");
        assert!(result.is_err());
        assert_eq!(fs::read(blocker).unwrap(), b"canonical");
    }
}
