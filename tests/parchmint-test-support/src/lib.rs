use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use parchmint_project_format::{
    CanonicalBytes, CanonicalCodec, CanonicalInputSet, CanonicalRelativePath, CanonicalResource,
    FormatError, FormatVersion, ProjectFormatCodec,
};

/// Canonical fixture bytes, including the format control and resource paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalResourceSet {
    pub format_version: FormatVersion,
    pub resources: BTreeMap<CanonicalRelativePath, CanonicalBytes>,
}

pub struct ScopedProject {
    pub root: PathBuf,
}

impl Drop for ScopedProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Debug)]
pub enum FixtureError {
    MissingFixture(PathBuf),
    InvalidFixture(io::Error),
    InvalidFormat(FormatError),
}

impl fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFixture(path) => write!(formatter, "fixture not found: {path:?}"),
            Self::InvalidFixture(error) => write!(formatter, "fixture is invalid: {error}"),
            Self::InvalidFormat(error) => write!(formatter, "fixture format is invalid: {error}"),
        }
    }
}

impl std::error::Error for FixtureError {}

impl From<io::Error> for FixtureError {
    fn from(error: io::Error) -> Self {
        Self::InvalidFixture(error)
    }
}

impl From<FormatError> for FixtureError {
    fn from(error: FormatError) -> Self {
        Self::InvalidFormat(error)
    }
}

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl ScopedProject {
    pub fn from_fixture(fixture: &str) -> Result<Self, FixtureError> {
        let source = resolve_fixture_path(fixture);
        if !source.is_dir() {
            return Err(FixtureError::MissingFixture(source));
        }

        let target = temporary_root();
        copy_dir_recursive(&source, &target)?;

        Ok(Self { root: target })
    }

    pub fn canonical_bytes(&self) -> Result<CanonicalResourceSet, FixtureError> {
        let codec = ProjectFormatCodec::default();
        let mut files = read_fixture_files(self.root.as_path())?;
        files.remove(".parchmint/root-id");
        let format_control = files.remove(".parchmint/format-version").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "fixture is missing format control",
            )
        })?;

        let decoded = codec.decode_project(CanonicalInputSet {
            format_control: Some(format_control),
            resources: files
                .into_iter()
                .map(|(path, bytes)| Ok((CanonicalRelativePath::parse(path)?, bytes)))
                .collect::<Result<BTreeMap<_, _>, FormatError>>()?,
        })?;

        let mut resources = BTreeMap::new();
        for (path, resource) in decoded.resources {
            let mut canonical = codec.encode(&resource)?;
            canonical.path = path;
            resources.insert(canonical.path.clone(), canonical);
        }

        let control = codec.encode(&CanonicalResource::FormatControl(decoded.format_version))?;
        resources.insert(control.path.clone(), control);

        Ok(CanonicalResourceSet {
            format_version: decoded.format_version,
            resources,
        })
    }

    pub fn canonical_document_bytes(&self) -> Result<CanonicalResourceSet, FixtureError> {
        self.canonical_bytes()
    }
}

fn temporary_root() -> PathBuf {
    let sequence = FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = std::env::temp_dir();
    #[cfg(windows)]
    let mut path = temporary;
    #[cfg(not(windows))]
    let mut path = fs::canonicalize(&temporary).unwrap_or(temporary);
    path.push("parchmint-test-support");
    path.push(format!(
        "fixture-{pid}-{seed}",
        pid = std::process::id(),
        seed = sequence,
    ));
    path
}

fn resolve_fixture_path(fixture: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let direct = manifest_dir.join("fixtures").join(fixture);
    if direct.is_dir() {
        return direct;
    }

    let with_extension = direct.with_extension("tmx");
    if with_extension.is_dir() {
        return with_extension;
    }

    direct
}

fn read_fixture_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, FixtureError> {
    let mut output = BTreeMap::new();
    collect_files(root, root, &mut output)?;
    Ok(output)
}

fn collect_files(
    root: &Path,
    current: &Path,
    output: &mut BTreeMap<String, Vec<u8>>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let metadata = entry.file_type()?;
        if metadata.is_dir() {
            if is_git_directory(&entry.path()) {
                continue;
            }
            collect_files(root, &entry.path(), output)?;
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        let mut bytes = Vec::new();
        fs::File::open(entry.path())?.read_to_end(&mut bytes)?;

        let entry_path = entry.path();
        let rel = entry_path.strip_prefix(root).unwrap_or(&entry_path);
        let path = rel.to_string_lossy().replace('\\', "/");
        output.insert(path, bytes);
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if is_git_directory(&source_path) {
                continue;
            }
            copy_dir_recursive(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn is_git_directory(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == ".git")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_fixture_bytes_are_stable() {
        let first =
            ScopedProject::from_fixture("canonical/minimal-project").expect("fixture should exist");
        let second =
            ScopedProject::from_fixture("canonical/minimal-project").expect("fixture should exist");

        assert_eq!(
            first.canonical_bytes().expect("fixture should decode"),
            second.canonical_bytes().expect("fixture should decode")
        );
    }

    #[test]
    fn fixture_helpers_skip_git_but_keep_parchmint() {
        let source = temporary_root();
        let target = temporary_root();
        fs::create_dir_all(source.join(".git")).expect("git directory should be created");
        fs::create_dir_all(source.join(".parchmint"))
            .expect("parchmint directory should be created");
        fs::write(source.join(".git/HEAD"), b"git metadata")
            .expect("git metadata should be written");
        fs::write(source.join(".parchmint/format-version"), b"format control")
            .expect("parchmint metadata should be written");

        copy_dir_recursive(&source, &target).expect("fixture should be copied");
        assert!(!target.join(".git").exists());
        assert!(target.join(".parchmint/format-version").is_file());

        let files = read_fixture_files(&source).expect("fixture files should be collected");
        assert!(!files.contains_key(".git/HEAD"));
        assert!(files.contains_key(".parchmint/format-version"));

        fs::remove_dir_all(&source).expect("source fixture should be removed");
        fs::remove_dir_all(&target).expect("copied fixture should be removed");
    }
}
