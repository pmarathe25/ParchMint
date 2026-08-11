//! Reusable Iced composition for the editor center region.
//!
//! The project surface owns surrounding chrome. This module owns only the
//! formatting toolbar, tab strips, local find controls, and mounted prose
//! canvases that belong in the editor center.

use std::collections::BTreeMap;

use iced::widget::{
    Space, button, column, container, mouse_area, pick_list, row, sensor, stack, text, text_input,
    tooltip,
};
use iced::{
    Element, Font, Length,
    alignment::{Horizontal, Vertical},
};
use parchmint_editor_api::{EditorError, ViewId};
use parchmint_editor_iced::{
    EditorViewport, MountedEditorHost, MountedEditorMessage, MountedEditorUpdate,
};

use crate::{
    EditorMessage, EditorPane, EditorPaneState, EditorWorkspace, FindDirection, FormattingCommand,
    LocalSearchState, SpellingMenu, SpellingMenuAction, TabSpec,
    components::{self, ButtonKind, Interaction, Surface},
    design_tokens::{COMPACT_CONTROL_HEIGHT, ParchMintTheme},
};

/// The non-document state a center pane can render while an editor host is unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditorCenterPaneState {
    Empty,
    Loading,
    Error(String),
    /// Deterministic prose used only by the headless visual-verification
    /// catalog when a real mounted editor host is unavailable.
    VerificationProse {
        heading: &'static str,
        paragraphs: &'static [&'static str],
    },
}

/// A caller-owned slot for one workspace pane.
#[derive(Clone)]
pub(crate) enum EditorPaneSlot {
    Mounted {
        host: MountedEditorHost,
        replace_draft: String,
    },
    State(EditorCenterPaneState),
}

impl EditorPaneSlot {
    pub(crate) fn mounted(host: MountedEditorHost) -> Self {
        Self::Mounted {
            host,
            replace_draft: String::new(),
        }
    }

    pub(crate) fn state(state: EditorCenterPaneState) -> Self {
        Self::State(state)
    }

    fn host(&self) -> Option<&MountedEditorHost> {
        match self {
            Self::Mounted { host, .. } => Some(host),
            Self::State(_) => None,
        }
    }

    fn replace_draft(&self) -> &str {
        match self {
            Self::Mounted { replace_draft, .. } => replace_draft,
            Self::State(_) => "",
        }
    }

    fn render_state(&self) -> Option<&EditorCenterPaneState> {
        match self {
            Self::Mounted { .. } => None,
            Self::State(state) => Some(state),
        }
    }
}

/// Caller-owned mounted host mapping. The renderer never creates, remounts,
/// or drops a shared editor session on its own.
#[derive(Default)]
pub(crate) struct EditorHostSlots {
    slots: BTreeMap<EditorPane, EditorPaneSlot>,
}

impl EditorHostSlots {
    pub(crate) fn insert(&mut self, pane: EditorPane, slot: EditorPaneSlot) {
        self.slots.insert(pane, slot);
    }

    pub(crate) fn remove(&mut self, pane: EditorPane) {
        self.slots.remove(&pane);
    }

    pub(crate) fn slot(&self, pane: EditorPane) -> Option<&EditorPaneSlot> {
        self.slots.get(&pane)
    }

    pub(crate) fn set_replace_draft(&mut self, pane: EditorPane, value: String) {
        if let Some(EditorPaneSlot::Mounted { replace_draft, .. }) = self.slots.get_mut(&pane) {
            *replace_draft = value;
        }
    }

    /// Routes a mounted-surface message only when its pane and view identity
    /// still match the caller's retained host.
    pub(crate) fn update_mounted(
        &self,
        pane: EditorPane,
        view: ViewId,
        message: MountedEditorMessage,
    ) -> Result<MountedEditorUpdate, EditorError> {
        let Some(host) = self.slot(pane).and_then(EditorPaneSlot::host) else {
            return Err(EditorError::InvalidCommand {
                reason: "mounted editor message has no host slot",
            });
        };
        if host.config().view() != view {
            return Err(EditorError::InvalidCommand {
                reason: "mounted editor message view does not match host slot",
            });
        }
        host.update(message)
    }
}

