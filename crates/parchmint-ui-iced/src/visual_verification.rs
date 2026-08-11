//! Stable production-composition inputs for external visual verification.
//!
//! The catalog mirrors the checked-in Penpot baseline fixture IDs. It never
//! renders legacy fixture-only Iced surfaces.

use std::path::PathBuf;

#[cfg(feature = "visual-verification")]
use std::borrow::Cow;

/// One checked-in Penpot baseline fixture that can be rendered headlessly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualTarget {
    Launcher,
    EditorSingle,
    EditorDual,
    Cards,
    GlobalSearch,
    History,
    SettingsAppearance,
    Export,
    ErrorRecovery,
    RecentlyDeleted,
}

impl VisualTarget {
    pub const ALL: [Self; 10] = [
        Self::Launcher,
        Self::EditorSingle,
        Self::EditorDual,
        Self::Cards,
        Self::GlobalSearch,
        Self::History,
        Self::SettingsAppearance,
        Self::Export,
        Self::ErrorRecovery,
        Self::RecentlyDeleted,
    ];

    /// The fixture ID recorded by the Penpot reference manifest.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Launcher => "launcher-default",
            Self::EditorSingle => "editor-single-default",
            Self::EditorDual => "editor-dual-default",
            Self::Cards => "cards-default",
            Self::GlobalSearch => "global-search-default",
            Self::History => "history-default",
            Self::SettingsAppearance => "settings-appearance-default",
            Self::Export => "export-default",
            Self::ErrorRecovery => "error-recovery-default",
            Self::RecentlyDeleted => "recently-deleted-default",
        }
    }

    /// The stable basename of the checked-in Penpot PNG for one appearance.
    pub const fn reference_id(self, appearance: VisualAppearance) -> &'static str {
        match (self, appearance) {
            (Self::Launcher, VisualAppearance::Light) => "launcher-light",
            (Self::Launcher, VisualAppearance::Dark) => "launcher-dark",
            (Self::EditorSingle, VisualAppearance::Light) => "editor-single-light",
            (Self::EditorSingle, VisualAppearance::Dark) => "editor-single-dark",
            (Self::EditorDual, VisualAppearance::Light) => "editor-dual-light",
            (Self::EditorDual, VisualAppearance::Dark) => "editor-dual-dark",
            (Self::Cards, VisualAppearance::Light) => "cards-light",
            (Self::Cards, VisualAppearance::Dark) => "cards-dark",
            (Self::GlobalSearch, VisualAppearance::Light) => "global-search-light",
            (Self::GlobalSearch, VisualAppearance::Dark) => "global-search-dark",
            (Self::History, VisualAppearance::Light) => "history-light",
            (Self::History, VisualAppearance::Dark) => "history-dark",
            (Self::SettingsAppearance, VisualAppearance::Light) => "settings-appearance-light",
            (Self::SettingsAppearance, VisualAppearance::Dark) => "settings-appearance-dark",
            (Self::Export, VisualAppearance::Light) => "export-project-output-controls-light",
            (Self::Export, VisualAppearance::Dark) => "export-project-output-controls-dark",
            (Self::ErrorRecovery, VisualAppearance::Light) => "error-recovery-light",
            (Self::ErrorRecovery, VisualAppearance::Dark) => "error-recovery-dark",
            (Self::RecentlyDeleted, VisualAppearance::Light) => "recently-deleted-light",
            (Self::RecentlyDeleted, VisualAppearance::Dark) => "recently-deleted-dark",
        }
    }
}

/// Appearance selected by the external capture tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualAppearance {
    Light,
    Dark,
}

impl VisualAppearance {
    pub const ALL: [Self; 2] = [Self::Light, Self::Dark];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// Catalog viewport dimensions and scale for a production target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualTargetSpec {
    pub target: VisualTarget,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub production_native: bool,
}

impl VisualTargetSpec {
    pub const fn physical_size(self) -> (u32, u32) {
        (self.width * self.scale, self.height * self.scale)
    }
}

const fn spec(target: VisualTarget) -> VisualTargetSpec {
    VisualTargetSpec {
        target,
        width: 1440,
        height: 900,
        scale: 2,
        production_native: true,
    }
}

pub const VISUAL_TARGET_SPECS: &[VisualTargetSpec] = &[
    spec(VisualTarget::Launcher),
    spec(VisualTarget::EditorSingle),
    spec(VisualTarget::EditorDual),
    spec(VisualTarget::Cards),
    spec(VisualTarget::GlobalSearch),
    spec(VisualTarget::History),
    spec(VisualTarget::SettingsAppearance),
    spec(VisualTarget::Export),
    spec(VisualTarget::ErrorRecovery),
    spec(VisualTarget::RecentlyDeleted),
];

pub const fn visual_target_spec(target: VisualTarget) -> VisualTargetSpec {
    spec(target)
}

/// Result metadata from a newly written capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualCapture {
    pub target: VisualTarget,
    pub appearance: VisualAppearance,
    pub renderer: &'static str,
    pub logical_size: (u32, u32),
    pub physical_size: (u32, u32),
    pub output_path: PathBuf,
}

/// Capture failure without exposing the headless renderer's error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualCaptureError(pub String);

impl std::fmt::Display for VisualCaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for VisualCaptureError {}

/// Captures one production-composition target to a new PNG.
///
/// `output_stem` is a stem path. Existing renderer output is rejected; the
/// verification crate owns comparison reports and diffs.
#[cfg(feature = "visual-verification")]
pub fn capture_visual(
    target: VisualTarget,
    appearance: VisualAppearance,
    output_stem: impl AsRef<std::path::Path>,
) -> Result<VisualCapture, VisualCaptureError> {
    use iced::Size;
    use iced_test::Simulator;

    let output_stem = output_stem.as_ref();
    let output_path = renderer_output_path(output_stem);
    if output_path.exists() {
        return Err(VisualCaptureError(format!(
            "capture output already exists: {}",
            output_path.display()
        )));
    }
    let spec = visual_target_spec(target);
    let presentation = presentation(appearance);
    let mut simulator = Simulator::<()>::with_size(
        visual_settings(),
        Size::new(spec.width as f32, spec.height as f32),
        production_element(target, appearance),
    );
    let snapshot = simulator
        .snapshot(&presentation.iced_theme())
        .map_err(|error| VisualCaptureError(error.to_string()))?;
    snapshot
        .matches_image(output_stem)
        .map_err(|error| VisualCaptureError(error.to_string()))?;
    Ok(VisualCapture {
        target,
        appearance,
        renderer: "tiny-skia",
        logical_size: (spec.width, spec.height),
        physical_size: spec.physical_size(),
        output_path,
    })
}

