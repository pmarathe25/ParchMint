//! Reusable Iced composition for the editor center region.
//!
//! The project surface owns surrounding chrome. This module owns only the
//! formatting toolbar, tab strips, local find controls, and mounted prose
//! canvases that belong in the editor center.

use std::collections::BTreeMap;

use iced::widget::{
    Space, button, column, container, mouse_area, opaque, pick_list, row, sensor, stack, text,
    text_input,
};
use iced::{
    Background, Element, Font, Length,
    alignment::{Horizontal, Vertical},
    font,
};
use parchmint_editor_api::{EditorError, ViewId};
use parchmint_editor_iced::{
    EditorViewport, MountedEditorHost, MountedEditorMessage, MountedEditorUpdate,
};

use crate::{
    EditorMessage, EditorPane, EditorPaneState, EditorWorkspace, F6Region, FindDirection,
    FormattingCommand, HarnessTarget, LocalSearchState, SpellingMenu, SpellingMenuAction, TabSpec,
    components::{self, ButtonKind, Interaction, Surface},
    design_tokens::{COMPACT_CONTROL_HEIGHT, ParchMintTheme},
    focus, harness_target, hierarchy_drag,
    icons::{Icon, icon_sized},
    stationary_tooltip,
};

const EDITOR_TOOLBAR_CONTROL_HEIGHT: u16 = COMPACT_CONTROL_HEIGHT + 4;

/// Surrounding controls to include around a mounted manuscript pane.
///
/// The project shell selects this presentation for destinations whose center
/// is a manuscript preview rather than the authoring workspace. The mounted
/// host, viewport sensor, focus region, and message routing stay identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorCenterChrome {
    Full,
    ManuscriptOnly,
}

/// The non-document state a center pane can render while an editor host is unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditorCenterPaneState {
    Empty,
    Loading,
    Error(String),
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
    ClearHierarchyDropTarget(EditorPane),
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
            Self::BeginSplitResize
            | Self::HierarchyDropTarget(_)
            | Self::ClearHierarchyDropTarget(_)
            | Self::CommitHierarchyDrop => Vec::new(),
            Self::Workspace(message) => vec![message.clone()],
            Self::PaneWorkspace { pane, message } => {
                vec![EditorMessage::FocusPane(*pane), message.clone()]
            }
            Self::Mounted {
                message: MountedEditorMessage::Blur | MountedEditorMessage::ViewportChanged(_),
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
    editor_center_surface_with_chrome(
        workspace,
        theme,
        slots,
        spelling_menu,
        EditorCenterChrome::Full,
    )
}

/// Composes the editor center with an explicit surrounding-chrome policy.
pub(crate) fn editor_center_surface_with_chrome(
    workspace: &EditorWorkspace,
    theme: ParchMintTheme,
    slots: &EditorHostSlots,
    spelling_menu: Option<&SpellingMenu>,
    chrome: EditorCenterChrome,
) -> Element<'static, EditorCenterMessage> {
    let primary = editor_pane_surface(
        workspace,
        EditorPane::Primary,
        theme,
        slots,
        spelling_menu,
        chrome,
    );
    // A slot may outlive its mounted Canvas for the remainder of the current
    // Iced event cycle. It must not keep an otherwise empty companion pane
    // visible: closing its last tab is the explicit signal to collapse the
    // split and give the primary pane the full editor width.
    let companion_visible = workspace.pane(EditorPane::Companion).is_populated();
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
                slots,
                spelling_menu,
                chrome,
            ))
            .width(Length::FillPortion(companion_portion)),
        ]
        .spacing(0)
        .height(Length::Fill)
        .into()
    } else {
        container(primary).width(Length::Fill).into()
    };

    let center_content: Element<'static, EditorCenterMessage> = match chrome {
        EditorCenterChrome::Full => column![
            focus::f6_region(
                F6Region::FormattingToolbar,
                formatting_toolbar(workspace, theme),
            ),
            panes,
        ]
        .spacing(0)
        .into(),
        EditorCenterChrome::ManuscriptOnly => panes,
    };
    let center = container(center_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(0)
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
    layers.into()
}

fn spelling_menu_overlay(
    content: Element<'static, EditorCenterMessage>,
    menu: &SpellingMenu,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let bounds = menu.bounds();
    stack![
        content,
        mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
            .on_press(EditorCenterMessage::DismissSpellingMenu),
        container(opaque(spelling_menu_popover(menu, theme)))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                top: bounds.top(),
                right: 0.0,
                bottom: 0.0,
                left: bounds.left(),
            })
            .align_x(Horizontal::Left)
            .align_y(Vertical::Top),
    ]
    .into()
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
    container(actions.spacing(3))
        .padding(6)
        .width(menu.bounds().width())
        .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Rest))
        .into()
}

