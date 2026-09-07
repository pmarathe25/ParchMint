//! Custom virtualized Iced adapter for ParchMint editor-core sessions.

mod adapter;
mod iced_surface;
mod layout;
mod mounted_binding;
mod text_context;

pub use adapter::{
    BlockRelayout, EditorFrameReport, EditorIcedAdapter, EditorIcedConfig, EditorResourceLimits,
    EditorStartupError, MountedViewPresentation, MountedViewSnapshot, ProjectionBudget,
};
pub use iced_surface::{
    EditorSurfaceColor, EditorSurfaceTheme, MountedEditorClipboardIntent, MountedEditorConfig,
    MountedEditorHost, MountedEditorKeyCommand, MountedEditorMessage, MountedEditorUpdate,
};
pub use layout::{
    BlockLayoutGeometry, EditorFontFamily, EditorLayoutMetrics, EditorRectangle,
    EditorScalarGeometry, EditorViewport, VisibleEditorBlock,
};
pub use mounted_binding::{MountedEditorBinding, MountedEditorBindingConfig, MountedEditorSession};

pub use text_context::EditorTextContext;