#[cfg(feature = "visual-verification")]
fn visual_settings() -> iced::Settings {
    iced::Settings {
        default_font: iced::Font::with_name("Source Sans 3"),
        fonts: vec![
            Cow::Borrowed(include_bytes!(
                "../assets/fonts/source-sans-3/SourceSans3-Regular.ttf"
            )),
            Cow::Borrowed(include_bytes!(
                "../assets/fonts/source-sans-3/SourceSans3-Medium.ttf"
            )),
            Cow::Borrowed(include_bytes!(
                "../assets/fonts/source-sans-3/SourceSans3-Semibold.ttf"
            )),
            Cow::Borrowed(include_bytes!(
                "../assets/fonts/source-sans-3/SourceSans3-Bold.ttf"
            )),
            Cow::Borrowed(include_bytes!(
                "../assets/fonts/source-serif-4/SourceSerif4-Regular.ttf"
            )),
        ],
        ..iced::Settings::default()
    }
}

#[cfg(feature = "visual-verification")]
fn renderer_output_path(output_stem: &std::path::Path) -> PathBuf {
    let output_name = output_stem
        .file_stem()
        .map(std::ffi::OsStr::to_string_lossy)
        .unwrap_or_default();
    output_stem
        .with_file_name(format!("{output_name}-tiny-skia"))
        .with_extension("png")
}

#[cfg(feature = "visual-verification")]
fn presentation(appearance: VisualAppearance) -> crate::design_tokens::ParchMintTheme {
    use parchmint_preferences::ResolvedAppearance;

    crate::design_tokens::ParchMintTheme::new(match appearance {
        VisualAppearance::Light => ResolvedAppearance::Light,
        VisualAppearance::Dark => ResolvedAppearance::Dark,
    })
}

#[cfg(feature = "visual-verification")]
fn production_element(
    target: VisualTarget,
    appearance: VisualAppearance,
) -> iced::Element<'static, ()> {
    use crate::{
        ProjectWorkspace, RibbonDestination,
        iced_editor_surface::{EditorCenterChrome, editor_center_surface_with_chrome},
        iced_project_surface::{ProjectSurfaceMessage, verification_project_surface},
    };

    if target == VisualTarget::Launcher {
        // Keep launcher access isolated behind native's verification seam.
        return crate::native::NativeDesktop::verification_launcher_element();
    }

    let snapshot = verification_snapshot(target);
    let workspace = verification_workspace(target, appearance, &snapshot);
    let workspace: &'static ProjectWorkspace = Box::leak(Box::new(workspace));
    let slots = editor_slots(&snapshot, workspace, target, appearance);
    let theme = presentation(appearance);
    let chrome = if target == VisualTarget::GlobalSearch {
        EditorCenterChrome::ManuscriptOnly
    } else {
        EditorCenterChrome::Full
    };
    let editor = editor_center_surface_with_chrome(workspace.editor(), theme, &slots, None, chrome)
        .map(ProjectSurfaceMessage::EditorCenter);
    let destination = match target {
        VisualTarget::EditorSingle | VisualTarget::EditorDual | VisualTarget::ErrorRecovery => {
            RibbonDestination::Editor
        }
        VisualTarget::Cards => RibbonDestination::Cards,
        VisualTarget::GlobalSearch => RibbonDestination::GlobalSearch,
        VisualTarget::History => RibbonDestination::History,
        VisualTarget::SettingsAppearance => RibbonDestination::Settings,
        VisualTarget::Export => RibbonDestination::Export,
        VisualTarget::RecentlyDeleted => RibbonDestination::RecentlyDeleted,
        VisualTarget::Launcher => unreachable!("launcher returned above"),
    };
    verification_project_surface(workspace, destination, theme, editor).map(|_| ())
}

#[cfg(feature = "visual-verification")]
fn editor_slots(
    snapshot: &parchmint_ui_api::ProjectSnapshot,
    workspace: &crate::ProjectWorkspace,
    target: VisualTarget,
    appearance: VisualAppearance,
) -> crate::iced_editor_surface::EditorHostSlots {
    use crate::{
        EditorPane,
        iced_editor_surface::{EditorHostSlots, EditorPaneSlot},
    };

    let mut slots = EditorHostSlots::default();
    if !matches!(
        target,
        VisualTarget::EditorSingle | VisualTarget::EditorDual | VisualTarget::GlobalSearch
    ) {
        return slots;
    }
    for pane in [EditorPane::Primary, EditorPane::Companion] {
        let Some(document_id) = workspace.editor().pane(pane).active_document() else {
            continue;
        };
        let document = snapshot
            .documents
            .iter()
            .find(|document| stable_id(document.document_id.as_bytes()) == document_id)
            .expect("mounted production tab has a snapshot document");
        let adapter = parchmint_editor_iced::EditorIcedAdapter::new(
            parchmint_editor_iced::EditorIcedConfig::default(),
        )
        .expect("verification adapter starts");
        let viewport = verification_editor_viewport(target, pane, workspace);
        let theme = match appearance {
            VisualAppearance::Light => parchmint_editor_iced::EditorSurfaceTheme::light(),
            VisualAppearance::Dark => parchmint_editor_iced::EditorSurfaceTheme::dark(),
        };
        let binding = parchmint_editor_iced::MountedEditorBinding::mount(
            &adapter,
            parchmint_editor_iced::MountedEditorBindingConfig::new(
                parchmint_editor_iced::MountedEditorSession::Open(
                    crate::project_runtime::canonical_load(snapshot, document.document_id)
                        .expect("verification snapshot is canonical"),
                ),
                parchmint_platform_api::WindowCapability::new(900, 1),
                workspace.editor().pane(pane).view(),
                viewport,
                theme,
            ),
        )
        .expect("verification editor host mounts");
        slots.insert(pane, EditorPaneSlot::mounted(binding.host().clone()));
    }
    slots
}

/// Returns the initial mounted-host allocation for a 1440 x 900 Penpot target.
///
/// A native window subsequently reflows the host through the viewport sensor,
/// but a headless capture has no application update loop to consume that
/// sensor message before its first frame. The initial geometry must therefore
/// be the real pane allocation, not a generic editor-sized fallback.
#[cfg(feature = "visual-verification")]
fn verification_editor_viewport(
    target: VisualTarget,
    pane: crate::EditorPane,
    workspace: &crate::ProjectWorkspace,
) -> parchmint_editor_iced::EditorViewport {
    const EDITOR_SPLITTER_WIDTH: u32 = 8;
    const FORMAT_TOOLBAR_HEIGHT: u32 = 44;
    const TAB_STRIP_HEIGHT: u32 = 36;

    let (width, height) = match target {
        VisualTarget::EditorSingle => {
            let center = crate::iced_project_surface::verification_center_geometry(
                crate::RibbonDestination::Editor,
            );
            (
                center.width,
                center.height - FORMAT_TOOLBAR_HEIGHT - TAB_STRIP_HEIGHT,
            )
        }
        VisualTarget::EditorDual => {
            let center = crate::iced_project_surface::verification_center_geometry(
                crate::RibbonDestination::Editor,
            );
            let available_width = center.width - EDITOR_SPLITTER_WIDTH;
            let portion = match pane {
                crate::EditorPane::Primary => workspace.editor().split_ratio(),
                crate::EditorPane::Companion => 1.0 - workspace.editor().split_ratio(),
            };
            (
                (available_width as f64 * portion).round() as u32,
                center.height - FORMAT_TOOLBAR_HEIGHT - TAB_STRIP_HEIGHT,
            )
        }
        VisualTarget::GlobalSearch => {
            let center = crate::iced_project_surface::verification_center_geometry(
                crate::RibbonDestination::GlobalSearch,
            );
            (center.width, center.height)
        }
        _ => unreachable!("only mounted editor targets request a viewport"),
    };
    parchmint_editor_iced::EditorViewport::new(width as f32, height as f32)
        .expect("verification pane allocation is valid")
}