/// Typed output from the center surface. The native integration routes the
/// workspace and mounted messages through their existing owners.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EditorCenterMessage {
    BeginSplitResize,
    HierarchyDropTarget(EditorPane),
    CommitHierarchyDrop,
    Workspace(EditorMessage),
    PaneWorkspace {
        pane: EditorPane,
        message: EditorMessage,
    },
    Mounted {
        pane: EditorPane,
        view: ViewId,
        message: MountedEditorMessage,
    },
    SetReplaceDraft {
        pane: EditorPane,
        value: String,
    },
    ChooseSpellingAction(SpellingMenuAction),
    DismissSpellingMenu,
}

impl EditorCenterMessage {
    /// Resolves the editor-workspace reducer messages required by a center
    /// control. Pane-local controls focus their pane before changing its
    /// local search state; mounted messages also establish that focus.
    pub(crate) fn workspace_messages(&self) -> Vec<EditorMessage> {
        match self {
            Self::BeginSplitResize | Self::HierarchyDropTarget(_) | Self::CommitHierarchyDrop => {
                Vec::new()
            }
            Self::Workspace(message) => vec![message.clone()],
            Self::PaneWorkspace { pane, message } => {
                vec![EditorMessage::FocusPane(*pane), message.clone()]
            }
            Self::Mounted {
                message: MountedEditorMessage::ViewportChanged(_),
                ..
            } => Vec::new(),
            Self::Mounted { pane, .. } => vec![EditorMessage::FocusPane(*pane)],
            Self::SetReplaceDraft { .. } => Vec::new(),
            Self::ChooseSpellingAction(_) | Self::DismissSpellingMenu => Vec::new(),
        }
    }
}

/// Composes only the editor-center region. Explorer, Inspector, ribbon, and
/// status bar remain separate project-surface responsibilities.
pub(crate) fn editor_center_surface(
    workspace: &EditorWorkspace,
    theme: ParchMintTheme,
    slots: &EditorHostSlots,
    spelling_menu: Option<&SpellingMenu>,
) -> Element<'static, EditorCenterMessage> {
    let toolbar = formatting_toolbar(workspace, theme);
    let primary = editor_pane_surface(workspace, EditorPane::Primary, theme, slots);
    let companion_visible = workspace.pane(EditorPane::Companion).is_populated()
        || slots.slot(EditorPane::Companion).is_some();
    let panes: Element<'static, EditorCenterMessage> = if companion_visible {
        let primary_portion = (workspace.split_ratio() * 1000.0).round() as u16;
        let companion_portion = 1000_u16.saturating_sub(primary_portion);
        row![
            container(primary).width(Length::FillPortion(primary_portion)),
            mouse_area(
                container(Space::new().width(1).height(Length::Fill))
                    .width(8)
                    .height(Length::Fill)
                    .style(move |_| components::surface(
                        theme,
                        Surface::Elevated,
                        Interaction::Rest,
                    )),
            )
            .on_press(EditorCenterMessage::BeginSplitResize)
            .interaction(iced::mouse::Interaction::ResizingHorizontally),
            container(editor_pane_surface(
                workspace,
                EditorPane::Companion,
                theme,
                slots
            ))
            .width(Length::FillPortion(companion_portion)),
        ]
        .spacing(0)
        .height(Length::Fill)
        .into()
    } else {
        container(primary).width(Length::Fill).into()
    };

    let center = container(column![toolbar, panes].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([6, 12])
        .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest));
    let mut layers = stack![center].width(Length::Fill).height(Length::Fill);
    if workspace.link_editor().is_open() {
        layers = layers.push(
            container(link_editor_popover(workspace, theme))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding([52, 12])
                .align_x(Horizontal::Center)
                .align_y(Vertical::Top),
        );
    }
    if let Some(menu) = spelling_menu {
        layers = layers.push(
            container(spelling_menu_popover(menu, theme))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding([82, 24])
                .align_x(Horizontal::Center)
                .align_y(Vertical::Top),
        );
    }
    layers.into()
}

