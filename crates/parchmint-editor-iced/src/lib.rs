//! Custom virtualized Iced adapter for ParchMint editor-core sessions.

mod adapter;
#[allow(
    dead_code,
    reason = "the private renderer surface is exercised through the adapter bridge"
)]
mod iced_surface;
mod layout;

pub use adapter::{
    BlockRelayout, EditorFrameReport, EditorIcedAdapter, EditorIcedConfig, EditorResourceLimits,
    EditorStartupError, MountedViewPresentation, MountedViewSnapshot, ProjectionBudget,
};
pub use layout::{
    BlockLayoutGeometry, EditorLayoutMetrics, EditorRectangle, EditorScalarGeometry,
    EditorViewport, VisibleEditorBlock,
};