#[cfg(feature = "visual-verification")]
fn verification_snapshot(target: VisualTarget) -> parchmint_ui_api::ProjectSnapshot {
    use parchmint_application::{DocumentSnapshot, DocumentVisibility};
    use parchmint_domain::{
        DocumentId, MetadataApplicability, MetadataFieldDefinition, MetadataFieldId,
        MetadataTextKind, NodeId, Project, ProjectCommand, ProjectId, apply_project_command,
    };
    use parchmint_editor_api::EditorRevision;

    let part_one = NodeId::from_bytes([0x13; 16]);
    let part_two = NodeId::from_bytes([0x23; 16]);
    let chapter_one_node = NodeId::from_bytes([0x11; 16]);
    let chapter_two_node = NodeId::from_bytes([0x21; 16]);
    let harbor_notes_node = NodeId::from_bytes([0x31; 16]);
    let map_node = NodeId::from_bytes([0x41; 16]);
    let old_chapter_node = NodeId::from_bytes([0x51; 16]);
    let draft_scenes_node = NodeId::from_bytes([0x61; 16]);
    let chapter_one = DocumentId::from_bytes([0x12; 16]);
    let chapter_two = DocumentId::from_bytes([0x22; 16]);
    let harbor_notes = DocumentId::from_bytes([0x32; 16]);
    let map = DocumentId::from_bytes([0x42; 16]);
    let old_chapter = DocumentId::from_bytes([0x52; 16]);
    let point_of_view = MetadataFieldId::from_bytes([0x71; 16]);
    let target_words = MetadataFieldId::from_bytes([0x72; 16]);
    let words = MetadataFieldId::from_bytes([0x73; 16]);
    let status = MetadataFieldId::from_bytes([0x74; 16]);
    let mut project = Project::new(ProjectId::from_bytes([0x10; 16]));
    project.display_title = "The Glass Harbor".to_owned();
    project.author = Some("Mara Venn".to_owned());
    project
        .nodes
        .try_insert_group(part_one, NodeId::manuscript_root(), 0, "Part One")
        .expect("verification group is valid");
    project
        .nodes
        .try_insert_document(chapter_one_node, chapter_one, part_one, 0, "Chapter One")
        .expect("verification document is valid");
    project
        .nodes
        .try_insert_document(chapter_two_node, chapter_two, part_one, 1, "Chapter Two")
        .expect("verification second document is valid");
    project
        .nodes
        .try_insert_group(part_two, NodeId::manuscript_root(), 1, "Part Two")
        .expect("verification second group is valid");
    project
        .nodes
        .try_insert_document(map_node, map, part_two, 0, "Map of the Coast")
        .expect("verification map is valid");
    project
        .nodes
        .try_insert_document(
            harbor_notes_node,
            harbor_notes,
            NodeId::research_root(),
            0,
            "Harbor Notes",
        )
        .expect("verification research document is valid");
    project
        .nodes
        .try_insert_document(old_chapter_node, old_chapter, part_one, 2, "Old Chapter")
        .expect("verification deleted chapter is valid");
    project
        .nodes
        .try_insert_group(draft_scenes_node, part_one, 3, "Draft Scenes")
        .expect("verification deleted group is valid");
    for field in [
        MetadataFieldDefinition {
            id: point_of_view,
            label: "POV".to_owned(),
            description: Some("Narrative perspective".to_owned()),
            applicability: MetadataApplicability::GroupsAndDocuments,
            text_kind: MetadataTextKind::SingleLine,
            default_value: None,
            visible_on_cards: true,
        },
        MetadataFieldDefinition {
            id: target_words,
            label: "Target".to_owned(),
            description: None,
            applicability: MetadataApplicability::Groups,
            text_kind: MetadataTextKind::SingleLine,
            default_value: None,
            visible_on_cards: true,
        },
        MetadataFieldDefinition {
            id: words,
            label: "Words".to_owned(),
            description: None,
            applicability: MetadataApplicability::Documents,
            text_kind: MetadataTextKind::SingleLine,
            default_value: None,
            visible_on_cards: true,
        },
        MetadataFieldDefinition {
            id: status,
            label: "Status".to_owned(),
            description: None,
            applicability: MetadataApplicability::Documents,
            text_kind: MetadataTextKind::SingleLine,
            default_value: None,
            visible_on_cards: false,
        },
    ] {
        project = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::upsert_metadata_field(field),
        )
        .expect("verification metadata is valid")
        .project;
    }
    for command in [
        ProjectCommand::set_synopsis(
            chapter_one_node,
            "The harbor has fallen silent, and Mara must decide whom to trust.",
        ),
        ProjectCommand::set_synopsis(part_one, "The opening movement of the novel."),
        ProjectCommand::set_synopsis(
            chapter_two_node,
            "The sealed letter changes what Mara believes.",
        ),
        ProjectCommand::set_synopsis(part_two, "The fallout after the harbor reveal."),
        ProjectCommand::set_metadata_value(
            chapter_one_node,
            point_of_view,
            Some("Mara".to_owned()),
        ),
        ProjectCommand::set_metadata_value(
            chapter_two_node,
            point_of_view,
            Some("Mara".to_owned()),
        ),
        ProjectCommand::set_metadata_value(part_one, point_of_view, Some("Multiple".to_owned())),
        ProjectCommand::set_metadata_value(part_two, point_of_view, Some("Multiple".to_owned())),
        ProjectCommand::set_metadata_value(part_one, target_words, Some("3,500 words".to_owned())),
        ProjectCommand::set_metadata_value(part_two, target_words, Some("3,200 words".to_owned())),
        ProjectCommand::set_metadata_value(chapter_one_node, words, Some("1,240".to_owned())),
        ProjectCommand::set_metadata_value(chapter_two_node, words, Some("1,080".to_owned())),
        ProjectCommand::set_metadata_value(chapter_one_node, status, Some("Draft".to_owned())),
        ProjectCommand::delete_node_at(old_chapter_node, 1_725_000_000_000),
        ProjectCommand::delete_node_at(draft_scenes_node, 1_725_000_001_000),
    ] {
        project = apply_project_command(&project, project.revision, command)
            .expect("verification project command is valid")
            .project;
    }
    let chapter_one_body = match target {
        VisualTarget::GlobalSearch => {
            "<h1>Chapter One</h1><p>The harbor held the last of the evening light.</p><p>Active match is outlined; other matches remain highlighted.</p><p>Search results are revalidated before navigation.</p>".to_owned()
        }
        VisualTarget::History => {
            "<p>Chapter One</p><p>The harbor held the last of the evening light.</p><p>Mara turned the sealed letter in her fingers.</p><p>By morning, the tide had erased every footprint.</p>".to_owned()
        }
        _ => "<h1>Chapter One</h1><p>The harbor held the last of the evening light. Mara waited beneath the clock tower, turning the unopened letter between her fingers.</p><blockquote>“Some journeys begin long before the road.”</blockquote><hr><p>By morning, the tide had erased every footprint.</p>".to_owned(),
    };

    parchmint_ui_api::ProjectSnapshot {
        project,
        document_summaries: Vec::new(),
        documents: vec![
            DocumentSnapshot {
                document_id: chapter_one,
                body: chapter_one_body,
                comments: Vec::new(),
                revision: EditorRevision::from(1),
                visibility: DocumentVisibility::Open,
            },
            DocumentSnapshot {
                document_id: chapter_two,
                body: "<h1>Chapter Two</h1><p>Rain found the city before dawn.</p><p>Another manuscript document opens on the right.</p>".to_owned(),
                comments: Vec::new(),
                revision: EditorRevision::from(1),
                visibility: DocumentVisibility::Closed,
            },
            DocumentSnapshot {
                document_id: harbor_notes,
                body: "<h1>Harbor Notes</h1><p>A harbor road beneath the cliffs.</p><p>Every harbor keeps its own weather.</p>".to_owned(),
                comments: Vec::new(),
                revision: EditorRevision::from(1),
                visibility: DocumentVisibility::Closed,
            },
            DocumentSnapshot {
                document_id: map,
                body: "<h1>Map of the Coast</h1><p>The coast road follows the harbor cliffs.</p>".to_owned(),
                comments: Vec::new(), revision: EditorRevision::from(1), visibility: DocumentVisibility::Closed,
            },
            DocumentSnapshot {
                document_id: old_chapter,
                body: "<h1>Old Chapter</h1><p>The harbor held the last of the evening light.</p><blockquote>“Some journeys begin long before the road.”</blockquote><p>Mara turned the <strong>unopened letter</strong> in her fingers.</p>".to_owned(),
                comments: Vec::new(), revision: EditorRevision::from(1), visibility: DocumentVisibility::Closed,
            },
        ],
        styles_css: String::new(),
    }
}