fn spelling_menu_popover(
    menu: &SpellingMenu,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let actions = menu.actions().iter().cloned().fold(
        column![text(menu.word().to_owned()).size(13)],
        |column, action| {
            let label = match &action {
                SpellingMenuAction::AddComment => "Add Comment".to_owned(),
                SpellingMenuAction::Replace(value) => value.clone(),
                SpellingMenuAction::AddToDictionary(scope) => match scope {
                    crate::SpellingDictionaryScope::Project => {
                        "Add to Project Dictionary".to_owned()
                    }
                    crate::SpellingDictionaryScope::Global => "Add to Global Dictionary".to_owned(),
                },
                SpellingMenuAction::RemoveFromDictionary(scope) => match scope {
                    crate::SpellingDictionaryScope::Project => {
                        "Remove from Project Dictionary".to_owned()
                    }
                    crate::SpellingDictionaryScope::Global => {
                        "Remove from Global Dictionary".to_owned()
                    }
                },
                SpellingMenuAction::Ignore => "Ignore".to_owned(),
            };
            column.push(
                button(text(label).size(12))
                    .padding([5, 8])
                    .on_press(EditorCenterMessage::ChooseSpellingAction(action))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            button_interaction(status, false),
                        )
                    }),
            )
        },
    );
    container(
        actions
            .push(
                button(text("Dismiss").size(12))
                    .padding([5, 8])
                    .on_press(EditorCenterMessage::DismissSpellingMenu)
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            button_interaction(status, false),
                        )
                    }),
            )
            .spacing(3),
    )
    .padding(10)
    .width(280)
    .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Focused))
    .into()
}

fn formatting_toolbar(
    workspace: &EditorWorkspace,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let commands = [
        ("B", FormattingCommand::Bold),
        ("I", FormattingCommand::Italic),
        ("U", FormattingCommand::Underline),
        ("S", FormattingCommand::Strikethrough),
        ("Small Caps", FormattingCommand::SmallCaps),
        ("Superscript", FormattingCommand::Superscript),
        ("Subscript", FormattingCommand::Subscript),
        ("☷", FormattingCommand::BulletedList),
        ("⇣", FormattingCommand::NumberedList),
        ("❞", FormattingCommand::BlockQuote),
        ("Link", FormattingCommand::Link),
        ("Scene Break", FormattingCommand::SceneBreak),
        ("Page Break", FormattingCommand::PageBreak),
    ];
    let style_selector = pick_list(
        workspace.style_names().to_vec(),
        Some(workspace.active_style().to_owned()),
        |style| {
            EditorCenterMessage::Workspace(EditorMessage::Format(
                FormattingCommand::ParagraphStyle(style),
            ))
        },
    )
    .placeholder("Paragraph style")
    .width(160);
    let controls = commands.into_iter().fold(
        row![
            button(text("Undo").size(12))
                .padding([5, 7])
                .height(u32::from(COMPACT_CONTROL_HEIGHT))
                .on_press(EditorCenterMessage::Workspace(EditorMessage::Undo))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    button_interaction(status, false),
                )),
            button(text("Redo").size(12))
                .padding([5, 7])
                .height(u32::from(COMPACT_CONTROL_HEIGHT))
                .on_press(EditorCenterMessage::Workspace(EditorMessage::Redo))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    button_interaction(status, false),
                )),
            button(text("Save").size(12))
                .padding([5, 7])
                .height(u32::from(COMPACT_CONTROL_HEIGHT))
                .on_press(EditorCenterMessage::Workspace(EditorMessage::Save))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    button_interaction(status, false),
                )),
            button(text("Find").size(12))
                .padding([5, 7])
                .height(u32::from(COMPACT_CONTROL_HEIGHT))
                .on_press(EditorCenterMessage::Workspace(EditorMessage::OpenLocalFind))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    button_interaction(status, false),
                )),
            style_selector
        ]
        .spacing(4),
        |row, (label, command)| {
            row.push(
                button(text(label).size(12))
                    .padding([5, 7])
                    .height(u32::from(COMPACT_CONTROL_HEIGHT))
                    .on_press(EditorCenterMessage::Workspace(EditorMessage::Format(
                        command,
                    )))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            button_interaction(status, false),
                        )
                    }),
            )
        },
    );
    container(controls.wrap().vertical_spacing(4))
        .padding([6, 8])
        .width(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Rest))
        .into()
}

