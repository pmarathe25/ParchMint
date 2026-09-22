//! Supplied pencil artwork and symbolic fallbacks shared by Iced surfaces.

use iced::widget::{Svg, svg, svg::Handle};
use parchmint_design_system::production_icon_svg;

/// A product icon backed by pencil artwork or the design-system catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Icon {
    Project,
    Rename,
    Move,
    Save,
    Bold,
    Italic,
    Underline,
    NewDocument,
    NewGroup,
    NewProject,
    OpenProject,
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
    Close,
    PreviousMatch,
    NextMatch,
    MatchCase,
    WholeWords,
    ReorderGrip,
    LastOpened,
    Strikethrough,
}

impl Icon {
    const fn catalog_name(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::PreviousMatch => "previous-match",
            Self::NextMatch => "next-match",
            Self::MatchCase => "match-case",
            Self::WholeWords => "whole-words",
            Self::ReorderGrip => "reorder-grip",
            Self::LastOpened => "last-opened-clock",
            Self::Strikethrough => "strikethrough",

            Self::Rename => "rename",
            Self::Move => "move",
            Self::Save => "save",
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Underline => "underline",
            Self::NewDocument => "new-document",
            Self::NewGroup => "new-group",
            Self::NewProject => "new-project",
            Self::OpenProject => "open-project",
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
pub(crate) fn icon_sized<'a>(icon: Icon, size: u16) -> Svg<'a> {
    static HANDLES: std::sync::OnceLock<std::collections::HashMap<&'static str, Handle>> =
        std::sync::OnceLock::new();
    let handles = HANDLES.get_or_init(|| {
        [
            (
                "align-left",
                Handle::from_memory(include_bytes!("../assets/pencil/align-left.svg").as_slice()),
            ),
            (
                "align-center",
                Handle::from_memory(include_bytes!("../assets/pencil/align-center.svg").as_slice()),
            ),
            (
                "align-right",
                Handle::from_memory(include_bytes!("../assets/pencil/align-right.svg").as_slice()),
            ),
            (
                "align-justify",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/align-justify.svg").as_slice(),
                ),
            ),
            (
                "line-spacing",
                Handle::from_memory(include_bytes!("../assets/pencil/line-spacing.svg").as_slice()),
            ),
            (
                "block-quote",
                Handle::from_memory(include_bytes!("../assets/pencil/block-quote.svg").as_slice()),
            ),
            (
                "chevron-down",
                Handle::from_memory(include_bytes!("../assets/pencil/chevron-down.svg").as_slice()),
            ),
            (
                "chevron-right",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/chevron-right.svg").as_slice(),
                ),
            ),
            (
                "sidebar-toggle",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/sidebar-toggle.svg").as_slice(),
                ),
            ),
            (
                "page-break-menu",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/page-break-menu.svg").as_slice(),
                ),
            ),
            (
                "exit-focus",
                Handle::from_memory(include_bytes!("../assets/pencil/exit-focus.svg").as_slice()),
            ),
            (
                "close",
                Handle::from_memory(include_bytes!("../assets/pencil/close.svg").as_slice()),
            ),
            (
                "previous-match",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/previous-match.svg").as_slice(),
                ),
            ),
            (
                "next-match",
                Handle::from_memory(include_bytes!("../assets/pencil/next-match.svg").as_slice()),
            ),
            (
                "match-case",
                Handle::from_memory(include_bytes!("../assets/pencil/match-case.svg").as_slice()),
            ),
            (
                "whole-words",
                Handle::from_memory(include_bytes!("../assets/pencil/whole-words.svg").as_slice()),
            ),
            (
                "reorder-grip",
                Handle::from_memory(include_bytes!("../assets/pencil/reorder-grip.svg").as_slice()),
            ),
            (
                "last-opened-clock",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/last-opened-clock.svg").as_slice(),
                ),
            ),
            (
                "strikethrough",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/strikethrough.svg").as_slice(),
                ),
            ),
            (
                "project-workspace",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/project-workspace.svg").as_slice(),
                ),
            ),
            (
                "editor",
                Handle::from_memory(include_bytes!("../assets/pencil/editor.svg").as_slice()),
            ),
            (
                "overview-cards",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/overview-cards.svg").as_slice(),
                ),
            ),
            (
                "history",
                Handle::from_memory(include_bytes!("../assets/pencil/history.svg").as_slice()),
            ),
            (
                "recently-deleted",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/recently-deleted.svg").as_slice(),
                ),
            ),
            (
                "export",
                Handle::from_memory(include_bytes!("../assets/pencil/export.svg").as_slice()),
            ),
            (
                "settings",
                Handle::from_memory(include_bytes!("../assets/pencil/settings.svg").as_slice()),
            ),
            (
                "bulleted-list",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/bulleted-list.svg").as_slice(),
                ),
            ),
            (
                "numbered-list",
                Handle::from_memory(
                    include_bytes!("../assets/pencil/numbered-list.svg").as_slice(),
                ),
            ),
            (
                "search",
                Handle::from_memory(include_bytes!("../assets/pencil/search.svg").as_slice()),
            ),
            (
                "link",
                Handle::from_memory(include_bytes!("../assets/pencil/link.svg").as_slice()),
            ),
            (
                "split-pane",
                Handle::from_memory(include_bytes!("../assets/pencil/split-pane.svg").as_slice()),
            ),
            (
                "comments",
                Handle::from_memory(include_bytes!("../assets/pencil/comments.svg").as_slice()),
            ),
            (
                "focus-mode",
                Handle::from_memory(include_bytes!("../assets/pencil/focus-mode.svg").as_slice()),
            ),
            (
                "bold",
                Handle::from_memory(include_bytes!("../assets/pencil/bold.svg").as_slice()),
            ),
            (
                "italic",
                Handle::from_memory(include_bytes!("../assets/pencil/italic.svg").as_slice()),
            ),
            (
                "underline",
                Handle::from_memory(include_bytes!("../assets/pencil/underline.svg").as_slice()),
            ),
            (
                "new-document",
                Handle::from_memory(include_bytes!("../assets/pencil/new-document.svg").as_slice()),
            ),
            (
                "new-group",
                Handle::from_memory(include_bytes!("../assets/pencil/new-group.svg").as_slice()),
            ),
            (
                "new-project",
                Handle::from_memory(include_bytes!("../assets/pencil/new-project.svg").as_slice()),
            ),
            (
                "open-project",
                Handle::from_memory(include_bytes!("../assets/pencil/open-project.svg").as_slice()),
            ),
            (
                "rename",
                Handle::from_memory(include_bytes!("../assets/pencil/rename.svg").as_slice()),
            ),
            (
                "move",
                Handle::from_memory(include_bytes!("../assets/pencil/move.svg").as_slice()),
            ),
            (
                "save",
                Handle::from_memory(include_bytes!("../assets/pencil/save.svg").as_slice()),
            ),
        ]
        .into_iter()
        .collect()
    });
    let pencil = match icon {
        Icon::Project => Some("project-workspace"),
        Icon::Editor => Some("editor"),
        Icon::Overview => Some("overview-cards"),
        Icon::History => Some("history"),
        Icon::RecentlyDeleted => Some("recently-deleted"),
        Icon::Export => Some("export"),
        Icon::Settings => Some("settings"),
        Icon::BulletedList => Some("bulleted-list"),
        Icon::NumberedList => Some("numbered-list"),
        Icon::Search => Some("search"),
        Icon::Link => Some("link"),
        Icon::SplitEditor => Some("split-pane"),
        Icon::Comment => Some("comments"),
        Icon::FocusWriting => Some("focus-mode"),
        Icon::Rename => Some("rename"),
        Icon::Move => Some("move"),
        Icon::Save => Some("save"),
        Icon::Bold => Some("bold"),
        Icon::Italic => Some("italic"),
        Icon::Underline => Some("underline"),
        Icon::NewDocument => Some("new-document"),
        Icon::NewGroup => Some("new-group"),
        Icon::NewProject => Some("new-project"),
        Icon::OpenProject => Some("open-project"),
        Icon::AlignLeft => Some("align-left"),
        Icon::AlignCenter => Some("align-center"),
        Icon::AlignRight => Some("align-right"),
        Icon::AlignJustify => Some("align-justify"),
        Icon::LineSpacing => Some("line-spacing"),
        Icon::BlockQuote => Some("block-quote"),
        Icon::ChevronDown => Some("chevron-down"),
        Icon::ChevronRight => Some("chevron-right"),
        Icon::ExplorerPane => Some("sidebar-toggle"),
        Icon::PageBreak => Some("page-break-menu"),
        Icon::RestoreLayout => Some("exit-focus"),
        Icon::Close => Some("close"),
        Icon::PreviousMatch => Some("previous-match"),
        Icon::NextMatch => Some("next-match"),
        Icon::MatchCase => Some("match-case"),
        Icon::WholeWords => Some("whole-words"),
        Icon::ReorderGrip => Some("reorder-grip"),
        Icon::LastOpened => Some("last-opened-clock"),
        Icon::Strikethrough => Some("strikethrough"),
    };
    let handle = pencil.map(|name| handles[name].clone()).unwrap_or_else(|| {
        Handle::from_memory(
            production_icon_svg(icon.catalog_name())
                .expect("checked-in symbolic icon")
                .as_bytes(),
        )
    });
    svg(handle)
        .width(f32::from(size))
        .height(f32::from(size))
        .style(move |theme: &iced::Theme, _| iced::widget::svg::Style {
            color: if pencil.is_some() {
                Some(crate::components::presentation(theme).palette().pencil_icon)
            } else {
                Some(theme.palette().text)
            },
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
