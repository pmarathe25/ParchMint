#![allow(missing_docs)]
//! Programmatic path normalization and preflight validation shared by every
//! Qt-facing native dialog result.

use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectPathIntent {
    Open,
    CreateParent,
    FileDestination,
    FileSource,
}

#[derive(Debug, Error)]
pub enum PathInputError {
    #[error("Choose a path before continuing")]
    Empty,
    #[error("The selected path contains an invalid NUL character")]
    Nul,
    #[error("The selected URL is malformed: {0}")]
    MalformedUrl(String),
    #[error("Remote URLs are not supported; choose a local file or folder")]
    RemoteUrl,
    #[error("The selected path does not exist: {0}")]
    Missing(PathBuf),
    #[error("The selected project path is a regular file, not a folder: {0}")]
    RegularFile(PathBuf),
    #[error("The selected path is not a regular file: {0}")]
    NotRegularFile(PathBuf),
    #[error("The selected parent is not a folder: {0}")]
    ParentNotDirectory(PathBuf),
    #[error("The selected location is inaccessible: {path}: {error}")]
    Inaccessible {
        path: PathBuf,
        error: std::io::Error,
    },
    #[error("Enter a project name")]
    EmptyProjectName,
    #[error("Project names cannot be '.' or '..' and cannot contain path separators")]
    InvalidProjectName,
    #[error("A regular file already exists at the new project path: {0}")]
    DestinationFile(PathBuf),
    #[error("The new project folder is not empty: {0}")]
    DestinationNotEmpty(PathBuf),
}

/// Converts a QString-compatible input without consulting process CWD.
/// Relative paths are anchored at the platform Documents location supplied by
/// Qt, and `file:` URLs are decoded exactly once.
pub fn normalize_path_input(
    input: &str,
    documents: &Path,
    home: Option<&Path>,
) -> Result<PathBuf, PathInputError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(PathInputError::Empty);
    }
    if trimmed.contains('\0') {
        return Err(PathInputError::Nul);
    }

    let decoded = if trimmed
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
    {
        decode_file_url(trimmed)?
    } else if trimmed.contains("://") {
        return Err(PathInputError::RemoteUrl);
    } else {
        trimmed.to_owned()
    };

    let expanded = if decoded == "~" {
        home.map_or_else(|| PathBuf::from(decoded), Path::to_owned)
    } else if let Some(rest) = decoded
        .strip_prefix("~/")
        .or_else(|| decoded.strip_prefix("~\\"))
    {
        home.map_or_else(|| PathBuf::from(&decoded), |home| home.join(rest))
    } else {
        PathBuf::from(decoded)
    };
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        documents.join(expanded)
    };
    Ok(lexically_normalize(&absolute))
}

pub fn validate_project_path(path: &Path, intent: ProjectPathIntent) -> Result<(), PathInputError> {
    match intent {
        ProjectPathIntent::Open => {
            let metadata = metadata(path)?;
            if metadata.is_file() {
                return Err(PathInputError::RegularFile(path.to_owned()));
            }
            if !metadata.is_dir() {
                return Err(PathInputError::ParentNotDirectory(path.to_owned()));
            }
            fs::read_dir(path).map_err(|error| PathInputError::Inaccessible {
                path: path.to_owned(),
                error,
            })?;
        }
        ProjectPathIntent::CreateParent => {
            let metadata = metadata(path)?;
            if !metadata.is_dir() {
                return Err(PathInputError::ParentNotDirectory(path.to_owned()));
            }
            fs::read_dir(path).map_err(|error| PathInputError::Inaccessible {
                path: path.to_owned(),
                error,
            })?;
        }
        ProjectPathIntent::FileSource => {
            let metadata = metadata(path)?;
            if !metadata.is_file() {
                return Err(PathInputError::NotRegularFile(path.to_owned()));
            }
        }
        ProjectPathIntent::FileDestination => {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .ok_or_else(|| PathInputError::ParentNotDirectory(path.to_owned()))?;
            let metadata = metadata(parent)?;
            if !metadata.is_dir() {
                return Err(PathInputError::ParentNotDirectory(parent.to_owned()));
            }
        }
    }
    Ok(())
}

pub fn validate_project_creation(parent: &Path, name: &str) -> Result<PathBuf, PathInputError> {
    validate_project_path(parent, ProjectPathIntent::CreateParent)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(PathInputError::EmptyProjectName);
    }
    if matches!(name, "." | "..")
        || name.contains(['/', '\\', '\0'])
        || Path::new(name).components().count() != 1
    {
        return Err(PathInputError::InvalidProjectName);
    }
    let destination = parent.join(name);
    if destination.is_file() {
        return Err(PathInputError::DestinationFile(destination));
    }
    if destination.is_dir()
        && fs::read_dir(&destination)
            .map_err(|error| PathInputError::Inaccessible {
                path: destination.clone(),
                error,
            })?
            .next()
            .is_some()
    {
        return Err(PathInputError::DestinationNotEmpty(destination));
    }
    Ok(destination)
}

