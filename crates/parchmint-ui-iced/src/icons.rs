//! Source-authored symbolic icons shared by Iced presentation surfaces.

use iced::widget::{Svg, svg, svg::Handle};
use parchmint_design_system::production_icon_svg;

/// A product icon whose vector geometry is checked into the design-system catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Icon {
    Project,
    Editor,
    Cards,
    History,
    RecentlyDeleted,
    Export,
    Settings,
    BulletedList,
    BlockQuote,
    Link,
}

impl Icon {
    const fn catalog_name(self) -> &'static str {
        match self {
            Self::Project => "workspace-project",
            Self::Editor => "workspace-editor",
            Self::Cards => "workspace-cards",
            Self::History => "workspace-history",
            Self::RecentlyDeleted => "workspace-deleted",
            Self::Export => "workspace-export",
            Self::Settings => "workspace-settings",
            Self::BulletedList => "format-bulleted-list",
            Self::BlockQuote => "format-block-quote",
            Self::Link => "format-link",
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

#[cfg(test)]
mod tests {
    use super::Icon;
    use parchmint_design_system::production_icon_svg;

    #[test]
    fn icon_kinds_resolve_to_checked_in_workspace_vectors() {
        assert_eq!(Icon::Project.catalog_name(), "workspace-project");
        assert_eq!(Icon::Editor.catalog_name(), "workspace-editor");
        assert_eq!(Icon::Cards.catalog_name(), "workspace-cards");
        assert_eq!(Icon::History.catalog_name(), "workspace-history");
        assert_eq!(Icon::RecentlyDeleted.catalog_name(), "workspace-deleted");
        assert_eq!(Icon::Export.catalog_name(), "workspace-export");
        assert_eq!(Icon::Settings.catalog_name(), "workspace-settings");
        assert_eq!(Icon::BulletedList.catalog_name(), "format-bulleted-list");
        assert_eq!(Icon::BlockQuote.catalog_name(), "format-block-quote");
        assert_eq!(Icon::Link.catalog_name(), "format-link");

        for icon_kind in [
            Icon::Project,
            Icon::Editor,
            Icon::Cards,
            Icon::History,
            Icon::RecentlyDeleted,
            Icon::Export,
            Icon::Settings,
            Icon::BulletedList,
            Icon::BlockQuote,
            Icon::Link,
        ] {
            assert!(production_icon_svg(icon_kind.catalog_name()).is_some());
        }
    }
}