fn link_editor_popover(
    workspace: &EditorWorkspace,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let link_editor = workspace.link_editor();
    let target = link_editor.target().to_owned();
    let url_input = text_input("https://example.com", &target)
        .on_input(|target| EditorCenterMessage::Workspace(EditorMessage::SetLinkTarget(target)))
        .padding([6, 8])
        .style(move |_, status| components::field_style(theme, field_interaction(status)));
    let mut content = column![
        text("Link destination").size(14),
        text("URL").size(12),
        url_input,
    ]
    .spacing(6);
    if let Some(error) = link_editor.validation_error() {
        content = content.push(text(error.to_owned()).size(12));
    }
    content = content.push(
        row![
            button(text("Apply Link").size(12))
                .padding([5, 7])
                .on_press(EditorCenterMessage::Workspace(EditorMessage::ApplyLink))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Primary,
                    button_interaction(status, false),
                )),
            button(text("Remove Link").size(12))
                .padding([5, 7])
                .on_press(EditorCenterMessage::Workspace(EditorMessage::RemoveLink))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Secondary,
                    button_interaction(status, false),
                )),
            button(text("Cancel").size(12))
                .padding([5, 7])
                .on_press(EditorCenterMessage::Workspace(
                    EditorMessage::CancelLinkEditor
                ))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    button_interaction(status, false),
                )),
        ]
        .spacing(4),
    );
    container(content)
        .padding(12)
        .width(360)
        .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Focused))
        .into()
}

fn editor_pane_surface(
    workspace: &EditorWorkspace,
    pane: EditorPane,
    theme: ParchMintTheme,
    slots: &EditorHostSlots,
) -> Element<'static, EditorCenterMessage> {
    let state = workspace.pane(pane);
    let tabs = tab_strip(
        state,
        pane,
        workspace.focused_pane() == pane,
        slots
            .slot(pane)
            .and_then(EditorPaneSlot::host)
            .map_or(1.0, |host| host.viewport().width),
        workspace.tab_drag_source(pane),
        workspace.tab_drag_target(pane),
        theme,
    );
    let search = local_search_bar(workspace.local_search(state.view()), pane, theme, slots);
    let view = state.view();
    let viewport_message = move |size: iced::Size| EditorCenterMessage::Mounted {
        pane,
        view,
        message: MountedEditorMessage::ViewportChanged(
            EditorViewport::new(size.width.max(1.0), size.height.max(1.0))
                .expect("sensor clamps editor viewport dimensions"),
        ),
    };
    let body = sensor(pane_body(state, pane, theme, slots))
        .key((pane, view))
        .on_show(viewport_message)
        .on_resize(viewport_message);
    mouse_area(
        container(column![tabs, search, body].spacing(6))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest)),
    )
    .on_enter(EditorCenterMessage::HierarchyDropTarget(pane))
    .on_release(EditorCenterMessage::CommitHierarchyDrop)
    .into()
}

fn tab_strip(
    state: &EditorPaneState,
    pane: EditorPane,
    focused: bool,
    measured_width: f32,
    drag_source: Option<&str>,
    drag_target: Option<usize>,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let active = state.active_document();
    let layout =
        EditorWorkspace::tab_strip_layout(measured_width, state.tabs(), active.unwrap_or_default());
    let tab_count = state.tabs().len();
    let tabs = state.tabs().iter().zip(layout.tabs()).enumerate().fold(
        row![].spacing(2),
        |row, (index, (tab, presentation))| {
            row.push(tab_button(
                tab,
                presentation,
                TabButtonContext {
                    pane,
                    index,
                    count: tab_count,
                    focused,
                    drag_source: drag_source == Some(tab.id()),
                    drag_target: drag_target == Some(index),
                    theme,
                },
            ))
        },
    );
    mouse_area(
        container(tabs)
            .padding([0, 6])
            .width(Length::Fill)
            .height(32)
            .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
    )
    .on_release(EditorCenterMessage::Workspace(EditorMessage::CommitTabDrag))
    .into()
}

