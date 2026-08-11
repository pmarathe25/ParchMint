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
        EditorMessage, EditorPane, ProjectFixture, ProjectMessage, ProjectWorkspace,
        RibbonDestination, SelectionGesture,
        iced_editor_surface::editor_center_surface,
        iced_project_surface::{ProjectSurfaceMessage, verification_project_surface},
    };

    if target == VisualTarget::Launcher {
        // Keep launcher access isolated behind native's verification seam.
        return crate::native::NativeDesktop::verification_launcher_element();
    }

    let project_fixture = match target {
        VisualTarget::EditorSingle | VisualTarget::EditorDual => ProjectFixture::Explorer,
        VisualTarget::Cards => ProjectFixture::Cards,
        VisualTarget::GlobalSearch => ProjectFixture::GlobalSearch,
        VisualTarget::History => ProjectFixture::History,
        VisualTarget::SettingsAppearance => ProjectFixture::SettingsAppearance,
        VisualTarget::Export => ProjectFixture::Export,
        VisualTarget::ErrorRecovery => ProjectFixture::ErrorRecovery,
        VisualTarget::RecentlyDeleted => ProjectFixture::RecentlyDeleted,
        VisualTarget::Launcher => unreachable!("launcher returned above"),
    };
    let mut workspace = ProjectWorkspace::from_fixture(project_fixture);
    // The Penpot boards consistently inspect Chapter One. Keep that selection
    // in the verification-only fixture so the real Inspector composition is
    // populated rather than being an empty shell.
    if target != VisualTarget::ErrorRecovery {
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
    }
    if target == VisualTarget::EditorSingle {
        let companion = workspace.editor().pane(EditorPane::Companion);
        if let Some(document_id) = companion.active_document().map(str::to_owned) {
            workspace.editor_mut().update(EditorMessage::CloseTab {
                pane: EditorPane::Companion,
                document_id,
            });
        }
    }
    populate_editor_tabs(&mut workspace, target);
    let workspace: &'static ProjectWorkspace = Box::leak(Box::new(workspace));
    let slots = editor_slots(target);
    let theme = presentation(appearance);
    let editor = editor_center_surface(workspace.editor(), theme, &slots)
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
fn populate_editor_tabs(workspace: &mut crate::ProjectWorkspace, target: VisualTarget) {
    if !matches!(
        target,
        VisualTarget::EditorSingle | VisualTarget::EditorDual
    ) {
        return;
    }
    let tabs = [
        ("chapter-one", "Chapter One"),
        ("chapter-two", "Chapter Two"),
        ("harbor-notes", "Harbor Notes"),
        ("map-of-the-coast", "Map of the Coast"),
    ];
    for pane in [crate::EditorPane::Primary, crate::EditorPane::Companion] {
        if target == VisualTarget::EditorSingle && pane == crate::EditorPane::Companion {
            continue;
        }
        for (id, title) in tabs {
            workspace
                .editor_mut()
                .update(crate::EditorMessage::OpenTab {
                    pane,
                    tab: crate::TabSpec::new(id, title),
                });
        }
        let active = if pane == crate::EditorPane::Primary {
            "chapter-one"
        } else {
            "chapter-two"
        };
        workspace
            .editor_mut()
            .update(crate::EditorMessage::ActivateTab {
                pane,
                document_id: active.to_owned(),
            });
    }
}

#[cfg(feature = "visual-verification")]
fn editor_slots(target: VisualTarget) -> crate::iced_editor_surface::EditorHostSlots {
    use crate::{
        EditorPane,
        iced_editor_surface::{EditorCenterPaneState, EditorHostSlots, EditorPaneSlot},
    };

    let mut slots = EditorHostSlots::default();
    if matches!(
        target,
        VisualTarget::EditorSingle | VisualTarget::EditorDual
    ) {
        slots.insert(
            EditorPane::Primary,
            EditorPaneSlot::state(EditorCenterPaneState::VerificationProse {
                heading: "Chapter One",
                paragraphs: &[
                    "The harbor held the last of the evening light. Mara waited beneath the clock tower, turning the unopened letter between her fingers.",
                    "“Some journeys begin long before the road.”",
                    "✱",
                    "By morning, the tide had erased every footprint.",
                ],
            }),
        );
    }
    if target == VisualTarget::EditorDual {
        slots.insert(
            EditorPane::Companion,
            EditorPaneSlot::state(EditorCenterPaneState::VerificationProse {
                heading: "Chapter Two",
                paragraphs: &[
                    "Rain found the city before dawn.",
                    "Another manuscript document opens on the right.",
                ],
            }),
        );
    }
    slots
}

#[cfg(all(test, feature = "visual-verification"))]
mod tests {
    use iced::Size;
    use iced_test::Simulator;

    use super::*;
    use crate::{
        EditorPane, ProjectFixture, ProjectMessage, ProjectWorkspace, SelectionGesture,
        iced_editor_surface::{EditorCenterPaneState, EditorPaneSlot},
    };

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
    fn every_production_composition_renders_headlessly_in_both_appearances() {
        for target in VisualTarget::ALL {
            let spec = visual_target_spec(target);
            for appearance in VisualAppearance::ALL {
                let theme = presentation(appearance);
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
    fn editor_catalog_uses_populated_prose_and_inspector_fixture_state() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        assert_eq!(workspace.explorer().selected_ids(), ["chapter-one"]);
        populate_editor_tabs(&mut workspace, VisualTarget::EditorDual);
        assert_eq!(workspace.editor().pane(EditorPane::Primary).tabs().len(), 4);
        assert_eq!(
            workspace.editor().pane(EditorPane::Companion).tabs().len(),
            4
        );

        let slots = editor_slots(VisualTarget::EditorDual);
        assert!(matches!(
            slots.slot(EditorPane::Primary),
            Some(EditorPaneSlot::State(
                EditorCenterPaneState::VerificationProse { .. }
            ))
        ));
        assert!(matches!(
            slots.slot(EditorPane::Companion),
            Some(EditorPaneSlot::State(
                EditorCenterPaneState::VerificationProse { .. }
            ))
        ));
    }
}