fn formatting_toolbar(
    workspace: &EditorWorkspace,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    // Keep this ordered like the production editor ribbon. Less-frequent
    // commands remain available through the existing keyboard routing;
    // putting every command in this narrow row previously made the controls
    // wrap over the pane at desktop widths.
    let text_commands = [
        ("B", FormattingCommand::Bold),
        ("I", FormattingCommand::Italic),
        ("U", FormattingCommand::Underline),
        ("S", FormattingCommand::Strikethrough),
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
    .width(118);
    let controls =
        text_commands
            .into_iter()
            .fold(row![style_selector].spacing(4), |row, (label, command)| {
                let control_font = match label {
                    "B" => Font {
                        weight: font::Weight::Bold,
                        ..Font::with_name("Source Sans 3")
                    },
                    "I" => Font {
                        style: font::Style::Italic,
                        ..Font::with_name("Source Sans 3")
                    },
                    _ => Font {
                        weight: font::Weight::Medium,
                        ..Font::with_name("Source Sans 3")
                    },
                };
                row.push(
                    button(text(label).size(14).font(control_font))
                        .padding([4, 7])
                        .height(u32::from(EDITOR_TOOLBAR_CONTROL_HEIGHT))
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
            });
    let controls = controls
        .push(formatting_icon_button(
            Icon::BulletedList,
            "Bulleted list",
            FormattingCommand::BulletedList,
            theme,
        ))
        .push(formatting_text_button(
            "1.",
            FormattingCommand::NumberedList,
            theme,
        ))
        .push(formatting_icon_button(
            Icon::BlockQuote,
            "Block quote",
            FormattingCommand::BlockQuote,
            theme,
        ))
        .push(formatting_icon_button(
            Icon::Link,
            "Link",
            FormattingCommand::Link,
            theme,
        ))
        .push(formatting_text_button(
            "Scene Break",
            FormattingCommand::SceneBreak,
            theme,
        ))
        .push(formatting_text_button(
            "Page Break",
            FormattingCommand::PageBreak,
            theme,
        ));
    container(controls)
        .padding([6, 8])
        .width(Length::Fill)
        // The Penpot toolbar is a flat panel. An elevated surface adds a
        // large scrim shadow that is repeatedly repainted while its controls
        // hover, causing the visible dark flicker across the whole bar.
        .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest))
        .into()
}

fn formatting_text_button(
    label: &'static str,
    command: FormattingCommand,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    button(text(label).size(14))
        .padding([4, 7])
        .height(u32::from(EDITOR_TOOLBAR_CONTROL_HEIGHT))
        .on_press(EditorCenterMessage::Workspace(EditorMessage::Format(
            command,
        )))
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, button_interaction(status, false))
        })
        .into()
}