fn metadata(path: &Path) -> Result<fs::Metadata, PathInputError> {
    fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PathInputError::Missing(path.to_owned())
        } else {
            PathInputError::Inaccessible {
                path: path.to_owned(),
                error,
            }
        }
    })
}

fn decode_file_url(value: &str) -> Result<String, PathInputError> {
    let mut remainder = value
        .get(5..)
        .ok_or_else(|| PathInputError::MalformedUrl(value.into()))?;
    if let Some(authority_and_path) = remainder.strip_prefix("//") {
        let slash = authority_and_path.find('/');
        let (authority, path) = slash.map_or((authority_and_path, ""), |index| {
            (&authority_and_path[..index], &authority_and_path[index..])
        });
        if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
            return Err(PathInputError::RemoteUrl);
        }
        remainder = path;
    }
    if remainder.is_empty() {
        return Err(PathInputError::MalformedUrl(value.into()));
    }
    let decoded = percent_decode(remainder)?;
    #[cfg(windows)]
    let decoded = decoded
        .strip_prefix('/')
        .filter(|path| path.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(&decoded)
        .to_owned();
    Ok(decoded)
}

fn percent_decode(value: &str) -> Result<String, PathInputError> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|byte| hex(*byte));
            let low = bytes.get(index + 2).and_then(|byte| hex(*byte));
            let (Some(high), Some(low)) = (high, low) else {
                return Err(PathInputError::MalformedUrl(value.into()));
            };
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| PathInputError::MalformedUrl(value.into()))
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn lexically_normalize(path: &Path) -> PathBuf {
    let mut prefix = OsString::new();
    let mut absolute = false;
    let mut parts = Vec::<OsString>::new();
    for component in path.components() {
        match component {
            Component::Prefix(value) => value.as_os_str().clone_into(&mut prefix),
            Component::RootDir => absolute = true,
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.last().is_some_and(|part| part != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..".into());
                }
            }
            Component::Normal(value) => parts.push(value.to_owned()),
        }
    }
    let mut normalized = PathBuf::new();
    if !prefix.is_empty() {
        normalized.push(prefix);
    }
    if absolute {
        normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR));
    }
    for part in parts {
        normalized.push(part);
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_native_home_relative_and_file_url_inputs() {
        let root = tempfile::tempdir().unwrap();
        let documents = root.path().join("A Writer/Documents");
        let home = root.path().join("A Writer");
        assert_eq!(
            normalize_path_input(" Drafts/My #1% Novel ", &documents, Some(&home)).unwrap(),
            documents.join("Drafts/My #1% Novel")
        );
        assert_eq!(
            normalize_path_input("~/My Novel", &documents, Some(&home)).unwrap(),
            home.join("My Novel")
        );
        assert_eq!(
            normalize_path_input("~\\My Novel", &documents, Some(&home)).unwrap(),
            home.join("My Novel")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn decodes_unix_file_urls_once() {
        assert_eq!(
            normalize_path_input(
                "file:///Users/A%20Writer/%E6%9C%AC%23%25",
                Path::new("/documents"),
                None
            )
            .unwrap(),
            Path::new("/Users/A Writer/本#%")
        );
    }

    #[cfg(windows)]
    #[test]
    fn decodes_windows_file_urls_once() {
        assert_eq!(
            normalize_path_input(
                "file:///C:/Users/A%20Writer/%E6%9C%AC%23%25",
                Path::new(r"C:\Documents"),
                None
            )
            .unwrap(),
            Path::new(r"C:\Users\A Writer\本#%")
        );
    }

    #[test]
    fn rejects_empty_malformed_and_remote_urls() {
        let documents = Path::new("/documents");
        assert!(matches!(
            normalize_path_input("  ", documents, None),
            Err(PathInputError::Empty)
        ));
        assert!(matches!(
            normalize_path_input("file:///bad%2", documents, None),
            Err(PathInputError::MalformedUrl(_))
        ));
        assert!(matches!(
            normalize_path_input("https://example.test/project", documents, None),
            Err(PathInputError::RemoteUrl)
        ));
        assert!(matches!(
            normalize_path_input("file://server/share", documents, None),
            Err(PathInputError::RemoteUrl)
        ));
    }

    #[test]
    fn create_and_open_preflight_distinguish_files_missing_and_nonempty() {
        let directory = tempfile::tempdir().unwrap();
        let regular = directory.path().join("project.parchmint");
        fs::write(&regular, b"not a directory").unwrap();
        assert!(matches!(
            validate_project_path(&regular, ProjectPathIntent::Open),
            Err(PathInputError::RegularFile(_))
        ));
        assert!(matches!(
            validate_project_path(&directory.path().join("missing"), ProjectPathIntent::Open),
            Err(PathInputError::Missing(_))
        ));
        let destination = validate_project_creation(directory.path(), "Novel #1").unwrap();
        assert_eq!(destination, directory.path().join("Novel #1"));
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("occupied"), b"x").unwrap();
        assert!(matches!(
            validate_project_creation(directory.path(), "Novel #1"),
            Err(PathInputError::DestinationNotEmpty(_))
        ));
    }
}
