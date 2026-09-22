//! Source-authored symbolic icons shared by Iced presentation surfaces.

use iced::widget::{Svg, svg, svg::Handle};
use parchmint_design_system::production_icon_svg;

/// A product icon whose vector geometry is checked into the design-system catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Icon {
    Project,
    Editor,
    Overview,
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignJustify,
    LineSpacing,
    History,
    RecentlyDeleted,
    Export,
    Settings,
    BulletedList,
    NumberedList,
    Search,
    ChevronDown,
    ChevronRight,
    BlockQuote,
    Link,
    SplitEditor,
    ExplorerPane,
    PageBreak,
    Comment,
    FocusWriting,
    RestoreLayout,
}

impl Icon {
    const fn catalog_name(self) -> &'static str {
        match self {
            Self::AlignLeft => "format-align-left",
            Self::AlignCenter => "format-align-center",
            Self::AlignRight => "format-align-right",
            Self::AlignJustify => "format-align-justify",
            Self::LineSpacing => "format-line-spacing",
            Self::Editor => "workspace-editor",
            Self::Overview => "workspace-cards",
            Self::Project => "workspace-project",
            Self::History => "workspace-history",
            Self::RecentlyDeleted => "workspace-deleted",
            Self::Export => "workspace-export",
            Self::Settings => "workspace-settings",
            Self::Search => "workspace-search",
            Self::ChevronDown => "chevron-down",
            Self::ChevronRight => "chevron-right",
            Self::BulletedList => "format-bulleted-list",
            Self::NumberedList => "format-numbered-list",
            Self::BlockQuote => "format-block-quote",
            Self::Link => "format-link",
            Self::SplitEditor => "editor-split",
            Self::ExplorerPane => "explorer-pane",
            Self::PageBreak => "format-page-break",
            Self::Comment => "editor-comment",
            Self::FocusWriting => "workspace-focus",
            Self::RestoreLayout => "workspace-restore-layout",
        }
    }
}

/// Builds a 20 px symbolic SVG. Callers can override its dimensions and style.
pub(crate) fn icon(icon: Icon) -> Svg<'static> {
    icon_sized(icon, 20)
}

/// Builds a symbolic SVG at a square size in logical pixels.
pub(crate) fn icon_sized(icon: Icon, size: u16) -> Svg<'static> {
    let source = production_icon_svg(icon.catalog_name())
        .expect("workspace icon is checked into the design-system catalog");

    svg(Handle::from_memory(source.as_bytes()))
        .width(f32::from(size))
        .height(f32::from(size))
        .style(|theme: &iced::Theme, _| iced::widget::svg::Style {
            color: Some(theme.palette().text),
        })
}

/// Transparent vector typewriter drawn from the supplied ParchMint artwork.
pub(crate) fn brand<'a>(size: u16) -> Svg<'a> {
    svg(Handle::from_memory(
        include_bytes!("../assets/parchmint-brand.svg").as_slice(),
    ))
    .width(u32::from(size))
    .height(u32::from(size))
}

#[cfg(test)]
mod tests {
    use super::Icon;
    use parchmint_design_system::production_icon_svg;

    #[test]
    fn icon_kinds_resolve_to_checked_in_workspace_vectors() {
        assert_eq!(Icon::Project.catalog_name(), "workspace-project");
        assert_eq!(Icon::History.catalog_name(), "workspace-history");
        assert_eq!(Icon::RecentlyDeleted.catalog_name(), "workspace-deleted");
        assert_eq!(Icon::Export.catalog_name(), "workspace-export");
        assert_eq!(Icon::Settings.catalog_name(), "workspace-settings");
        assert_eq!(Icon::BulletedList.catalog_name(), "format-bulleted-list");
        assert_eq!(Icon::BlockQuote.catalog_name(), "format-block-quote");
        assert_eq!(Icon::Link.catalog_name(), "format-link");

        for icon_kind in [
            Icon::Project,
            Icon::History,
            Icon::RecentlyDeleted,
            Icon::Export,
            Icon::Settings,
            Icon::BulletedList,
            Icon::Search,
            Icon::ChevronDown,
            Icon::ChevronRight,
            Icon::BlockQuote,
            Icon::Link,
            Icon::SplitEditor,
            Icon::ExplorerPane,
            Icon::PageBreak,
            Icon::Comment,
            Icon::FocusWriting,
            Icon::RestoreLayout,
        ] {
            assert!(production_icon_svg(icon_kind.catalog_name()).is_some());
        }
    }
}
