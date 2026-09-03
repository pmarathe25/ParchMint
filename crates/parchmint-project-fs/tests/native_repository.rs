use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use parchmint_project_format::CanonicalRelativePath;
use parchmint_project_fs::{
    FsError, FsProjectRepository, NativeProjectFileSystem, ProjectFileSystem, UntrustedProjectPath,
};
use parchmint_project_repository::{
    CreateProject, DocumentId, ProjectPath, ProjectRepository, RepositoryError,
};

mod common;

use common::TestDir;

const LOCK_HELPER_MODE: &str = "PARCHMINT_PROJECT_FS_LOCK_HELPER_MODE";
const LOCK_HELPER_PATH: &str = "PARCHMINT_PROJECT_FS_LOCK_HELPER_PATH";

fn request(path: PathBuf) -> CreateProject {
    CreateProject {
        path: ProjectPath::new(path),
        manifest: "[project]\ntitle = \"Native project\"\n".into(),
        documents: BTreeMap::from([(
            DocumentId::new("first-document"),
            b"<p data-block-id=\"first\">First draft</p>".to_vec(),
        )]),
    }
}

fn only_html_file(root: &Path) -> PathBuf {
    let mut pending = vec![root.to_path_buf()];
    let mut matches = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).expect("project directory should remain readable") {
            let entry = entry.expect("project entry should be readable");
            let file_type = entry.file_type().expect("entry type should be readable");
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if entry.path().extension().and_then(|value| value.to_str()) == Some("html") {
                matches.push(entry.path());
            }
        }
    }
    assert_eq!(matches.len(), 1, "fixture should contain one document");
    matches.pop().expect("one HTML file was found")
}

#[test]
fn create_is_durable_and_a_new_repository_can_open_the_project() {
    let parent = TestDir::new("create-open");
    let project_path = parent.join("novel");
    let repository = FsProjectRepository::native();

    let created = repository
        .create(request(project_path.clone()))
        .expect("new project should be created");
    assert_eq!(created.snapshot.path.as_path(), project_path);
    assert_eq!(
        created.snapshot.document_ids,
        [DocumentId::new("first-document")]
    );
    assert_eq!(
        fs::read(project_path.join(".parchmint/format-version"))
            .expect("format control should be durable"),
        b"1\n"
    );
    assert_eq!(
        fs::read(project_path.join("project.toml")).expect("manifest should be durable"),
        request(project_path.clone()).manifest.as_bytes()
    );

    drop(created);
    drop(repository);

    let reopened_repository = FsProjectRepository::native();
    let reopened = reopened_repository
        .open(ProjectPath::new(project_path))
        .expect("a complete project should reopen in a fresh repository");
    assert_eq!(
        reopened.snapshot.document_ids,
        [DocumentId::new("first-document")]
    );
    assert_eq!(
        reopened_repository
            .load_document(DocumentId::new("first-document"))
            .expect("document should load after reopen"),
        b"<p data-block-id=\"first\">First draft</p>"
    );
}

#[test]
fn creation_inside_an_existing_git_worktree_is_rejected_without_partial_output() {
    let worktree = TestDir::new("git-worktree");
    fs::create_dir(worktree.join(".git")).expect("Git marker should be created");
    let project_path = worktree.join("books/novel");

    let result = FsProjectRepository::native().create(request(project_path.clone()));

    assert!(result.is_err(), "nested project creation must be refused");
    assert!(
        !project_path.exists(),
        "a rejected create must not leave a partial project"
    );
}

#[test]
fn one_writer_lock_is_enforced_across_processes() {
    let parent = TestDir::new("locking");
    let project_path = parent.join("novel");
    let mut child = lock_helper("hold", &project_path);
    let child_stdout = child.stdout.take().expect("helper stdout should be piped");
    let mut child_stdout = BufReader::new(child_stdout);
    loop {
        let mut line = String::new();
        let bytes = child_stdout
            .read_line(&mut line)
            .expect("helper readiness should be readable");
        assert_ne!(bytes, 0, "helper exited before acquiring its lock");
        if line.contains("locked") {
            break;
        }
    }

    let files = NativeProjectFileSystem::new();
    assert!(matches!(
        files.acquire(UntrustedProjectPath::new(project_path.clone())),
        Err(FsError::Locked { .. })
    ));

    writeln!(
        child.stdin.as_mut().expect("helper stdin should be piped"),
        "release"
    )
    .expect("helper should receive release signal");
    assert!(child.wait().expect("helper should exit").success());
    let (_root, lease) = files
        .acquire(UntrustedProjectPath::new(project_path))
        .expect("the lock should be acquirable after its owner exits");
    drop(lease);
}

#[test]
fn a_dead_process_lock_is_recovered_by_the_new_owner() {
    let parent = TestDir::new("stale-lock");
    let project_path = parent.join("novel");
    let status = lock_helper("exit-without-drop", &project_path)
        .wait()
        .expect("crashing helper should exit");
    assert!(status.success());

    let files = NativeProjectFileSystem::new();
    let (_root, lease) = files
        .acquire(UntrustedProjectPath::new(project_path))
        .expect("a demonstrably dead owner's stale lock should be recoverable");
    drop(lease);
}

