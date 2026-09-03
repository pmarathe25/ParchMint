use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub struct TestDir(PathBuf);

impl TestDir {
    pub fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = fs::canonicalize(std::env::temp_dir())
            .expect("temporary directory should resolve")
            .join(format!(
                "parchmint-project-fs-{label}-{pid}-{sequence}",
                pid = std::process::id()
            ));
        fs::create_dir(&path).expect("test root should be created");
        Self(path)
    }

    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.0.join(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