fn formatting_icon_button(
    icon: Icon,
    tooltip_label: &'static str,
    command: FormattingCommand,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    stationary_tooltip::tooltip(
        button(icon_sized(icon, 16))
            .padding([4, 7])
            .height(u32::from(EDITOR_TOOLBAR_CONTROL_HEIGHT))
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
        container(text(tooltip_label).size(12)).padding([4, 6]),
        components::surface(theme, Surface::Panel, Interaction::Rest),
    )
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
    spelling_menu: Option<&SpellingMenu>,
    chrome: EditorCenterChrome,
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
    let search = workspace.local_search(state.view());
    let view = state.view();
    let viewport_message = move |size: iced::Size| EditorCenterMessage::Mounted {
        pane,
        view,
        message: MountedEditorMessage::ViewportChanged(
            EditorViewport::new(size.width.max(1.0), size.height.max(1.0))
                .expect("sensor clamps editor viewport dimensions"),
        ),
    };
    let target = match pane {
        EditorPane::Primary => HarnessTarget::EditorPrimary,
        EditorPane::Companion => HarnessTarget::EditorCompanion,
    };
    let body: Element<'static, EditorCenterMessage> = sensor(harness_target::target(
        target,
        pane_body(state, pane, theme, slots),
    ))
    .key((pane, view))
    .on_show(viewport_message)
    .on_resize(viewport_message)
    .into();
    let body = if let Some(menu) = spelling_menu.filter(|menu| menu.pane() == pane) {
        spelling_menu_overlay(body, menu, theme)
    } else {
        body
    };
    let body = if workspace.focused_pane() == pane {
        focus::f6_region(F6Region::FocusedEditor, body)
    } else {
        body
    };
    let content: Element<'static, EditorCenterMessage> = match chrome {
        EditorCenterChrome::Full if search.is_open() => {
            column![tabs, local_search_bar(search, pane, theme, slots), body]
                .spacing(6)
                .into()
        }
        EditorCenterChrome::Full => {
            // A collapsed local-find surface is command-only and must not
            // create a phantom gutter between the tab strip and manuscript
            // canvas.
            column![tabs, body].spacing(0).into()
        }
        EditorCenterChrome::ManuscriptOnly => body,
    };
    hierarchy_drag::target(
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest)),
        None,
        |bounds, point| bounds.contains(point).then_some(()),
        move |_| EditorCenterMessage::HierarchyDropTarget(pane),
        move |_| EditorCenterMessage::ClearHierarchyDropTarget(pane),
    )
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
    let tabs = layout
        .tabs()
        .iter()
        .fold(row![].spacing(2), |row, presentation| {
            let index = presentation.source_index();
            let tab = &state.tabs()[index];
            row.push(tab_button(
                tab,
                presentation,
                TabButtonContext {
                    pane,
                    index,
                    focused,
                    drag_source: drag_source == Some(tab.id()),
                    drag_target: drag_target == Some(index),
                    theme,
                },
            ))
        });
    let tabs = if layout.overflow_tabs().is_empty() {
        tabs
    } else {
        let pane_for_overflow = pane;
        tabs.push(
            pick_list(
                layout.overflow_tabs().to_vec(),
                Option::<crate::TabOverflowItem>::None,
                move |tab| {
                    EditorCenterMessage::Workspace(EditorMessage::ActivateTab {
                        pane: pane_for_overflow,
                        document_id: tab.id().to_owned(),
                    })
                },
            )
            .placeholder(format!("{} tabs", layout.overflow_tabs().len()))
            .width(58),
        )
    };
    let strip: Element<'static, EditorCenterMessage> = container(tabs)
        .padding([0, 2])
        .width(Length::Fill)
        .height(36)
        .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest))
        .into();
    if focused {
        focus::f6_region(F6Region::ActiveTab, strip)
    } else {
        strip
    }
}

#[derive(Debug, Clone, Copy)]
struct TabButtonContext {
    pane: EditorPane,
    index: usize,
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
    let mut title_font = Font {
        weight: font::Weight::Medium,
        ..Font::with_name("Source Sans 3")
    };
    if tab.is_preview() {
        title_font.style = font::Style::Italic;
    }
    let activate: Element<'static, EditorCenterMessage> =
        button(text(title).size(13).font(title_font))
            .padding([7, 8])
            .width(Length::Fill)
            .on_press(EditorCenterMessage::Workspace(EditorMessage::ActivateTab {
                pane,
                document_id: id.clone(),
            }))
            .style(move |_, status| {
                flat_tab_button_style(theme, tab_interaction(status, active, focused))
            })
            .into();
    let activate = if let Some(tooltip) = presentation.tooltip().map(str::to_owned) {
        stationary_tooltip::tooltip(
            activate,
            container(text(tooltip).size(12)).padding([4, 6]),
            components::surface(theme, Surface::Panel, Interaction::Rest),
        )
        .into()
    } else {
        activate
    };
    let close = button(text("×").size(14))
        .padding([6, 6])
        .width(presentation.close_bounds().width())
        .on_press(EditorCenterMessage::Workspace(EditorMessage::CloseTab {
            pane,
            document_id: id.clone(),
        }))
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, button_interaction(status, false))
        });
    let close: Element<'static, EditorCenterMessage> = stationary_tooltip::tooltip(
        close,
        container(text(format!("Close {}", presentation.full_title())).size(12)).padding([4, 6]),
        components::surface(theme, Surface::Panel, Interaction::Rest),
    )
    .into();
    mouse_area(
        container(
            column![
                row![activate, close]
                    .width(Length::Fill)
                    .height(Length::Fill),
                container(Space::new())
                    .height(2)
                    .width(Length::Fill)
                    .style(move |_| tab_underline_style(theme, active)),
            ]
            .spacing(0),
        )
        .width(presentation.bounds().width())
        .style(move |_| tab_container_style(theme, drag_source, drag_target)),
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

fn flat_tab_button_style(
    theme: ParchMintTheme,
    interaction: Interaction,
) -> iced::widget::button::Style {
    let palette = theme.palette();
    let background = match interaction {
        Interaction::Hovered => Some(Background::Color(palette.control_hover)),
        Interaction::Pressed => Some(Background::Color(palette.control_pressed)),
        _ => None,
    };
    iced::widget::button::Style {
        background,
        text_color: if matches!(interaction, Interaction::Rest) {
            palette.secondary_text
        } else {
            palette.primary_text
        },
        border: Default::default(),
        shadow: Default::default(),
        snap: true,
    }
}

fn tab_underline_style(theme: ParchMintTheme, active: bool) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: active.then_some(Background::Color(theme.palette().accent)),
        ..Default::default()
    }
}