#[test]
fn active_lease_uses_its_retained_lock_handle_for_checked_reads() {
    let parent = TestDir::new("retained-lock-handle");
    let project_path = parent.join("novel");
    let files = NativeProjectFileSystem::new();
    let (root, lease) = files
        .create_root(UntrustedProjectPath::new(project_path.clone()))
        .expect("project root should be created and locked");
    let manuscript = project_path.join("manuscript");
    fs::create_dir(&manuscript).expect("manuscript directory should be created");
    let document = CanonicalRelativePath::parse("manuscript/chapter.html")
        .expect("document path should be canonical");
    fs::write(manuscript.join("chapter.html"), b"<p>draft</p>")
        .expect("document should be written");

    assert_eq!(
        files
            .read(&root, &document)
            .expect("lock owner should be able to read its project"),
        b"<p>draft</p>"
    );

    drop(lease);
    assert!(matches!(
        files.read(&root, &document),
        Err(FsError::NotLockOwner { .. })
    ));
}

fn lock_helper(mode: &str, path: &Path) -> std::process::Child {
    Command::new(env::current_exe().expect("test executable path should be available"))
        .args(["--exact", "lock_process_helper", "--nocapture"])
        .env(LOCK_HELPER_MODE, mode)
        .env(LOCK_HELPER_PATH, path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("lock helper should start")
}

#[test]
fn lock_process_helper() {
    let Some(mode) = env::var_os(LOCK_HELPER_MODE) else {
        return;
    };
    let path = env::var_os(LOCK_HELPER_PATH).expect("helper project path should be supplied");
    let files = NativeProjectFileSystem::new();
    let (_root, lease) = files
        .create_root(UntrustedProjectPath::new(PathBuf::from(path)))
        .expect("helper should create and lock the project");
    println!("locked");
    std::io::stdout()
        .flush()
        .expect("helper readiness should flush");

    if mode == "exit-without-drop" {
        std::mem::forget(lease);
        std::process::exit(0);
    }

    let mut release = String::new();
    std::io::stdin()
        .read_line(&mut release)
        .expect("helper release should be readable");
    assert_eq!(release.trim(), "release");
    drop(lease);
}

#[test]
fn opening_rejects_missing_and_corrupt_canonical_resources() {
    let parent = TestDir::new("integrity");
    let missing_path = parent.join("missing");
    let repository = FsProjectRepository::native();
    let opened = repository
        .create(request(missing_path.clone()))
        .expect("fixture should be created");
    drop(opened);
    fs::remove_file(missing_path.join("project.toml")).expect("manifest should be removed");
    assert!(matches!(
        repository.open(ProjectPath::new(missing_path)),
        Err(RepositoryError::MissingResource { .. })
    ));

    let corrupt_path = parent.join("corrupt");
    let opened = repository
        .create(request(corrupt_path.clone()))
        .expect("second fixture should be created");
    drop(opened);
    fs::write(corrupt_path.join(".parchmint/format-version"), b"999\n")
        .expect("format control should be corrupted");
    assert!(
        repository.open(ProjectPath::new(corrupt_path)).is_err(),
        "unsupported format control must fail before editing starts"
    );
}

#[test]
fn document_integrity_is_checked_when_the_lazy_body_is_loaded() {
    let parent = TestDir::new("lazy-integrity");
    let project_path = parent.join("novel");
    let repository = FsProjectRepository::native();
    let opened = repository
        .create(request(project_path.clone()))
        .expect("fixture should be created");
    drop(opened);
    fs::write(only_html_file(&project_path), b"<script>corrupt()</script>")
        .expect("document should be corrupted");

    let reopened = repository
        .open(ProjectPath::new(project_path))
        .expect("opening should not eagerly decode document bodies");
    assert!(
        repository
            .load_document(DocumentId::new("first-document"))
            .is_err(),
        "invalid canonical HTML must not be returned to the caller"
    );
    drop(reopened);
}

#[test]
fn checked_reads_reject_a_symlink_or_reparse_escape() {
    let parent = TestDir::new("read-escape");
    let project_path = parent.join("novel");
    let outside = parent.join("outside.txt");
    fs::write(&outside, b"outside secret").expect("outside file should be created");
    let files = NativeProjectFileSystem::new();
    let (root, lease) = files
        .create_root(UntrustedProjectPath::new(project_path.clone()))
        .expect("project root should be created");
    fs::create_dir(project_path.join("manuscript")).expect("document directory should be created");

    if let Err(error) = symlink_file(&outside, &project_path.join("manuscript/escape.html")) {
        if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied {
            drop(lease);
            return;
        }
        panic!("test symlink should be created: {error}");
    }

    let escaped = CanonicalRelativePath::parse("manuscript/escape.html")
        .expect("the lexical path itself is canonical");
    assert!(matches!(
        files.read(&root, &escaped),
        Err(FsError::UnsafePath { .. })
    ));
    drop(lease);
}

#[cfg(unix)]
fn symlink_file(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(windows)]
fn symlink_file(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(original, link)
}