#[derive(Debug, Clone, Copy)]
struct TabButtonContext {
    pane: EditorPane,
    index: usize,
    count: usize,
    focused: bool,
    drag_source: bool,
    drag_target: bool,
    theme: ParchMintTheme,
}

fn tab_button(
    tab: &TabSpec,
    presentation: &crate::TabLayout,
    context: TabButtonContext,
) -> Element<'static, EditorCenterMessage> {
    let TabButtonContext {
        pane,
        index,
        count: tab_count,
        focused,
        drag_source,
        drag_target,
        theme,
    } = context;
    let id = tab.id().to_owned();
    let active = presentation.is_active();
    let title = if tab.is_dirty() {
        format!("{} •", presentation.display_title())
    } else {
        presentation.display_title().to_owned()
    };
    let activate: Element<'static, EditorCenterMessage> = button(text(title).size(13))
        .padding([6, 8])
        .width(Length::Fill)
        .on_press(EditorCenterMessage::Workspace(EditorMessage::ActivateTab {
            pane,
            document_id: id.clone(),
        }))
        .style(move |_, status| {
            components::button_style(
                theme,
                ButtonKind::Tab,
                tab_interaction(status, active, focused),
            )
        })
        .into();
    let activate = if let Some(full_title) = presentation.tooltip() {
        tooltip(
            activate,
            container(text(full_title.to_owned()).size(12)).padding([4, 6]),
            tooltip::Position::Bottom,
        )
        .into()
    } else {
        activate
    };
    let drag_handle = tooltip(
        mouse_area(container(text("⠿").size(12)).padding([5, 3]))
            .on_press(EditorCenterMessage::Workspace(
                EditorMessage::BeginTabDrag {
                    pane,
                    document_id: id.clone(),
                },
            ))
            .interaction(iced::mouse::Interaction::Grab),
        container(text(format!("Reorder {}", presentation.full_title())).size(12)).padding([4, 6]),
        tooltip::Position::Bottom,
    );
    let mut actions = row![drag_handle].spacing(1);
    if index > 0 {
        actions = actions.push(button(text("←").size(11)).on_press(
            EditorCenterMessage::Workspace(EditorMessage::MoveTab {
                pane,
                document_id: id.clone(),
                target_index: index - 1,
            }),
        ));
    }
    if index + 1 < tab_count {
        actions = actions.push(button(text("→").size(11)).on_press(
            EditorCenterMessage::Workspace(EditorMessage::MoveTab {
                pane,
                document_id: id.clone(),
                target_index: index + 1,
            }),
        ));
    }
    let close = button(text("×").size(14))
        .padding([5, 7])
        .width(32)
        .on_press(EditorCenterMessage::Workspace(EditorMessage::CloseTab {
            pane,
            document_id: id.clone(),
        }))
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, button_interaction(status, false))
        });
    let close: Element<'static, EditorCenterMessage> = tooltip(
        close,
        container(text(format!("Close {}", presentation.full_title())).size(12)).padding([4, 6]),
        tooltip::Position::Bottom,
    )
    .into();
    mouse_area(
        container(row![activate, actions, close].width(Length::Fill))
            .width(Length::FillPortion(1))
            .max_width(200)
            .style(move |_| {
                components::surface(
                    theme,
                    Surface::Panel,
                    if drag_target && !drag_source {
                        Interaction::Selected
                    } else if drag_source {
                        Interaction::Focused
                    } else {
                        Interaction::Rest
                    },
                )
            }),
    )
    .on_enter(EditorCenterMessage::Workspace(
        EditorMessage::SetTabDragTarget {
            pane,
            target_index: index,
        },
    ))
    .interaction(if drag_source {
        iced::mouse::Interaction::Grabbing
    } else {
        iced::mouse::Interaction::Pointer
    })
    .into()
}

