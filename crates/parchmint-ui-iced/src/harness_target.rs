//! Stable, non-visual targets used by the headless interaction harness.
//!
//! Iced's current test selector can query widget IDs but not accessible names
//! or roles. These IDs preserve the production widget tree and let the harness
//! synthesize real pointer and keyboard input without depending on geometry.

use iced::{
    Element,
    widget::{Id, container},
};

use crate::{EditorPane, RibbonDestination};

/// A stable target in the production desktop surface.
///
/// Keys identify controls, never author-visible titles, so renaming content or
/// translating labels does not invalidate an interaction workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessTarget {
    Ribbon(RibbonDestination),
    ExplorerSearch,
    ExplorerRename,
    CardsList,
    HistoryTimeline,
    HistoryComparison,
    GlobalSearchQuery,
    GlobalReplacement,
    GlobalReplacementReview,
    ModalCancel,
    ModalConfirm,
    EditorPrimary,
    EditorCompanion,
    InspectorTitle,
    InspectorSynopsis,
    MetadataFieldName,
    AddComment,
    CommentDraft,
    CommentEdit,
    CommentReply,
    Bold,
    ParagraphStyle,
    FontFamily,
    FontSize,
    ManageStyles,
    ManageMetadata,
    ListMenu,
    ListBulleted,
    ListNumbered,
    OverviewAdd,
    BreakMenu,
    LocalFind(EditorPane),
    LocalReplace(EditorPane),
    TabOverflow(EditorPane),
    NewTab(EditorPane),
    DraftTitle,
    ExportBrowse,
    ExportStart,
    ExportTitles,
    StyleInheritance,
    ToggleCompanion,
    PaneFocus(EditorPane),
    ToggleExplorer,
    ToggleInspector,
    CommentMenu(usize),
}

impl HarnessTarget {
    pub(crate) fn id(self) -> Id {
        Id::new(match self {
            Self::Ribbon(RibbonDestination::Editor) => "harness.ribbon.editor",
            Self::Ribbon(RibbonDestination::Cards) => "harness.ribbon.cards",
            Self::Ribbon(RibbonDestination::History) => "harness.ribbon.history",
            Self::Ribbon(RibbonDestination::RecentlyDeleted) => "harness.ribbon.recently-deleted",
            Self::Ribbon(RibbonDestination::Export) => "harness.ribbon.export",
            Self::Ribbon(RibbonDestination::Settings) => "harness.ribbon.settings",
            Self::Ribbon(RibbonDestination::GlobalSearch) => "harness.ribbon.global-search",
            Self::ExplorerSearch => "harness.explorer.search",
            Self::ExplorerRename => "harness.explorer.rename",
            Self::CardsList => "harness.cards.list",
            Self::HistoryTimeline => "harness.history.timeline",
            Self::HistoryComparison => "harness.history.comparison",
            Self::GlobalSearchQuery => "global-search-query",
            Self::GlobalReplacement => "global-search-replacement",
            Self::GlobalReplacementReview => "harness.global-search.review-replacement",
            Self::ModalCancel => "parchmint-focus-modal-cancel",
            Self::ModalConfirm => "parchmint-focus-modal-confirm",
            Self::EditorPrimary => "harness.editor.primary",
            Self::EditorCompanion => "harness.editor.companion",
            Self::InspectorTitle => "harness.inspector.title",
            Self::InspectorSynopsis => "harness.inspector.synopsis",
            Self::MetadataFieldName => "harness.settings.metadata-field-name",
            Self::AddComment => "harness.editor.add-comment",
            Self::CommentDraft => "harness.comment.draft",
            Self::CommentEdit => "harness.comment.edit",
            Self::CommentReply => "harness.comment.reply",
            Self::Bold => "harness.editor.bold",
            Self::ParagraphStyle => "harness.editor.paragraph-style",
            Self::FontFamily => "harness.editor.font-family",
            Self::FontSize => "harness.editor.font-size",
            Self::ManageStyles => "harness.editor.manage-styles",
            Self::ManageMetadata => "harness.overview.manage-metadata",
            Self::ListBulleted => "list-bulleted",
            Self::ListNumbered => "list-numbered",
            Self::OverviewAdd => "overview-add",
            Self::ListMenu => "harness.editor.list-menu",
            Self::BreakMenu => "harness.editor.break-menu",
            Self::LocalFind(EditorPane::Primary) => "harness.local-find.primary",
            Self::LocalFind(EditorPane::Companion) => "harness.local-find.companion",
            Self::LocalReplace(EditorPane::Primary) => "harness.local-replace.primary",
            Self::LocalReplace(EditorPane::Companion) => "harness.local-replace.companion",
            Self::TabOverflow(EditorPane::Primary) => "harness.tab-overflow.primary",
            Self::TabOverflow(EditorPane::Companion) => "harness.tab-overflow.companion",
            Self::NewTab(EditorPane::Primary) => "harness.new-tab.primary",
            Self::NewTab(EditorPane::Companion) => "harness.new-tab.companion",
            Self::DraftTitle => "harness.draft.title",
            Self::ExportBrowse => "harness.export.browse",
            Self::ExportStart => "harness.export.start",
            Self::ExportTitles => "harness.export.titles",
            Self::StyleInheritance => "harness.settings.style-inheritance",
            Self::ToggleCompanion => "harness.toggle-companion",
            Self::PaneFocus(EditorPane::Primary) => "harness.pane-focus.primary",
            Self::PaneFocus(EditorPane::Companion) => "harness.pane-focus.companion",
            Self::ToggleExplorer => "harness.toggle-explorer",
            Self::ToggleInspector => "harness.toggle-inspector",
            Self::CommentMenu(index) => return format!("harness.comment.menu.{index}").into(),
        })
    }
}

/// Preserves a control's layout and behavior while giving it a stable test ID.
pub(crate) fn target<'a, Message>(
    target: HarnessTarget,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message>
where
    Message: 'a,
{
    target_id(target.id(), content)
}

/// Preserves a control's layout and behavior while giving it a dynamic stable
/// test ID derived from a serialized domain node ID.
pub(crate) fn target_id<'a, Message>(
    id: Id,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message>
where
    Message: 'a,
{
    container(content).id(id).into()
}

pub(crate) fn explorer_row_id(node_id: &str) -> Id {
    format!("harness.explorer.node.{node_id}").into()
}

pub(crate) fn card_id(node_id: &str) -> Id {
    format!("harness.cards.node.{node_id}").into()
}

pub(crate) fn card_disclosure_id(node_id: &str) -> Id {
    format!("harness.cards.disclosure.{node_id}").into()
}

pub(crate) fn history_checkpoint_id(checkpoint_id: &str) -> Id {
    format!("harness.history.checkpoint.{checkpoint_id}").into()
}

/// Identifies one rendered tab without depending on its author-visible title.
pub(crate) fn editor_tab_id(pane: EditorPane, document_id: &str) -> Id {
    let pane = match pane {
        EditorPane::Primary => "primary",
        EditorPane::Companion => "companion",
    };
    format!("harness.editor.tab.{pane}.{document_id}").into()
}
