use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

use parchmint_project_fs::{
    AtomicFileOps, CheckedTarget, FsAtomicWriter, FsError, NativeAtomicFileOps,
    NativeProjectFileSystem, ProjectFileSystem, TemporaryFile, TemporaryWrite,
    UntrustedProjectPath,
};
use parchmint_project_repository::{AtomicWritePlan, AtomicWriter, StagedResource, WriteError};

mod common;

use common::TestDir;

#[derive(Clone, Copy)]
enum ReplaceFault {
    BeforeFirstReplace,
    AfterFirstReplace,
}

struct FaultingOps {
    inner: NativeAtomicFileOps,
    fault: ReplaceFault,
    replacements: AtomicUsize,
}

impl FaultingOps {
    fn new(inner: NativeAtomicFileOps, fault: ReplaceFault) -> Self {
        Self {
            inner,
            fault,
            replacements: AtomicUsize::new(0),
        }
    }
}

impl AtomicFileOps for FaultingOps {
    fn write_temporary(&self, write: TemporaryWrite) -> Result<TemporaryFile, FsError> {
        self.inner.write_temporary(write)
    }

    fn flush_file(&self, file: &TemporaryFile) -> Result<(), FsError> {
        self.inner.flush_file(file)
    }

    fn replace(&self, file: TemporaryFile, target: &CheckedTarget) -> Result<(), FsError> {
        let replacement = self.replacements.fetch_add(1, Ordering::SeqCst);
        if replacement == 0 && matches!(self.fault, ReplaceFault::BeforeFirstReplace) {
            return Err(FsError::injected("replace"));
        }

        self.inner.replace(file, target)?;
        if replacement == 0 && matches!(self.fault, ReplaceFault::AfterFirstReplace) {
            return Err(FsError::injected("replace outcome unknown"));
        }
        Ok(())
    }

    fn flush_parent(&self, target: &CheckedTarget) -> Result<(), FsError> {
        self.inner.flush_parent(target)
    }
}

fn replacement_plan() -> AtomicWritePlan {
    AtomicWritePlan::new(vec![
        StagedResource {
            path: "project.toml".into(),
            bytes: b"new manifest".to_vec(),
        },
        StagedResource {
            path: "styles.css".into(),
            bytes: b"new styles".to_vec(),
        },
    ])
}

fn assert_complete_old_or_new(root: &Path) {
    let manifest = fs::read(root.join("project.toml")).expect("manifest should remain available");
    let styles = fs::read(root.join("styles.css")).expect("styles should remain available");
    assert!(
        (manifest == b"old manifest" && styles == b"old styles")
            || (manifest == b"new manifest" && styles == b"new styles"),
        "recovery must expose one complete canonical generation"
    );
}

fn interrupt_and_reconcile(fault: ReplaceFault) {
    let parent = TestDir::new("reconcile");
    let project_path = parent.join("novel");
    let files = NativeProjectFileSystem::new();
    let (root, lease) = files
        .create_root(UntrustedProjectPath::new(project_path.clone()))
        .expect("project root should be created");
    fs::write(project_path.join("project.toml"), b"old manifest")
        .expect("old manifest should be written");
    fs::write(project_path.join("styles.css"), b"old styles")
        .expect("old styles should be written");

    let operations = FaultingOps::new(NativeAtomicFileOps::new(root.clone()), fault);
    let writer = FsAtomicWriter::new(operations);
    let staged = writer
        .stage(replacement_plan())
        .expect("temporary files and transaction record should stage");
    assert!(matches!(
        writer.commit(staged),
        Err(WriteError::Interrupted)
    ));
    drop(writer);
    drop(lease);

    let (root, lease) = files
        .acquire(UntrustedProjectPath::new(project_path.clone()))
        .expect("project should reopen after the interrupted writer exits");
    let records = files
        .transaction_records(&root)
        .expect("an interrupted commit should leave a readable transaction record");
    assert_eq!(records.len(), 1);
    let recovery_writer = FsAtomicWriter::new(NativeAtomicFileOps::new(root.clone()));
    assert!(
        recovery_writer
            .reconcile(records.into_iter().next().expect("one record"))
            .expect("the lock owner should reconcile the interrupted transaction")
            .is_reconciled()
    );
    assert_complete_old_or_new(&project_path);
    assert!(
        files
            .transaction_records(&root)
            .expect("transaction directory should remain readable")
            .is_empty(),
        "completed reconciliation should remove its durable record"
    );
    drop(lease);
}

#[test]
fn failure_before_replacement_reopens_to_a_complete_generation() {
    interrupt_and_reconcile(ReplaceFault::BeforeFirstReplace);
}

#[test]
fn failure_after_a_partial_replacement_reopens_to_a_complete_generation() {
    interrupt_and_reconcile(ReplaceFault::AfterFirstReplace);
}

