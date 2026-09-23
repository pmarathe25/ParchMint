//! Editor toolbar, tabs, local search, and mounted document views.

use std::collections::BTreeMap;

use crate::components::{button_interaction, field_interaction, multiline_field_style};

use iced::widget::{
    Space, column, container, mouse_area, opaque, responsive, row, sensor, stack, text, text_editor,
};
use iced::{
    Background, Element, Length,
    alignment::{Horizontal, Vertical},
};
use parchmint_editor_api::{EditorError, ViewId};
use parchmint_editor_iced::{
    EditorViewport, MountedEditorHost, MountedEditorMessage, MountedEditorUpdate,
};

use crate::components::{semantic_button as button, semantic_text_input as text_input};

use crate::{
    EditorMessage, EditorPane, EditorPaneState, EditorWorkspace, F6Region, FindDirection,
    FormattingCommand, HarnessTarget, LocalSearchState, SpellingMenu, SpellingMenuAction, TabSpec,
    components::{self, ButtonKind, Interaction, Surface},
    design_tokens::{COMPACT_CONTROL_HEIGHT, ParchMintTheme},
    focus, harness_target, hierarchy_drag,
    icons::{Icon, icon_sized},
    right_click, stationary_tooltip,
};

const EDITOR_TOOLBAR_CONTROL_HEIGHT: u16 = COMPACT_CONTROL_HEIGHT + 4;

/// Content shown when a pane has no mounted editor.
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
    ManageStyles,
    NewTab(EditorPane),
    OpenDocumentContext {
        pane: EditorPane,
        document_id: String,
        point: crate::Point,
    },
    Scratch {
        pane: EditorPane,
        id: String,
        action: text_editor::Action,
    },
    BeginComment,
    BeginSplitResize,
    HierarchyDropTarget(EditorPane),
    ClearHierarchyDropTarget(EditorPane),
    Workspace(EditorMessage),
    PaneWorkspace {
        pane: EditorPane,
        message: EditorMessage,
    },
    Mounted {
        pane: EditorPane,
        view: ViewId,
        mount_generation: u64,
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
            Self::Scratch { .. }
            | Self::ManageStyles
            | Self::NewTab(_)
            | Self::OpenDocumentContext { .. }
            | Self::BeginComment
            | Self::BeginSplitResize
            | Self::HierarchyDropTarget(_)
            | Self::ClearHierarchyDropTarget(_) => Vec::new(),
            Self::Workspace(message) => vec![message.clone()],
            Self::PaneWorkspace { pane, message } => {
                vec![EditorMessage::FocusPane(*pane), message.clone()]
            }
            Self::Mounted {
                message:
                    MountedEditorMessage::Blur
                    | MountedEditorMessage::OpenLink(_)
                    | MountedEditorMessage::ViewportChanged(_)
                    | MountedEditorMessage::Scroll { .. },
                ..
            } => Vec::new(),
            Self::Mounted {
                pane,
                message:
                    MountedEditorMessage::HoverComment {
                        link_target,
                        comment_id,
                        anchor_bounds,
                    },
                ..
            } => {
                let mut messages = vec![EditorMessage::SetHoveredLink(link_target.clone())];
                if comment_id.is_some() {
                    messages.push(EditorMessage::SetCommentHover {
                        pane: *pane,
                        comment_id: comment_id.clone(),
                        anchor_bounds: crate::Rect::new(
                            anchor_bounds.0,
                            anchor_bounds.1,
                            anchor_bounds.2,
                            anchor_bounds.3,
                        ),
                    });
                }
                messages
            }
            Self::Mounted { pane, .. } => vec![EditorMessage::FocusPane(*pane)],
            Self::SetReplaceDraft { .. } => Vec::new(),
            Self::ChooseSpellingAction(_) | Self::DismissSpellingMenu => Vec::new(),
        }
    }
}

/// Composes only the editor-center region. Explorer, Inspector, ribbon, and
/// status bar remain separate project-surface responsibilities.
#[cfg(test)]
pub(crate) fn editor_center_surface<'a>(
    workspace: &'a EditorWorkspace,
    theme: ParchMintTheme,
    slots: &EditorHostSlots,
    spelling_menu: Option<&SpellingMenu>,
) -> Element<'a, EditorCenterMessage> {
    editor_center_surface_with_breadcrumbs(
        workspace,
        theme,
        slots,
        spelling_menu,
        &BTreeMap::new(),
        false,
    )
}

/// Composes the editor center with pane-specific hierarchy context.
pub(crate) fn editor_center_surface_with_breadcrumbs<'a>(
    workspace: &'a EditorWorkspace,
    theme: ParchMintTheme,
    slots: &EditorHostSlots,
    spelling_menu: Option<&SpellingMenu>,
    breadcrumbs: &BTreeMap<EditorPane, Vec<String>>,
    hierarchy_drag_active: bool,
) -> Element<'a, EditorCenterMessage> {
    let primary = editor_pane_surface(
        workspace,
        EditorPane::Primary,
        theme,
        slots,
        spelling_menu,
        hierarchy_drag_active,
        breadcrumbs
            .get(&EditorPane::Primary)
            .cloned()
            .unwrap_or_default(),
    );
    let companion_visible = workspace.companion_is_visible();
    let expanded = workspace.expanded_pane();
    let primary_portion = (workspace.split_ratio() * 1000.0).round() as u16;
    let companion = editor_pane_surface(
        workspace,
        EditorPane::Companion,
        theme,
        slots,
        spelling_menu,
        hierarchy_drag_active,
        breadcrumbs
            .get(&EditorPane::Companion)
            .cloned()
            .unwrap_or_default(),
    );
    let splitter = mouse_area(
        container(
            container(Space::new().width(1).height(Length::Fill)).style(move |_| {
                iced::widget::container::Style {
                    background: Some(theme.palette().divider.into()),
                    ..Default::default()
                }
            }),
        )
        .width(8)
        .height(Length::Fill)
        .align_x(Horizontal::Center)
        .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest)),
    )
    .on_press(EditorCenterMessage::BeginSplitResize)
    .interaction(iced::mouse::Interaction::ResizingHorizontally);
    let panes = crate::motion::row(vec![
        crate::motion::slot(
            primary,
            Length::FillPortion(primary_portion),
            expanded != Some(EditorPane::Companion),
        ),
        crate::motion::slot(
            splitter,
            Length::Fixed(8.0),
            companion_visible && expanded.is_none(),
        ),
        crate::motion::slot(
            companion,
            Length::FillPortion(1000_u16.saturating_sub(primary_portion)),
            companion_visible && expanded != Some(EditorPane::Primary),
        ),
    ]);

    let center = container(panes)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(0)
        .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest));
    let companion_toggle = stationary_tooltip::tooltip(
        harness_target::target(
            HarnessTarget::ToggleCompanion,
            button(container(icon_sized(Icon::SplitEditor, 16)).center(Length::Fill))
                .width(32)
                .height(32)
                .padding(0)
                .on_press(EditorCenterMessage::Workspace(
                    EditorMessage::ToggleCompanion,
                ))
                .style(move |_, status| {
                    components::button_style(
                        theme,
                        ButtonKind::Quiet,
                        button_interaction(status, companion_visible),
                    )
                }),
        ),
        container(
            text(if companion_visible {
                "Close second pane"
            } else {
                "Open second pane"
            })
            .size(12),
        )
        .padding([4, 6]),
        components::surface(theme, Surface::Elevated, Interaction::Rest),
    );
    let mut layers = stack![center].width(Length::Fill).height(Length::Fill);
    if expanded.is_none() {
        layers = layers.push(
            container(companion_toggle)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Right)
                .align_y(Vertical::Top)
                .padding(iced::Padding {
                    top: 2.0,
                    right: 32.0,
                    bottom: 0.0,
                    left: 0.0,
                }),
        );
    }
    layers.into()
}

fn spelling_menu_overlay<'a>(
    content: Element<'a, EditorCenterMessage>,
    menu: &SpellingMenu,
    theme: ParchMintTheme,
) -> Element<'a, EditorCenterMessage> {
    let bounds = menu.bounds();
    stack![
        content,
        container(opaque(hierarchy_drag::commit_on_click_away(
            crate::motion::enter("editor menu", spelling_menu_popover(menu, theme)),
            EditorCenterMessage::DismissSpellingMenu
        )))
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
    let actions = menu
        .actions()
        .iter()
        .cloned()
        .fold(column![], |column, action| {
            let label = match &action {
                SpellingMenuAction::Edit(label) => (*label).to_owned(),
                SpellingMenuAction::AddComment => "Add Comment".to_owned(),
                SpellingMenuAction::CopyLink(_) => "Copy link address".to_owned(),
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
            column.push(components::context_action(
                label,
                EditorCenterMessage::ChooseSpellingAction(action),
                theme,
            ))
        });
    container(
        iced::widget::scrollable(actions.spacing(2))
            .height((menu.bounds().height() - 12.0).max(0.0)),
    )
    .padding(6)
    .width(menu.bounds().width())
    .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Rest))
    .into()
}

#[cfg(test)]
pub(crate) fn formatting_toolbar(
    workspace: &EditorWorkspace,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    formatting_toolbar_for_width(workspace, theme, false)
}