#[cfg(feature = "visual-verification")]
fn verification_node_id() -> String {
    "11".repeat(16)
}

#[cfg(feature = "visual-verification")]
fn verification_chapter_two_node_id() -> String {
    "21".repeat(16)
}

#[cfg(feature = "visual-verification")]
fn verification_part_one_node_id() -> String {
    "13".repeat(16)
}

#[cfg(feature = "visual-verification")]
fn verification_harbor_notes_node_id() -> String {
    "31".repeat(16)
}

#[cfg(feature = "visual-verification")]
fn verification_map_node_id() -> String {
    "41".repeat(16)
}

#[cfg(feature = "visual-verification")]
fn verification_deleted_node_id() -> String {
    "51".repeat(16)
}

#[cfg(feature = "visual-verification")]
fn stable_id(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(feature = "visual-verification")]
fn verification_semantic_body(
    document_id: parchmint_domain::DocumentId,
    body: String,
) -> parchmint_editor_api::SemanticDocument {
    parchmint_editor_core::EditorCoreSession::open(
        parchmint_editor_core::CanonicalDocumentLoad::new(document_id, body),
    )
    .expect("verification document is canonical")
    .canonical_projection()
    .semantic()
    .clone()
}

/// Builds the selected History document exactly as native integration does:
/// from the active document in the loaded project snapshot.
#[cfg(feature = "visual-verification")]
fn verification_history_current_document(
    snapshot: &parchmint_ui_api::ProjectSnapshot,
    workspace: &crate::ProjectWorkspace,
) -> Option<crate::HistoryCurrentDocument> {
    let document_id = workspace.focused_history_document()?.to_owned();
    let document = snapshot
        .documents
        .iter()
        .find(|document| stable_id(document.document_id.as_bytes()) == document_id)?;
    let title = snapshot
        .project
        .nodes
        .iter()
        .find_map(|(_, node)| match node.kind {
            parchmint_domain::NodeKind::Document(candidate)
                if candidate == document.document_id =>
            {
                Some(node.title.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| "Active document".to_owned());
    Some(crate::HistoryCurrentDocument {
        document_id,
        title,
        body: document.body.clone(),
        semantic: verification_semantic_body(document.document_id, document.body.clone()),
    })
}

/// Hydrates every catalog board through the public workspace reducer and task
/// completion boundary. The deterministic payloads model service responses;
/// rendering consumes only the resulting production workspace state.
#[cfg(feature = "visual-verification")]
fn verification_workspace(
    target: VisualTarget,
    appearance: VisualAppearance,
    snapshot: &parchmint_ui_api::ProjectSnapshot,
) -> crate::ProjectWorkspace {
    use crate::{
        ContentState, DragDestination, GlobalSearchResult, HistoryCheckpointCategory,
        HistoryCheckpointRow, HistoryDocumentPreview, HistoryPreviewData, ProjectMessage,
        ProjectTask, ProjectTaskCompletion, ProjectTaskPayload, SelectionGesture, SettingsCategory,
    };
    use parchmint_domain::ProjectExportSetting;

    let chapter_one = verification_node_id();
    let part_one = verification_part_one_node_id();
    let chapter_two = verification_chapter_two_node_id();
    let harbor_notes = verification_harbor_notes_node_id();
    let map = verification_map_node_id();
    let deleted = verification_deleted_node_id();
    let chapter_one_document = stable_id(snapshot.documents[0].document_id.as_bytes());
    let chapter_two_document = stable_id(snapshot.documents[1].document_id.as_bytes());
    let harbor_notes_document = stable_id(snapshot.documents[2].document_id.as_bytes());
    let mut workspace = crate::ProjectWorkspace::from_snapshot(snapshot);
    let _ = workspace.update(ProjectMessage::SelectHierarchy {
        node_id: chapter_one.clone(),
        gesture: SelectionGesture::Replace,
    });

    match target {
        VisualTarget::Launcher => unreachable!("launcher has its own native state"),
        VisualTarget::EditorSingle => {
            let _ = workspace.update(ProjectMessage::ToggleHierarchyExpanded(part_one.clone()));
            let _ = workspace.update(ProjectMessage::OpenHierarchyNode(chapter_two.clone()));
            let _ = workspace.update(ProjectMessage::DropHierarchy {
                source_id: harbor_notes,
                destination: DragDestination::EditorPane(crate::EditorPane::Primary),
            });
            let _ = workspace.update(ProjectMessage::OpenHierarchyNode(map.clone()));
            let _ = workspace.update(ProjectMessage::OpenHierarchyNode(chapter_one.clone()));
        }
        VisualTarget::EditorDual => {
            let _ = workspace.update(ProjectMessage::ToggleHierarchyExpanded(part_one.clone()));
            for node_id in [chapter_two.clone(), map.clone(), chapter_one.clone()] {
                let _ = workspace.update(ProjectMessage::OpenHierarchyNode(node_id));
            }
            for node_id in [
                chapter_one.clone(),
                chapter_two.clone(),
                map.clone(),
                chapter_two.clone(),
            ] {
                let _ = workspace.update(ProjectMessage::OpenHierarchyNodeInCompanion(node_id));
            }
            let _ = workspace.update(ProjectMessage::OpenHierarchyNode(chapter_one.clone()));
            let _ = workspace
                .editor_mut()
                .update(crate::EditorMessage::FocusPane(crate::EditorPane::Primary));
        }
        VisualTarget::Cards => {
            let _ = workspace.update(ProjectMessage::SetCardsSection(stable_id(
                parchmint_domain::NodeId::manuscript_root().as_bytes(),
            )));
            let _ = workspace.update(ProjectMessage::ToggleHierarchyExpanded(part_one));
            let _ = workspace.update(ProjectMessage::ActivateCard(chapter_one));
        }
        VisualTarget::GlobalSearch => {
            let _ = workspace.update(ProjectMessage::OpenHierarchyNode(chapter_one.clone()));
            let _ = workspace.update(ProjectMessage::ShowGlobalSearch);
            let _ = workspace.update(ProjectMessage::SetGlobalSearchQuery("harbor".to_owned()));
            let ticket = workspace.begin_task(ProjectTask::GlobalSearch {
                generation: workspace.global_search().query_generation(),
            });
            assert!(
                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                    ticket,
                    ProjectTaskPayload::SearchBatch {
                        results: vec![
                            GlobalSearchResult {
                                document_id: chapter_one_document.clone(),
                                match_id: "chapter-one-harbor-1".to_owned(),
                                prefix: "...the ".to_owned(),
                                matching_text: "harbor".to_owned(),
                                suffix: " held the last of the evening light.".to_owned(),
                                indexed_revision: 1,
                            },
                            GlobalSearchResult {
                                document_id: chapter_one_document.clone(),
                                match_id: "chapter-one-harbor-2".to_owned(),
                                prefix: "...a ".to_owned(),
                                matching_text: "harbor".to_owned(),
                                suffix: " road beneath the cliffs.".to_owned(),
                                indexed_revision: 1,
                            },
                            GlobalSearchResult {
                                document_id: harbor_notes_document.clone(),
                                match_id: "harbor-notes-1".to_owned(),
                                prefix: "...the ".to_owned(),
                                matching_text: "harbor".to_owned(),
                                suffix: " held the last of the evening light.".to_owned(),
                                indexed_revision: 1,
                            },
                            GlobalSearchResult {
                                document_id: harbor_notes_document.clone(),
                                match_id: "harbor-notes-2".to_owned(),
                                prefix: "...a ".to_owned(),
                                matching_text: "harbor".to_owned(),
                                suffix: " road beneath the cliffs.".to_owned(),
                                indexed_revision: 1,
                            },
                            GlobalSearchResult {
                                document_id: harbor_notes_document.clone(),
                                match_id: "harbor-notes-3".to_owned(),
                                prefix: "The old ".to_owned(),
                                matching_text: "harbor".to_owned(),
                                suffix: " maps were water-stained.".to_owned(),
                                indexed_revision: 1,
                            },
                            GlobalSearchResult {
                                document_id: harbor_notes_document.clone(),
                                match_id: "harbor-notes-4".to_owned(),
                                prefix: "At dawn, the ".to_owned(),
                                matching_text: "harbor".to_owned(),
                                suffix: " was quiet again.".to_owned(),
                                indexed_revision: 1,
                            },
                        ],
                        finished: true,
                    },
                ))
            );
        }
        VisualTarget::History => {
            let history_checkpoint_body = "<p>Chapter One</p><p>The harbor held the last of the evening light.</p><p>Mara turned the unopened letter in her fingers.</p><p>By morning, the tide had erased every footprint.</p>".to_owned();
            let checkpoints = vec![
                HistoryCheckpointRow {
                    checkpoint_id: "autosave-chapter-one".to_owned(),
                    sequence: 4,
                    category: HistoryCheckpointCategory::Autosave,
                    affected_document_ids: vec![chapter_one_document],
                    name: Some("Chapter One".to_owned()),
                },
                HistoryCheckpointRow {
                    checkpoint_id: "before-revisions".to_owned(),
                    sequence: 3,
                    category: HistoryCheckpointCategory::NamedSnapshot,
                    affected_document_ids: Vec::new(),
                    name: Some("Before revisions".to_owned()),
                },
                HistoryCheckpointRow {
                    checkpoint_id: "moved-chapter-two".to_owned(),
                    sequence: 2,
                    category: HistoryCheckpointCategory::StructuralChange,
                    affected_document_ids: vec![chapter_two_document],
                    name: Some("Moved Chapter Two".to_owned()),
                },
                HistoryCheckpointRow {
                    checkpoint_id: "saved-chapter-one".to_owned(),
                    sequence: 1,
                    category: HistoryCheckpointCategory::ExplicitSave,
                    affected_document_ids: vec![stable_id(
                        snapshot.documents[0].document_id.as_bytes(),
                    )],
                    name: Some("Chapter One".to_owned()),
                },
            ];
            let ticket = workspace.begin_task(ProjectTask::LoadHistory);
            assert!(
                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                    ticket,
                    ProjectTaskPayload::HistoryLoaded {
                        checkpoints: checkpoints.clone(),
                    },
                ))
            );
            let _ = workspace.update(ProjectMessage::SelectHistoryCheckpoint(
                "autosave-chapter-one".to_owned(),
            ));
            workspace.set_history_current_document(verification_history_current_document(
                snapshot, &workspace,
            ));
            let ticket = workspace.begin_task(ProjectTask::PreviewHistory {
                checkpoint_id: "autosave-chapter-one".to_owned(),
            });
            assert!(
                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                    ticket,
                    ProjectTaskPayload::HistoryPreviewReady {
                        preview: HistoryPreviewData {
                            checkpoint: checkpoints[0].clone(),
                            resource_paths: vec![
                                "1 Current: The harbor held the last of the evening light."
                                    .to_owned(),
                                "2 Current: Mara waited beneath the clock tower.".to_owned(),
                                "3 Checkpoint: The unopened letter was sealed.".to_owned(),
                                "4 Checkpoint: By morning, the tide had erased every footprint."
                                    .to_owned(),
                            ],
                            document: Some(HistoryDocumentPreview {
                                document_id: stable_id(
                                    snapshot.documents[0].document_id.as_bytes()
                                ),
                                canonical_path: "documents/chapter-one.html".to_owned(),
                                semantic: verification_semantic_body(
                                    snapshot.documents[0].document_id,
                                    history_checkpoint_body,
                                ),
                            }),
                        },
                    },
                ))
            );
        }
        VisualTarget::SettingsAppearance => {
            let _ = workspace.update(ProjectMessage::SelectSettingsCategory(
                SettingsCategory::Appearance,
            ));
        }
        VisualTarget::Export => {
            let _ = workspace.update(ProjectMessage::SetExportOutputName(
                "the-glass-harbor.html".to_owned(),
            ));
            let _ = workspace.update(ProjectMessage::SetExportDestination(Some(
                "~/Documents".to_owned(),
            )));
            let _ = workspace.update(ProjectMessage::SetExportNumbering(true));
            let _ = workspace.update(ProjectMessage::SetExportTitleSetting(
                ProjectExportSetting::Inherit,
            ));
        }
        VisualTarget::ErrorRecovery => {
            let ticket = workspace.begin_recovery_reconciliation();
            assert!(
                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                    ticket,
                    ProjectTaskPayload::RecoveryAvailable {
                        accepted_records: 37,
                        affected_documents: vec![(chapter_one_document, 1)],
                        isolation: Some("Chapter One journal is ready to reconcile; the last durable history checkpoint remains preserved.".to_owned()),
                    },
                ))
            );
            assert_eq!(workspace.content_state(), &ContentState::Recovery);
        }
        VisualTarget::RecentlyDeleted => {
            let _ = workspace.update(ProjectMessage::SelectRecentlyDeleted(deleted));
        }
    }
    assert_scenario_contract(target, appearance, &workspace);
    workspace
}