#[test]
fn successful_commit_replaces_every_target_and_clears_recovery_state() {
    let parent = TestDir::new("commit");
    let project_path = parent.join("novel");
    let files = NativeProjectFileSystem::new();
    let (root, lease) = files
        .create_root(UntrustedProjectPath::new(project_path.clone()))
        .expect("project root should be created");
    fs::write(project_path.join("project.toml"), b"old manifest")
        .expect("old manifest should be written");
    fs::write(project_path.join("styles.css"), b"old styles")
        .expect("old styles should be written");
    let writer = FsAtomicWriter::new(NativeAtomicFileOps::new(root.clone()));

    let staged = writer
        .stage(replacement_plan())
        .expect("writes should stage");
    writer.commit(staged).expect("commit should become durable");

    assert_eq!(
        fs::read(project_path.join("project.toml")).expect("manifest should be readable"),
        b"new manifest"
    );
    assert_eq!(
        fs::read(project_path.join("styles.css")).expect("styles should be readable"),
        b"new styles"
    );
    assert!(
        files
            .transaction_records(&root)
            .expect("transaction directory should be readable")
            .is_empty()
    );
    drop(lease);
}

#[test]
fn stage_rejects_portable_path_collisions_without_touching_targets() {
    let parent = TestDir::new("collisions");
    let project_path = parent.join("novel");
    let files = NativeProjectFileSystem::new();
    let (root, lease) = files
        .create_root(UntrustedProjectPath::new(project_path.clone()))
        .expect("project root should be created");
    let writer = FsAtomicWriter::new(NativeAtomicFileOps::new(root));

    for paths in [
        ["manuscript/Chapter.html", "manuscript/chapter.html"],
        ["manuscript/caf\u{e9}.html", "manuscript/cafe\u{301}.html"],
    ] {
        let plan = AtomicWritePlan::new(
            paths
                .into_iter()
                .map(|path| StagedResource {
                    path: path.into(),
                    bytes: b"<p>body</p>".to_vec(),
                })
                .collect(),
        );
        assert!(matches!(writer.stage(plan), Err(WriteError::UnsafePath(_))));
    }

    assert!(!project_path.join("manuscript").exists());
    drop(lease);
}

#[test]
fn target_identity_is_rechecked_immediately_before_replacement() {
    let parent = TestDir::new("target-swap");
    let project_path = parent.join("novel");
    let outside = parent.join("outside");
    fs::create_dir(&outside).expect("outside directory should be created");
    fs::write(outside.join("document.html"), b"outside original")
        .expect("outside file should be created");
    let files = NativeProjectFileSystem::new();
    let (root, lease) = files
        .create_root(UntrustedProjectPath::new(project_path.clone()))
        .expect("project root should be created");
    fs::create_dir(project_path.join("manuscript"))
        .expect("project document directory should be created");
    fs::write(project_path.join("manuscript/document.html"), b"old body")
        .expect("old target should be written");
    let writer = FsAtomicWriter::new(NativeAtomicFileOps::new(root));
    let staged = writer
        .stage(AtomicWritePlan::new(vec![StagedResource {
            path: "manuscript/document.html".into(),
            bytes: b"new body".to_vec(),
        }]))
        .expect("write should stage before the directory swap");

    fs::rename(
        project_path.join("manuscript"),
        project_path.join("original-manuscript"),
    )
    .expect("original directory should move aside");
    if let Err(error) = symlink_dir(&outside, &project_path.join("manuscript")) {
        if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied {
            drop(lease);
            return;
        }
        panic!("test symlink should be created: {error}");
    }

    assert!(matches!(
        writer.commit(staged),
        Err(WriteError::UnsafePath(_)) | Err(WriteError::Interrupted)
    ));
    assert_eq!(
        fs::read(outside.join("document.html")).expect("outside file should remain readable"),
        b"outside original",
        "replacement must never follow a swapped directory outside the root"
    );
    drop(lease);
}

#[test]
fn a_foreign_lock_owner_cannot_reconcile_an_interrupted_transaction() {
    let parent = TestDir::new("foreign-recovery");
    let first_path = parent.join("first");
    let second_path = parent.join("second");
    let files = NativeProjectFileSystem::new();
    let (first_root, first_lease) = files
        .create_root(UntrustedProjectPath::new(first_path))
        .expect("first root should be created");
    let (second_root, second_lease) = files
        .create_root(UntrustedProjectPath::new(second_path))
        .expect("second root should be created");
    let first_writer = FsAtomicWriter::new(NativeAtomicFileOps::new(first_root.clone()));
    let staged = first_writer
        .stage(replacement_plan())
        .expect("first-root transaction should stage");
    let record = files
        .transaction_records(&first_root)
        .expect("record should be readable")
        .into_iter()
        .next()
        .expect("staging should persist a transaction record");
    let foreign_writer = FsAtomicWriter::new(NativeAtomicFileOps::new(second_root));

    assert!(matches!(
        foreign_writer.reconcile(record),
        Err(WriteError::ForeignRoot)
    ));

    first_writer
        .abandon(staged)
        .expect("the owning writer should clean up its transaction");
    drop(first_lease);
    drop(second_lease);
}

#[cfg(unix)]
fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(windows)]
fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(original, link)
}