pub(crate) fn formatting_toolbar_for_width(
    workspace: &EditorWorkspace,
    theme: ParchMintTheme,
    wide: bool,
) -> Element<'static, EditorCenterMessage> {
    let enabled = workspace
        .pane(workspace.focused_pane())
        .active_document()
        .is_some();
    if !enabled {
        return Space::new().height(0).into();
    }
    let style_selector = harness_target::target(
        HarnessTarget::ParagraphStyle,
        crate::action_menu::menu_with_footer(
            menu_trigger(workspace.active_style().to_owned(), 104.0, theme),
            workspace
                .style_names()
                .iter()
                .map(|style| {
                    (
                        style.clone(),
                        format_message(FormattingCommand::ParagraphStyle(style.clone())),
                    )
                })
                .collect(),
            (
                "Manage Styles".into(),
                EditorCenterMessage::ManageStyles,
                HarnessTarget::ManageStyles,
            ),
            theme,
            190.0,
        ),
    );
    use parchmint_editor_api::{InlineFont, InlineFontFamily, SemanticInlineMark};
    let family = harness_target::target(
        HarnessTarget::FontFamily,
        toolbar_tooltip(
            font_menu(
                workspace
                    .effective_style()
                    .font_family
                    .as_deref()
                    .unwrap_or("Source Serif 4")
                    .to_owned(),
                ["Style default", "Serif", "Sans serif", "Monospace"]
                    .into_iter()
                    .map(|label| {
                        let family = match label {
                            "Serif" => Some(InlineFontFamily::Serif),
                            "Sans serif" => Some(InlineFontFamily::SansSerif),
                            "Monospace" => Some(InlineFontFamily::Monospace),
                            _ => None,
                        };
                        (
                            label.to_owned(),
                            format_message(FormattingCommand::InlineFont(InlineFont::Family(
                                family,
                            ))),
                        )
                    })
                    .collect(),
                110.0,
                theme,
            ),
            "Font family",
            theme,
        ),
    );
    let active_size = workspace
        .active_inline_marks()
        .iter()
        .find_map(|mark| match mark {
            SemanticInlineMark::FontSize(size) => Some(size.to_string()),
            _ => None,
        });
    let mut sizes = vec!["Auto".to_owned()];
    sizes.extend(
        [8, 9, 10, 11, 12, 14, 15, 16, 18, 20, 24, 28, 32, 36, 48, 72].map(|size| size.to_string()),
    );
    if let Some(size) = &active_size
        && !sizes.contains(size)
    {
        sizes.push(size.clone());
    }
    let size = harness_target::target(
        HarnessTarget::FontSize,
        toolbar_tooltip(
            font_menu(
                active_size.unwrap_or_else(|| {
                    workspace
                        .effective_style()
                        .font_size_points
                        .unwrap_or(15.0)
                        .to_string()
                }),
                sizes
                    .into_iter()
                    .map(|size| {
                        let command = format_message(FormattingCommand::InlineFont(
                            InlineFont::Size(size.parse().ok()),
                        ));
                        (size, command)
                    })
                    .collect(),
                52.0,
                theme,
            ),
            "Font size",
            theme,
        ),
    );
    let mut marks = row![].spacing(2).align_y(Vertical::Center);
    let mut extra_marks = row![].spacing(2).align_y(Vertical::Center);
    for (label, command, mark) in [
        (
            "B",
            FormattingCommand::Bold,
            parchmint_editor_api::SemanticInlineMark::Bold,
        ),
        (
            "I",
            FormattingCommand::Italic,
            parchmint_editor_api::SemanticInlineMark::Italic,
        ),
        (
            "U",
            FormattingCommand::Underline,
            parchmint_editor_api::SemanticInlineMark::Underline,
        ),
        (
            "S",
            FormattingCommand::Strikethrough,
            parchmint_editor_api::SemanticInlineMark::Strikethrough,
        ),
    ] {
        let active = workspace.active_inline_marks().contains(&mark);
        let name = match label {
            "B" => "Bold",
            "I" => "Italic",
            "U" => "Underline",
            _ => "Strikethrough",
        };
        let control = toolbar_button(
            match label {
                "B" => Element::from(icon_sized(Icon::Bold, 20)),
                "I" => Element::from(icon_sized(Icon::Italic, 20)),
                "U" => Element::from(icon_sized(Icon::Underline, 20)),
                _ => Element::from(icon_sized(Icon::Strikethrough, 20)),
            },
            format_message(command),
            active,
            theme,
        );
        let control = if label == "B" {
            harness_target::target(HarnessTarget::Bold, control)
        } else {
            control
        };
        if matches!(label, "B" | "I") {
            marks = marks.push(toolbar_tooltip(control, name, theme));
        } else {
            extra_marks = extra_marks.push(toolbar_tooltip(control, name, theme));
        }
    }
    let in_list = matches!(
        workspace.active_block_kind,
        Some(
            parchmint_editor_api::SemanticBlockKind::OrderedListItem
                | parchmint_editor_api::SemanticBlockKind::UnorderedListItem
        )
    );
    let numbered = workspace.active_block_kind
        == Some(parchmint_editor_api::SemanticBlockKind::OrderedListItem);
    let lists = row![
        harness_target::target(
            HarnessTarget::ListBulleted,
            toolbar_tooltip(
                toolbar_button(
                    icon_sized(
                        if numbered {
                            Icon::NumberedList
                        } else {
                            Icon::BulletedList
                        },
                        20
                    ),
                    format_message(if numbered {
                        FormattingCommand::NumberedList
                    } else {
                        FormattingCommand::BulletedList
                    }),
                    in_list,
                    theme
                ),
                "List",
                theme
            )
        ),
        harness_target::target(
            HarnessTarget::ListMenu,
            toolbar_tooltip(
                crate::action_menu::icon_menu(
                    button(container(icon_sized(Icon::ChevronDown, 12)).center(Length::Fill))
                        .width(16)
                        .height(32)
                        .padding(0)
                        .on_press_maybe(enabled.then_some(()))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            button_interaction(status, false)
                        ))
                        .into(),
                    vec![
                        (
                            Icon::BulletedList,
                            "Bulleted list",
                            HarnessTarget::ListBulleted,
                            format_message(FormattingCommand::BulletedList)
                        ),
                        (
                            Icon::NumberedList,
                            "Numbered list",
                            HarnessTarget::ListNumbered,
                            format_message(FormattingCommand::NumberedList)
                        ),
                    ],
                    theme
                ),
                "List options",
                theme
            )
        ),
    ]
    .spacing(0);
    let breaks = harness_target::target(
        HarnessTarget::BreakMenu,
        toolbar_tooltip(
            crate::action_menu::action_menu(
                Icon::PageBreak,
                vec![
                    ("Page break", format_message(FormattingCommand::PageBreak)),
                    ("Scene break", format_message(FormattingCommand::SceneBreak)),
                ],
                enabled,
                theme,
            ),
            "Insert break",
            theme,
        ),
    );
    let paragraphs = row![formatting_icon_button(
        Icon::BlockQuote,
        "Block quote",
        FormattingCommand::BlockQuote,
        theme
    )]
    .spacing(2)
    .align_y(Vertical::Center);
    if wide {
        return row![
            style_selector,
            family,
            size,
            container(iced::widget::rule::vertical(1)).height(20),
            marks,
            extra_marks,
            lists,
            paragraphs,
            paragraph_options(workspace, theme),
            container(iced::widget::rule::vertical(1)).height(20),
            breaks
        ]
        .spacing(6)
        .align_y(Vertical::Center)
        .into();
    }
    let more = harness_target::target(
        HarnessTarget::FormattingMenu,
        crate::action_menu::panel(
            components::semantic_button(
                text("Format").size(13).color(theme.palette().primary_text),
            )
            .height(32)
            .padding([7, 10])
            .on_press(())
            .style(move |_, status| {
                components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    button_interaction(status, false),
                )
            })
            .into(),
            column![
                text("Font").size(12),
                row![family, size, extra_marks]
                    .spacing(8)
                    .align_y(Vertical::Center),
                iced::widget::rule::horizontal(1),
                text("Paragraph").size(12),
                row![paragraphs, paragraph_options(workspace, theme), breaks].spacing(12),
            ]
            .spacing(10)
            .into(),
            theme,
            340.0,
        ),
    );
    container(
        row![
            style_selector,
            container(iced::widget::rule::vertical(1)).height(20),
            marks,
            lists,
            container(iced::widget::rule::vertical(1)).height(20),
            more,
        ]
        .spacing(10)
        .align_y(Vertical::Center),
    )
    .padding(0)
    .width(Length::Shrink)
    .into()
}

fn format_message(command: FormattingCommand) -> EditorCenterMessage {
    EditorCenterMessage::Workspace(EditorMessage::Format(command))
}

fn menu_trigger(label: String, width: f32, theme: ParchMintTheme) -> Element<'static, ()> {
    components::semantic_button(
        row![
            text(label).size(13).color(theme.palette().primary_text),
            Space::new().width(Length::Fill),
            icon_sized(Icon::ChevronDown, 10)
        ]
        .align_y(Vertical::Center),
    )
    .width(width)
    .height(32)
    .padding([7, 8])
    .on_press(())
    .style(move |_, status| {
        components::button_style(theme, ButtonKind::Quiet, button_interaction(status, false))
    })
    .into()
}

fn paragraph_options(
    workspace: &EditorWorkspace,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    use parchmint_editor_api::{ParagraphFormatCommand as Format, TextAlignment};
    let alignment = workspace
        .effective_style()
        .alignment
        .unwrap_or(TextAlignment::Start);
    let align_icon = match alignment {
        TextAlignment::Start => Icon::AlignLeft,
        TextAlignment::Center => Icon::AlignCenter,
        TextAlignment::End => Icon::AlignRight,
        TextAlignment::Justify => Icon::AlignJustify,
    };
    let align = crate::action_menu::symbol_menu(
        align_icon,
        vec![
            (
                Icon::AlignLeft,
                "Align left",
                format_message(FormattingCommand::ParagraphFormat(Format::Alignment(
                    TextAlignment::Start,
                ))),
            ),
            (
                Icon::AlignCenter,
                "Center",
                format_message(FormattingCommand::ParagraphFormat(Format::Alignment(
                    TextAlignment::Center,
                ))),
            ),
            (
                Icon::AlignRight,
                "Align right",
                format_message(FormattingCommand::ParagraphFormat(Format::Alignment(
                    TextAlignment::End,
                ))),
            ),
            (
                Icon::AlignJustify,
                "Justify",
                format_message(FormattingCommand::ParagraphFormat(Format::Alignment(
                    TextAlignment::Justify,
                ))),
            ),
        ],
        theme,
    );
    let spacing = crate::action_menu::action_menu(
        Icon::LineSpacing,
        [
            ("Single", 100),
            ("1.15", 115),
            ("1.5", 150),
            ("Double", 200),
        ]
        .map(|(label, percent)| {
            (
                label,
                format_message(FormattingCommand::ParagraphFormat(Format::LineSpacing(
                    percent,
                ))),
            )
        })
        .to_vec(),
        true,
        theme,
    );
    row![
        harness_target::target(
            HarnessTarget::Alignment,
            toolbar_tooltip(align, "Alignment", theme)
        ),
        harness_target::target(
            HarnessTarget::LineSpacing,
            toolbar_tooltip(spacing, "Line spacing", theme)
        ),
    ]
    .spacing(2)
    .into()
}