fn local_search_bar(
    search: &LocalSearchState,
    pane: EditorPane,
    theme: ParchMintTheme,
    slots: &EditorHostSlots,
) -> Element<'static, EditorCenterMessage> {
    if !search.is_open() {
        // The collapsed Find controls do not reserve a visual row. The
        // command remains reachable through the native command routing; when
        // opened, the full local Find/Replace surface is inserted here.
        return Space::new().height(0).into();
    }

    let draft = slots
        .slot(pane)
        .map(EditorPaneSlot::replace_draft)
        .unwrap_or_default()
        .to_owned();
    let query_value = search.query().to_owned();
    let matches = search.matches().len();
    let case_sensitive = search.case_sensitive();
    let whole_word = search.whole_word();
    let replace_visible = search.replace_visible();
    let query = text_input("Find", &query_value)
        .on_input(move |query| EditorCenterMessage::PaneWorkspace {
            pane,
            message: EditorMessage::SetFindQuery(query),
        })
        .padding([5, 8])
        .style(move |_, status| components::field_style(theme, field_interaction(status)));
    let controls = row![
        query,
        find_button(
            "Previous",
            pane,
            EditorMessage::NavigateFind(FindDirection::Previous),
            theme
        ),
        find_button(
            "Next",
            pane,
            EditorMessage::NavigateFind(FindDirection::Next),
            theme
        ),
        button(text(if case_sensitive { "Aa ✓" } else { "Aa" }).size(12))
            .padding([5, 7])
            .on_press(EditorCenterMessage::PaneWorkspace {
                pane,
                message: EditorMessage::SetFindOptions {
                    case_sensitive: !case_sensitive,
                    whole_word,
                },
            })
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                button_interaction(status, case_sensitive),
            )),
        button(text(if whole_word { "Whole ✓" } else { "Whole" }).size(12))
            .padding([5, 7])
            .on_press(EditorCenterMessage::PaneWorkspace {
                pane,
                message: EditorMessage::SetFindOptions {
                    case_sensitive,
                    whole_word: !whole_word,
                },
            })
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                button_interaction(status, whole_word),
            )),
        button(
            text(if replace_visible {
                "Hide replace"
            } else {
                "Replace"
            })
            .size(12)
        )
        .padding([5, 7])
        .on_press(EditorCenterMessage::PaneWorkspace {
            pane,
            message: EditorMessage::SetReplaceVisible(!replace_visible),
        })
        .style(move |_, status| components::button_style(
            theme,
            ButtonKind::Quiet,
            button_interaction(status, replace_visible),
        )),
        button(text("Close").size(12))
            .padding([5, 7])
            .on_press(EditorCenterMessage::PaneWorkspace {
                pane,
                message: EditorMessage::CloseLocalFind,
            })
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                button_interaction(status, false),
            )),
    ]
    .spacing(4);
    let content = if replace_visible {
        column![
            controls,
            row![
                text_input("Replace with", &draft)
                    .on_input(move |value| EditorCenterMessage::SetReplaceDraft { pane, value })
                    .padding([5, 8])
                    .style(move |_, status| components::field_style(
                        theme,
                        field_interaction(status)
                    )),
                find_button(
                    "Replace",
                    pane,
                    EditorMessage::ReplaceActiveMatch(draft.clone()),
                    theme,
                ),
                find_button(
                    "Replace all",
                    pane,
                    EditorMessage::ReplaceAllMatches(draft.clone()),
                    theme,
                ),
            ]
            .spacing(4),
        ]
        .spacing(4)
    } else {
        column![controls].spacing(4)
    };
    container(column![content, text(format!("{matches} matches")).size(12)].spacing(3))
        .padding([4, 6])
        .width(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Rest))
        .into()
}

fn find_button(
    label: &'static str,
    pane: EditorPane,
    message: EditorMessage,
    theme: ParchMintTheme,
) -> iced::widget::Button<'static, EditorCenterMessage> {
    button(text(label).size(12))
        .padding([5, 7])
        .on_press(EditorCenterMessage::PaneWorkspace { pane, message })
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, button_interaction(status, false))
        })
}

