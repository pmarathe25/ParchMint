//! Agent-facing complete-application UI driver and acceptance scenarios.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// An isolated filesystem root removed when its owner is dropped.
pub struct IsolatedRun {
    root: PathBuf,
}

impl IsolatedRun {
    pub fn new(label: &str) -> std::io::Result<Self> {
        static NEXT_RUN: AtomicU64 = AtomicU64::new(1);
        let temporary = fs::canonicalize(std::env::temp_dir())?;
        let root = loop {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let candidate = temporary.join(format!(
                "parchmint-ui-{label}-{}-{stamp}-{}",
                std::process::id(),
                NEXT_RUN.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for IsolatedRun {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

use parchmint_desktop::{DesktopInteractionHarness, HarnessWindow, LaunchRequest};

pub fn create_project(run: &IsolatedRun, project: &Path, title: &str) -> DesktopInteractionHarness {
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("launch application");
    harness
        .click_text(HarnessWindow::Project, "My Writing")
        .expect("open project menu");
    harness
        .click_text(HarnessWindow::Project, "Create Project")
        .expect("open project form");
    harness
        .type_into(HarnessWindow::Project, "Project title", title)
        .expect("enter title");
    harness
        .type_into(
            HarnessWindow::Project,
            "Project destination",
            project.to_string_lossy(),
        )
        .expect("enter destination");
    harness
        .click_text(HarnessWindow::Project, "Create and Open")
        .expect("create project");
    harness
}

pub fn create_group(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent menu");
    harness
        .click_text(HarnessWindow::Project, "New group")
        .expect("create group");
    harness
        .replace_text_and_submit(HarnessWindow::Project, "New Group", title)
        .expect("name group");
}

pub fn create_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent menu");
    harness
        .click_text(HarnessWindow::Project, "New document")
        .expect("create document");
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Untitled", title)
        .expect("name document");
}
