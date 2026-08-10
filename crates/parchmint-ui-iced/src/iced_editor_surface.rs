//! Private Iced composition for deterministic editor-workspace fixtures.
//!
//! The public presentation boundary exposes only ParchMint values. This module
//! proves those values can drive the pinned renderer without a native display.

use iced::widget::{column, container, row, text};
use iced::{Background, Element, Length, Theme, border};

use crate::{EditorMessage, EditorPane, EditorPaneState, EditorWorkspace};

pub(crate) fn fixture_surface(workspace: &EditorWorkspace) -> Element<'static, EditorMessage> {
    let toolbar = container(text(
        "Style  B  I  U  S  Lists  Quote  Link  Scene Break  Page Break",
    ))
    .padding([10, 16])
    .width(Length::Fill)
    .height(44)
    .style(toolbar_style);
    let primary = pane_surface(
        workspace.pane(EditorPane::Primary),
        workspace.focused_pane() == EditorPane::Primary,
    );
    let companion = pane_surface(
        workspace.pane(EditorPane::Companion),
        workspace.focused_pane() == EditorPane::Companion,
    );
    let panes = row![primary, companion].spacing(8).height(Length::Fill);
    let status = workspace.status_bar();
    let status = container(text(format!(
        "{:?}    Manuscript {} words",
        status.current_count(),
        status.manuscript_total()
    )))
    .padding([7, 12])
    .width(Length::Fill)
    .height(32)
    .style(status_style);

    container(column![toolbar, panes, status].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(workspace_style)
        .into()
}

fn pane_surface(pane: &EditorPaneState, focused: bool) -> Element<'static, EditorMessage> {
    let active = pane.active_document().unwrap_or_default().to_owned();
    let tabs = pane.tabs().iter().fold(row![].spacing(1), |tabs, tab| {
        let dirty = if tab.is_dirty() { " •" } else { "" };
        tabs.push(
            container(text(format!("{}{}  ×", tab.title(), dirty)))
                .padding([7, 10])
                .height(32)
                .width(Length::FillPortion(1))
                .style(tab_style),
        )
    });
    let body = container(column![
        text(active).size(20),
        text("The deterministic mounted editor surface keeps view-local focus, scroll, search, and decorations."),
        text(format!("scroll {:.0}px", pane.scroll_offset())).size(12),
    ].spacing(14))
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill);

    container(column![tabs, body])
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .style(move |theme| pane_style(theme, focused))
        .into()
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

fn status_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.strong.color)),
        text_color: Some(palette.background.strong.text),
        ..container::Style::default()
    }
}

fn tab_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.weak.text),
        border: border::color(palette.background.strong.color).width(1),
        ..container::Style::default()
    }
}

fn pane_style(theme: &Theme, focused: bool) -> container::Style {
    let palette = theme.extended_palette();
    let border = if focused {
        border::color(palette.primary.strong.color).width(2)
    } else {
        border::color(palette.background.strong.color).width(1)
    };
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        border,
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use iced::{Settings, Size, Theme};
    use iced_test::Simulator;

    use super::*;
    use crate::{EditorFixture, EditorMessage};

    fn assert_fixture_hash(fixture: EditorFixture, theme: &Theme, stem: &str) {
        let workspace = EditorWorkspace::from_fixture(fixture);
        let mut simulator = Simulator::<EditorMessage>::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            fixture_surface(&workspace),
        );
        let snapshot = simulator.snapshot(theme).expect("headless editor snapshot");
        let renderer = format!("{snapshot:?}");
        assert!(
            renderer.contains("renderer: \"tiny-skia\""),
            "headless fixture requires the pinned tiny-skia renderer: {renderer}"
        );
        let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(stem);
        assert!(
            golden
                .with_file_name(format!("{stem}-tiny-skia.sha256"))
                .is_file(),
            "checked-in tiny-skia fixture hash is required for {stem}"
        );
        assert!(
            snapshot.matches_hash(golden).expect("compare fixture hash"),
            "editor fixture hash changed for {stem}"
        );
    }

    #[test]
    fn dual_pane_fixture_renders_headlessly_in_light_and_dark() {
        assert_fixture_hash(EditorFixture::DualPane, &Theme::Light, "editor-dual-light");
        assert_fixture_hash(EditorFixture::DualPane, &Theme::Dark, "editor-dual-dark");
    }

    #[test]
    fn same_document_two_views_fixture_remains_a_separate_surface() {
        assert_fixture_hash(
            EditorFixture::SameDocumentTwoViews,
            &Theme::Light,
            "editor-same-document-two-views-light",
        );
    }
}
