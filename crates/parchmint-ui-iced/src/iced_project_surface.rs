//! Private Iced composition for deterministic project-workspace fixtures.

use iced::widget::{column, container, row, text};
use iced::{Background, Element, Length, Theme, border};

use crate::{ContentState, ProjectFixture, ProjectMessage, ProjectWorkspace, SidebarSurface};

pub(crate) fn fixture_surface(workspace: &ProjectWorkspace) -> Element<'static, ProjectMessage> {
    let ribbon = container(text(
        "Editor    Cards    History    Recently Deleted    Export    Settings",
    ))
    .padding([10, 16])
    .width(Length::Fill)
    .height(52)
    .style(toolbar_style);

    let sidebar = container(
        column![
            text(match workspace.sidebar_surface() {
                SidebarSurface::Explorer => "Explorer                         Search",
                SidebarSurface::GlobalSearch => "Back to Explorer        Global Search",
            })
            .size(16),
            text(sidebar_text(workspace)).size(13),
        ]
        .spacing(14),
    )
    .padding(16)
    .width(280)
    .height(Length::Fill)
    .style(sidebar_style);

    let main = container(
        column![
            text(main_title(workspace)).size(22),
            text(main_text(workspace)).size(14),
        ]
        .spacing(18),
    )
    .padding(24)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(main_style);

    let inspector = container(
        column![
            text("Inspector").size(16),
            text("Synopsis").size(13),
            text(
                workspace
                    .explorer()
                    .synopsis("chapter-one")
                    .unwrap_or("No selection")
                    .to_owned(),
            )
            .size(12),
            text("Metadata\nPoint of view    first person").size(12),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(320)
    .height(Length::Fill)
    .style(sidebar_style);

    let workspace_row = row![sidebar, main, inspector].height(Length::Fill);
    let status = container(text(status_text(workspace)).size(12))
        .padding([7, 12])
        .width(Length::Fill)
        .height(32)
        .style(status_style);

    container(column![ribbon, workspace_row, status])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(workspace_style)
        .into()
}

fn sidebar_text(workspace: &ProjectWorkspace) -> String {
    match workspace.sidebar_surface() {
        SidebarSurface::Explorer => {
            "▾ Manuscript\n  ▾ Part One\n      Chapter One\n      Chapter Two\n    Chapter Three\n▾ Research\n    Research Notes"
                .to_owned()
        }
        SidebarSurface::GlobalSearch => format!(
            "Query: {}\nAa   Whole word\n\nChapter One\n… beside the river, the path …",
            workspace.global_search().query()
        ),
    }
}

fn main_title(workspace: &ProjectWorkspace) -> &'static str {
    match workspace.fixture() {
        ProjectFixture::Explorer => "Chapter One",
        ProjectFixture::Cards => "Cards · Manuscript",
        ProjectFixture::GlobalSearch => "Replace Preview",
        ProjectFixture::History => "Project History",
        ProjectFixture::RecentlyDeleted => "Recently Deleted",
        ProjectFixture::SettingsAppearance => "Settings · Appearance",
        ProjectFixture::Export => "Export",
        ProjectFixture::ErrorRecovery => "Recover unsaved changes",
    }
}

fn main_text(workspace: &ProjectWorkspace) -> String {
    match workspace.fixture() {
        ProjectFixture::Explorer => {
            "The river narrowed beyond the old stone bridge.\n\nA complete editor surface remains mounted in the project shell."
                .to_owned()
        }
        ProjectFixture::Cards => "Part One\n\n  Chapter One\n  A first-person opening beside the river.\n\n  Chapter Two\n\nChapter Three"
            .to_owned(),
        ProjectFixture::GlobalSearch => {
            "☒ Manuscript\n  ☑ Chapter One\n    ☑ river — first match\n    ☑ river — second match\n  ☐ Chapter Two"
                .to_owned()
        }
        ProjectFixture::History => "Today\n\nDraft Two · Named snapshot\nAutosave · Chapter One\n\nCheckpoint                         Current\nThe narrow river                   The winding river"
            .to_owned(),
        ProjectFixture::RecentlyDeleted => "Deleted Part\nFormer location: Part One\n\nFormatted preview\nThe complete deleted subtree is available to restore."
            .to_owned(),
        ProjectFixture::SettingsAppearance => "Appearance\n\n◉ System    ○ Light    ○ Dark\n\nSystem follows the operating-system appearance while ParchMint is running."
            .to_owned(),
        ProjectFixture::Export => format!(
            "Scope                 Entire Manuscript\nOutput                {}\nTitles and page breaks Inherit\nNumber documents       {}\n\nExport",
            workspace.export().output_name(),
            if workspace.export().numbers_documents() {
                "On"
            } else {
                "Off"
            }
        ),
        ProjectFixture::ErrorRecovery => match workspace.content_state() {
            ContentState::Recovery => "ParchMint can replay valid unsaved edits on top of the last completed autosave.\n\nRecover edits    Open last saved"
                .to_owned(),
            ContentState::Empty => "No content yet".to_owned(),
            ContentState::Loading => "Loading project…".to_owned(),
            ContentState::Error(error) => format!("The project needs attention\n\n{error}"),
            ContentState::Ready => "Recovered edits are ready in the editor.".to_owned(),
        },
    }
}

fn status_text(workspace: &ProjectWorkspace) -> String {
    format!(
        "Explorer shown    Inspector shown                                  {:?}",
        workspace.save().state()
    )
}

fn workspace_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.base.text),
        ..container::Style::default()
    }
}

fn toolbar_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        border: border::color(palette.background.strong.color).width(1),
        ..container::Style::default()
    }
}

fn sidebar_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.weak.text),
        border: border::color(palette.background.strong.color).width(1),
        ..container::Style::default()
    }
}

fn main_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        ..container::Style::default()
    }
}

fn status_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.strong.color)),
        text_color: Some(palette.background.strong.text),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use iced::{Settings, Size, Theme};
    use iced_test::Simulator;
    use parchmint_preferences::ResolvedAppearance;

    use super::*;

    fn assert_fixture_hash(fixture: ProjectFixture, theme: &Theme, appearance: ResolvedAppearance) {
        let workspace = ProjectWorkspace::from_fixture(fixture);
        let stem = workspace.fixture_reference(appearance);
        let mut simulator = Simulator::<ProjectMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            fixture_surface(&workspace),
        );
        let snapshot = simulator
            .snapshot(theme)
            .expect("headless project snapshot");
        let renderer = format!("{snapshot:?}");
        assert!(
            renderer.contains("renderer: \"tiny-skia\""),
            "headless fixture requires the pinned tiny-skia renderer: {renderer}"
        );
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(stem);
        assert!(
            snapshot.matches_hash(&base).expect("compare fixture hash"),
            "project fixture hash changed for {stem}"
        );
        assert!(
            base.with_file_name(format!("{stem}-tiny-skia.sha256"))
                .is_file(),
            "checked-in tiny-skia fixture hash is required for {stem}"
        );
    }

    #[test]
    fn every_requirement_linked_project_view_renders_in_light_and_dark() {
        for fixture in [
            ProjectFixture::Explorer,
            ProjectFixture::Cards,
            ProjectFixture::GlobalSearch,
            ProjectFixture::History,
            ProjectFixture::RecentlyDeleted,
            ProjectFixture::SettingsAppearance,
            ProjectFixture::Export,
            ProjectFixture::ErrorRecovery,
        ] {
            assert_fixture_hash(fixture, &Theme::Light, ResolvedAppearance::Light);
            assert_fixture_hash(fixture, &Theme::Dark, ResolvedAppearance::Dark);
        }
    }
}
