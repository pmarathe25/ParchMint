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

pub(crate) const GROUP_GAP: f32 = 8.0;

pub(crate) fn grid_indent(depth: usize, width: f32) -> f32 {
    (depth as f32 * 24.0).min(width * 0.3)
}

pub(crate) const CARD_HEIGHT: f32 = 112.0;
pub(crate) const ADD_HEIGHT: f32 = 88.0;
pub(crate) const CARD_FONT: Font = Font::with_name("Source Sans 3");
pub(crate) const SCROLLBAR_GUTTER: f32 = 12.0;

pub(crate) fn column_count(width: f32) -> usize {
    ((width + 12.0) / 236.0).floor().max(1.0) as usize
}

impl CardItem<'_> {
    pub(crate) fn grid_indent(&self, width: f32) -> f32 {
        grid_indent(self.depth, width)
    }

    pub(crate) fn grid_width(&self, width: f32, columns: usize) -> f32 {
        let available = width - 2.0 * self.grid_indent(width);
        if self.kind == HierarchyRowKind::Group && self.expanded {
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
        u32::from(if self.kind == HierarchyRowKind::Group && self.expanded {
            UI_HEADING.size
        } else {
            UI_BODY.size
        })
    }

    pub(crate) fn heading_height(&self, width: f32) -> f32 {
        if self.details_expanded {
            text_height(
                self.title,
                (width - 144.0).max(40.0),
                self.title_size(),
                24.0,
                self.title_font(),
            )
            .max(24.0)
        } else {
            24.0
        }
    }

    pub(crate) fn metadata_width(&self, width: f32) -> f32 {
        metadata_width(
            width,
            self.kind == HierarchyRowKind::Group && self.expanded,
            metadata_columns(
                width,
                self.kind == HierarchyRowKind::Group && self.expanded,
                self.editable_metadata.len(),
            ),
        )
    }

    pub(crate) fn synopsis_height(&self, width: f32) -> f32 {
        let metadata = if self.kind == HierarchyRowKind::Group
            && self.expanded
            && !self.editable_metadata.is_empty()
        {
            let columns = metadata_columns(width, true, self.editable_metadata.len());
            self.metadata_width(width) * columns as f32 + (columns - 1) as f32 * 8.0 + 12.0
        } else {
            0.0
        };
        let height = (text_height(
            self.synopsis,
            (width - 30.0 - metadata).max(1.0),
            14,
            20.0,
            CARD_FONT,
        ) + 4.0)
            .max(24.0);
        if self.details_expanded {
            height
        } else {
            height.min(64.0)
        }
    }

    pub(crate) fn needs_expansion(&self, width: f32) -> bool {
        if self.kind == HierarchyRowKind::Group {
            return false;
        }
        if self.has_hidden_metadata {
            return true;
        }
        if self.editable_metadata.len() > 3 {
            return true;
        }
        let field_width = self.metadata_width(width);
        if self
            .editable_metadata
            .iter()
            .any(|(_, value)| metadata_height(value, field_width) > 22.0)
        {
            return true;
        }
        text_height(self.synopsis, (width - 30.0).max(1.0), 14, 20.0, CARD_FONT) + 4.0 > 64.0
            || text_height(
                self.title,
                (width - 144.0).max(40.0),
                self.title_size(),
                24.0,
                self.title_font(),
            ) > 24.0
    }

    pub(crate) fn row_height(&self, width: f32) -> f32 {
        let field_width = self.metadata_width(width);
        let columns = metadata_columns(
            width,
            self.kind == HierarchyRowKind::Group && self.expanded,
            self.editable_metadata.len(),
        );
        let metadata = self
            .editable_metadata
            .chunks(columns)
            .map(|fields| {
                fields
                    .iter()
                    .map(|(label, value)| {
                        text_height(label, field_width, 12, 15.6, CARD_FONT).max(16.0)
                            + if self.details_expanded {
                                metadata_height(value, field_width)
                            } else {
                                22.0
                            }
                            + 2.0
                    })
                    .fold(0.0, f32::max)
            })
            .sum::<f32>()
            + self
                .editable_metadata
                .len()
                .div_ceil(columns)
                .saturating_sub(1) as f32
                * 6.0;
        let synopsis = self.synopsis_height(width)
            + if self.kind == HierarchyRowKind::Group
                && self.expanded
                && !self.editable_metadata.is_empty()
            {
                18.0
            } else {
                0.0
            };
        let body = if self.kind == HierarchyRowKind::Group && self.expanded {
            synopsis.max(metadata)
        } else {
            synopsis
                + if self.editable_metadata.is_empty() {
                    0.0
                } else {
                    8.0 + metadata
                }
        };
        let natural = 24.0 + self.heading_height(width) + 4.0 + body;
        let height = if self.kind == HierarchyRowKind::Group && self.expanded {
            natural
        } else {
            natural.max(CARD_HEIGHT)
        };
        height.ceil() + CARDS_ROW_GAP
    }
}

pub(crate) fn metadata_columns(width: f32, group: bool, fields: usize) -> usize {
    if group {
        ((width / 360.0).floor() as usize)
            .clamp(1, 3)
            .min(fields.max(1))
    } else {
        (((width - 16.0) / 88.0).floor() as usize)
            .clamp(1, 3)
            .min(fields.max(1))
    }
}

pub(crate) fn metadata_width(width: f32, group: bool, columns: usize) -> f32 {
    if group {
        (width * 0.32).clamp(88.0, 180.0)
    } else {
        ((width - 24.0 - (columns - 1) as f32 * 8.0) / columns as f32).max(1.0)
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
    use std::{
        cell::RefCell,
        collections::HashMap,
        hash::{Hash, Hasher},
    };
    thread_local! {
        static CACHE: RefCell<HashMap<u64, Size>> = RefCell::new(HashMap::new());
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (content, width.to_bits(), size, line_height.to_bits(), font).hash(&mut hasher);
    let key = hasher.finish();
    if let Some(size) = CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return size;
    }
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
    let size = Size::new(paragraph.min_width(), paragraph.min_height());
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 8192 {
            cache.clear();
        }
        cache.insert(key, size);
    });
    size
}

pub(crate) fn metadata_height(value: &str, width: f32) -> f32 {
    (text_height(value, (width - 6.0).max(1.0), 13, 18.0, CARD_FONT) + 4.0).max(22.0)
}
