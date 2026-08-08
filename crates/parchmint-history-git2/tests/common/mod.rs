use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use parchmint_history_api::{
    CheckpointCategory, CheckpointInput, CheckpointIntentHash, HistoryError, HistoryStore,
    SnapshotName,
};
use parchmint_history_git2::Git2HistoryStore;
use parchmint_project_format::{
    CanonicalBytes, CanonicalCodec, CanonicalRelativePath, CanonicalResource, ContentHash,
    FormatVersion, ProjectFormatCodec,
};
use parchmint_project_fs::{
    NativeProjectFileSystem, ProjectFileSystem, ProjectLockLease,
    ProjectRootCapability as NativeProjectRoot, UntrustedProjectPath,
};
use parchmint_project_repository::ProjectRootCapability;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
static NEXT_PROJECT: AtomicU64 = AtomicU64::new(9);

pub const TEST_DOCUMENT: parchmint_history_api::DocumentId =
    parchmint_history_api::DocumentId::from_bytes([0x44; 16]);

pub struct TestDir(PathBuf);

impl TestDir {
    pub fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "parchmint-history-git2-{label}-{pid}-{sequence}",
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

#[derive(Clone, Debug)]
pub struct ProjectVersion {
    resources: BTreeMap<CanonicalRelativePath, (Vec<u8>, ContentHash)>,
}

impl ProjectVersion {
    pub fn named(label: &str) -> Self {
        let codec = ProjectFormatCodec::default();
        let mut resources = BTreeMap::new();

        insert(
            &mut resources,
            codec
                .encode(&CanonicalResource::FormatControl(FormatVersion::V1))
                .expect("format control should encode"),
        );

        let manifest = format!(
            "title = \"{label}\"\nsynopsis = \"{label} synopsis\"\ndeleted = []\n\n[metadata]\nstatus = \"draft\"\n"
        );
        let manifest = codec
            .decode_manifest(manifest.as_bytes())
            .expect("manifest fixture should decode");
        insert(
            &mut resources,
            codec
                .encode(&CanonicalResource::Manifest(manifest))
                .expect("manifest fixture should encode"),
        );

        let styles = codec
            .decode_styles(b"p { font-weight: normal; }\n")
            .expect("styles fixture should decode");
        insert(
            &mut resources,
            codec
                .encode(&CanonicalResource::Styles(styles))
                .expect("styles fixture should encode"),
        );

        let dictionary = codec
            .decode_dictionary(b"ParchMint\n")
            .expect("dictionary fixture should decode");
        insert(
            &mut resources,
            codec
                .encode(&CanonicalResource::Dictionary(dictionary))
                .expect("dictionary fixture should encode"),
        );

        let document = format!("<p data-block-id=\"block-1\">{label}</p>\n");
        let document = codec
            .decode_document(document.as_bytes())
            .expect("document fixture should decode");
        insert(
            &mut resources,
            codec
                .encode(&CanonicalResource::Document(document))
                .expect("document fixture should encode"),
        );

        let annotations = codec
            .decode_annotations(
                br#"{"schema":"parchmint.annotation-sidecar/v1","document_id":"document-1","threads":[]}"#,
            )
            .expect("annotation fixture should decode");
        insert(
            &mut resources,
            codec
                .encode(&CanonicalResource::Annotations(annotations))
                .expect("annotation fixture should encode"),
        );

        Self { resources }
    }

    pub fn hashes(&self) -> BTreeMap<CanonicalRelativePath, ContentHash> {
        self.resources
            .iter()
            .map(|(path, (_, hash))| (path.clone(), *hash))
            .collect()
    }

    pub fn bytes(&self) -> BTreeMap<CanonicalRelativePath, Vec<u8>> {
        self.resources
            .iter()
            .map(|(path, (bytes, _))| (path.clone(), bytes.clone()))
            .collect()
    }
}

fn insert(
    resources: &mut BTreeMap<CanonicalRelativePath, (Vec<u8>, ContentHash)>,
    encoded: CanonicalBytes,
) {
    resources.insert(encoded.path, (encoded.bytes, encoded.hash));
}

pub struct LockedProject {
    _parent: TestDir,
    pub path: PathBuf,
    pub root: NativeProjectRoot,
    lease: Option<ProjectLockLease>,
    pub project: ProjectRootCapability,
}

impl LockedProject {
    pub fn new(label: &str) -> Self {
        let parent = TestDir::new(label);
        let path = parent.join("novel");
        let (root, lease) = NativeProjectFileSystem::new()
            .create_root(UntrustedProjectPath::new(path.clone()))
            .expect("native project root should be created and locked");
        Self {
            _parent: parent,
            path,
            root,
            lease: Some(lease),
            project: ProjectRootCapability::new(NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)),
        }
    }

    pub fn store(&self) -> Git2HistoryStore {
        Git2HistoryStore::new(self.root.clone())
    }

    pub fn initialize(&self) -> Git2HistoryStore {
        let store = self.store();
        store
            .initialize(self.project.clone())
            .expect("History should initialize");
        store
    }

    pub fn write(&self, version: &ProjectVersion) {
        write_resources(&self.path, &version.bytes(), false);
    }

    pub fn write_crlf(&self, version: &ProjectVersion) {
        write_resources(&self.path, &version.bytes(), true);
    }

    pub fn read(&self, path: &CanonicalRelativePath) -> Vec<u8> {
        fs::read(self.path.join(path.as_str())).expect("canonical project file should be readable")
    }

    pub fn release_lock(&mut self) {
        drop(self.lease.take());
    }

    pub fn reacquire(&mut self) {
        let (root, lease) = NativeProjectFileSystem::new()
            .acquire(UntrustedProjectPath::new(self.path.clone()))
            .expect("project lock should be reacquired");
        self.root = root;
        self.lease = Some(lease);
    }
}

fn write_resources(root: &Path, resources: &BTreeMap<CanonicalRelativePath, Vec<u8>>, crlf: bool) {
    for (path, canonical_bytes) in resources {
        let target = root.join(path.as_str());
        fs::create_dir_all(target.parent().expect("resource should have a parent"))
            .expect("resource parent should be created");
        let bytes = if crlf && path.as_str() != ".parchmint/format-version" {
            String::from_utf8(canonical_bytes.clone())
                .expect("line-ending fixture should be UTF-8")
                .replace('\n', "\r\n")
                .into_bytes()
        } else {
            canonical_bytes.clone()
        };
        fs::write(target, bytes).expect("canonical fixture should be written");
    }
}

pub fn checkpoint(
    store: &Git2HistoryStore,
    intent: u8,
    version: &ProjectVersion,
    category: CheckpointCategory,
) -> Result<parchmint_history_api::CheckpointId, HistoryError> {
    store.checkpoint(CheckpointInput {
        intent_hash: CheckpointIntentHash::from_bytes([intent; 32]),
        resources: version.hashes(),
        category,
        affected_documents: vec![TEST_DOCUMENT],
        name: None,
    })
}

pub fn named_checkpoint(
    store: &Git2HistoryStore,
    intent: u8,
    version: &ProjectVersion,
    name: &str,
) -> Result<parchmint_history_api::CheckpointId, HistoryError> {
    store.checkpoint(CheckpointInput {
        intent_hash: CheckpointIntentHash::from_bytes([intent; 32]),
        resources: version.hashes(),
        category: CheckpointCategory::NamedSnapshot,
        affected_documents: Vec::new(),
        name: Some(SnapshotName::new(name).expect("snapshot name should be valid")),
    })
}