#[cfg(feature = "visual-verification")]
fn assert_scenario_contract(
    target: VisualTarget,
    _appearance: VisualAppearance,
    workspace: &crate::ProjectWorkspace,
) {
    use crate::{ContentState, SettingsCategory};
    use parchmint_preferences::AppearanceMode;

    match target {
        VisualTarget::Launcher => {}
        VisualTarget::EditorSingle => {
            let primary = workspace.editor().pane(crate::EditorPane::Primary);
            assert_eq!(
                primary
                    .tabs()
                    .iter()
                    .map(|tab| tab.title())
                    .collect::<Vec<_>>(),
                [
                    "Chapter One",
                    "Chapter Two",
                    "Harbor Notes",
                    "Map of the Coast"
                ]
            );
            assert_eq!(
                primary.active_document(),
                primary.tabs().first().map(|tab| tab.id())
            );
            assert!(
                !workspace
                    .editor()
                    .pane(crate::EditorPane::Companion)
                    .is_populated()
            );
            assert_part_one_expanded(workspace);
            assert_chapter_one_inspector(workspace);
        }
        VisualTarget::EditorDual => {
            let primary = workspace.editor().pane(crate::EditorPane::Primary);
            let companion = workspace.editor().pane(crate::EditorPane::Companion);
            assert_eq!(
                primary
                    .tabs()
                    .iter()
                    .map(|tab| tab.title())
                    .collect::<Vec<_>>(),
                ["Chapter One", "Chapter Two", "Map of the Coast"]
            );
            assert_eq!(
                companion
                    .tabs()
                    .iter()
                    .map(|tab| tab.title())
                    .collect::<Vec<_>>(),
                ["Chapter One", "Chapter Two", "Map of the Coast"]
            );
            assert_eq!(
                primary.active_document(),
                primary.tabs().first().map(|tab| tab.id())
            );
            assert_eq!(
                companion.active_document(),
                companion.tabs().get(1).map(|tab| tab.id())
            );
            assert_part_one_expanded(workspace);
            assert_chapter_one_inspector(workspace);
        }
        VisualTarget::Cards => {
            let cards = workspace.cards().items();
            let manuscript_section =
                stable_id(parchmint_domain::NodeId::manuscript_root().as_bytes());
            assert_eq!(workspace.cards().section_id(), manuscript_section.as_str());
            assert!(
                cards
                    .iter()
                    .any(|item| item.title == "Part One" && item.expanded)
            );
            assert!(cards.iter().any(|item| item.title == "Part One"
                && item.synopsis == "The opening movement of the novel."));
            assert!(cards.iter().any(|item| item.title == "Chapter One"
                && item.synopsis
                    == "The harbor has fallen silent, and Mara must decide whom to trust."));
            assert!(cards.iter().any(|item| item.title == "Chapter Two"
                && item.synopsis == "The sealed letter changes what Mara believes."));
            assert!(cards.iter().any(|item| item.title == "Part Two"
                && item.synopsis == "The fallout after the harbor reveal."));
            assert!(cards.iter().any(|item| {
                item.title == "Chapter One"
                    && item
                        .metadata
                        .iter()
                        .any(|(_, label, value)| *label == "POV" && *value == Some("Mara"))
                    && item
                        .metadata
                        .iter()
                        .any(|(_, label, value)| *label == "Words" && *value == Some("1,240"))
            }));
            assert!(
                cards
                    .iter()
                    .any(|item| item.title == "Chapter One" && item.visible)
            );
            assert!(
                cards
                    .iter()
                    .any(|item| item.title == "Chapter Two" && item.visible)
            );
            assert!(workspace.cards().last_activated_document().is_some());
            assert_chapter_one_inspector(workspace);
        }
        VisualTarget::GlobalSearch => {
            assert_eq!(workspace.global_search().query(), "harbor");
            assert!(workspace.global_search().replacement().is_empty());
            assert!(workspace.global_search().is_complete());
            assert_eq!(workspace.global_search().results().len(), 6);
            assert!(workspace.global_search().results_are_grouped_by_document());
            let results = workspace.global_search().results();
            let active_document = workspace
                .editor()
                .pane(crate::EditorPane::Primary)
                .active_document()
                .expect("search has an active Chapter One document");
            assert!(
                results[..2]
                    .iter()
                    .all(|result| result.document_id.as_str() == active_document)
            );
            let harbor_notes_document = &results[2].document_id;
            assert_ne!(harbor_notes_document.as_str(), active_document);
            assert!(
                results[2..]
                    .iter()
                    .all(|result| result.document_id.as_str() == harbor_notes_document.as_str())
            );
            assert_eq!(
                results[0].prefix.as_str(),
                "...the ",
                "first Chapter One result matches the Penpot scenario text"
            );
            assert_eq!(results[1].suffix, " road beneath the cliffs.");
            assert_eq!(
                results[..4]
                    .iter()
                    .map(|result| (result.prefix.as_str(), result.suffix.as_str()))
                    .collect::<Vec<_>>(),
                [
                    ("...the ", " held the last of the evening light."),
                    ("...a ", " road beneath the cliffs."),
                    ("...the ", " held the last of the evening light."),
                    ("...a ", " road beneath the cliffs."),
                ],
                "visible Search result groups follow the checked-in Penpot reference order"
            );
            assert_eq!(
                workspace
                    .global_search()
                    .results()
                    .iter()
                    .filter(|result| result.document_id
                        == workspace
                            .editor()
                            .pane(crate::EditorPane::Primary)
                            .active_document()
                            .unwrap())
                    .count(),
                2
            );
            assert!(
                workspace
                    .editor()
                    .pane(crate::EditorPane::Primary)
                    .is_populated()
            );
            assert_chapter_one_inspector(workspace);
        }
        VisualTarget::History => {
            assert_eq!(workspace.history().checkpoints().len(), 4);
            assert_eq!(
                workspace.history().selected_checkpoint_id(),
                Some("autosave-chapter-one")
            );
            assert!(
                workspace
                    .history()
                    .preview()
                    .and_then(|preview| preview.document.as_ref())
                    .is_some()
            );
            assert_eq!(
                workspace
                    .history()
                    .current_document()
                    .map(|document| document.title.as_str()),
                Some("Chapter One")
            );
            let checkpoint = workspace
                .history()
                .preview()
                .and_then(|preview| preview.document.as_ref())
                .expect("selected history checkpoint includes a semantic document")
                .semantic
                .plain_text();
            let current = workspace
                .history()
                .current_document()
                .expect("history supplies the current mounted document")
                .semantic
                .plain_text();
            assert!(checkpoint.contains("unopened letter"));
            assert!(current.contains("sealed letter"));
            assert_ne!(current, checkpoint);
            let comparison = workspace
                .history()
                .comparison()
                .expect("selected checkpoint and current document are comparable");
            assert_eq!(comparison.document_title, "Chapter One");
            assert_eq!(comparison.lines.len(), 4);
            assert_eq!(
                comparison.change_summary(),
                crate::HistoryChangeSummary {
                    added_lines: 0,
                    removed_lines: 0,
                    modified_lines: 1,
                }
            );
            let modified = comparison
                .lines
                .iter()
                .find(|line| line.kind == crate::HistoryComparisonLineKind::Modified)
                .expect("comparison includes the changed line rendered by History");
            assert_eq!(
                modified.before.as_ref().map(|line| line.line_number),
                Some(3)
            );
            assert_eq!(
                modified.after.as_ref().map(|line| line.line_number),
                Some(3)
            );
            assert!(
                modified
                    .before
                    .as_ref()
                    .expect("modified checkpoint line exists")
                    .spans
                    .iter()
                    .any(
                        |span| span.kind == crate::HistoryComparisonSpanKind::Removed
                            && span.text == "unopened"
                    )
            );
            assert!(
                modified
                    .after
                    .as_ref()
                    .expect("modified current line exists")
                    .spans
                    .iter()
                    .any(|span| span.kind == crate::HistoryComparisonSpanKind::Added
                        && span.text == "sealed")
            );
        }
        VisualTarget::SettingsAppearance => {
            assert_eq!(
                workspace.settings().selected_category(),
                SettingsCategory::Appearance
            );
            assert_eq!(workspace.settings().appearance(), AppearanceMode::System);
        }
        VisualTarget::Export => {
            assert_eq!(workspace.export().output_name(), "the-glass-harbor.html");
            assert_eq!(workspace.export().destination(), Some("~/Documents"));
            assert!(workspace.export().numbers_documents());
            assert!(workspace.export().can_start());
            assert_eq!(
                workspace.export().project_settings().emit_titles,
                parchmint_domain::ProjectExportSetting::Inherit
            );
            assert!(!workspace.export().project_settings().starts_new_page);
        }
        VisualTarget::ErrorRecovery => {
            assert_eq!(workspace.content_state(), &ContentState::Recovery);
            assert_eq!(workspace.recovery().accepted_records(), 37);
            assert!(workspace.recovery().isolation().is_some());
        }
        VisualTarget::RecentlyDeleted => {
            assert_eq!(workspace.recently_deleted().items().len(), 2);
            assert_eq!(
                workspace.recently_deleted().selected_item_id(),
                Some("51515151515151515151515151515151")
            );
            let preview = workspace
                .recently_deleted()
                .selected_preview()
                .expect("Old Chapter has a canonical preview");
            assert_eq!(preview.title, "Old Chapter");
            let text = preview.semantic.plain_text();
            assert!(text.contains("The harbor held the last of the evening light."));
            assert!(text.contains("Some journeys begin long before the road."));
            assert!(text.contains("Mara turned the unopened letter in her fingers."));
            assert!(
                preview
                    .semantic
                    .blocks()
                    .iter()
                    .flat_map(|block| block.marks())
                    .any(|mark| {
                        matches!(mark.mark(), parchmint_editor_api::SemanticInlineMark::Bold)
                            && mark.range().start().value() < mark.range().end().value()
                    })
            );
        }
    }
}