fn font_menu(
    label: String,
    options: Vec<(String, EditorCenterMessage)>,
    width: f32,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    crate::action_menu::anchored_menu(
        menu_trigger(label, width, theme),
        options,
        theme,
        width.max(120.0),
    )
}

fn toolbar_button(
    content: impl Into<Element<'static, EditorCenterMessage>>,
    message: EditorCenterMessage,
    active: bool,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    button(container(content).center(Length::Fill))
        .padding(0)
        .width(32)
        .height(u32::from(EDITOR_TOOLBAR_CONTROL_HEIGHT))
        .on_press(message)
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, button_interaction(status, active))
        })
        .into()
}

fn toolbar_tooltip(
    control: impl Into<Element<'static, EditorCenterMessage>>,
    label: &'static str,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    stationary_tooltip::tooltip(
        control,
        container(text(label).size(12)).padding([4, 6]),
        components::surface(theme, Surface::Elevated, Interaction::Rest),
    )
}

fn formatting_icon_button(
    icon: Icon,
    label: &'static str,
    command: FormattingCommand,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    toolbar_tooltip(
        toolbar_button(icon_sized(icon, 20), format_message(command), false, theme),
        label,
        theme,
    )
}

fn link_editor_popover(
    workspace: &EditorWorkspace,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let link_editor = workspace.link_editor();
    let target = link_editor.target().to_owned();
    let url_input = text_input("https://example.com", &target)
        .on_input(|target| EditorCenterMessage::Workspace(EditorMessage::SetLinkTarget(target)))
        .on_submit(EditorCenterMessage::Workspace(EditorMessage::ApplyLink))
        .padding([6, 8])
        .style(move |_, status| components::field_style(theme, field_interaction(status)));
    let internal = link_editor.internal;
    let mode_button = |label, internal_mode| {
        button(text(label).size(13))
            .padding([5, 10])
            .on_press(EditorCenterMessage::Workspace(
                EditorMessage::SetLinkInternal(internal_mode),
            ))
            .style(move |_, status| {
                components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    button_interaction(status, internal == internal_mode),
                )
            })
    };
    let mode = row![mode_button("Web", false), mode_button("Document", true)].spacing(4);
    let mut content = column![text("Link destination").size(14), mode].spacing(6);
    if internal {
        let mut locations = column![].spacing(2);
        for item in workspace.link_outline.rows().into_iter().filter(|item| {
            crate::iced_project_surface::hierarchy_row_is_visible(
                &workspace.link_outline,
                item.parent_id,
            )
        }) {
            let toggle =
                EditorCenterMessage::Workspace(EditorMessage::ToggleLinkGroup(item.id.to_owned()));
            let document_id = item
                .document_id
                .filter(|_| item.kind == crate::HierarchyRowKind::Document);
            let select = document_id
                .map(|id| {
                    EditorCenterMessage::Workspace(EditorMessage::SetLinkTarget(format!(
                        "parchmint://document/{id}"
                    )))
                })
                .unwrap_or_else(|| toggle.clone());
            locations = locations.push(components::location_row(
                item.title.to_owned(),
                crate::iced_project_surface::hierarchy_depth(
                    &workspace.link_outline,
                    item.parent_id,
                ),
                (item.kind != crate::HierarchyRowKind::Document).then_some(item.expanded),
                document_id.is_some_and(|id| target == format!("parchmint://document/{id}")),
                toggle,
                select,
                theme,
            ));
        }
        content = content.push(container(iced::widget::scrollable(locations)).max_height(240));
    } else {
        content = content.push(url_input);
    }
    if let Some(error) = link_editor.validation_error() {
        content = content.push(text(error.to_owned()).size(12));
    }
    content = content.push(
        row![
            button(text("Apply Link").size(12))
                .padding([5, 7])
                .on_press_maybe(
                    (!target.trim().is_empty())
                        .then_some(EditorCenterMessage::Workspace(EditorMessage::ApplyLink))
                )
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

fn editor_pane_surface<'a>(
    workspace: &'a EditorWorkspace,
    pane: EditorPane,
    theme: ParchMintTheme,
    slots: &EditorHostSlots,
    spelling_menu: Option<&SpellingMenu>,
    hierarchy_drag_active: bool,
    breadcrumb: Vec<String>,
) -> Element<'a, EditorCenterMessage> {
    let state = workspace.pane(pane);
    let tabs = tab_strip(
        workspace.tab_positions[usize::from(pane == EditorPane::Companion)].clone(),
        state,
        pane,
        workspace.focused_pane() == pane,
        workspace.expanded_pane() == Some(pane),
        pane == workspace
            .expanded_pane()
            .unwrap_or(if workspace.companion_is_visible() {
                EditorPane::Companion
            } else {
                EditorPane::Primary
            }),
        workspace.tab_drag_source(pane),
        workspace.tab_drag_target(pane),
        breadcrumb.clone(),
        theme,
    );
    let search = workspace.local_search(state.view());
    let view = state.view();
    let mount_generation = state.mount_generation();
    let viewport_message = move |size: iced::Size| EditorCenterMessage::Mounted {
        pane,
        view,
        mount_generation,
        message: MountedEditorMessage::ViewportChanged(
            EditorViewport::new(size.width.max(1.0), size.height.max(1.0))
                .expect("sensor clamps editor viewport dimensions"),
        ),
    };
    let target = match pane {
        EditorPane::Primary => HarnessTarget::EditorPrimary,
        EditorPane::Companion => HarnessTarget::EditorCompanion,
    };
    let body: Element<'a, EditorCenterMessage> = sensor(harness_target::target(
        target,
        if let Some((id, content)) = state
            .active_document()
            .and_then(|id| workspace.scratch(id).map(|content| (id, content)))
        {
            text_editor(content)
                .id(iced::widget::Id::from(format!("scratch-editor-{id}")))
                .placeholder("")
                .height(Length::Fill)
                .padding(iced::Padding {
                    top: 32.0,
                    right: 54.0,
                    bottom: 32.0,
                    left: 54.0,
                })
                .size(18)
                .on_action(move |action| EditorCenterMessage::Scratch {
                    pane,
                    id: id.to_owned(),
                    action,
                })
                .style(move |_, status| {
                    let mut style = multiline_field_style(theme, status);
                    style.border = iced::Border::default();
                    style.background = theme.palette().manuscript.into();
                    style
                })
                .into()
        } else {
            pane_body(state, pane, theme, slots)
        },
    ))
    .key((
        pane,
        view,
        mount_generation,
        slots.slot(pane).and_then(EditorPaneSlot::host).is_some(),
    ))
    .on_show(viewport_message)
    .on_resize(viewport_message)
    .into();
    let body = if let Some(menu) = spelling_menu.filter(|menu| menu.pane() == pane) {
        spelling_menu_overlay(body, menu, theme)
    } else {
        body
    };
    let body = if workspace.focused_pane() == pane
        && workspace.comment_composer(pane).is_none()
        && !workspace.link_editor().is_open()
        && spelling_menu.is_none()
        && let Some(anchor) = slots
            .slot(pane)
            .and_then(EditorPaneSlot::host)
            .and_then(|host| {
                host.selection_anchor().map(|mut anchor| {
                    anchor.x = anchor.x.min((host.viewport().width - 84.0).max(4.0));
                    anchor
                })
            }) {
        let actions = row![
            harness_target::target(
                HarnessTarget::AddComment,
                toolbar_tooltip(
                    toolbar_button(
                        row![icon_sized(Icon::Comment, 18), text("+").size(12)].spacing(0),
                        EditorCenterMessage::BeginComment,
                        false,
                        theme
                    ),
                    "Add comment",
                    theme
                )
            ),
            harness_target::target(
                HarnessTarget::Link,
                formatting_icon_button(Icon::Link, "Link", FormattingCommand::Link, theme)
            ),
        ]
        .spacing(2);
        stack![
            body,
            container(opaque(container(actions).padding(3).style(move |_| {
                components::surface(theme, Surface::Elevated, Interaction::Rest)
            })))
            .padding(iced::Padding {
                top: anchor.y + anchor.height + 4.0,
                left: anchor.x.max(4.0),
                right: 0.0,
                bottom: 0.0
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Left)
            .align_y(Vertical::Top)
        ]
        .into()
    } else {
        body
    };
    let hovered_thread = workspace.hovered_comment(pane).and_then(|hover| {
        workspace
            .comment_thread(hover.comment_id())
            .map(|thread| (hover.clone(), thread.clone()))
    });
    let body = match (workspace.comment_composer(pane), hovered_thread) {
        (Some(composer), _) => comment_composer_overlay(body, composer, workspace, theme),
        (None, Some((hover, thread))) => {
            comment_hover_overlay(body, &hover, &thread, workspace, theme)
        }
        (None, None) => body,
    };
    let body = if workspace.link_editor().is_open() && workspace.focused_pane() == pane {
        let anchor = slots
            .slot(pane)
            .and_then(EditorPaneSlot::host)
            .and_then(|host| host.action_anchor())
            .map(|r| crate::Rect::new(r.x, r.y, r.width, r.height))
            .unwrap_or(crate::Rect::new(20.0, 20.0, 1.0, 20.0));
        crate::anchored_popover::anchored(
            body,
            link_editor_popover(workspace, theme),
            anchor,
            crate::anchored_popover::Dismissal::OutsideClick,
            EditorCenterMessage::Workspace(EditorMessage::CancelLinkEditor),
        )
    } else {
        body
    };
    let body = container(container(body).max_width(800))
        .center_x(Length::Fill)
        .height(Length::Fill);
    let body = body.into();
    let body = if workspace.focused_pane() == pane {
        focus::f6_region(F6Region::FocusedEditor, body)
    } else {
        body
    };
    let search_open = search.is_open();
    let content = column![
        crate::motion::reveal(workspace.expanded_pane().is_none(), tabs),
        crate::motion::reveal(
            workspace.expanded_pane().is_none(),
            container(
                row![
                    text(breadcrumb.join(" > "))
                        .size(12)
                        .color(theme.palette().secondary_text)
                        .width(Length::Fill),
                    stationary_tooltip::tooltip(
                        harness_target::target_id(
                            iced::widget::Id::from(format!("breadcrumb-search-{pane:?}")),
                            button(icon_sized(Icon::Search, 16))
                                .padding(3)
                                .on_press(EditorCenterMessage::PaneWorkspace {
                                    pane,
                                    message: if search.is_open() {
                                        EditorMessage::CloseLocalFind
                                    } else {
                                        EditorMessage::OpenLocalFind
                                    }
                                })
                                .style(move |_, status| components::button_style(
                                    theme,
                                    ButtonKind::Quiet,
                                    button_interaction(status, search_open)
                                ))
                        ),
                        text("Find in document").size(12),
                        components::surface(theme, Surface::Elevated, Interaction::Rest)
                    )
                ]
                .align_y(Vertical::Center)
            )
            .padding([2, 12])
        ),
        crate::motion::reveal(
            search.is_open(),
            container(local_search_bar(search, pane, theme, slots)).padding([6, 0])
        ),
        body
    ]
    .spacing(0);
    let targets = hierarchy_drag::targets();
    let content = hierarchy_drag::target(
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest)),
        None,
        &targets,
        |bounds, point| bounds.contains(point).then_some(()),
    );
    hierarchy_drag::surface(
        content,
        targets,
        hierarchy_drag_active || workspace.tab_drag_is_active(),
        false,
        move |target| {
            if target.is_some() {
                EditorCenterMessage::HierarchyDropTarget(pane)
            } else {
                EditorCenterMessage::ClearHierarchyDropTarget(pane)
            }
        },
        EditorCenterMessage::ClearHierarchyDropTarget(pane),
    )
}

