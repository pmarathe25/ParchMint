use parchmint_editor_api::{
    BlockId, CanonicalDocumentLoad, DocumentId, DocumentPosition, EditorAdapter,
    SharedEditorSession, ViewId,
};
use parchmint_editor_iced::{
    EditorIcedAdapter, EditorIcedConfig, EditorResourceLimits, VisibleEditorBlock,
};
use parchmint_platform_api::WindowCapability;

pub fn adapter_with_cache_limit(limit: usize) -> EditorIcedAdapter {
    EditorIcedAdapter::new(EditorIcedConfig {
        resource_limits: EditorResourceLimits {
            max_visible_blocks_per_view: limit,
            ..EditorResourceLimits::default()
        },
        ..EditorIcedConfig::default()
    })
    .expect("adapter config")
}

pub fn mount(adapter: &EditorIcedAdapter, session: SharedEditorSession, view: ViewId) {
    let host = adapter
        .create_view_host(WindowCapability::new(1, 1), view)
        .expect("create host capability");
    adapter
        .attach_view(session, view, host)
        .expect("mount view");
}

pub fn open(adapter: &EditorIcedAdapter, body: &str) -> SharedEditorSession {
    adapter
        .open_session(CanonicalDocumentLoad::new(document(9), body))
        .expect("open editor session")
}

pub fn block(value: u8) -> BlockId {
    BlockId::from_bytes([value; 16])
}

pub fn view(value: u8) -> ViewId {
    ViewId::from_bytes([value; 16])
}

pub fn visible(block: BlockId, text: &str) -> VisibleEditorBlock {
    VisibleEditorBlock::new(block, text, DocumentPosition::default())
}

fn document(value: u8) -> DocumentId {
    DocumentId::from_bytes([value; 16])
}