#[cfg(feature = "visual-verification")]
fn assert_chapter_one_inspector(workspace: &crate::ProjectWorkspace) {
    let chapter_one = verification_node_id();
    assert_eq!(workspace.inspector_node_id(), Some(chapter_one.as_str()));
    assert_eq!(
        workspace.explorer().synopsis(&chapter_one),
        Some("The harbor has fallen silent, and Mara must decide whom to trust.")
    );
    let items = workspace.inspector().metadata_items(&chapter_one);
    assert!(
        items
            .iter()
            .any(|item| item.label == "POV" && item.effective_value == Some("Mara"))
    );
    assert!(
        items
            .iter()
            .any(|item| item.label == "Status" && item.effective_value == Some("Draft"))
    );
}

#[cfg(feature = "visual-verification")]
fn assert_part_one_expanded(workspace: &crate::ProjectWorkspace) {
    let part_one = verification_part_one_node_id();
    let rows = workspace.explorer().rows();
    let row = rows
        .iter()
        .find(|row| row.id == part_one.as_str())
        .expect("verification project includes Part One");
    assert!(row.expanded, "Part One exposes Chapter One and Chapter Two");
    assert!(
        rows.iter()
            .any(|row| row.title == "Chapter One" && row.parent_id == Some(part_one.as_str()))
    );
    assert!(
        rows.iter()
            .any(|row| row.title == "Chapter Two" && row.parent_id == Some(part_one.as_str()))
    );
}