fn tab_container_style(
    theme: ParchMintTheme,
    drag_source: bool,
    drag_target: bool,
) -> iced::widget::container::Style {
    if drag_target && !drag_source {
        components::surface(theme, Surface::Panel, Interaction::Selected)
    } else if drag_source {
        components::surface(theme, Surface::Panel, Interaction::Focused)
    } else {
        iced::widget::container::Style::default()
    }
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
        .id(HarnessTarget::LocalFind(pane).id())
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
                    .id(HarnessTarget::LocalReplace(pane).id())
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
    use iced::{Point, Settings, Size};
    use iced_test::Simulator;
    use parchmint_domain::DocumentId;
    use parchmint_editor_api::{
        CanonicalDocumentLoad, EditorAdapter as _, EditorCommand as AdapterEditorCommand,
        EditorCommandKind, EditorCommandOrigin,
    };
    use parchmint_editor_iced::{
        EditorIcedAdapter, EditorIcedConfig, EditorSurfaceTheme, MountedEditorBinding,
        MountedEditorBindingConfig, MountedEditorSession,
    };
    use parchmint_platform_api::WindowCapability;
    use parchmint_preferences::ResolvedAppearance;

    use super::*;
    use crate::{
        EditorFixture, FindMatch, Rect, SpellingMenuRequest, design_tokens::ParchMintTheme,
    };

    fn apply_surface_messages(
        workspace: &mut EditorWorkspace,
        slots: &mut EditorHostSlots,
        messages: impl IntoIterator<Item = EditorCenterMessage>,
    ) -> Vec<crate::EditorEffect> {
        let mut effects = Vec::new();
        for message in messages {
            match message {
                EditorCenterMessage::Workspace(message) => {
                    effects.extend(workspace.update(message))
                }
                EditorCenterMessage::PaneWorkspace { pane, message } => {
                    effects.extend(workspace.update(EditorMessage::FocusPane(pane)));
                    effects.extend(workspace.update(message));
                }
                EditorCenterMessage::Mounted {
                    pane,
                    view,
                    message,
                } => {
                    if !matches!(message, MountedEditorMessage::ViewportChanged(_)) {
                        effects.extend(workspace.update(EditorMessage::FocusPane(pane)));
                    }
                    slots
                        .update_mounted(pane, view, message)
                        .expect("rendered mounted message reaches its retained host");
                }
                EditorCenterMessage::SetReplaceDraft { pane, value } => {
                    slots.set_replace_draft(pane, value);
                }
                // The project shell owns hierarchy drag state. Pointer entry
                // and release during these editor flows legitimately publish
                // these surface-level signals without changing editor state.
                EditorCenterMessage::HierarchyDropTarget(_)
                | EditorCenterMessage::ClearHierarchyDropTarget(_)
                | EditorCenterMessage::CommitHierarchyDrop => {}
                unsupported @ (EditorCenterMessage::BeginSplitResize
                | EditorCenterMessage::ChooseSpellingAction(_)
                | EditorCenterMessage::DismissSpellingMenu) => {
                    panic!(
                        "the editor flow fixture does not model this center message: {unsupported:?}"
                    );
                }
            }
        }
        effects
    }

    fn shared_document_slots(
        workspace: &EditorWorkspace,
    ) -> (
        EditorIcedAdapter,
        parchmint_editor_api::SharedEditorSession,
        EditorHostSlots,
    ) {
        let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("test adapter");
        let document = DocumentId::from_bytes([88; 16]);
        let viewport = EditorViewport::new(460.0, 480.0).expect("test viewport");
        let primary_view = workspace.pane(EditorPane::Primary).view();
        let primary = MountedEditorBinding::mount(
            &adapter,
            MountedEditorBindingConfig::new(
                MountedEditorSession::Open(CanonicalDocumentLoad::new(document, "river river")),
                WindowCapability::new(81, 1),
                primary_view,
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("primary host mounts");
        let session = primary.session();
        let companion = MountedEditorBinding::mount(
            &adapter,
            MountedEditorBindingConfig::new(
                MountedEditorSession::Reuse(session.clone()),
                WindowCapability::new(81, 1),
                workspace.pane(EditorPane::Companion).view(),
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("companion host joins the document session");
        let mut slots = EditorHostSlots::default();
        slots.insert(
            EditorPane::Primary,
            EditorPaneSlot::mounted(primary.host().clone()),
        );
        slots.insert(
            EditorPane::Companion,
            EditorPaneSlot::mounted(companion.host().clone()),
        );
        (adapter, session, slots)
    }

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
    fn command_routing_remains_available_to_the_workspace_reducer() {
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
    fn spelling_menu_popover_emits_add_comment_without_dismissing_it() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let effects = workspace.update(EditorMessage::OpenSpellingMenu(
            SpellingMenuRequest::new(
                EditorPane::Primary,
                "Comment",
                Rect::new(100.0, 100.0, 1.0, 18.0),
                Rect::new(0.0, 0.0, 500.0, 400.0),
            )
            .with_spelling_actions(false),
        ));
        let [crate::EditorEffect::ShowSpellingMenu(menu)] = effects.as_slice() else {
            panic!("expected the comment context menu")
        };
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
            editor_center_surface(&workspace, theme, &slots, Some(menu)),
        );

        simulator
            .click("Add Comment")
            .expect("comment action target");

        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            [EditorCenterMessage::ChooseSpellingAction(
                SpellingMenuAction::AddComment
            )]
        );
    }

    #[test]
    fn normal_tabs_do_not_expose_an_implementation_drag_affordance() {
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

        assert!(simulator.click("⠿").is_err());
    }

    #[test]
    fn clicking_a_rendered_tab_close_removes_it_without_committing_a_drag() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
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

        simulator.click("×").expect("primary tab close target");
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(messages.iter().any(|message| matches!(
            message,
            EditorCenterMessage::Workspace(EditorMessage::CloseTab {
                pane: EditorPane::Primary,
                document_id,
            }) if document_id == "chapter-one"
        )));
        assert!(!messages.iter().any(|message| matches!(
            message,
            EditorCenterMessage::Workspace(EditorMessage::CommitTabDrag)
        )));
        apply_surface_messages(&mut workspace, &mut slots, messages);
        assert!(
            workspace
                .pane(EditorPane::Primary)
                .tabs()
                .iter()
                .all(|tab| tab.id() != "chapter-one")
        );
    }

    #[test]
    fn rendered_panes_switch_toolbar_and_inspector_targets_with_focus() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let (_, _, mut slots) = shared_document_slots(&workspace);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator
            .click("Chapter Two")
            .expect("rendered companion tab");
        assert!(
            apply_surface_messages(&mut workspace, &mut slots, simulator.into_messages(),)
                .is_empty()
        );
        assert_eq!(workspace.focused_pane(), EditorPane::Companion);
        assert_eq!(
            workspace.inspector_context(),
            &crate::InspectorContext::Document {
                document_id: "chapter-two".to_owned(),
            }
        );

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator.click("B").expect("rendered bold toolbar button");
        assert!(matches!(
            apply_surface_messages(&mut workspace, &mut slots, simulator.into_messages()).as_slice(),
            [crate::EditorEffect::Command {
                view,
                command: crate::EditorCommand::ToggleBold,
            }] if *view == workspace.pane(EditorPane::Companion).view()
        ));

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator
            .click("Chapter One")
            .expect("rendered primary tab");
        apply_surface_messages(&mut workspace, &mut slots, simulator.into_messages());
        assert_eq!(workspace.focused_pane(), EditorPane::Primary);
        assert_eq!(
            workspace.inspector_context(),
            &crate::InspectorContext::Document {
                document_id: "chapter-one".to_owned(),
            }
        );
    }

    #[test]
    fn rendered_shared_document_edit_and_undo_preserve_view_local_state() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::SameDocumentTwoViews);
        let (adapter, session, mut slots) = shared_document_slots(&workspace);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let primary_view = workspace.pane(EditorPane::Primary).view();
        let companion_view = workspace.pane(EditorPane::Companion).view();

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator.point_at(Point::new(120.0, 140.0));
        let click_statuses = simulator.simulate(iced_test::simulator::click());
        assert!(click_statuses.contains(&iced::event::Status::Captured));
        assert_eq!(simulator.typewrite("X"), iced::event::Status::Captured);
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(messages.iter().any(|message| matches!(
            message,
            EditorCenterMessage::Mounted {
                pane: EditorPane::Primary,
                message: MountedEditorMessage::InsertText(value),
                ..
            } if value == "X"
        )));
        apply_surface_messages(&mut workspace, &mut slots, messages);
        assert_eq!(workspace.focused_pane(), EditorPane::Primary);
        assert_eq!(
            adapter
                .revision(session.clone())
                .expect("shared revision")
                .value(),
            1
        );
        assert_ne!(
            adapter
                .selection(session.clone(), primary_view)
                .expect("primary selection"),
            adapter
                .selection(session.clone(), companion_view)
                .expect("companion selection")
        );

        workspace.update(EditorMessage::FocusPane(EditorPane::Companion));
        workspace.update(EditorMessage::OpenLocalFind);
        workspace.update(EditorMessage::SetFindQuery("river".to_owned()));
        workspace.update(EditorMessage::SetFindMatches(vec![FindMatch::new(0, 5)]));
        workspace.update(EditorMessage::SetSelectionWordCount {
            pane: EditorPane::Companion,
            words: Some(2),
        });
        assert!(!workspace.local_search(primary_view).is_open());
        assert_eq!(workspace.local_search(companion_view).query(), "river");
        assert_eq!(
            workspace.status_bar().current_count(),
            crate::StatusCount::Selection(2)
        );

        workspace.update(EditorMessage::FocusPane(EditorPane::Primary));
        let undo = workspace.update(EditorMessage::Undo);
        let [
            crate::EditorEffect::Command {
                view,
                command: crate::EditorCommand::Undo,
            },
        ] = undo.as_slice()
        else {
            panic!("focused primary undo must reach the shared editor session");
        };
        adapter
            .execute(
                session.clone(),
                EditorCommandOrigin::new(*view),
                AdapterEditorCommand::new(
                    adapter
                        .revision(session.clone())
                        .expect("revision before undo"),
                    EditorCommandKind::Undo,
                ),
            )
            .expect("undo applies to the shared session");
        assert_eq!(
            adapter
                .primary_visible_block(session)
                .expect("shared primary block")
                .text(),
            "river river"
        );
    }

    #[test]
    fn rendered_local_replace_controls_stay_scoped_to_the_focused_view() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let (_, _, mut slots) = shared_document_slots(&workspace);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let primary_view = workspace.pane(EditorPane::Primary).view();
        let companion_view = workspace.pane(EditorPane::Companion).view();
        workspace.update(EditorMessage::FocusPane(EditorPane::Companion));
        workspace.update(EditorMessage::OpenLocalFind);

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator.click("Find").expect("visible local Find input");
        assert_eq!(simulator.typewrite("river"), iced::event::Status::Captured);
        apply_surface_messages(&mut workspace, &mut slots, simulator.into_messages());
        assert_eq!(workspace.local_search(companion_view).query(), "river");
        assert!(workspace.local_search(primary_view).query().is_empty());

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator.click("Replace").expect("visible replace toggle");
        apply_surface_messages(&mut workspace, &mut slots, simulator.into_messages());
        assert!(workspace.local_search(companion_view).replace_visible());

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator
            .click("Replace with")
            .expect("visible replacement input");
        assert_eq!(simulator.typewrite("scene"), iced::event::Status::Captured);
        apply_surface_messages(&mut workspace, &mut slots, simulator.into_messages());
        assert_eq!(
            slots
                .slot(EditorPane::Companion)
                .expect("companion slot")
                .replace_draft(),
            "scene"
        );
        assert!(
            slots
                .slot(EditorPane::Primary)
                .expect("primary slot")
                .replace_draft()
                .is_empty()
        );

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator
            .click("Replace all")
            .expect("visible replace-all control");
        assert!(matches!(
            apply_surface_messages(&mut workspace, &mut slots, simulator.into_messages()).as_slice(),
            [crate::EditorEffect::Command {
                view,
                command: crate::EditorCommand::ReplaceAllFindMatches { replacement },
            }] if *view == companion_view && replacement == "scene"
        ));
        assert_eq!(workspace.focused_pane(), EditorPane::Companion);
        assert!(!workspace.local_search(primary_view).is_open());
    }
}