fn comment_hover_overlay<'a>(
    content: Element<'a, EditorCenterMessage>,
    hover: &crate::CommentHover,
    thread: &crate::CommentThreadView,
    workspace: &'a EditorWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, EditorCenterMessage> {
    let quote = match thread.anchor() {
        crate::CommentAnchor::Range { quote, .. }
        | crate::CommentAnchor::Position { quote, .. }
        | crate::CommentAnchor::Orphaned { quote, .. } => quote.clone(),
        crate::CommentAnchor::Document { .. } => "Whole document".to_owned(),
    };
    let card = comment_thread_card("", quote, thread, workspace, theme);
    anchored_comment_overlay(content, hover.anchor_bounds(), card, true, theme)
}

fn comment_key_binding(
    press: text_editor::KeyPress,
    submit: EditorMessage,
) -> Option<text_editor::Binding<EditorCenterMessage>> {
    if press.key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) {
        Some(if press.modifiers.shift() || press.modifiers.alt() {
            text_editor::Binding::Enter
        } else {
            text_editor::Binding::Custom(EditorCenterMessage::Workspace(submit))
        })
    } else {
        text_editor::Binding::from_key_press(press)
    }
}

fn comment_composer_overlay<'a>(
    content: Element<'a, EditorCenterMessage>,
    composer: crate::CommentComposer,
    workspace: &'a EditorWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, EditorCenterMessage> {
    let mut card = column![
        text_editor(workspace.comment_draft())
            .id(HarnessTarget::CommentDraft.id())
            .key_binding(|press| comment_key_binding(
                press,
                EditorMessage::CreateComment {
                    document_level: false
                }
            ))
            .placeholder("Write a comment")
            .on_action(|action| {
                EditorCenterMessage::Workspace(EditorMessage::EditCommentDraft(action))
            })
            .height(Length::Fixed(76.0))
            .style(move |_, status| multiline_field_style(theme, status)),
        row![
            Space::new().width(Length::Fill),
            comment_popover_action(
                "Cancel",
                EditorCenterMessage::Workspace(EditorMessage::CancelCommentComposer),
                theme
            ),
            comment_popover_action(
                "Add comment",
                EditorCenterMessage::Workspace(EditorMessage::CreateComment {
                    document_level: false
                }),
                theme
            ),
        ]
        .spacing(6),
    ]
    .spacing(7);
    if let Some(feedback) = workspace.comment_feedback().map(str::to_owned) {
        card = card.push(
            text(feedback)
                .size(11)
                .color(theme.palette().secondary_text),
        );
    }
    anchored_comment_overlay(content, composer.anchor_bounds(), card.into(), false, theme)
}

pub(crate) fn comment_thread_card<'a>(
    _status: &str,
    quote: String,
    thread: &crate::CommentThreadView,
    workspace: &'a EditorWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, EditorCenterMessage> {
    let thread_id = thread.id().to_owned();
    let quote = quote.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut card = column![
        row![
            container(text(quote).size(12).color(theme.palette().secondary_text))
                .padding([4, 8])
                .width(Length::Fill)
                .style(move |_| iced::widget::container::Style {
                    background: Some(theme.palette().comment_active.scale_alpha(0.10).into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            text(if thread.resolved() { "Resolved" } else { "" })
                .size(11)
                .color(theme.palette().comment_resolved),
            stationary_tooltip::tooltip(
                comment_popover_action(
                    if thread.resolved() { "↶" } else { "✓" },
                    EditorCenterMessage::Workspace(EditorMessage::ToggleCommentResolved {
                        thread_id: thread_id.clone(),
                        resolved: !thread.resolved(),
                    }),
                    theme,
                ),
                text(if thread.resolved() {
                    "Reopen thread"
                } else {
                    "Resolve thread"
                })
                .size(12),
                components::surface(theme, Surface::Elevated, Interaction::Rest),
            ),
        ]
        .spacing(6)
        .align_y(Vertical::Center),
    ]
    .spacing(5);

    for (message_index, message) in thread.messages().iter().enumerate() {
        let message_id = message.id().to_owned();
        if workspace.editing_comment_message() == Some((thread_id.as_str(), message_id.as_str())) {
            let edit_thread = thread_id.clone();
            card = card
                .push(
                    text_editor(
                        workspace
                            .comment_reply_draft(&thread_id)
                            .expect("every rendered comment thread has an edit draft"),
                    )
                    .id(HarnessTarget::CommentEdit.id())
                    .key_binding({
                        let thread_id = thread_id.clone();
                        let message_id = message_id.clone();
                        move |press| comment_key_binding(press, EditorMessage::SaveEditedCommentMessage {
                            thread_id: thread_id.clone(), message_id: message_id.clone(),
                        })
                    })
                    .placeholder("Edit comment")
                    .on_action(move |action| {
                        EditorCenterMessage::Workspace(EditorMessage::EditCommentReplyDraft {
                            thread_id: edit_thread.clone(),
                            action,
                        })
                    })
                    .height(Length::Fixed(76.0))
                    .style(move |_, status| multiline_field_style(theme, status)),
                )
                .push(
                    row![
                        comment_popover_action(
                            "Save edit",
                            EditorCenterMessage::Workspace(
                                EditorMessage::SaveEditedCommentMessage {
                                    thread_id: thread_id.clone(),
                                    message_id: message_id.clone(),
                                },
                            ),
                            theme,
                        ),
                        comment_popover_action(
                            "Cancel edit",
                            EditorCenterMessage::Workspace(
                                EditorMessage::CancelEditCommentMessage,
                            ),
                            theme,
                        ),
                    ]
                    .spacing(6),
                );
        } else {
            let edit_thread = thread_id.clone();
            let body = message.body().to_owned();
            card = card.push(
                row![
                    text(message.body().to_owned()).size(13).width(Length::Fill),
                    harness_target::target(
                        HarnessTarget::CommentMenu(message_index),
                        crate::action_menu::notifying_menu(
                            button(text("⋮").size(20))
                                .padding([0, 7])
                                .on_press(())
                                .style(move |_, status| components::button_style(
                                    theme,
                                    ButtonKind::Quiet,
                                    button_interaction(status, false)
                                ))
                                .into(),
                            vec![
                                (
                                    "Edit".into(),
                                    EditorCenterMessage::Workspace(
                                        EditorMessage::BeginEditCommentMessage {
                                            thread_id: edit_thread.clone(),
                                            message_id: message_id.clone(),
                                            body,
                                        }
                                    )
                                ),
                                (
                                    "Delete message".into(),
                                    EditorCenterMessage::Workspace(
                                        EditorMessage::DeleteCommentMessage {
                                            thread_id: edit_thread.clone(),
                                            message_id,
                                        }
                                    )
                                ),
                                (
                                    "Delete thread".into(),
                                    EditorCenterMessage::Workspace(
                                        EditorMessage::RequestDeleteCommentThread(edit_thread)
                                    )
                                ),
                            ],
                            theme,
                            168.0,
                            |open| EditorCenterMessage::Workspace(
                                EditorMessage::SetCommentActionsOpen(open)
                            ),
                        )
                    ),
                ]
                .spacing(6),
            );
        }
    }

    if workspace
        .editing_comment_message()
        .is_none_or(|(editing_thread, _)| editing_thread != thread_id)
    {
        let reply_thread = thread_id.clone();
        card = card.push(
            text_editor(
                workspace
                    .comment_reply_draft(&thread_id)
                    .expect("every rendered comment thread has a reply draft"),
            )
            .id(HarnessTarget::CommentReply.id())
            .key_binding({
                let thread_id = thread_id.clone();
                move |press| {
                    comment_key_binding(
                        press,
                        EditorMessage::SubmitCommentReply {
                            thread_id: thread_id.clone(),
                        },
                    )
                }
            })
            .placeholder("Reply…")
            .on_action(move |action| {
                EditorCenterMessage::Workspace(EditorMessage::EditCommentReplyDraft {
                    thread_id: reply_thread.clone(),
                    action,
                })
            })
            .height(Length::Fixed(76.0))
            .style(move |_, status| multiline_field_style(theme, status)),
        );
    }

    if workspace.editing_comment_message().is_none() {
        card = card.push(row![
            Space::new().width(Length::Fill),
            comment_popover_action(
                "Reply",
                EditorCenterMessage::Workspace(EditorMessage::SubmitCommentReply {
                    thread_id: thread_id.clone()
                }),
                theme,
            ),
        ]);
    }
    if workspace.pending_delete_comment() == Some(thread_id.as_str()) {
        card = card.push(
            row![
                text("Delete this thread?").size(11),
                comment_popover_action(
                    "Confirm delete",
                    EditorCenterMessage::Workspace(EditorMessage::ConfirmDeleteCommentThread),
                    theme,
                ),
                comment_popover_action(
                    "Cancel",
                    EditorCenterMessage::Workspace(EditorMessage::CancelDeleteCommentThread),
                    theme,
                ),
            ]
            .spacing(6),
        );
    }
    if let Some(feedback) = workspace.comment_feedback().map(str::to_owned) {
        card = card.push(
            text(feedback)
                .size(11)
                .color(theme.palette().secondary_text),
        );
    }
    card.into()
}

fn anchored_comment_overlay<'a>(
    content: Element<'a, EditorCenterMessage>,
    anchor: crate::Rect,
    card: Element<'a, EditorCenterMessage>,
    hover: bool,
    theme: ParchMintTheme,
) -> Element<'a, EditorCenterMessage> {
    crate::anchored_popover::anchored(
        content,
        container(card)
            .width(320)
            .padding(10)
            .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Rest))
            .into(),
        anchor,
        if hover {
            crate::anchored_popover::Dismissal::Hover
        } else {
            crate::anchored_popover::Dismissal::Explicit
        },
        EditorCenterMessage::Workspace(if hover {
            EditorMessage::DismissCommentHover
        } else {
            EditorMessage::CancelCommentComposer
        }),
    )
}