#[cfg(all(test, feature = "visual-verification"))]
mod tests {
    use iced::widget::{Canvas, Row, Space, canvas};
    use iced::{Color, Point, Rectangle, Size};
    use iced_test::Simulator;

    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct ScaleTransformMarker;

    impl canvas::Program<()> for ScaleTransformMarker {
        type State = ();

        fn draw(
            &self,
            _state: &Self::State,
            renderer: &iced::Renderer,
            _theme: &iced::Theme,
            bounds: Rectangle,
            _cursor: iced::mouse::Cursor,
        ) -> Vec<canvas::Geometry> {
            let mut frame = canvas::Frame::new(renderer, bounds.size());
            frame.fill_rectangle(
                Point::new(20.0, 10.0),
                Size::new(4.0, 4.0),
                Color::from_rgb8(255, 0, 0),
            );
            vec![frame.into_geometry()]
        }
    }

    #[test]
    fn catalog_has_every_penpot_fixture_at_the_2x_reference_size() {
        assert_eq!(VisualTarget::ALL.len(), 10);
        assert_eq!(VISUAL_TARGET_SPECS.len(), VisualTarget::ALL.len());
        for target in VisualTarget::ALL {
            let spec = visual_target_spec(target);
            assert_eq!((spec.width, spec.height), (1440, 900));
            assert_eq!(spec.physical_size(), (2880, 1800));
            assert!(spec.production_native);
            for appearance in VisualAppearance::ALL {
                assert!(!target.reference_id(appearance).is_empty());
            }
        }
    }