fn pane_body(
    state: &EditorPaneState,
    pane: EditorPane,
    theme: ParchMintTheme,
    slots: &EditorHostSlots,
) -> Element<'static, EditorCenterMessage> {
    let fallback = match slots.slot(pane).and_then(EditorPaneSlot::render_state) {
        Some(EditorCenterPaneState::Empty) => state_center(
            "No document open",
            "Open a document to begin writing.",
            theme,
        ),
        Some(EditorCenterPaneState::Loading) => {
            state_center("Loading document", "Preparing the editor surface.", theme)
        }
        Some(EditorCenterPaneState::Error(error)) => {
            state_center("Document unavailable", error, theme)
        }
        Some(EditorCenterPaneState::VerificationProse {
            heading,
            paragraphs,
        }) => verification_prose_center(heading, paragraphs, theme),
        None if !state.is_populated() => state_center(
            "No document open",
            "Open a document to begin writing.",
            theme,
        ),
        None => state_center("Loading document", "Preparing the editor surface.", theme),
    };
    slots
        .slot(pane)
        .and_then(EditorPaneSlot::host)
        .map(|host| {
            let view = state.view();
            host.element()
                .map(move |message| EditorCenterMessage::Mounted {
                    pane,
                    view,
                    message,
                })
        })
        .unwrap_or(fallback)
}

/// A clearly isolated visual-catalog stand-in for a mounted prose canvas.
///
/// Native windows always render `MountedEditorHost`; this is intentionally
/// available only through `visual_verification::editor_slots` so the capture
/// catalog can retain representative manuscript density while editor mounting
/// is asynchronous.
fn verification_prose_center(
    heading: &'static str,
    paragraphs: &'static [&'static str],
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let prose = paragraphs.iter().fold(
        column![
            text(heading)
                .size(24)
                .font(Font::with_name("Source Serif 4"))
        ]
        .spacing(20),
        |column, paragraph| {
            column.push(
                text(*paragraph)
                    .size(20)
                    .font(Font::with_name("Source Serif 4")),
            )
        },
    );
    container(prose)
        .padding([45, 40])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest))
        .into()
}

fn state_center(
    title: &'static str,
    detail: &str,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    container(column![text(title).size(16), text(detail.to_owned()).size(13)].spacing(6))
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest))
        .into()
}

fn button_interaction(status: iced::widget::button::Status, selected: bool) -> Interaction {
    match status {
        iced::widget::button::Status::Active if selected => Interaction::Selected,
        iced::widget::button::Status::Active => Interaction::Rest,
        iced::widget::button::Status::Hovered => Interaction::Hovered,
        iced::widget::button::Status::Pressed => Interaction::Pressed,
        iced::widget::button::Status::Disabled => Interaction::Disabled,
    }
}

fn tab_interaction(
    status: iced::widget::button::Status,
    active: bool,
    pane_focused: bool,
) -> Interaction {
    match status {
        iced::widget::button::Status::Active if active && pane_focused => Interaction::Selected,
        iced::widget::button::Status::Active if active => Interaction::Focused,
        iced::widget::button::Status::Active => Interaction::Rest,
        iced::widget::button::Status::Hovered => Interaction::Hovered,
        iced::widget::button::Status::Pressed => Interaction::Pressed,
        iced::widget::button::Status::Disabled => Interaction::Disabled,
    }
}

fn field_interaction(status: iced::widget::text_input::Status) -> Interaction {
    match status {
        iced::widget::text_input::Status::Active => Interaction::Rest,
        iced::widget::text_input::Status::Hovered => Interaction::Hovered,
        iced::widget::text_input::Status::Focused { .. } => Interaction::Focused,
        iced::widget::text_input::Status::Disabled => Interaction::Disabled,
    }
}

#[cfg(test)]
mod tests {
    use iced::{Settings, Size};
    use iced_test::Simulator;
    use parchmint_preferences::ResolvedAppearance;

    use super::*;
    use crate::{EditorFixture, design_tokens::ParchMintTheme};

