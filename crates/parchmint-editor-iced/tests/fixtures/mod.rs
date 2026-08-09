mod editor_iced;
mod editor_save_recovery;

pub use editor_iced::{adapter_with_cache_limit, block, mount, open, view, visible};
pub use editor_save_recovery::{
    Boundary, EditorSaveRecoveryHarness, PersistenceFailure, document_id, durable_vector,
    recovered_body,
};