    #[test]
    fn mounted_editor_viewports_match_the_target_pane_allocations() {
        let snapshot = verification_snapshot(VisualTarget::EditorSingle);
        let single = verification_workspace(
            VisualTarget::EditorSingle,
            VisualAppearance::Light,
            &snapshot,
        );
        assert_eq!(
            verification_editor_viewport(
                VisualTarget::EditorSingle,
                crate::EditorPane::Primary,
                &single,
            ),
            parchmint_editor_iced::EditorViewport::new(840.0, 736.0)
                .expect("single-pane allocation"),
        );

        let dual_snapshot = verification_snapshot(VisualTarget::EditorDual);
        let dual = verification_workspace(
            VisualTarget::EditorDual,
            VisualAppearance::Light,
            &dual_snapshot,
        );
        for pane in [crate::EditorPane::Primary, crate::EditorPane::Companion] {
            assert_eq!(
                verification_editor_viewport(VisualTarget::EditorDual, pane, &dual),
                parchmint_editor_iced::EditorViewport::new(416.0, 736.0)
                    .expect("dual-pane allocation"),
            );
        }

        let search_snapshot = verification_snapshot(VisualTarget::GlobalSearch);
        let search = verification_workspace(
            VisualTarget::GlobalSearch,
            VisualAppearance::Light,
            &search_snapshot,
        );
        assert_eq!(
            verification_editor_viewport(
                VisualTarget::GlobalSearch,
                crate::EditorPane::Primary,
                &search,
            ),
            parchmint_editor_iced::EditorViewport::new(760.0, 848.0)
                .expect("manuscript-only search allocation"),
        );
    }

    #[test]
    fn launcher_and_mounted_editor_render_headlessly_with_bounded_renderer_state() {
        for (target, appearance) in [
            (VisualTarget::Launcher, VisualAppearance::Light),
            (VisualTarget::EditorSingle, VisualAppearance::Light),
        ] {
            let spec = visual_target_spec(target);
            let theme = presentation(appearance);
            {
                let mut simulator = Simulator::<()>::with_size(
                    visual_settings(),
                    Size::new(spec.width as f32, spec.height as f32),
                    production_element(target, appearance),
                );
                let snapshot = simulator
                    .snapshot(&theme.iced_theme())
                    .expect("production composition should render headlessly");
                assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
            }
        }
    }

    #[test]
    fn production_cards_target_renders_the_selected_inspector_content() {
        let target = VisualTarget::Cards;
        let appearance = VisualAppearance::Light;
        let spec = visual_target_spec(target);
        let theme = presentation(appearance);
        let mut simulator = Simulator::<()>::with_size(
            visual_settings(),
            Size::new(spec.width as f32, spec.height as f32),
            production_element(target, appearance),
        );

        assert!(simulator.find("Inspector").is_ok());
        assert!(simulator.find("No selection").is_err());
        for content in [
            "Chapter One",
            "SYNOPSIS",
            "The harbor has fallen silent, and Mara must decide whom to trust.",
            "POV",
            "Mara",
            "Status",
            "Draft",
            "No comments",
        ] {
            assert!(
                simulator.find(content).is_ok(),
                "production Cards Inspector shows {content}"
            );
        }
        let snapshot = simulator
            .snapshot(&theme.iced_theme())
            .expect("production Cards target renders headlessly");
        assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
    }

    #[test]
    fn canvas_translation_is_scaled_to_physical_pixels() {
        let element: iced::Element<'static, ()> = Row::new()
            .push(Space::new().width(100.0))
            .push(Canvas::new(ScaleTransformMarker).width(80.0).height(40.0))
            .into();
        let mut simulator =
            Simulator::<()>::with_size(iced::Settings::default(), Size::new(200.0, 60.0), element);
        let snapshot = simulator
            .snapshot(&iced::Theme::Light)
            .expect("translated Canvas should render headlessly");

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let output_stem = std::env::temp_dir().join(format!(
            "parchmint-iced-tiny-skia-scale-{}-{unique}",
            std::process::id()
        ));
        snapshot
            .matches_image(&output_stem)
            .expect("write Canvas transform regression screenshot");
        let output_path = renderer_output_path(&output_stem);

        let file =
            std::fs::File::open(&output_path).expect("open Canvas transform regression screenshot");
        let decoder = png::Decoder::new(std::io::BufReader::new(file));
        let mut reader = decoder
            .read_info()
            .expect("decode Canvas transform regression screenshot");
        let mut bytes = vec![
            0;
            reader
                .output_buffer_size()
                .expect("Canvas transform screenshot should fit in memory")
        ];
        let info = reader
            .next_frame(&mut bytes)
            .expect("read Canvas transform regression pixels");
        assert_eq!((info.width, info.height), (400, 120));
        assert_eq!(info.color_type, png::ColorType::Rgba);

        let marker_x = bytes[..info.buffer_size()]
            .chunks_exact(4)
            .enumerate()
            .filter_map(|(index, pixel)| {
                (pixel == [255, 0, 0, 255]).then_some(index % info.width as usize)
            })
            .min();
        std::fs::remove_file(output_path).expect("remove Canvas transform regression screenshot");

        assert_eq!(
            marker_x,
            Some(240),
            "logical Canvas x=100 plus local marker x=20 must scale to physical x=240 at 2x"
        );
    }

    #[test]
    fn editor_catalog_hydrates_production_snapshot_and_mounts_real_host() {
        let snapshot = verification_snapshot(VisualTarget::EditorDual);
        let workspace =
            verification_workspace(VisualTarget::EditorDual, VisualAppearance::Light, &snapshot);
        assert_eq!(
            workspace
                .editor()
                .pane(crate::EditorPane::Primary)
                .tabs()
                .len(),
            3
        );
        assert_eq!(
            workspace
                .editor()
                .pane(crate::EditorPane::Companion)
                .tabs()
                .len(),
            3
        );
        assert_eq!(snapshot.documents.len(), 5);
        assert!(snapshot.documents[0].body.contains("harbor held the last"));
        assert!(
            snapshot.documents[0]
                .body
                .contains("<blockquote>“Some journeys begin long before the road.”</blockquote>")
        );
        assert_eq!(
            workspace.explorer().synopsis(&verification_node_id()),
            Some("The harbor has fallen silent, and Mara must decide whom to trust.")
        );

        let slots = editor_slots(
            &snapshot,
            &workspace,
            VisualTarget::EditorDual,
            VisualAppearance::Light,
        );
        assert!(slots.slot(crate::EditorPane::Primary).is_some());
        assert!(slots.slot(crate::EditorPane::Companion).is_some());
    }

    #[test]
    fn history_catalog_mounts_the_selected_checkpoint_comparison() {
        let spec = visual_target_spec(VisualTarget::History);
        let mut simulator = Simulator::<()>::with_size(
            visual_settings(),
            Size::new(spec.width as f32, spec.height as f32),
            production_element(VisualTarget::History, VisualAppearance::Light),
        );
        assert!(simulator.find("Checkpoint").is_ok());
        assert!(simulator.find("Current").is_ok());
        assert!(simulator.find("unopened").is_ok());
        assert!(simulator.find("sealed").is_ok());
    }

    #[test]
    fn catalog_scenarios_satisfy_their_production_state_contracts() {
        for target in VisualTarget::ALL {
            if target == VisualTarget::Launcher {
                continue;
            }
            for appearance in VisualAppearance::ALL {
                let snapshot = verification_snapshot(target);
                let workspace = verification_workspace(target, appearance, &snapshot);
                assert_scenario_contract(target, appearance, &workspace);
            }
        }
    }
}