fn comment_popover_action(
    label: impl Into<String>,
    message: EditorCenterMessage,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let label = label.into();
    let kind = match label.as_str() {
        "Reply" | "Add comment" | "Save edit" => ButtonKind::Primary,
        "Confirm delete" => ButtonKind::Destructive,
        _ => ButtonKind::Quiet,
    };
    button(text(label).size(12))
        .padding([5, 9])
        .on_press(message)
        .style(move |_, status| {
            components::button_style(theme, kind, button_interaction(status, false))
        })
        .into()
}

#[allow(clippy::too_many_arguments)]
fn tab_strip(
    positions: crate::motion::Positions,
    state: &EditorPaneState,
    pane: EditorPane,
    focused: bool,
    expanded: bool,
    show_companion_toggle: bool,
    drag_source: Option<&str>,
    drag_target: Option<usize>,
    breadcrumb: Vec<String>,
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let tabs = state.tabs().to_vec();
    let active_document = state.active_document().map(str::to_owned);
    let drag_source = drag_source.map(str::to_owned);
    responsive(move |available| {
        tab_strip_for_width(
            positions.clone(),
            &tabs,
            active_document.as_deref(),
            pane,
            focused,
            expanded,
            show_companion_toggle,
            drag_source.as_deref(),
            drag_target,
            available.width,
            &breadcrumb,
            theme,
        )
    })
    .width(Length::Fill)
    .height(36)
    .into()
}

#[allow(clippy::too_many_arguments)]
fn tab_strip_for_width(
    positions: crate::motion::Positions,
    tabs: &[TabSpec],
    active_document: Option<&str>,
    pane: EditorPane,
    focused: bool,
    expanded: bool,
    show_companion_toggle: bool,
    drag_source: Option<&str>,
    drag_target: Option<usize>,
    available_width: f32,
    breadcrumb: &[String],
    theme: ParchMintTheme,
) -> Element<'static, EditorCenterMessage> {
    let layout = EditorWorkspace::tab_strip_layout(
        (available_width - if show_companion_toggle { 104.0 } else { 72.0 }).max(0.0),
        tabs,
        active_document.unwrap_or_default(),
    );
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for tab in tabs {
        tab.id().hash(&mut hash);
    }
    active_document.hash(&mut hash);
    let generation = hash.finish();
    positions.retain(
        &layout
            .tabs()
            .iter()
            .map(|tab| tabs[tab.source_index()].id())
            .collect::<Vec<_>>(),
    );
    let tabs = layout
        .tabs()
        .iter()
        .fold(row![].align_y(Vertical::Center), |row, presentation| {
            let index = presentation.source_index();
            let tab = &tabs[index];
            row.push(crate::motion::reflow(
                positions.clone(),
                tab.id(),
                generation,
                true,
                tab_button(
                    tab,
                    presentation,
                    (presentation.is_active() && !breadcrumb.is_empty())
                        .then(|| breadcrumb.join(" > ")),
                    TabButtonContext {
                        pane,
                        index,
                        focused,
                        drag_source: drag_source == Some(tab.id()),
                        drag_target: drag_target == Some(index),
                        theme,
                    },
                ),
            ))
        });
    let tabs = if layout.overflow_tabs().is_empty() {
        tabs
    } else {
        let trigger = button(
            container(
                row![
                    text(format!("{}", layout.overflow_tabs().len())).size(12),
                    icon_sized(Icon::ChevronDown, 10),
                ]
                .spacing(4)
                .align_y(Vertical::Center),
            )
            .center(Length::Fill),
        )
        .width(44)
        .height(32)
        .padding(0)
        .on_press(())
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, button_interaction(status, false))
        });
        tabs.push(stationary_tooltip::tooltip(
            harness_target::target(
                HarnessTarget::TabOverflow(pane),
                crate::action_menu::anchored_menu(
                    trigger.into(),
                    layout
                        .overflow_tabs()
                        .iter()
                        .map(|tab| {
                            (
                                tab.to_string(),
                                EditorCenterMessage::Workspace(EditorMessage::ActivateTab {
                                    pane,
                                    document_id: tab.id().to_owned(),
                                }),
                            )
                        })
                        .collect(),
                    theme,
                    280.0,
                ),
            ),
            container(text("More tabs").size(12)).padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        ))
    };
    let tabs = tabs
        .push(stationary_tooltip::tooltip(
            harness_target::target(
                HarnessTarget::NewTab(pane),
                button(container(text("+").size(22)).center(Length::Fill))
                    .width(32)
                    .height(32)
                    .padding(0)
                    .on_press(EditorCenterMessage::NewTab(pane))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            button_interaction(status, false),
                        )
                    }),
            ),
            container(
                text({
                    let binding = crate::shortcut_router::label("file.new-tab");
                    if binding.is_empty() {
                        "New draft".into()
                    } else {
                        format!("New draft ({binding})")
                    }
                })
                .size(12),
            )
            .padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        ))
        .push(Space::new().width(Length::Fill))
        .push(Space::new().width(if show_companion_toggle { 32 } else { 0 }))
        .push(stationary_tooltip::tooltip(
            harness_target::target(
                HarnessTarget::PaneFocus(pane),
                button(
                    container(icon_sized(
                        if expanded {
                            Icon::RestoreLayout
                        } else {
                            Icon::FocusWriting
                        },
                        16,
                    ))
                    .center(Length::Fill),
                )
                .width(32)
                .height(32)
                .padding(0)
                .on_press_maybe(
                    active_document.map(|_| {
                        EditorCenterMessage::Workspace(EditorMessage::TogglePaneFocus(pane))
                    }),
                )
                .style(move |_, status| {
                    components::button_style(
                        theme,
                        ButtonKind::Quiet,
                        button_interaction(status, expanded),
                    )
                }),
            ),
            container(
                text(if expanded {
                    "Exit focus"
                } else {
                    "Focus document"
                })
                .size(12),
            )
            .padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        ));
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
    location: Option<String>,
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
    let title_font = crate::editor_workspace::tab_title_font(tab.is_preview());
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
    let drag_id = id.clone();
    let drag_width = presentation.bounds().width();
    let activate = hierarchy_drag::source_with_pointer(
        format!("tab-{pane:?}-{id}"),
        activate,
        EditorCenterMessage::Workspace(EditorMessage::ActivateTab {
            pane,
            document_id: id.clone(),
        }),
        None,
        move |point, bounds| {
            EditorCenterMessage::Workspace(EditorMessage::BeginTabPointerDrag {
                pane,
                document_id: drag_id.clone(),
                grab_offset: crate::Point::new(point.x - bounds.x, point.y - bounds.y),
                width: drag_width,
            })
        },
    );
    let activate =
        if let Some(tooltip) = location.or_else(|| presentation.tooltip().map(str::to_owned)) {
            stationary_tooltip::tooltip(
                activate,
                container(text(tooltip).size(12)).padding([4, 6]),
                components::surface(theme, Surface::Elevated, Interaction::Rest),
            )
        } else {
            activate
        };
    let close = button(icon_sized(Icon::Close, 16))
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
        harness_target::target_id(
            iced::widget::Id::from(format!("tab-close-{pane:?}-{id}")),
            close,
        ),
        container(text(format!("Close {}", presentation.full_title())).size(12)).padding([4, 6]),
        components::surface(theme, Surface::Elevated, Interaction::Rest),
    );
    let context_document = id.clone();
    harness_target::target_id(
        harness_target::editor_tab_id(pane, &id),
        right_click::right_click_area(
            mouse_area(
                container(
                    column![
                        row![activate, close]
                            .width(Length::Fill)
                            .height(Length::Fill),
                        container(Space::new())
                            .height(2)
                            .width(Length::Fill)
                            .style(move |_| tab_underline_style(theme, active, focused)),
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
            }),
            move |point| EditorCenterMessage::OpenDocumentContext {
                pane,
                document_id: context_document.clone(),
                point: crate::Point::new(point.x, point.y),
            },
        ),
    )
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

fn tab_underline_style(
    theme: ParchMintTheme,
    active: bool,
    focused: bool,
) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: active.then_some(Background::Color(if focused {
            theme.palette().accent
        } else {
            theme.palette().strong_border
        })),
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
        iced::widget::container::Style {
            background: Some(theme.palette().control_hover.into()),
            border: iced::Border {
                color: theme.palette().strong_border,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        }
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
        find_icon_button(
            "Previous",
            "↑",
            pane,
            EditorMessage::NavigateFind(FindDirection::Previous),
            theme,
            matches > 0,
            false
        ),
        find_icon_button(
            "Next",
            "↓",
            pane,
            EditorMessage::NavigateFind(FindDirection::Next),
            theme,
            matches > 0,
            false
        ),
        find_icon_button(
            "Match case",
            "Aa",
            pane,
            EditorMessage::SetFindOptions {
                case_sensitive: !case_sensitive,
                whole_word
            },
            theme,
            true,
            case_sensitive
        ),
        find_icon_button(
            "Whole words",
            "ab",
            pane,
            EditorMessage::SetFindOptions {
                case_sensitive,
                whole_word: !whole_word
            },
            theme,
            true,
            whole_word
        ),
        button(text("Replace…").size(12))
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
    ]
    .spacing(4)
    .align_y(Vertical::Center);
    let content = if replace_visible {
        column![
            row![query, controls].spacing(4).align_y(Vertical::Center),
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
                    matches > 0,
                ),
                find_button(
                    "Replace all",
                    pane,
                    EditorMessage::ReplaceAllMatches(draft.clone()),
                    theme,
                    matches > 0,
                ),
            ]
            .spacing(4)
            .align_y(Vertical::Center),
        ]
        .spacing(4)
    } else {
        column![row![query, controls].spacing(4).align_y(Vertical::Center)]
    };
    let mut body = column![content].spacing(3);
    if !query_value.is_empty() {
        body = body.push(
            text(if matches == 0 {
                "No matches in this document.".to_owned()
            } else {
                format!(
                    "{} of {matches} {}",
                    search.active_match_position().unwrap_or(0),
                    if matches == 1 { "match" } else { "matches" }
                )
            })
            .size(11)
            .color(theme.palette().secondary_text),
        );
    }
    container(body)
        .padding([4, 6])
        .width(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Rest))
        .into()
}