    #[test]
    fn pane_local_messages_focus_before_reaching_workspace_reducer() {
        let message = EditorCenterMessage::PaneWorkspace {
            pane: EditorPane::Companion,
            message: EditorMessage::OpenLocalFind,
        };
        assert_eq!(
            message.workspace_messages(),
            vec![
                EditorMessage::FocusPane(EditorPane::Companion),
                EditorMessage::OpenLocalFind,
            ]
        );
    }

    #[test]
    fn always_visible_editor_actions_reach_the_workspace_reducer() {
        for message in [
            EditorMessage::Undo,
            EditorMessage::Redo,
            EditorMessage::Save,
            EditorMessage::OpenLocalFind,
        ] {
            let center = EditorCenterMessage::Workspace(message.clone());
            assert_eq!(center.workspace_messages(), vec![message]);
        }
    }

    #[test]
    fn mounted_message_keeps_its_pane_and_view_context() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let view = workspace.pane(EditorPane::Companion).view();
        let message = EditorCenterMessage::Mounted {
            pane: EditorPane::Companion,
            view,
            message: MountedEditorMessage::InsertText("x".to_owned()),
        };
        assert_eq!(
            message.workspace_messages(),
            vec![EditorMessage::FocusPane(EditorPane::Companion)]
        );
        assert!(matches!(
            message,
            EditorCenterMessage::Mounted { pane: EditorPane::Companion, view: message_view, .. } if message_view == view
        ));
    }

    #[test]
    fn viewport_measurement_reflows_without_stealing_editor_focus() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let view = workspace.pane(EditorPane::Companion).view();
        let message = EditorCenterMessage::Mounted {
            pane: EditorPane::Companion,
            view,
            message: MountedEditorMessage::ViewportChanged(
                EditorViewport::new(480.0, 320.0).expect("viewport"),
            ),
        };
        assert!(message.workspace_messages().is_empty());
    }

    #[test]
    fn state_only_center_surfaces_render_in_both_appearances() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let mut slots = EditorHostSlots::default();
        slots.insert(
            EditorPane::Primary,
            EditorPaneSlot::state(EditorCenterPaneState::Loading),
        );
        slots.insert(
            EditorPane::Companion,
            EditorPaneSlot::state(EditorCenterPaneState::Error(
                "Editor failed to load.".to_owned(),
            )),
        );
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            let mut simulator = Simulator::with_size(
                Settings::default(),
                Size::new(960.0, 600.0),
                editor_center_surface(&workspace, theme, &slots, None),
            );
            let snapshot = simulator
                .snapshot(&theme.iced_theme())
                .expect("headless center snapshot");
            assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
        }
    }

    #[test]
    fn link_editor_popover_renders_in_both_appearances() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        workspace.update(EditorMessage::OpenLinkEditor);
        let mut slots = EditorHostSlots::default();
        slots.insert(
            EditorPane::Primary,
            EditorPaneSlot::state(EditorCenterPaneState::Loading),
        );
        slots.insert(
            EditorPane::Companion,
            EditorPaneSlot::state(EditorCenterPaneState::Loading),
        );
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            let mut simulator = Simulator::with_size(
                Settings::default(),
                Size::new(960.0, 600.0),
                editor_center_surface(&workspace, theme, &slots, None),
            );
            let snapshot = simulator
                .snapshot(&theme.iced_theme())
                .expect("headless center snapshot");
            assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
        }
    }

    #[test]
    fn tab_drag_handle_publishes_a_typed_source_without_removing_button_fallbacks() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let mut slots = EditorHostSlots::default();
        slots.insert(
            EditorPane::Primary,
            EditorPaneSlot::state(EditorCenterPaneState::Loading),
        );
        slots.insert(
            EditorPane::Companion,
            EditorPaneSlot::state(EditorCenterPaneState::Loading),
        );
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );

        simulator.click("⠿").expect("tab drag handle");

        assert!(simulator.into_messages().any(|message| matches!(
            message,
            EditorCenterMessage::Workspace(EditorMessage::BeginTabDrag {
                pane: EditorPane::Primary,
                document_id,
            }) if document_id == "chapter-one"
        )));
    }
}
