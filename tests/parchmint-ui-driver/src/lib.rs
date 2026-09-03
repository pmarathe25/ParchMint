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
        let root = fs::canonicalize(std::env::temp_dir())?.join(format!(
            "parchmint-ui-{label}-{}-{}",
            std::process::id(),
            NEXT_RUN.fetch_add(1, Ordering::Relaxed)
        ));
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;
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