fn find_icon_button(
    label: &'static str,
    glyph: &'static str,
    pane: EditorPane,
    message: EditorMessage,
    theme: ParchMintTheme,
    enabled: bool,
    selected: bool,
) -> Element<'static, EditorCenterMessage> {
    let symbol = icon_sized(
        match glyph {
            "↑" => Icon::PreviousMatch,
            "↓" => Icon::NextMatch,
            "Aa" => Icon::MatchCase,
            _ => Icon::WholeWords,
        },
        20,
    );
    stationary_tooltip::tooltip(
        harness_target::target_id(
            iced::widget::Id::from(format!("find-{pane:?}-{label}")),
            button(container(symbol).center(24))
                .padding(2)
                .on_press_maybe(
                    enabled.then_some(EditorCenterMessage::PaneWorkspace { pane, message }),
                )
                .style(move |_, status| {
                    components::button_style(
                        theme,
                        ButtonKind::Quiet,
                        button_interaction(status, selected),
                    )
                }),
        ),
        text(label).size(12),
        components::surface(theme, Surface::Elevated, Interaction::Rest),
    )
}

fn find_button(
    label: &'static str,
    pane: EditorPane,
    message: EditorMessage,
    theme: ParchMintTheme,
    enabled: bool,
) -> iced::widget::Button<'static, EditorCenterMessage> {
    button(text(label).size(12))
        .padding([5, 7])
        .on_press_maybe(enabled.then_some(EditorCenterMessage::PaneWorkspace { pane, message }))
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
            let mount_generation = state.mount_generation();
            host.element_on_manuscript_surface()
                .map(move |message| EditorCenterMessage::Mounted {
                    pane,
                    view,
                    mount_generation,
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

fn tab_interaction(
    status: iced::widget::button::Status,
    active: bool,
    pane_focused: bool,
) -> Interaction {
    match status {
        iced::widget::button::Status::Active if active && pane_focused => Interaction::Selected,
        iced::widget::button::Status::Active if active => Interaction::Focused,
        _ => button_interaction(status, false),
    }
}

#[cfg(test)]
mod tests {
    use iced::{Point, Settings, Size};
    use iced_test::Simulator;
    use parchmint_domain::DocumentId;
    use parchmint_editor_api::{
        BlockId, CanonicalComment, CanonicalDocumentLoad, CommentId, EditorAdapter as _,
        EditorCommand as AdapterEditorCommand, EditorCommandKind, EditorCommandOrigin,
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
                    ..
                } => {
                    if let MountedEditorMessage::HoverComment {
                        comment_id,
                        anchor_bounds,
                        ..
                    } = message
                    {
                        effects.extend(workspace.update(EditorMessage::SetCommentHover {
                            pane,
                            comment_id,
                            anchor_bounds: Rect::new(
                                anchor_bounds.0,
                                anchor_bounds.1,
                                anchor_bounds.2,
                                anchor_bounds.3,
                            ),
                        }));
                    } else {
                        if !matches!(message, MountedEditorMessage::ViewportChanged(_)) {
                            effects.extend(workspace.update(EditorMessage::FocusPane(pane)));
                        }
                        slots
                            .update_mounted(pane, view, message)
                            .expect("rendered mounted message reaches its retained host");
                    }
                }
                EditorCenterMessage::SetReplaceDraft { pane, value } => {
                    slots.set_replace_draft(pane, value);
                }
                // The project shell owns hierarchy drag state. Pointer entry
                // and release during these editor flows legitimately publish
                // these surface-level signals without changing editor state.
                EditorCenterMessage::HierarchyDropTarget(_)
                | EditorCenterMessage::ClearHierarchyDropTarget(_) => {}
                unsupported @ (EditorCenterMessage::Scratch { .. }
                | EditorCenterMessage::ManageStyles
                | EditorCenterMessage::NewTab(_)
                | EditorCenterMessage::OpenDocumentContext { .. }
                | EditorCenterMessage::BeginComment
                | EditorCenterMessage::BeginSplitResize
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
    fn expanded_formatting_toolbar_fits_without_an_overflow_panel() {
        let workspace = EditorWorkspace::from_fixture(crate::EditorFixture::DualPane);
        let mut simulator = iced_test::Simulator::with_size(
            iced::Settings::default(),
            iced::Size::new(740.0, 52.0),
            formatting_toolbar_for_width(
                &workspace,
                ParchMintTheme::new(parchmint_preferences::ResolvedAppearance::Light),
                true,
            ),
        );
        assert!(simulator.find(HarnessTarget::FormattingMenu.id()).is_err());
        for target in [
            HarnessTarget::ParagraphStyle,
            HarnessTarget::FontFamily,
            HarnessTarget::FontSize,
            HarnessTarget::Bold,
            HarnessTarget::BreakMenu,
        ] {
            let bounds = simulator.find(target.id()).unwrap().bounds();
            assert!(bounds.x + bounds.width <= 740.0, "{target:?}: {bounds:?}");
            assert!(bounds.y + bounds.height <= 52.0);
        }
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
    fn compact_toolbar_keeps_usable_targets_on_one_line() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(480.0, 64.0),
            formatting_toolbar(&workspace, theme),
        );

        let bold = simulator.find(HarnessTarget::Bold.id()).unwrap().bounds();
        let lists = simulator
            .find(HarnessTarget::ListBulleted.id())
            .unwrap()
            .bounds();
        assert!(simulator.find(HarnessTarget::AddComment.id()).is_err());
        assert!(simulator.find(HarnessTarget::Link.id()).is_err());
        let breaks = simulator
            .find(HarnessTarget::FormattingMenu.id())
            .unwrap()
            .bounds();
        for bounds in [bold, lists, breaks] {
            assert!(bounds.width >= 32.0 && bounds.height >= 32.0);
        }
        assert!(breaks.y < bold.y + bold.height, "toolbar must fit one line");
        assert!(breaks.x + breaks.width <= 480.0);
    }

    #[test]
    fn companion_control_stays_in_place_when_opening_or_closing_a_pane() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let mut positions = Vec::new();
        for action in [
            None,
            Some(EditorMessage::ToggleCompanion),
            Some(EditorMessage::ToggleCompanion),
        ] {
            if let Some(action) = action {
                workspace.update(action);
            }
            let mut simulator = Simulator::with_size(
                Settings::default(),
                Size::new(1000.0, 700.0),
                editor_center_surface(
                    &workspace,
                    ParchMintTheme::new(ResolvedAppearance::Light),
                    &EditorHostSlots::default(),
                    None,
                ),
            );
            positions.push(
                simulator
                    .find(HarnessTarget::ToggleCompanion.id())
                    .unwrap()
                    .bounds(),
            );
        }
        assert!(positions.iter().all(|bounds| *bounds == positions[0]));
    }

    #[test]
    fn format_panel_keeps_settings_accessible_and_dismisses_cleanly() {
        let _motion = crate::motion::SettledMotion::new();
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            let mut simulator = Simulator::with_size(
                Settings::default(),
                Size::new(480.0, 360.0),
                formatting_toolbar(&workspace, theme),
            );
            assert!(simulator.find(HarnessTarget::FontFamily.id()).is_err());
            simulator.click(HarnessTarget::FormattingMenu.id()).unwrap();
            simulator.click(HarnessTarget::FontFamily.id()).unwrap();
            simulator.tap_key(iced::keyboard::key::Named::ArrowDown);
            simulator.tap_key(iced::keyboard::key::Named::ArrowDown);
            simulator.tap_key(iced::keyboard::key::Named::Enter);
            simulator.click(HarnessTarget::LineSpacing.id()).unwrap();
            simulator.tap_key(iced::keyboard::key::Named::ArrowUp);
            simulator.tap_key(iced::keyboard::key::Named::Enter);
            assert!(simulator.find(HarnessTarget::FontSize.id()).is_ok());
            if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
                assert!(
                    simulator
                        .snapshot(&theme.iced_theme())
                        .unwrap()
                        .matches_image(
                            std::path::PathBuf::from(root)
                                .join(format!("format-panel-{appearance:?}")),
                        )
                        .unwrap()
                );
            }
            simulator.tap_key(iced::keyboard::key::Named::Escape);
            assert!(simulator.find(HarnessTarget::FontSize.id()).is_err());
            simulator.click(HarnessTarget::FormattingMenu.id()).unwrap();
            simulator.point_at(Point::new(20.0, 320.0));
            simulator.simulate(iced_test::simulator::click());
            assert!(simulator.find(HarnessTarget::FontSize.id()).is_err());
            assert_eq!(
                simulator.into_messages().collect::<Vec<_>>(),
                [
                    format_message(FormattingCommand::InlineFont(
                        parchmint_editor_api::InlineFont::Family(Some(
                            parchmint_editor_api::InlineFontFamily::Serif
                        ))
                    )),
                    format_message(FormattingCommand::ParagraphFormat(
                        parchmint_editor_api::ParagraphFormatCommand::LineSpacing(200)
                    )),
                ]
            );
        }
    }

    #[test]
    fn toolbar_menus_select_actions_and_dismiss_without_editing() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(680.0, 240.0),
            formatting_toolbar(&workspace, theme),
        );
        simulator.click(HarnessTarget::ListBulleted.id()).unwrap();
        simulator.click(HarnessTarget::ListMenu.id()).unwrap();
        simulator.click(HarnessTarget::ListNumbered.id()).unwrap();
        simulator.click(HarnessTarget::FormattingMenu.id()).unwrap();
        simulator.click(HarnessTarget::BreakMenu.id()).unwrap();
        simulator.tap_key(iced::keyboard::key::Named::ArrowUp);
        simulator.tap_key(iced::keyboard::key::Named::Enter);
        simulator.click(HarnessTarget::BreakMenu.id()).unwrap();
        simulator.tap_key(iced::keyboard::key::Named::ArrowDown);
        simulator.tap_key(iced::keyboard::key::Named::Enter);
        simulator.click(HarnessTarget::BreakMenu.id()).unwrap();
        simulator.tap_key(iced::keyboard::key::Named::Escape);
        simulator.click(HarnessTarget::BreakMenu.id()).unwrap();
        simulator.point_at(Point::new(20.0, 210.0));
        simulator.simulate(iced_test::simulator::click());
        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            [
                format_message(FormattingCommand::BulletedList),
                format_message(FormattingCommand::NumberedList),
                format_message(FormattingCommand::SceneBreak),
                format_message(FormattingCommand::PageBreak),
            ]
        );
    }

    #[test]
    fn overflow_menu_selects_hidden_documents_and_stays_within_the_pane() {
        let _motion = crate::motion::SettledMotion::new();
        crate::visual_verification::load_test_fonts();
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        for (id, title) in [
            ("one", "The letter"),
            ("two", "A window facing the harbor"),
            ("three", "After midnight"),
            ("four", "Home"),
        ] {
            workspace.update(EditorMessage::OpenTab {
                pane: EditorPane::Primary,
                tab: TabSpec::new(id, title),
            });
        }
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            let mut simulator = Simulator::with_size(
                Settings::default(),
                Size::new(360.0, 280.0),
                tab_strip(
                    Default::default(),
                    workspace.pane(EditorPane::Primary),
                    EditorPane::Primary,
                    true,
                    false,
                    true,
                    None,
                    None,
                    Vec::new(),
                    theme,
                ),
            );
            simulator
                .click(HarnessTarget::TabOverflow(EditorPane::Primary).id())
                .unwrap();
            if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
                assert!(
                    simulator
                        .snapshot(&theme.iced_theme())
                        .unwrap()
                        .matches_image(
                            std::path::PathBuf::from(root)
                                .join(format!("tab-overflow-{appearance:?}")),
                        )
                        .unwrap()
                );
            }
            simulator.tap_key(iced::keyboard::key::Named::ArrowDown);
            simulator.tap_key(iced::keyboard::key::Named::Enter);
            assert_eq!(
                simulator.into_messages().collect::<Vec<_>>(),
                [EditorCenterMessage::Workspace(EditorMessage::ActivateTab {
                    pane: EditorPane::Primary,
                    document_id: "chapter-one".into()
                })]
            );
        }
    }

    #[test]
    fn each_pane_has_a_focus_button_that_fits_narrow_tab_bars() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        for pane in [EditorPane::Primary, EditorPane::Companion] {
            for width in [280.0, 680.0] {
                let state = workspace.pane(pane);
                let mut simulator = Simulator::with_size(
                    Settings::default(),
                    Size::new(width, 36.0),
                    tab_strip(
                        Default::default(),
                        state,
                        pane,
                        true,
                        false,
                        pane == EditorPane::Companion,
                        None,
                        None,
                        Vec::new(),
                        ParchMintTheme::new(ResolvedAppearance::Light),
                    ),
                );
                let target = HarnessTarget::PaneFocus(pane).id();
                let bounds = simulator
                    .find(target.clone())
                    .unwrap()
                    .visible_bounds()
                    .unwrap();
                assert!(bounds.width >= 32.0 && bounds.height >= 32.0);
                assert!(bounds.x + bounds.width <= width);
                assert!(simulator.find("Pane").is_err());
                simulator.click(target).unwrap();
                let messages = simulator.into_messages().collect::<Vec<_>>();
                assert_eq!(
                    messages,
                    [EditorCenterMessage::Workspace(
                        EditorMessage::TogglePaneFocus(pane)
                    )]
                );
            }
        }
    }

    #[test]
    fn mounted_message_keeps_its_pane_and_view_context() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let view = workspace.pane(EditorPane::Companion).view();
        let message = EditorCenterMessage::Mounted {
            pane: EditorPane::Companion,
            view,
            mount_generation: workspace.pane(EditorPane::Companion).mount_generation(),
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
    fn comment_hover_stays_presentation_only_without_stealing_editor_focus() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let view = workspace.pane(EditorPane::Companion).view();
        let message = EditorCenterMessage::Mounted {
            pane: EditorPane::Companion,
            view,
            mount_generation: workspace.pane(EditorPane::Companion).mount_generation(),
            message: MountedEditorMessage::HoverComment {
                link_target: None,
                comment_id: Some("comment".to_owned()),
                anchor_bounds: (24.0, 36.0, 30.0, 14.0),
            },
        };
        assert_eq!(
            message.workspace_messages(),
            vec![
                EditorMessage::SetHoveredLink(None),
                EditorMessage::SetCommentHover {
                    pane: EditorPane::Companion,
                    comment_id: Some("comment".to_owned()),
                    anchor_bounds: Rect::new(24.0, 36.0, 30.0, 14.0),
                }
            ]
        );
    }

    #[test]
    fn comment_hover_popover_owns_thread_actions_without_changing_pane_focus() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::SameDocumentTwoViews);
        let comment_id = "07070707070707070707070707070707".to_owned();
        workspace.reconcile_document_comments(
            "chapter-one",
            &[CanonicalComment::new(
                CommentId::from_bytes([7; 16]),
                parchmint_editor_api::EditorSelection::new(1.into(), 4.into()),
                "Check the weather.",
                BlockId::from_bytes([3; 16]),
            )],
        );
        workspace.update(EditorMessage::SetCommentHover {
            pane: EditorPane::Companion,
            comment_id: Some(comment_id.clone()),
            anchor_bounds: Rect::new(24.0, 36.0, 30.0, 14.0),
        });
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

        simulator
            .click("✓")
            .expect("comment hover popover resolve control");
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert_eq!(
            messages,
            [EditorCenterMessage::Workspace(
                EditorMessage::ToggleCommentResolved {
                    thread_id: comment_id.clone(),
                    resolved: true,
                }
            )]
        );

        let effects = apply_surface_messages(&mut workspace, &mut slots, messages);
        assert_eq!(workspace.focused_pane(), EditorPane::Primary);
        assert!(matches!(
            effects.as_slice(),
            [crate::EditorEffect::Command {
                command: crate::EditorCommand::SetCommentResolved {
                    thread_id: selected,
                    resolved: true,
                },
                ..
            }] if selected == &comment_id
        ));
    }

    #[test]
    fn anchored_comment_composer_routes_creation_without_using_the_inspector() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        workspace.update(EditorMessage::BeginCommentAtSelection {
            pane: EditorPane::Primary,
            anchor_bounds: Rect::new(24.0, 36.0, 30.0, 14.0),
        });
        workspace.update(EditorMessage::SetCommentDraft("A visible note".to_owned()));
        let mut slots = EditorHostSlots::default();
        slots.insert(
            EditorPane::Primary,
            EditorPaneSlot::state(EditorCenterPaneState::Loading),
        );
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );

        assert!(simulator.find(HarnessTarget::CommentDraft.id()).is_ok());
        simulator
            .click("Add comment")
            .expect("anchored comment creation action");
        let messages = simulator.into_messages().collect::<Vec<_>>();
        let effects = apply_surface_messages(&mut workspace, &mut slots, messages);
        assert!(matches!(
            effects.as_slice(),
            [crate::EditorEffect::Command {
                command: crate::EditorCommand::CreateComment {
                    body,
                    document_level: false,
                    ..
                },
                ..
            }] if body == "A visible note"
        ));
        assert!(workspace.comment_composer(EditorPane::Primary).is_some());
        workspace.complete_comment_creation();
        assert!(workspace.comment_composer(EditorPane::Primary).is_none());
    }

    #[test]
    fn anchored_comment_composer_submits_enter_and_keeps_modified_enter_multiline() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        workspace.update(EditorMessage::BeginCommentAtSelection {
            pane: EditorPane::Primary,
            anchor_bounds: Rect::new(24.0, 36.0, 30.0, 14.0),
        });
        let mut slots = EditorHostSlots::default();
        slots.insert(
            EditorPane::Primary,
            EditorPaneSlot::state(EditorCenterPaneState::Loading),
        );
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );

        simulator
            .click(HarnessTarget::CommentDraft.id())
            .expect("focus the multiline comment composer");
        assert_eq!(
            simulator.tap_key(iced::keyboard::key::Named::Enter),
            iced::event::Status::Captured,
            "Enter must submit the comment"
        );
        for modifier in [
            iced::keyboard::Modifiers::SHIFT,
            iced::keyboard::Modifiers::ALT,
        ] {
            let mut event =
                iced_test::simulator::press_key(iced::keyboard::key::Named::Enter, None);
            if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { modifiers, .. }) =
                &mut event
            {
                *modifiers = modifier;
            }
            simulator.simulate([event]);
        }
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(messages.iter().any(|message| matches!(
            message,
            EditorCenterMessage::Workspace(EditorMessage::CreateComment {
                document_level: false
            })
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            EditorCenterMessage::Workspace(EditorMessage::EditCommentDraft(
                text_editor::Action::Edit(text_editor::Edit::Enter)
            ))
        )));
    }

    #[test]
    fn viewport_measurement_reflows_without_stealing_editor_focus() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let view = workspace.pane(EditorPane::Companion).view();
        let message = EditorCenterMessage::Mounted {
            pane: EditorPane::Companion,
            view,
            mount_generation: workspace.pane(EditorPane::Companion).mount_generation(),
            message: MountedEditorMessage::ViewportChanged(
                EditorViewport::new(480.0, 320.0).expect("viewport"),
            ),
        };
        assert!(message.workspace_messages().is_empty());
    }

    #[test]
    fn scrolling_research_does_not_redirect_manuscript_formatting() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let pane = workspace.pane(EditorPane::Companion);
        let message = EditorCenterMessage::Mounted {
            pane: EditorPane::Companion,
            view: pane.view(),
            mount_generation: pane.mount_generation(),
            message: MountedEditorMessage::Scroll {
                delta_y: 80.0,
                viewport: EditorViewport::new(400.0, 300.0).unwrap(),
            },
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
    fn internal_link_groups_only_toggle_children() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        workspace.update(EditorMessage::OpenLinkEditor);
        workspace.update(EditorMessage::SetLinkInternal(true));
        let group = workspace
            .link_outline
            .rows()
            .into_iter()
            .find(|item| item.kind == crate::HierarchyRowKind::Group)
            .unwrap();
        let id = group.id.to_owned();
        let title = group.title.to_owned();
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(600.0, 500.0),
            link_editor_popover(&workspace, ParchMintTheme::new(ResolvedAppearance::Light)),
        );
        simulator.click(title.as_str()).unwrap();
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert_eq!(
            messages,
            vec![EditorCenterMessage::Workspace(
                EditorMessage::ToggleLinkGroup(id)
            )]
        );
        assert!(workspace.link_editor().target().is_empty());
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
    fn secondary_click_on_either_panes_tab_targets_its_document_without_activation() {
        let workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let mut slots = EditorHostSlots::default();
        for pane in [EditorPane::Primary, EditorPane::Companion] {
            slots.insert(pane, EditorPaneSlot::state(EditorCenterPaneState::Loading));
        }
        for pane in [EditorPane::Primary, EditorPane::Companion] {
            let document = workspace.pane(pane).tabs()[0].id();
            let mut simulator = Simulator::with_size(
                Settings::default(),
                Size::new(960.0, 600.0),
                editor_center_surface(
                    &workspace,
                    ParchMintTheme::new(ResolvedAppearance::Light),
                    &slots,
                    None,
                ),
            );
            let bounds = simulator
                .find(harness_target::editor_tab_id(pane, document))
                .unwrap()
                .visible_bounds()
                .unwrap();
            simulator.point_at(bounds.center());
            simulator.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Right,
            ))]);
            let messages = simulator.into_messages().collect::<Vec<_>>();
            assert!(messages.iter().any(|message| matches!(message,
                EditorCenterMessage::OpenDocumentContext { document_id, point, .. }
                    if document_id == document && point.x() == bounds.center_x() && point.y() == bounds.center_y()
            )));
            assert!(!messages.iter().any(|message| matches!(
                message,
                EditorCenterMessage::Workspace(
                    EditorMessage::ActivateTab { .. } | EditorMessage::BeginTabPointerDrag { .. }
                )
            )));
        }
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

        simulator
            .click(iced::widget::Id::new("tab-close-Primary-chapter-one"))
            .expect("primary tab close target");
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
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(apply_surface_messages(&mut workspace, &mut slots, messages).is_empty());
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
            formatting_toolbar(&workspace, theme),
        );
        simulator
            .click(HarnessTarget::Bold.id())
            .expect("rendered bold toolbar button");
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(matches!(
            apply_surface_messages(&mut workspace, &mut slots, messages).as_slice(),
            [
                crate::EditorEffect::Command {
                    view,
                    command: crate::EditorCommand::ToggleBold,
                },
                crate::EditorEffect::RestoreEditorFocus { view: restored_view },
            ] if *view == workspace.pane(EditorPane::Companion).view()
                && *restored_view == workspace.pane(EditorPane::Companion).view()
        ));

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator
            .click("Chapter One")
            .expect("rendered primary tab");
        let messages = simulator.into_messages().collect::<Vec<_>>();
        apply_surface_messages(&mut workspace, &mut slots, messages);
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
    fn breadcrumb_search_toggles_the_originating_panes_search() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let slots = EditorHostSlots::default();
        for expected in [EditorMessage::OpenLocalFind, EditorMessage::CloseLocalFind] {
            let mut surface = Simulator::with_size(
                Settings::default(),
                Size::new(1280.0, 720.0),
                editor_center_surface(
                    &workspace,
                    ParchMintTheme::new(ResolvedAppearance::Light),
                    &slots,
                    None,
                ),
            );
            surface
                .click(iced::widget::Id::from("breadcrumb-search-Primary"))
                .unwrap();
            assert!(surface.into_messages().any(|message| message
                == EditorCenterMessage::PaneWorkspace {
                    pane: EditorPane::Primary,
                    message: expected.clone()
                }));
            workspace.update(expected);
        }
        assert!(
            !workspace
                .local_search(workspace.pane(EditorPane::Primary).view())
                .is_open()
        );
    }

    #[test]
    fn local_find_without_matches_disables_navigation_and_replacement() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        workspace.update(EditorMessage::OpenLocalFind);
        workspace.update(EditorMessage::SetReplaceVisible(true));
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let slots = EditorHostSlots::default();
        let search = workspace.local_search(workspace.pane(EditorPane::Primary).view());
        let mut surface = Simulator::with_size(
            Settings::default(),
            Size::new(440.0, 240.0),
            local_search_bar(search, EditorPane::Primary, theme, &slots),
        );
        assert!(surface.find("Enter text to search this document.").is_err());
        for label in ["Previous", "Next"] {
            surface
                .click(iced::widget::Id::from(format!("find-Primary-{label}")))
                .unwrap();
        }
        for label in ["Replace", "Replace all"] {
            surface.click(label).unwrap();
        }
        assert!(surface.find("Close").is_err());
        assert!(surface.into_messages().next().is_none());
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
        let messages = simulator.into_messages().collect::<Vec<_>>();
        apply_surface_messages(&mut workspace, &mut slots, messages);
        assert_eq!(workspace.local_search(companion_view).query(), "river");
        assert!(workspace.local_search(primary_view).query().is_empty());
        workspace.update(EditorMessage::SetFindMatches(vec![FindMatch::new(0, 5)]));

        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(960.0, 600.0),
            editor_center_surface(&workspace, theme, &slots, None),
        );
        simulator.click("Replace…").expect("visible replace toggle");
        let messages = simulator.into_messages().collect::<Vec<_>>();
        apply_surface_messages(&mut workspace, &mut slots, messages);
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
        let messages = simulator.into_messages().collect::<Vec<_>>();
        apply_surface_messages(&mut workspace, &mut slots, messages);
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
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(matches!(
            apply_surface_messages(&mut workspace, &mut slots, messages).as_slice(),
            [crate::EditorEffect::Command {
                view,
                command: crate::EditorCommand::ReplaceAllFindMatches { replacement },
            }] if *view == companion_view && replacement == "scene"
        ));
        assert_eq!(workspace.focused_pane(), EditorPane::Companion);
        assert!(!workspace.local_search(primary_view).is_open());
    }
    #[test]
    fn focus_mode_animates_the_mounted_pane_to_full_width() {
        use iced::advanced::renderer::Headless;
        use iced::advanced::widget::{Operation, operation};
        use iced_test::{
            Selector,
            runtime::{UserInterface, user_interface},
        };
        crate::motion::set_reduced(false);
        let start = std::time::Instant::now();
        let _clock = crate::motion::FixedTime::new(start);
        let settings = crate::visual_verification::visual_settings();
        crate::visual_verification::load_test_fonts();
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let (_adapter, _session, slots) = shared_document_slots(&workspace);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut renderer = iced::Renderer::new(settings.default_font, settings.default_text_size);
        let size = Size::new(960.0, 600.0);
        let mut ui = UserInterface::build(
            editor_center_surface(&workspace, theme, &slots, None),
            size,
            user_interface::Cache::default(),
            &mut renderer,
        );
        let bounds =
            |ui: &mut UserInterface<'_, EditorCenterMessage, iced::Theme, iced::Renderer>,
             renderer: &iced::Renderer| {
                let mut query = HarnessTarget::EditorPrimary.id().find();
                ui.operate(renderer, &mut operation::black_box(&mut query));
                match query.finish() {
                    operation::Outcome::Some(Some(target)) => target.bounds(),
                    _ => panic!("primary editor must stay mounted"),
                }
            };
        let initial = bounds(&mut ui, &renderer).width;
        let cache = ui.into_cache();
        workspace.update(EditorMessage::TogglePaneFocus(EditorPane::Primary));
        let mut ui = UserInterface::build(
            editor_center_surface(&workspace, theme, &slots, None),
            size,
            cache,
            &mut renderer,
        );
        let mut widths = Vec::new();
        for (name, elapsed) in [("start", 0), ("middle", 80), ("end", 250)] {
            ui.update(
                &[iced::Event::Window(iced::window::Event::RedrawRequested(
                    start + std::time::Duration::from_millis(elapsed),
                ))],
                iced::mouse::Cursor::Unavailable,
                &mut renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Vec::new(),
            );
            widths.push(bounds(&mut ui, &renderer).width);
            ui.draw(
                &mut renderer,
                &theme.iced_theme(),
                &iced::advanced::renderer::Style {
                    text_color: theme.palette().primary_text,
                },
                iced::mouse::Cursor::Unavailable,
            );
            let pixels = renderer.screenshot(Size::new(960, 600), 1.0, theme.palette().panel);
            let ink = pixels
                .chunks_exact(4)
                .enumerate()
                .filter(|(index, pixel)| {
                    let x = index % 960;
                    let y = index / 960;
                    (20..350).contains(&x)
                        && (30..160).contains(&y)
                        && pixel[0] < 150
                        && pixel[1] < 150
                        && pixel[2] < 150
                })
                .count();
            assert!(
                ink > 50,
                "manuscript text must remain painted in the {name} frame (ink={ink})"
            );
            if let Some(folder) = std::env::var_os("PARCHMINT_MOTION_FRAMES") {
                std::fs::create_dir_all(&folder).unwrap();
                let file = std::fs::File::create(
                    std::path::Path::new(&folder).join(format!("focus-{name}.png")),
                )
                .unwrap();
                let mut encoder = png::Encoder::new(file, 960, 600);
                encoder.set_color(png::ColorType::Rgba);
                encoder
                    .write_header()
                    .unwrap()
                    .write_image_data(&pixels)
                    .unwrap();
            }
        }
        assert_eq!(widths[0], initial);
        assert!(widths[1] > initial && widths[1] < widths[2]);
        assert_eq!(
            widths[2], 800.0,
            "focused writing surface retains its readable measure"
        );
    }
}
