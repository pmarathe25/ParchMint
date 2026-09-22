use iced::{
    Font, Size,
    advanced::text::{Paragraph, Renderer, Text, Wrapping},
    font,
};

use crate::{
    CardItem, HierarchyRowKind,
    design_tokens::{UI_BODY, UI_HEADING},
    project_workspace::CARDS_ROW_GAP,
};

pub(crate) const CARD_FONT: Font = Font::with_name("Source Sans 3");
pub(crate) const SCROLLBAR_GUTTER: f32 = 12.0;

pub(crate) fn column_count(_width: f32) -> usize {
    1
}

pub(crate) fn fields_width(width: f32, has_metadata: bool) -> f32 {
    let inner = (width - 24.0).max(1.0);
    if has_metadata && width >= 560.0 {
        (inner - 20.0) / 2.0
    } else {
        inner
    }
}

impl CardItem<'_> {
    pub(crate) fn grid_indent(&self, width: f32) -> f32 {
        (self.depth as f32 * 32.0).min(width * 0.3)
    }

    pub(crate) fn grid_width(&self, width: f32, columns: usize) -> f32 {
        let available = width - self.grid_indent(width);
        if self.kind == HierarchyRowKind::Group {
            available
        } else {
            (available - (columns - 1) as f32 * 12.0) / columns as f32
        }
    }

    pub(crate) fn title_font(&self) -> Font {
        Font {
            weight: font::Weight::Semibold,
            ..CARD_FONT
        }
    }

    pub(crate) fn title_size(&self) -> u32 {
        u32::from(if self.kind == HierarchyRowKind::Group {
            UI_HEADING.size
        } else {
            UI_BODY.size
        })
    }

    fn text_width(&self, width: f32) -> f32 {
        (width - 24.0).max(1.0)
    }

    pub(crate) fn synopsis_height(&self, width: f32) -> f32 {
        (text_height(
            self.synopsis,
            (fields_width(width, !self.editable_metadata.is_empty()) - 6.0).max(1.0),
            14,
            20.0,
            CARD_FONT,
        ) + 4.0)
            .max(24.0)
    }

    pub(crate) fn row_height(&self, width: f32) -> f32 {
        let group = self.kind == HierarchyRowKind::Group;
        let text_width = self.text_width(width);
        let controls_width = if group { 36.0 } else { 6.0 }
            + measured_text(
                &crate::components::word_count_label(self.words),
                f32::INFINITY,
                12,
                15.6,
                CARD_FONT,
            )
            .width;
        let mut height = 24.0
            + text_height(
                self.title,
                (text_width - controls_width).max(1.0),
                self.title_size(),
                24.0,
                self.title_font(),
            )
            .max(24.0);
        if !group || self.expanded {
            let synopsis = self.synopsis_height(width);
            let field_width = fields_width(width, !self.editable_metadata.is_empty());
            let metadata = self
                .editable_metadata
                .iter()
                .map(|(label, value)| {
                    metadata_height(value, (field_width - 96.0).max(30.0))
                        .max(text_height(label, 88.0, 12, 15.6, CARD_FONT) + 3.0)
                        + 4.0
                })
                .sum::<f32>()
                .max(4.0)
                - 4.0;
            height += 6.0
                + if width >= 560.0 {
                    synopsis.max(metadata)
                } else {
                    synopsis + if metadata > 0.0 { 4.0 + metadata } else { 0.0 }
                };
        }
        height.ceil() + CARDS_ROW_GAP
    }
}

pub(crate) fn text_height(
    content: &str,
    width: f32,
    size: u32,
    line_height: f32,
    font: Font,
) -> f32 {
    measured_text(content, width, size, line_height, font).height
}

fn measured_text(content: &str, width: f32, size: u32, line_height: f32, font: Font) -> Size {
    let paragraph = <iced::Renderer as Renderer>::Paragraph::with_text(Text {
        content,
        bounds: Size::new(width, f32::INFINITY),
        size: (size as f32).into(),
        line_height: iced::Pixels(line_height).into(),
        font,
        align_x: Default::default(),
        align_y: iced::alignment::Vertical::Top,
        shaping: Default::default(),
        wrapping: Wrapping::WordOrGlyph,
    });
    Size::new(paragraph.min_width(), paragraph.min_height())
}

pub(crate) fn metadata_height(value: &str, width: f32) -> f32 {
    (text_height(value, (width - 6.0).max(1.0), 13, 18.0, CARD_FONT) + 4.0).max(22.0)
}
