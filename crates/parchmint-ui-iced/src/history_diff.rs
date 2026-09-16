//! Aligned History comparisons with inline change highlighting.

use crate::{
    HistoryComparison, HistoryComparisonLineKind, HistoryComparisonSpanKind,
    HistoryComparisonTextLine, design_tokens::ParchMintTheme,
    iced_project_surface::ProjectSurfaceMessage,
};
use iced::advanced::{
    Layout, Widget, layout, mouse, renderer,
    widget::{Operation, Tree, tree},
};
use iced::widget::{Space, column, container, rich_text, row, span, text};
use iced::{Background, Element, Length, Rectangle, Size};

pub(crate) fn view(
    change: &HistoryComparison,
    theme: ParchMintTheme,
) -> Element<'static, ProjectSurfaceMessage> {
    let summary = change.change_summary();
    let counts = [
        (summary.added_lines, "added"),
        (summary.removed_lines, "removed"),
        (summary.modified_lines, "modified"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, label)| format!("{count} {label} line{}", if count == 1 { "" } else { "s" }))
    .collect::<Vec<_>>()
    .join(" · ");
    let mut content = column![
        text(change.document_title.clone()).size(15),
        text(if counts.is_empty() {
            "No text changes".to_owned()
        } else {
            counts
        })
        .size(12)
        .color(theme.palette().secondary_text),
        row![header("Saved version", theme), header("Current", theme)].spacing(12),
    ]
    .spacing(6);
    if change.lines.is_empty() {
        content = content.push(text("Empty document").size(13));
    }
    for line in &change.lines {
        let changed = line.kind != HistoryComparisonLineKind::Unchanged;
        content = content.push(
            row![
                cell(line.before.as_ref(), changed, false, theme),
                cell(line.after.as_ref(), changed, true, theme),
            ]
            .spacing(12),
        );
    }
    content.into()
}

fn header(label: &'static str, theme: ParchMintTheme) -> Element<'static, ProjectSurfaceMessage> {
    container(text(label).size(12).color(theme.palette().secondary_text))
        .padding([6, 8])
        .width(Length::FillPortion(1))
        .into()
}

fn cell(
    line: Option<&HistoryComparisonTextLine>,
    changed: bool,
    current: bool,
    theme: ParchMintTheme,
) -> Element<'static, ProjectSurfaceMessage> {
    let Some(line) = line else {
        return Space::new().width(Length::FillPortion(1)).into();
    };
    let marker = if !changed {
        " "
    } else if current {
        "+"
    } else {
        "−"
    };
    let spans = styled_spans(line, theme);
    let body = spans
        .iter()
        .map(|span| span.text.as_ref())
        .collect::<String>();
    let rendered = rich_text(spans)
        .size(14)
        .line_height(iced::Pixels(21.0))
        .wrapping(text::Wrapping::WordOrGlyph)
        .width(Length::Fill);
    container(
        row![
            text(format!("{marker} {}", line.line_number))
                .size(11)
                .line_height(iced::Pixels(21.0))
                .color(theme.palette().secondary_text)
                .width(38),
            Element::new(DiffText {
                rendered: rendered.into(),
                body
            }),
        ]
        .spacing(6),
    )
    .padding([5, 8])
    .width(Length::FillPortion(1))
    .style(move |_| iced::widget::container::Style {
        background: changed.then_some(Background::Color(if current {
            theme.palette().success_subtle
        } else {
            theme.palette().error_subtle
        })),
        ..Default::default()
    })
    .into()
}

fn styled_spans(
    line: &HistoryComparisonTextLine,
    theme: ParchMintTheme,
) -> Vec<text::Span<'static>> {
    line.spans
        .iter()
        .map(|part| {
            let content = span(part.text.clone());
            match part.kind {
                HistoryComparisonSpanKind::Unchanged => content,
                HistoryComparisonSpanKind::Added => {
                    content.background(theme.palette().success.scale_alpha(0.25))
                }
                HistoryComparisonSpanKind::Removed => {
                    content.background(theme.palette().error.scale_alpha(0.25))
                }
            }
        })
        .collect()
}

// Iced Rich does not expose its text to widget operations as plain Text does.
struct DiffText<'a> {
    rendered: Element<'a, ProjectSurfaceMessage>,
    body: String,
}

impl Widget<ProjectSurfaceMessage, iced::Theme, iced::Renderer> for DiffText<'_> {
    fn tag(&self) -> tree::Tag {
        self.rendered.as_widget().tag()
    }
    fn state(&self) -> tree::State {
        self.rendered.as_widget().state()
    }
    fn children(&self) -> Vec<Tree> {
        self.rendered.as_widget().children()
    }
    fn diff(&self, tree: &mut Tree) {
        self.rendered.as_widget().diff(tree);
    }
    fn size(&self) -> Size<Length> {
        self.rendered.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.rendered.as_widget_mut().layout(tree, renderer, limits)
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.rendered
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.text(None, layout.bounds(), &self.body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_workspace::compare_history_text;
    use iced::Settings;
    use iced_test::Simulator;
    use parchmint_preferences::ResolvedAppearance;

    #[test]
    fn changed_words_use_only_background_highlights_in_both_themes() {
        let comparison =
            compare_history_text("saved", "Chapter", "The blue house.", "The green house.");
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            let before = styled_spans(comparison.lines[0].before.as_ref().unwrap(), theme);
            let after = styled_spans(comparison.lines[0].after.as_ref().unwrap(), theme);
            assert!(before[0].highlight.is_none() && before[2].highlight.is_none());
            assert!(after[0].highlight.is_none() && after[2].highlight.is_none());
            assert!(before[1].highlight.is_some());
            assert!(after[1].highlight.is_some());
            assert_ne!(before[1].highlight, after[1].highlight);
            assert!(
                before
                    .iter()
                    .chain(&after)
                    .all(|span| !span.underline && !span.strikethrough)
            );
        }
    }

    #[test]
    fn comparison_columns_align_wrapped_and_one_sided_rows() {
        let long = "The old road wound past the fields and through the forest. ".repeat(8);
        let before = format!("Removed opening.\nAnchor\n{long}\nOld ending.\nFinal anchor");
        let after = "Anchor\nA short road.\nNew ending.\nFinal anchor\nAdded ending.";
        let comparison = compare_history_text("saved", "Chapter", &before, after);
        for width in [480.0, 960.0] {
            let mut simulator = Simulator::with_size(
                Settings::default(),
                Size::new(width, 4000.0),
                view(&comparison, ParchMintTheme::new(ResolvedAppearance::Light)),
            );
            let saved = simulator.find(long.as_str()).unwrap().bounds();
            let current = simulator.find("A short road.").unwrap().bounds();
            let next_saved = simulator.find("Old ending.").unwrap().bounds();
            let next_current = simulator.find("New ending.").unwrap().bounds();
            assert_eq!(saved.y, current.y);
            assert!(saved.height > current.height);
            assert_eq!(next_saved.y, next_current.y);
            assert!(next_saved.y >= saved.y + saved.height);
            assert!(saved.x + saved.width < current.x);
            assert!(current.x + current.width <= width);
            assert_eq!(
                simulator.find("Removed opening.").unwrap().bounds().x,
                saved.x
            );
            assert_eq!(
                simulator.find("Added ending.").unwrap().bounds().x,
                current.x
            );
        }
    }
}
