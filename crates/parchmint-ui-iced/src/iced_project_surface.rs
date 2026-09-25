//! Private Iced composition for production and deterministic project workspaces.

use crate::components::{
    button_interaction as interaction, field_interaction, multiline_field_style, page_title,
    semantic_pick_list as pick_list, word_count_label,
};

use crate::project_workspace::{EXPLORER_ROW_EXTENT, GlobalSearchRow, SEARCH_ROW_HEIGHT};
use iced::widget::{
    Space, checkbox, column, container, mouse_area, opaque, responsive, rich_text, row, scrollable,
    sensor, span, stack, text, text_editor,
};
use iced::{Background, Border, Color, Element, Font, Length, font};
use parchmint_editor_api::{SemanticBlock, SemanticBlockKind, SemanticInlineMark};
use parchmint_ui_api::HistoryMaintenanceStatus;

use crate::components::{semantic_button as button, semantic_text_input as text_input};

use crate::{
    CommentAnchor, ContentState, DragDestination, EditorMessage, F6Region, HarnessTarget,
    HierarchyItemKind, HierarchyRowKind, MetadataFieldApplicability, MetadataFieldTextKind, Point,
    ProjectMessage, ProjectModal, ProjectWorkspace, ReplacementCheckState,
    ReplacementPreviewRowKind, RestoreLocation, RibbonDestination, SaveState, SelectionGesture,
    SettingsCategory, SettingsDetail, ShellLayout, SidebarSurface, StatusCount, StyleProperty,
    components::{self, ButtonKind, Interaction, Surface},
    design_tokens::{
        ParchMintTheme, RIBBON_HEIGHT, SPACING_4, SPACING_8, SPACING_12, SPACING_16, SPACING_24,
        STATUS_HEIGHT, UI_BODY, UI_COMPACT, UI_HEADING, UI_LABEL, UI_TAB,
    },
    focus, harness_target, hierarchy_drag,
    iced_editor_surface::EditorCenterMessage,
    icons::{Icon, icon_sized},
    right_click, stationary_tooltip,
};

/// Surface events routed to project, editor, and shell reducers.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectSurfaceMessage {
    Project(ProjectMessage),
    EditorCenter(EditorCenterMessage),
    Navigate(RibbonDestination),
    OpenDocumentHistory(String),
    ShowProjectChooser,
    ToggleExplorer,
    ToggleInspector,
    BeginResize(SidebarPanel),
    LoadMoreHistory,
    /// The newly-created Explorer rename field has entered the rendered tree.
    HierarchyRenameShown(String),
    /// The transient metadata-field name control is ready to receive typing.
    MetadataFieldCreationShown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SidebarPanel {
    Explorer,
    Editor,
    Inspector,
}

pub(crate) fn hierarchy_rename_input_id(_node_id: &str) -> iced::widget::Id {
    // Only one Explorer entry can be edited at a time.
    crate::harness_target::HarnessTarget::ExplorerRename.id()
}

pub(crate) fn explorer_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("parchmint.explorer.scroll")
}

pub(crate) fn metadata_field_name_input_id() -> iced::widget::Id {
    HarnessTarget::MetadataFieldName.id()
}

// Divider lines belong to the adjacent sidebar's reference width; they must
// not shrink the manuscript allocation between the 280 px Explorer and 320 px
// Inspector columns.
const SIDEBAR_SPLITTER_WIDTH: u32 = 4;
const HIERARCHY_CONTEXT_MENU_WIDTH: f32 = 240.0;
const CONTEXT_ACTION_HEIGHT: f32 = 28.0;

/// Deterministic first-frame center allocation for reference verification.
///
/// Native layout receives its actual window geometry through `ShellLayout`,
/// while the headless verification host needs the same initial allocation
/// before its viewport sensor has delivered a reflow message.
#[cfg(feature = "visual-verification")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VerificationCenterGeometry {
    pub width: u32,
    pub height: u32,
}

#[cfg(feature = "visual-verification")]
pub(crate) fn verification_center_geometry(
    destination: RibbonDestination,
) -> VerificationCenterGeometry {
    match destination {
        RibbonDestination::Editor => VerificationCenterGeometry {
            width: 840,
            height: 816,
        },
        RibbonDestination::GlobalSearch => VerificationCenterGeometry {
            width: 760,
            height: 848,
        },
        _ => unreachable!("only editor-bearing reference destinations have a center host"),
    }
}

/// Composes the project chrome around an editor-owned center child.
///
/// `destination` is deliberately supplied by the shell rather than stored in
/// `ProjectWorkspace`: workspace state stays independent of window navigation.
#[cfg(test)]
pub(crate) fn project_surface<'a>(
    workspace: &'a ProjectWorkspace,
    destination: RibbonDestination,
    theme: ParchMintTheme,
    editor_child: Element<'a, ProjectSurfaceMessage>,
) -> Element<'a, ProjectSurfaceMessage> {
    let layout = ShellLayout::for_window(1440, 900);
    project_surface_with_layout(
        workspace,
        destination,
        theme,
        editor_child,
        "ParchMint",
        &layout,
        [true; 3],
    )
}

pub(crate) fn native_project_surface<'a>(
    workspace: &'a ProjectWorkspace,
    destination: RibbonDestination,
    theme: ParchMintTheme,
    editor_child: Element<'a, ProjectSurfaceMessage>,
    layout: &ShellLayout,
    inspector_expansion: [bool; 3],
) -> Element<'a, ProjectSurfaceMessage> {
    project_surface_with_layout(
        workspace,
        destination,
        theme,
        editor_child,
        workspace.project_title(),
        layout,
        inspector_expansion,
    )
}

/// Visual references have a fixed project name, while native integration owns
/// the live window title. Keeping the fixture label here avoids substituting
/// a verification name for a production project.
#[cfg(feature = "visual-verification")]
pub(crate) fn verification_project_surface<'a>(
    workspace: &'a ProjectWorkspace,
    destination: RibbonDestination,
    theme: ParchMintTheme,
    editor_child: Element<'a, ProjectSurfaceMessage>,
) -> Element<'a, ProjectSurfaceMessage> {
    let layout = ShellLayout::for_window(1440, 900);
    project_surface_with_layout(
        workspace,
        destination,
        theme,
        editor_child,
        "The Glass Harbor",
        &layout,
        [true; 3],
    )
}

fn project_surface_with_layout<'a>(
    workspace: &'a ProjectWorkspace,
    destination: RibbonDestination,
    theme: ParchMintTheme,
    editor_child: Element<'a, ProjectSurfaceMessage>,
    project_title: &'a str,
    layout: &ShellLayout,
    inspector_expansion: [bool; 3],
) -> Element<'a, ProjectSurfaceMessage> {
    let mut effective_layout = layout.clone();
    if destination == RibbonDestination::Cards {
        effective_layout.set_explorer_visible(false);
        effective_layout.set_inspector_visible(false);
    }
    if destination == RibbonDestination::Editor && workspace.editor().expanded_pane().is_some() {
        effective_layout.set_explorer_visible(false);
        effective_layout.set_inspector_visible(false);
    }
    let layout = &effective_layout;
    let ribbon = ribbon(
        workspace,
        project_title,
        destination,
        theme,
        layout.center().width().saturating_sub(48),
    );
    let center = column![
        ribbon,
        center_view(workspace, destination, theme, editor_child, layout)
    ];
    let center: Element<'a, ProjectSurfaceMessage> = if destination == RibbonDestination::Cards {
        center.into()
    } else {
        crate::motion::enter(format!("{destination:?}"), center)
    };
    let recovering = matches!(workspace.content_state(), ContentState::Recovery);
    let shows_explorer = !recovering
        && matches!(
            destination,
            RibbonDestination::Editor | RibbonDestination::GlobalSearch
        );
    let shows_inspector = !recovering
        && !workspace.replacement_preview().uses_middle_pane()
        && matches!(
            destination,
            RibbonDestination::Editor | RibbonDestination::GlobalSearch
        );
    let shows_status = !recovering
        && !(destination == RibbonDestination::Editor
            && workspace.editor().expanded_pane().is_some())
        && matches!(
            destination,
            RibbonDestination::Editor | RibbonDestination::Cards
        );
    let rail_width = if destination == RibbonDestination::GlobalSearch {
        360
    } else {
        layout.explorer().width()
    };
    // Overview never exposes either sidebar. Building the Explorer tree and
    // Comments inspector here still runs on every scroll event, even though
    // the instant row excludes both widgets from layout and drawing.
    let explorer_panel: Element<'a, ProjectSurfaceMessage> =
        if destination == RibbonDestination::Cards {
            Space::new().into()
        } else {
            row![
                left_rail(
                    workspace,
                    project_title,
                    theme,
                    rail_width.saturating_sub(SIDEBAR_SPLITTER_WIDTH),
                    true,
                    layout.requested_height as f32,
                ),
                sidebar_splitter(SidebarPanel::Explorer, theme)
            ]
            .into()
        };
    let inspector_panel: Element<'a, ProjectSurfaceMessage> =
        if destination == RibbonDestination::Cards {
            Space::new().into()
        } else {
            row![
                sidebar_splitter(SidebarPanel::Inspector, theme),
                inspector(
                    workspace,
                    theme,
                    layout
                        .inspector_width()
                        .saturating_sub(SIDEBAR_SPLITTER_WIDTH),
                    inspector_expansion,
                    false
                )
            ]
            .into()
        };
    let body_slots = vec![
        crate::motion::slot(
            explorer_panel,
            Length::Fixed(rail_width as f32),
            shows_explorer && layout.explorer_is_visible(),
        ),
        crate::motion::slot(center, Length::Fill, true),
        crate::motion::slot(
            inspector_panel,
            Length::Fixed(layout.inspector().width() as f32),
            shows_inspector && layout.inspector_is_visible(),
        ),
    ];
    let body = if destination == RibbonDestination::Cards {
        crate::motion::row_instant(body_slots)
    } else {
        crate::motion::row(body_slots)
    };
    let mut content = column![body]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill);
    if shows_status {
        content = content.push(status_bar(
            workspace,
            theme,
            shows_explorer && layout.explorer_is_visible(),
            shows_inspector && layout.inspector_is_visible(),
            destination == RibbonDestination::Editor,
        ));
    }
    let focused =
        destination == RibbonDestination::Editor && workspace.editor().expanded_pane().is_some();
    let content = crate::motion::row(vec![
        crate::motion::slot(
            navigation_rail(destination, theme),
            Length::Fixed(48.0),
            !focused,
        ),
        crate::motion::slot(content, Length::Fill, true),
    ]);
    let base: Element<'a, ProjectSurfaceMessage> = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Application, Interaction::Rest))
        .into();
    let base = stack![
        base,
        container(
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(move |_| iced::widget::container::Style {
                    background: Some(theme.palette().divider.into()),
                    ..Default::default()
                })
        )
        .padding(iced::Padding {
            top: 48.0,
            ..iced::Padding::ZERO
        })
        .width(if shows_explorer && layout.explorer_is_visible() {
            Length::Fixed(48.0 + rail_width as f32)
        } else {
            Length::Fixed(48.0)
        }),
        crate::motion::reveal(
            !focused,
            project_selector(
                project_title,
                shows_explorer && layout.explorer_is_visible(),
                rail_width,
                theme
            )
        )
    ];
    let base: Element<'a, ProjectSurfaceMessage> = base.into();
    let base = if let Some(source) = workspace.hierarchy_drag_source()
        && !workspace.hierarchy_drag_is_card()
        && let Some(item) = workspace.explorer().row(source)
    {
        let ghost = container(
            row![
                icon_sized(Icon::Project, 16),
                text(item.title.to_owned()).size(13)
            ]
            .spacing(8)
            .align_y(iced::alignment::Vertical::Center),
        )
        .padding([9, 12])
        .width(240)
        .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Focused));
        stack![
            base,
            crate::drag_ghost::floating(ghost.into(), Point::new(-12.0, -12.0), 240.0)
        ]
        .into()
    } else if destination == RibbonDestination::Cards {
        if let Some(source) = workspace.hierarchy_drag_source()
            && let Some(item) = workspace.cards().item_by_id(source)
        {
            stack![
                base,
                crate::drag_ghost::floating(
                    outline_card(
                        workspace,
                        theme,
                        item,
                        workspace.card_drag_geometry().1,
                        false,
                        &hierarchy_drag::targets(),
                        0,
                        true
                    ),
                    workspace.card_drag_geometry().0,
                    workspace.card_drag_geometry().1,
                )
            ]
            .into()
        } else {
            stack![base].into()
        }
    } else if let Some((tab, offset, width)) = workspace.editor().dragged_tab() {
        stack![
            base,
            iced::widget::canvas(crate::drag_ghost::DragGhost::tab(tab, offset, width, theme))
                .width(Length::Fill)
                .height(Length::Fill)
        ]
        .into()
    } else {
        stack![base].into()
    };
    // Context menus belong to the project surface, not the Explorer rail.
    // This lets a menu extend over the editor when there is room, and keeps
    // its anchor in the same window coordinate space as the right-click.
    let base = hierarchy_context_overlay(
        workspace,
        base,
        theme,
        layout.requested_width() as f32,
        layout.requested_height as f32,
    );
    if matches!(workspace.content_state(), ContentState::Recovery) {
        stack![
            base,
            opaque(
                container(crate::motion::enter(
                    "recovery",
                    recovery_modal(workspace, theme)
                ))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(|_| iced::widget::container::Style::default())
            )
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if let Some(modal) = workspace.modal() {
        let static_settings = matches!(&modal, ProjectModal::ManageSettings(_));
        let dragged_field = matches!(
            &modal,
            ProjectModal::ManageSettings(SettingsCategory::Metadata)
        )
        .then(|| workspace.settings().metadata_drag_source())
        .flatten()
        .and_then(|id| workspace.settings().metadata_field(id))
        .map(|field| field.label.to_owned());
        let dialog = modal_view(modal, workspace, theme);
        let dialog = if static_settings {
            dialog
        } else {
            crate::motion::enter("dialog", dialog)
        };
        let scoped_base = focus::input_scope(base, false);
        let modal_layer = opaque(
            container(dialog)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(move |_| components::scrim(theme)),
        );
        if let Some(label) = dragged_field {
            let ghost = container(
                row![icon_sized(Icon::ReorderGrip, 18), text(label).size(14)]
                    .spacing(8)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .padding([9, 12])
            .width(220)
            .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Focused));
            stack![
                scoped_base,
                modal_layer,
                crate::drag_ghost::floating(ghost.into(), Point::new(16.0, 12.0), 220.0)
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            stack![scoped_base, modal_layer]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    } else {
        base
    }
}

fn sidebar_splitter(
    panel: SidebarPanel,
    theme: ParchMintTheme,
) -> Element<'static, ProjectSurfaceMessage> {
    let divider: Element<'static, ProjectSurfaceMessage> = if panel == SidebarPanel::Explorer {
        column![
            container(Space::new().height(48)).style(move |_| components::surface(
                theme,
                Surface::Sidebar,
                Interaction::Rest
            )),
            panel_divider(theme, panel),
        ]
        .spacing(0)
        .height(Length::Fill)
        .into()
    } else {
        panel_divider(theme, panel)
    };
    mouse_area(divider)
        .on_press(ProjectSurfaceMessage::BeginResize(panel))
        .interaction(iced::mouse::Interaction::ResizingHorizontally)
        .into()
}

fn panel_divider(
    theme: ParchMintTheme,
    panel: SidebarPanel,
) -> Element<'static, ProjectSurfaceMessage> {
    let adjoining: Element<'static, ProjectSurfaceMessage> = if panel == SidebarPanel::Explorer {
        container(Space::new())
            .width(SIDEBAR_SPLITTER_WIDTH - 1)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest))
            .into()
    } else {
        column![
            container(Space::new())
                .width(SIDEBAR_SPLITTER_WIDTH - 1)
                .height(80)
                .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
            container(Space::new())
                .width(SIDEBAR_SPLITTER_WIDTH - 1)
                .height(Length::Fill)
                .style(move |_| {
                    components::surface(theme, Surface::Manuscript, Interaction::Rest)
                }),
        ]
        .spacing(0)
        .into()
    };
    container(
        row![
            adjoining,
            container(Space::new())
                .width(1)
                .height(Length::Fill)
                .style(move |_| iced::widget::container::Style {
                    background: Some(Background::Color(theme.palette().divider)),
                    ..Default::default()
                }),
        ]
        .spacing(0),
    )
    .width(SIDEBAR_SPLITTER_WIDTH)
    .height(Length::Fill)
    .into()
}

fn static_divider(theme: ParchMintTheme) -> Element<'static, ProjectSurfaceMessage> {
    container(Space::new())
        .width(1)
        .height(Length::Fill)
        .style(move |_| iced::widget::container::Style {
            background: Some(Background::Color(theme.palette().divider)),
            ..Default::default()
        })
        .into()
}

fn project_selector<'a>(
    title: &'a str,
    expanded: bool,
    width: u32,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let label = row![
        container(text(title).size(14).wrapping(text::Wrapping::None))
            .width(width.saturating_sub(40))
            .clip(true),
        icon_sized(Icon::ChevronDown, 12)
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Center);
    let contents = crate::motion::row(vec![
        crate::motion::slot(
            container(crate::icons::brand(36)).center(48),
            Length::Fixed(48.0),
            true,
        ),
        crate::motion::slot(
            container(label)
                .width(Length::Fill)
                .padding(iced::Padding {
                    left: 8.0,
                    right: 12.0,
                    ..iced::Padding::ZERO
                })
                .center_y(48)
                .clip(true),
            Length::Fixed(width as f32),
            expanded,
        ),
    ]);
    container(stationary_tooltip::tooltip(
        harness_target::target(
            HarnessTarget::ProjectMenu,
            button(
                container(contents)
                    .width(if expanded { 48.0 + width as f32 } else { 48.0 })
                    .height(48),
            )
            .padding(0)
            .on_press(ProjectSurfaceMessage::ShowProjectChooser)
            .style(move |_, status| {
                components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
            }),
        ),
        components::muted_label(components::tooltip_label("Projects", "file.projects")),
        components::surface(theme, Surface::Elevated, Interaction::Rest),
    ))
    .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest))
    .into()
}

fn navigation_rail<'a>(
    destination: RibbonDestination,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let mut items = column![Space::new().height(36), Space::new().height(8)].spacing(6);
    for (page, symbol, label, command) in [
        (
            RibbonDestination::Editor,
            Icon::Editor,
            "Editor",
            "view.editor",
        ),
        (
            RibbonDestination::Cards,
            Icon::Overview,
            "Overview",
            "view.overview",
        ),
        (
            RibbonDestination::History,
            Icon::History,
            "History",
            "view.history",
        ),
        (
            RibbonDestination::RecentlyDeleted,
            Icon::RecentlyDeleted,
            "Recently deleted",
            "view.deleted",
        ),
        (
            RibbonDestination::Export,
            Icon::Export,
            "Export",
            "view.export",
        ),
        (
            RibbonDestination::Settings,
            Icon::Settings,
            "Settings",
            "view.settings",
        ),
    ] {
        items = items.push(stationary_tooltip::tooltip(
            harness_target::target(
                HarnessTarget::Ribbon(page),
                button(container(icon_sized(symbol, 24)).center(36))
                    .padding(0)
                    .on_press(ProjectSurfaceMessage::Navigate(page))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(
                                status,
                                destination == page
                                    || page == RibbonDestination::Editor
                                        && destination == RibbonDestination::GlobalSearch,
                            ),
                        )
                    }),
            ),
            container(text(components::tooltip_label(label, command)).size(12)).padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        ));
    }
    focus::f6_region(
        F6Region::ModeSwitch,
        row![
            container(items)
                .padding(iced::Padding {
                    top: 8.0,
                    bottom: 8.0,
                    left: 5.0,
                    right: 6.0,
                })
                .width(47)
                .height(Length::Fill)
                .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
            column![
                Space::new().height(48),
                container(Space::new())
                    .width(1)
                    .height(Length::Fill)
                    .style(move |_| iced::widget::container::Style {
                        background: Some(Background::Color(theme.palette().divider)),
                        ..Default::default()
                    })
            ]
            .spacing(0),
        ],
    )
}

fn ribbon<'a>(
    workspace: &'a ProjectWorkspace,
    _project_title: &'a str,
    destination: RibbonDestination,
    theme: ParchMintTheme,
    width: u32,
) -> Element<'a, ProjectSurfaceMessage> {
    let editing = matches!(
        destination,
        RibbonDestination::Editor | RibbonDestination::GlobalSearch
    );
    if !editing {
        return Space::new().height(0).into();
    }
    let expanded = workspace.editor().expanded_pane();
    let tools = focus::f6_region(
        F6Region::FormattingToolbar,
        scrollable(
            crate::iced_editor_surface::formatting_toolbar_for_width(
                workspace.editor(),
                theme,
                width.saturating_sub(if expanded.is_some() { 148 } else { 24 }) >= 740,
            )
            .map(ProjectSurfaceMessage::EditorCenter),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(3).scroller_width(3),
        ))
        .width(Length::Fill),
    );
    let mut content = row![tools]
        .align_y(iced::alignment::Vertical::Center)
        .spacing(8);
    if let Some(pane) = expanded {
        content = content.push(focus::f6_region(
            F6Region::ModeSwitch,
            harness_target::target(
                HarnessTarget::PaneFocus(pane),
                button(
                    row![
                        icon_sized(Icon::RestoreLayout, 16),
                        text("Exit focus").size(13)
                    ]
                    .spacing(6),
                )
                .on_press(ProjectSurfaceMessage::EditorCenter(
                    EditorCenterMessage::Workspace(crate::EditorMessage::TogglePaneFocus(pane)),
                ))
                .style(move |_, status| {
                    components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
                }),
            ),
        ));
    }
    container(content)
        .padding([6, 12])
        .height(f32::from(RIBBON_HEIGHT))
        .width(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest))
        .into()
}

fn mode_switch_button_style(
    theme: ParchMintTheme,
    status: iced::widget::button::Status,
    selected: bool,
) -> iced::widget::button::Style {
    let mut style =
        components::button_style(theme, ButtonKind::Quiet, interaction(status, selected));
    if !selected && status != iced::widget::button::Status::Disabled {
        style.text_color = theme.palette().primary_text;
    }
    style
}

fn left_rail<'a>(
    workspace: &'a ProjectWorkspace,
    _project_title: &'a str,
    theme: ParchMintTheme,
    width: u32,
    inline_rename: bool,
    initial_viewport_upper_bound: f32,
) -> Element<'a, ProjectSurfaceMessage> {
    let content = match workspace.sidebar_surface() {
        SidebarSurface::Explorer => explorer_rail_with_rename(
            workspace,
            theme,
            inline_rename,
            initial_viewport_upper_bound,
        ),
        SidebarSurface::GlobalSearch => global_search_rail(workspace, theme),
    };
    container(column![Space::new().height(36), content].spacing(8))
        .padding(iced::Padding {
            top: 6.0,
            ..iced::Padding::new(SPACING_12)
        })
        .width(width)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest))
        .into()
}

#[cfg(test)]
fn explorer_rail<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    explorer_rail_with_rename(workspace, theme, true, 720.0)
}

pub(crate) fn explorer_rail_with_rename<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
    inline_rename: bool,
    initial_viewport_upper_bound: f32,
) -> Element<'a, ProjectSurfaceMessage> {
    let explorer = workspace.explorer();
    let targets = hierarchy_drag::targets();
    let force_full = workspace.hierarchy_rename().is_some();
    let window = explorer.visible_row_window(
        workspace.explorer_scroll_offset(),
        workspace.explorer_viewport_height(initial_viewport_upper_bound),
        force_full,
    );
    let current_scroll_offset = workspace.explorer_scroll_offset();
    let current_viewport_height = workspace.explorer_viewport_height(initial_viewport_upper_bound);
    let current_window_range =
        explorer.window_range(current_scroll_offset, current_viewport_height);
    let mut rows = column![].spacing(0.0);
    if !force_full && window.top_padding > 0.0 {
        rows = rows.push(Space::new().height(window.top_padding));
    }
    let rows = window.rows.into_iter().fold(rows, |column, item| {
        let depth = hierarchy_depth(explorer, item.parent_id);
        let disclosure: Element<'a, ProjectSurfaceMessage> = match item.kind {
            HierarchyRowKind::Root => container(icon_sized(
                if item.expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                18,
            ))
            .padding(SPACING_4)
            .width(26)
            .into(),
            HierarchyRowKind::Group => container(icon_sized(
                if item.expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                18,
            ))
            .width(26)
            .padding(4)
            .into(),
            HierarchyRowKind::Document => Space::new().width(26).into(),
        };
        let title = if item.cut_pending {
            format!("{}  (cut)", item.title)
        } else {
            item.title.to_owned()
        };
        let is_renaming = inline_rename
            && workspace
                .hierarchy_rename()
                .is_some_and(|(node_id, _)| node_id == item.id);
        let hierarchy_press = (!is_renaming).then(|| match item.kind {
            HierarchyRowKind::Root => {
                ProjectSurfaceMessage::Project(ProjectMessage::SelectHierarchy {
                    node_id: item.id.to_owned(),
                    gesture: SelectionGesture::Replace,
                })
            }
            HierarchyRowKind::Document => ProjectSurfaceMessage::Project(
                ProjectMessage::PreviewHierarchyNode(item.id.to_owned()),
            ),
            HierarchyRowKind::Group => ProjectSurfaceMessage::Project(
                ProjectMessage::SelectAndToggleHierarchyExpanded(item.id.to_owned()),
            ),
        });
        let select: Element<'a, ProjectSurfaceMessage> = if is_renaming {
            let draft = workspace
                .hierarchy_rename()
                .map(|(_, draft)| draft)
                .unwrap_or(item.title);
            hierarchy_drag::commit_on_click_away(
                sensor(
                    text_input("Rename", draft)
                        .id(hierarchy_rename_input_id(item.id))
                        .on_input(|title| {
                            ProjectSurfaceMessage::Project(ProjectMessage::SetHierarchyRenameDraft(
                                title,
                            ))
                        })
                        .on_submit(ProjectSurfaceMessage::Project(
                            ProjectMessage::CommitHierarchyRename,
                        ))
                        .padding([5, 6])
                        .width(Length::Fill),
                )
                .key(item.id.to_owned())
                .on_show({
                    let node_id = item.id.to_owned();
                    move |_| ProjectSurfaceMessage::HierarchyRenameShown(node_id.clone())
                }),
                ProjectSurfaceMessage::Project(ProjectMessage::CommitHierarchyRename),
            )
        } else {
            let row = mouse_area(
                container(text(title).size(u32::from(UI_TAB.size)))
                    .padding([SPACING_4, SPACING_8])
                    .width(Length::Fill)
                    .style(move |_| {
                        if item.selected {
                            iced::widget::container::Style {
                                background: Some(Background::Color(theme.palette().accent_subtle)),
                                ..Default::default()
                            }
                        } else {
                            iced::widget::container::Style::default()
                        }
                    }),
            )
            .interaction(iced::mouse::Interaction::Pointer);
            row.into()
        };
        let item_row: Element<'a, ProjectSurfaceMessage> =
            row![Space::new().width((depth * 14) as f32), disclosure, select]
                .spacing(1)
                .align_y(iced::alignment::Vertical::Center)
                .into();
        let item_row = match hierarchy_press {
            Some(on_press) => hierarchy_drag::source(
                item.id,
                item_row,
                on_press,
                match item.kind {
                    HierarchyRowKind::Document => Some(ProjectSurfaceMessage::Project(
                        ProjectMessage::OpenHierarchyNode(item.id.to_owned()),
                    )),
                    HierarchyRowKind::Root => Some(ProjectSurfaceMessage::Project(
                        ProjectMessage::ToggleHierarchyExpanded(item.id.to_owned()),
                    )),
                    HierarchyRowKind::Group => None,
                },
                ProjectSurfaceMessage::Project(ProjectMessage::BeginHierarchyDrag {
                    source_id: item.id.to_owned(),
                    gesture: SelectionGesture::Replace,
                }),
            ),
            None => item_row,
        };
        let node_id = item.id.to_owned();
        let drag_destination = workspace.hierarchy_drag_destination();
        let indicator = hierarchy_row_indicator(item.kind, &node_id, drag_destination, theme);
        let kind = item.kind;
        let target_id = node_id.clone();
        let row_body = hierarchy_drag::target(
            container(item_row)
                .width(Length::Fill)
                .height(Length::Fixed(EXPLORER_ROW_EXTENT))
                .align_y(iced::alignment::Vertical::Center)
                .style(move |_| {
                    if indicator.is_some() {
                        components::surface(theme, Surface::Panel, Interaction::Selected)
                    } else {
                        iced::widget::container::Style::default()
                    }
                }),
            indicator,
            &targets,
            move |bounds, point| {
                let destination = hierarchy_row_destination(kind, &target_id, bounds, point)?;
                workspace.preview_destination(&target_id, destination)
            },
        );
        let row_target = harness_target::target_id(
            harness_target::explorer_row_id(&node_id),
            right_click::right_click_area(row_body, move |point| {
                ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyContextMenu {
                    node_id: node_id.clone(),
                    point: Point::new(point.x, point.y),
                })
            }),
        );
        column.push(row_target)
    });
    let rows = if !force_full && window.bottom_padding > 0.0 {
        rows.push(Space::new().height(window.bottom_padding))
    } else {
        rows
    };
    let selection_shelf: Element<'a, ProjectSurfaceMessage> = {
        let selected_count = explorer.selected_ids().len();
        if selected_count > 1 {
            container(
                row![
                    text(format!("{selected_count} selected")).size(12),
                    Space::new().width(Length::Fill),
                    button(text("Copy").size(12))
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::CopySelection
                        ))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false),
                        )),
                    button(text("Move").size(12))
                        .on_press(ProjectSurfaceMessage::Project(ProjectMessage::CutSelection))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false),
                        )),
                    button(text("Delete").size(12))
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::DeleteSelection
                        ))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false),
                        )),
                ]
                .spacing(3)
                .align_y(iced::alignment::Vertical::Center),
            )
            .padding([4, 0])
            .style(move |_| components::surface(theme, Surface::Panel, Interaction::Selected))
            .into()
        } else {
            Space::new().height(0).into()
        }
    };
    let rail = column![
        row![
            components::muted_label("Explorer"),
            Space::new().width(Length::Fill),
            harness_target::target(
                HarnessTarget::ExplorerSearch,
                stationary_tooltip::tooltip(
                    button(icon_sized(Icon::Search, 16))
                        .width(32)
                        .height(32)
                        .padding(8)
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::ShowGlobalSearch
                        ))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false)
                        )),
                    container(
                        text(components::tooltip_label("Global Search", "search.global")).size(12)
                    )
                    .padding([4, 6]),
                    components::surface(theme, Surface::Elevated, Interaction::Rest),
                )
            )
        ]
        .spacing(4)
        .align_y(iced::alignment::Vertical::Center),
    ]
    .spacing(8)
    .height(Length::Fill);
    let rail = if explorer.selected_ids().len() > 1 {
        rail.push(selection_shelf)
    } else {
        rail
    };
    let scrollable = scrollable(Element::from(rows).map(Some))
        .id(explorer_scroll_id())
        .on_scroll(move |viewport| {
            let offset = viewport.absolute_offset().y;
            let height = viewport.bounds().height;
            (explorer.window_range(offset, height) != current_window_range
                || (height - current_viewport_height).abs() >= 1.0)
                .then_some(ProjectSurfaceMessage::Project(
                    ProjectMessage::SetExplorerViewport { offset, height },
                ))
        })
        .height(Length::Fill);
    let rail = rail.push(hierarchy_drag::surface(
        crate::scroll_gate::drop_none(scrollable),
        targets,
        workspace.hierarchy_drag_source().is_some(),
        false,
        |destination| {
            ProjectSurfaceMessage::Project(ProjectMessage::PreviewHierarchyDrop {
                surface: crate::HierarchySurface::Explorer,
                destination,
            })
        },
        ProjectSurfaceMessage::Project(ProjectMessage::LeaveHierarchySurface(
            crate::HierarchySurface::Explorer,
        )),
    ));
    let rail = if workspace.hierarchy_drag_source().is_some() {
        rail.push(
            text(if workspace.hierarchy_drag_destination().is_some() {
                "Release to move · Esc to cancel"
            } else {
                "Drag to move · Esc to cancel"
            })
            .size(12)
            .color(theme.palette().secondary_text),
        )
    } else {
        rail
    };
    focus::f6_region(F6Region::Explorer, rail)
}

fn hierarchy_row_indicator(
    kind: HierarchyRowKind,
    node_id: &str,
    current: Option<&DragDestination>,
    theme: ParchMintTheme,
) -> Option<hierarchy_drag::DropIndicator> {
    use hierarchy_drag::DropIndicatorPosition;

    let position = match current {
        Some(DragDestination::BeforeSibling(target)) if target == node_id => {
            DropIndicatorPosition::Before
        }
        Some(DragDestination::AfterSibling(target)) if target == node_id => {
            DropIndicatorPosition::After
        }
        Some(DragDestination::IntoGroup(target))
            if target == node_id
                && matches!(kind, HierarchyRowKind::Root | HierarchyRowKind::Group) =>
        {
            DropIndicatorPosition::Into
        }
        _ => return None,
    };
    Some(hierarchy_drag::DropIndicator {
        position,
        color: {
            let accent = theme.palette().accent;
            if matches!(position, hierarchy_drag::DropIndicatorPosition::Into) {
                Color { a: 0.18, ..accent }
            } else {
                accent
            }
        },
    })
}

fn hierarchy_row_destination(
    kind: HierarchyRowKind,
    node_id: &str,
    bounds: iced::Rectangle,
    point: iced::Point,
) -> Option<DragDestination> {
    if !bounds.contains(point) {
        return None;
    }
    let relative_y = (point.y - bounds.y) / bounds.height.max(1.0);
    if matches!(kind, HierarchyRowKind::Root | HierarchyRowKind::Group) {
        if relative_y < 0.25 {
            Some(DragDestination::BeforeSibling(node_id.to_owned()))
        } else if relative_y > 0.75 {
            Some(DragDestination::AfterSibling(node_id.to_owned()))
        } else {
            Some(DragDestination::IntoGroup(node_id.to_owned()))
        }
    } else if relative_y < 0.5 {
        Some(DragDestination::BeforeSibling(node_id.to_owned()))
    } else {
        Some(DragDestination::AfterSibling(node_id.to_owned()))
    }
}

fn hierarchy_context_overlay<'a>(
    workspace: &'a ProjectWorkspace,
    content: Element<'a, ProjectSurfaceMessage>,
    theme: ParchMintTheme,
    window_width: f32,
    window_height: f32,
) -> Element<'a, ProjectSurfaceMessage> {
    let base = content;
    if let Some((pane, document_id, point)) = workspace.tab_context() {
        let actions = column![
            components::context_action(
                "Close tab",
                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                    EditorMessage::CloseTab {
                        pane: *pane,
                        document_id: document_id.clone()
                    }
                )),
                theme
            ),
            components::context_action(
                "Move to other pane",
                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                    EditorMessage::MoveTabToOtherPane {
                        pane: *pane,
                        document_id: document_id.clone()
                    }
                )),
                theme
            ),
        ]
        .spacing(2);
        return stack![
            base,
            container(opaque(hierarchy_drag::commit_on_click_away(
                container(actions)
                    .padding(6)
                    .width(260)
                    .style(move |_| components::surface(
                        theme,
                        Surface::Elevated,
                        Interaction::Rest
                    )),
                ProjectSurfaceMessage::Project(ProjectMessage::CloseHierarchyContextMenu),
            )))
            .padding(iced::Padding {
                left: point.x().min(window_width - 268.0).max(8.0),
                top: point.y().min(window_height - 84.0).max(8.0),
                right: 0.0,
                bottom: 0.0,
            })
            .width(Length::Fill)
            .height(Length::Fill)
        ]
        .into();
    }
    let Some(node_id) = workspace.hierarchy_context_menu() else {
        return stack![base].into();
    };
    let Some(node) = workspace.explorer().row(node_id) else {
        return stack![base].into();
    };
    let id = node.id.to_owned();
    let mut actions = column![].spacing(2);
    if node.kind == HierarchyRowKind::Document {
        actions = actions
            .push(context_menu_button(
                "Open",
                ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyNode(id.clone())),
                theme,
            ))
            .push(context_menu_button(
                "Open beside",
                ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyNodeInCompanion(
                    id.clone(),
                )),
                theme,
            ));
        if let Some(document) = node.document_id {
            actions = actions.push(context_menu_button(
                "History",
                ProjectSurfaceMessage::OpenDocumentHistory(document.to_owned()),
                theme,
            ));
        }
    } else {
        actions = actions
            .push(context_menu_button(
                "New document",
                ProjectSurfaceMessage::Project(ProjectMessage::RequestCreateHierarchy {
                    parent_id: id.clone(),
                    kind: HierarchyItemKind::Document,
                }),
                theme,
            ))
            .push(context_menu_button(
                "New group",
                ProjectSurfaceMessage::Project(ProjectMessage::RequestCreateHierarchy {
                    parent_id: id.clone(),
                    kind: HierarchyItemKind::Group,
                }),
                theme,
            ));
    }
    if node.kind != HierarchyRowKind::Root {
        actions = actions
            .push(context_menu_button(
                "Rename",
                ProjectSurfaceMessage::Project(ProjectMessage::BeginHierarchyRename(id.clone())),
                theme,
            ))
            .push(context_menu_button(
                "Copy",
                ProjectSurfaceMessage::Project(ProjectMessage::CopySelection),
                theme,
            ))
            .push(context_menu_button(
                "Cut",
                ProjectSurfaceMessage::Project(ProjectMessage::CutSelection),
                theme,
            ))
            .push(context_menu_button(
                "Delete",
                ProjectSurfaceMessage::Project(ProjectMessage::DeleteSelection),
                theme,
            ));
    }
    // The secondary-click target reports project-window coordinates. The menu
    // is also composed at project-window scope, so no Explorer-rail offset or
    // rail-width overflow rule is appropriate here.
    let point = workspace.hierarchy_context_point();
    let left = if point.x() + HIERARCHY_CONTEXT_MENU_WIDTH <= window_width {
        point.x().max(0.0)
    } else {
        (point.x() - HIERARCHY_CONTEXT_MENU_WIDTH).max(0.0)
    };
    let action_count = match node.kind {
        HierarchyRowKind::Root => 2.0,
        HierarchyRowKind::Document => 7.0,
        HierarchyRowKind::Group => 6.0,
    };
    let menu_height = action_count * CONTEXT_ACTION_HEIGHT + (action_count - 1.0) * 2.0 + 12.0;
    let top = point.y().min(window_height - menu_height - 8.0).max(8.0);
    stack![
        base,
        container(opaque(hierarchy_drag::commit_on_click_away(
            crate::motion::enter(
                "context menu",
                container(actions)
                    .padding(6)
                    .width(HIERARCHY_CONTEXT_MENU_WIDTH)
                    .style(move |_| components::surface(
                        theme,
                        Surface::Elevated,
                        Interaction::Rest,
                    ))
            ),
            ProjectSurfaceMessage::Project(ProjectMessage::CloseHierarchyContextMenu),
        )))
        .padding(iced::Padding {
            top,
            right: 0.0,
            bottom: 0.0,
            left,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Top),
    ]
    .into()
}

fn context_menu_button(
    label: &'static str,
    message: ProjectSurfaceMessage,
    theme: ParchMintTheme,
) -> iced::widget::Button<'static, ProjectSurfaceMessage> {
    components::context_action(label, message, theme)
}

fn comment_anchor_summary(anchor: &CommentAnchor) -> String {
    match anchor {
        CommentAnchor::Range { quote, .. } => format!("“{quote}”"),
        CommentAnchor::Position { quote, .. } if quote.is_empty() => "At cursor".to_owned(),
        CommentAnchor::Position { quote, .. } => format!("At cursor · “{quote}”"),
        CommentAnchor::Document { .. } => "Document note".to_owned(),
        CommentAnchor::Orphaned { quote, .. } => format!("Anchor needs attention · “{quote}”"),
    }
}

pub(crate) fn hierarchy_row_is_visible<'a>(
    explorer: &'a crate::ExplorerState,
    mut parent_id: Option<&'a str>,
) -> bool {
    while let Some(id) = parent_id {
        let Some(parent) = explorer.row(id) else {
            return false;
        };
        if !parent.expanded {
            return false;
        }
        parent_id = parent.parent_id;
    }
    true
}

pub(crate) fn hierarchy_depth<'a>(
    explorer: &'a crate::ExplorerState,
    mut parent_id: Option<&'a str>,
) -> usize {
    let mut depth = 0;
    while let Some(id) = parent_id {
        depth += 1;
        parent_id = explorer.row(id).and_then(|parent| parent.parent_id);
    }
    depth
}

fn global_search_rail<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let search = workspace.global_search();
    let query = text_input("Search the project", search.query())
        .id(global_search_query_input_id())
        .on_input(|query| {
            ProjectSurfaceMessage::Project(ProjectMessage::SetGlobalSearchQuery(query))
        })
        .padding([7, 8])
        .style(move |_, status| components::field_style(theme, field_interaction(status)));
    let controls = row![stationary_tooltip::tooltip(
        button(text("←  Search").size(u32::from(UI_LABEL.size)))
            .on_press(ProjectSurfaceMessage::Project(ProjectMessage::ShowExplorer))
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                interaction(status, false)
            )),
        container(text("Back to Explorer").size(12)).padding([4, 6]),
        components::surface(theme, Surface::Elevated, Interaction::Rest),
    ),];
    let options = row![
        stationary_tooltip::tooltip(
            button(icon_sized(Icon::MatchCase, 20))
                .padding(2)
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::SetGlobalSearchOptions {
                        case_sensitive: !search.case_sensitive(),
                        whole_word: search.whole_word()
                    }
                ))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    interaction(status, search.case_sensitive())
                )),
            container(text("Match case").size(12)).padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        ),
        stationary_tooltip::tooltip(
            button(icon_sized(Icon::WholeWords, 20))
                .padding(2)
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::SetGlobalSearchOptions {
                        case_sensitive: search.case_sensitive(),
                        whole_word: !search.whole_word()
                    }
                ))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    interaction(status, search.whole_word())
                )),
            container(text("Match whole word").size(12)).padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        ),
    ]
    .align_y(iced::alignment::Vertical::Center)
    .spacing(2);
    let document_count = search.document_count();
    let results = search.windowed_rows().fold(column![], |column, row| {
        let content: Element<'a, ProjectSurfaceMessage> = match row {
            GlobalSearchRow::Document {
                document_id,
                matches,
            } => {
                let title = workspace
                    .explorer()
                    .title_for_document(document_id)
                    .unwrap_or("Unavailable document");
                let breadcrumb = workspace
                    .explorer()
                    .breadcrumb_for_document(document_id)
                    .map(|mut parts| {
                        parts.pop();
                        parts.join(" > ")
                    })
                    .unwrap_or_default();
                button(
                    row![
                        icon_sized(
                            if search.is_collapsed(document_id) {
                                Icon::ChevronRight
                            } else {
                                Icon::ChevronDown
                            },
                            18
                        ),
                        column![
                            row![
                                text(title)
                                    .size(u32::from(UI_LABEL.size))
                                    .width(Length::Fill),
                                text(search_match_count_label(matches))
                                    .size(u32::from(UI_COMPACT.size))
                                    .color(theme.palette().secondary_text)
                            ],
                            text(breadcrumb)
                                .size(11)
                                .color(theme.palette().secondary_text)
                        ]
                        .spacing(2)
                        .width(Length::Fill)
                    ]
                    .spacing(4)
                    .align_y(iced::alignment::Vertical::Center),
                )
                .width(Length::Fill)
                .padding([4, 3])
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::ToggleSearchDocument(document_id.to_owned()),
                ))
                .style(move |_, status| {
                    components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
                })
                .into()
            }
            GlobalSearchRow::Match(index) => {
                let result = &search.results()[index];
                let active = search.active_match_id() == Some(result.match_id.as_str());
                let highlight = if active {
                    theme.palette().search_match_active
                } else {
                    theme.palette().search_match
                };
                let snippet_spans: Vec<iced::widget::text::Span<'a>> = vec![
                    span(result.prefix.replace(['\r', '\n'], " ")),
                    span(result.matching_text.replace(['\r', '\n'], " "))
                        .font(Font {
                            weight: font::Weight::Bold,
                            ..Font::default()
                        })
                        .background(highlight),
                    span(result.suffix.replace(['\r', '\n'], " ")),
                ];
                let snippet = container(
                    rich_text(snippet_spans)
                        .size(12)
                        .line_height(iced::widget::text::LineHeight::Absolute(14.0.into()))
                        .height(28),
                )
                .padding([2, 3])
                .clip(true)
                .style(move |_| iced::widget::container::Style {
                    border: if active {
                        Border {
                            color: theme.palette().search_match_active,
                            width: 1.0,
                            radius: 2.0.into(),
                        }
                    } else {
                        Border::default()
                    },
                    ..Default::default()
                });
                stationary_tooltip::tooltip(
                    container(
                        button(snippet)
                            .padding([5, 6])
                            .height(Length::Fill)
                            .width(Length::Fill)
                            .on_press(ProjectSurfaceMessage::Project(
                                ProjectMessage::NavigateGlobalSearchResult(result.match_id.clone()),
                            ))
                            .style(move |_, status| {
                                components::button_style(
                                    theme,
                                    ButtonKind::Quiet,
                                    interaction(status, active),
                                )
                            }),
                    )
                    .id(HarnessTarget::GlobalSearchMatch(index).id()),
                    container(
                        text(format!(
                            "{}{}{}",
                            result.prefix, result.matching_text, result.suffix
                        ))
                        .size(12),
                    )
                    .max_width(360)
                    .padding([5, 8]),
                    components::surface(theme, Surface::Elevated, Interaction::Rest),
                )
            }
        };
        column.push(
            container(content)
                .height(SEARCH_ROW_HEIGHT)
                .width(Length::Fill)
                .clip(true),
        )
    });
    let result_count = search.results().len();
    let results = if search.query().is_empty() {
        results
    } else {
        column![
            Space::new().height(search.result_window_start() as f32 * SEARCH_ROW_HEIGHT),
            results,
            Space::new().height(search.result_window_bottom_padding()),
        ]
    };
    let replacement_count = search
        .results()
        .iter()
        .filter(|result| result.is_replaceable())
        .count();
    let replace = text_input("Replace with", search.replacement())
        .id(global_replacement_input_id())
        .on_input(|replacement| {
            ProjectSurfaceMessage::Project(ProjectMessage::SetGlobalReplacement(replacement))
        })
        .padding([7, 8])
        .width(Length::Fill)
        .style(move |_, status| components::field_style(theme, field_interaction(status)));
    let replace_action = harness_target::target(
        HarnessTarget::GlobalReplacementReview,
        button(
            text(if search.replacement().is_empty() {
                format!("Review {replacement_count} deletions")
            } else if replacement_count == 1 {
                "Review 1 replacement".to_owned()
            } else {
                format!("Review {replacement_count} replacements")
            })
            .size(13),
        )
        .on_press_maybe((search.is_complete() && replacement_count > 0).then_some(
            ProjectSurfaceMessage::Project(ProjectMessage::OpenReplacementPreview),
        ))
        .padding([5, 7])
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
        }),
    );
    let active_trail: Element<'a, ProjectSurfaceMessage> = search
        .active_match_id()
        .and_then(|active| {
            search
                .results()
                .iter()
                .position(|result| result.match_id == active)
        })
        .map(|index| {
            text(format!("Match {} of {}", index + 1, result_count,))
                .size(11)
                .color(theme.palette().secondary_text)
                .into()
        })
        .unwrap_or_else(|| Space::new().height(0).into());
    let mut content = column![
        controls,
        row![query, options]
            .spacing(4)
            .align_y(iced::alignment::Vertical::Center)
    ]
    .spacing(8);
    {
        if !workspace.replacement_preview().uses_middle_pane() {
            content = content.push(harness_target::target(
                HarnessTarget::GlobalReplaceToggle,
                button(text("Replace…").size(13))
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::ToggleGlobalReplace,
                    ))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false),
                        )
                    }),
            ));
            if search.replace_visible() {
                content = content.push(column![replace, replace_action].spacing(8));
            }
        }
        if !search.query().is_empty() {
            content = content.push(
                text(if let Some(error) = search.error() {
                    error.to_owned()
                } else if !search.is_complete() {
                    "Searching…".to_owned()
                } else {
                    global_search_result_count_label(result_count, document_count)
                })
                .size(12)
                .color(theme.palette().secondary_text),
            );
        }
    }
    content
        .push(
            column![
                active_trail,
                crate::scroll_gate::smooth(
                    scrollable(results)
                        .id(HarnessTarget::GlobalSearchResults.id())
                        .spacing(SPACING_4)
                        .on_scroll(|viewport| ProjectSurfaceMessage::Project(
                            ProjectMessage::SetGlobalSearchScroll(viewport.absolute_offset().y)
                        ))
                        .height(Length::Fill)
                )
            ]
            .spacing(SPACING_12)
            .height(Length::Fill),
        )
        .height(Length::Fill)
        .into()
}

pub(crate) fn global_search_query_input_id() -> iced::widget::Id {
    iced::widget::Id::new("global-search-query")
}

fn global_replacement_input_id() -> iced::widget::Id {
    iced::widget::Id::new("global-search-replacement")
}

fn search_match_count_label(count: usize) -> String {
    format!("{count} {}", if count == 1 { "match" } else { "matches" })
}

fn global_search_result_count_label(result_count: usize, document_count: usize) -> String {
    format!(
        "{} in {document_count} {}",
        search_match_count_label(result_count),
        if document_count == 1 {
            "document"
        } else {
            "documents"
        }
    )
}

fn center_view<'a>(
    workspace: &'a ProjectWorkspace,
    destination: RibbonDestination,
    theme: ParchMintTheme,
    editor_child: Element<'a, ProjectSurfaceMessage>,
    layout: &ShellLayout,
) -> Element<'a, ProjectSurfaceMessage> {
    let content = match workspace.content_state() {
        ContentState::Loading => {
            state_center("Loading project", "Project content is loading.", theme)
        }
        ContentState::Error(error) => state_center("Project needs attention", error, theme),
        ContentState::Recovery => Space::new().width(Length::Fill).height(Length::Fill).into(),
        ContentState::Ready | ContentState::Empty => match destination {
            RibbonDestination::Editor if workspace.replacement_preview().uses_middle_pane() => {
                search_center(workspace, theme)
            }
            RibbonDestination::Editor => editor_child,
            RibbonDestination::Cards => {
                let width = (layout.center().width().saturating_sub(48 + 32) as f32
                    - crate::cards_layout::SCROLLBAR_GUTTER)
                    .max(1.0);
                let height = layout.center().height().saturating_sub(16) as f32;
                focus::f6_region(
                    F6Region::FocusedEditor,
                    cards_center(workspace, theme, Some((width, height))),
                )
            }
            RibbonDestination::GlobalSearch => editor_child,
            RibbonDestination::History => history_center(workspace, theme),
            RibbonDestination::RecentlyDeleted => deleted_center(workspace, theme),
            RibbonDestination::Export => export_center(workspace, theme),
            RibbonDestination::Settings => settings_center(workspace, theme),
        },
    };
    // Each destination owns its content spacing.
    let surface = destination_canvas_surface(destination);
    container(content)
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, surface, Interaction::Rest))
        .into()
}

fn destination_canvas_surface(destination: RibbonDestination) -> Surface {
    match destination {
        RibbonDestination::Cards | RibbonDestination::Export | RibbonDestination::Settings => {
            Surface::Application
        }
        _ => Surface::Manuscript,
    }
}

fn overview_add<'a>(
    parent: &str,
    root: bool,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let half = |kind, symbol, label| {
        iced::widget::tooltip(
            button(
                container(
                    row![icon_sized(symbol, 28).style(move |_, status| {
                        let mut color = theme.palette().pencil_icon;
                        color.a = if matches!(status, iced::widget::svg::Status::Hovered) {
                            1.0
                        } else {
                            0.65
                        };
                        iced::widget::svg::Style { color: Some(color) }
                    })]
                    .spacing(6)
                    .align_y(iced::alignment::Vertical::Center),
                )
                .center_x(Length::Fill)
                .center_y(Length::Fill),
            )
            .width(Length::FillPortion(1))
            .height(crate::cards_layout::ADD_HEIGHT)
            .on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::RequestCreateHierarchy {
                    parent_id: parent.to_owned(),
                    kind,
                },
            ))
            .style(move |_, status| {
                components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
            }),
            text(components::tooltip_label(
                label,
                if kind == HierarchyItemKind::Group {
                    "outline.group"
                } else {
                    "outline.document"
                },
            )),
            iced::widget::tooltip::Position::Top,
        )
        .into()
    };
    let document: Element<'a, ProjectSurfaceMessage> = half(
        HierarchyItemKind::Document,
        Icon::NewDocument,
        "New document",
    );
    let group: Element<'a, ProjectSurfaceMessage> =
        half(HierarchyItemKind::Group, Icon::NewGroup, "New group");
    let document = harness_target::target_id(
        if root {
            HarnessTarget::OverviewAdd.id()
        } else {
            iced::widget::Id::from(format!("overview-document-{parent}"))
        },
        document,
    );
    let group = harness_target::target_id(
        if root {
            HarnessTarget::OverviewAddGroup.id()
        } else {
            iced::widget::Id::from(format!("overview-group-{parent}"))
        },
        group,
    );
    row![
        container(document).width(Length::FillPortion(1)),
        container(Space::new().width(1).height(32)).style(move |_| {
            iced::widget::container::Style {
                background: Some(theme.palette().divider.scale_alpha(0.45).into()),
                ..Default::default()
            }
        }),
        container(group).width(Length::FillPortion(1))
    ]
    .align_y(iced::alignment::Vertical::Center)
    .into()
}

fn cards_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
    size: Option<(f32, f32)>,
) -> Element<'a, ProjectSurfaceMessage> {
    let grid: Element<'a, ProjectSurfaceMessage> = if let Some((width, height)) = size {
        cards_grid(workspace, theme, width, height)
    } else {
        responsive(move |available| {
            cards_grid(
                workspace,
                theme,
                (available.width - crate::cards_layout::SCROLLBAR_GUTTER).max(1.0),
                available.height,
            )
        })
        .into()
    };
    right_click::right_click_area(
        container(grid)
            .padding(iced::Padding {
                top: 16.0,
                right: 16.0,
                bottom: 0.0,
                left: 16.0,
            })
            .width(Length::Fill)
            .height(Length::Fill),
        move |point| {
            ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyContextMenu {
                node_id: workspace.cards().section_id().to_owned(),
                point: Point::new(point.x, point.y),
            })
        },
    )
}

#[derive(Clone, Copy)]
struct CardsWindowCoverage {
    mounted_start: f32,
    mounted_end: f32,
    has_rows_before: bool,
    has_rows_after: bool,
    projected_scroll: f32,
}

fn needs_cards_window_refresh(
    offset: f32,
    viewport_height: f32,
    content_height: f32,
    coverage: CardsWindowCoverage,
) -> bool {
    const GUARD: f32 = 96.0;
    (coverage.has_rows_before && offset <= coverage.mounted_start + GUARD)
        || (coverage.has_rows_after && offset + viewport_height >= coverage.mounted_end - GUARD)
        || (offset <= 1.0 && coverage.projected_scroll > 1.0)
        || (offset + viewport_height >= content_height - 1.0
            && coverage.projected_scroll + viewport_height < content_height - 1.0)
}

pub(crate) fn cards_grid<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
    width: f32,
    height: f32,
) -> Element<'a, ProjectSurfaceMessage> {
    let cards = workspace.cards();
    let targets = hierarchy_drag::targets();
    let columns = crate::cards_layout::column_count(width);
    let window = cards.viewport_window(columns, width, height);
    let coverage = CardsWindowCoverage {
        mounted_start: window.top_padding,
        mounted_end: window.top_padding + window.rows.iter().map(|row| row.height).sum::<f32>(),
        has_rows_before: window.top_padding > 0.0,
        has_rows_after: window.bottom_padding > 0.0,
        projected_scroll: cards.scroll_offset(),
    };
    let visible = cards.items_in_window(&window);
    workspace
        .card_positions
        .retain(&visible.iter().map(|item| item.node_id).collect::<Vec<_>>());
    let generation = window.generation;
    let mut items = visible.into_iter().peekable();
    let mut frames: Vec<crate::card_frames::GroupFrame> = Vec::new();
    let mut frame_ids = std::collections::BTreeMap::new();
    let mut last_card_id: Option<String> = None;
    let mut grid = column![Space::new().height(window.top_padding)].spacing(0);
    for (row_index, grid_row) in window.rows.iter().enumerate() {
        // Iced omits a zero-height spacer from the column's child list.
        let row_index = row_index + usize::from(window.top_padding > 0.0);
        let closes_group = grid_row
            .add_to
            .as_deref()
            .is_some_and(|id| id != cards.section_id());
        let trailing = if closes_group {
            crate::cards_layout::GROUP_GAP + 8.0
        } else {
            0.0
        };
        let first =
            (grid_row.start < grid_row.end).then(|| items.peek().expect("projected grid row"));
        let own_group = first
            .filter(|item| item.kind == HierarchyRowKind::Group && item.expanded)
            .map(|item| item.node_id);
        let mut current = first
            .map(|item| item.node_id)
            .or(grid_row.add_to.as_deref());
        let mut ancestors = Vec::new();
        while let Some(id) = current {
            let Some(node) = workspace.displayed_explorer().row(id) else {
                break;
            };
            if node.kind == HierarchyRowKind::Group {
                ancestors.push(id);
            }
            current = node.parent_id;
        }
        ancestors.reverse();
        for (depth, id) in ancestors.into_iter().enumerate() {
            if first.is_some_and(|item| item.node_id == id) && own_group != Some(id) {
                continue;
            }
            let index = *frame_ids.entry(id.to_owned()).or_insert_with(|| {
                frames.push(crate::card_frames::GroupFrame {
                    id: id.to_owned(),
                    depth,
                    first_row: row_index,
                    last_row: row_index,
                    starts_here: own_group == Some(id),
                    ends_here: false,
                    last_card_id: last_card_id.clone(),
                });
                frames.len() - 1
            });
            frames[index].last_row = row_index;
            frames[index].ends_here = grid_row.add_to.as_deref() == Some(id);
            frames[index].last_card_id = last_card_id.clone();
        }
        let add_control = |parent: &str, depth: usize| {
            let indent = crate::cards_layout::grid_indent(depth, width);
            crate::card_frames::placeholder(
                container(overview_add(parent, parent == cards.section_id(), theme))
                    .width((width - 2.0 * indent - (columns - 1) as f32 * 12.0) / columns as f32),
                theme,
            )
        };
        if let Some(parent) = &grid_row.add_to
            && grid_row.start == grid_row.end
        {
            let indent = crate::cards_layout::grid_indent(grid_row.depth, width);
            let control = add_control(parent, grid_row.depth);
            grid = grid.push(
                container(row![Space::new().width(indent), control]).padding(iced::Padding {
                    bottom: crate::project_workspace::CARDS_ROW_GAP + trailing,
                    ..iced::Padding::ZERO
                }),
            );
            continue;
        }
        let first = items.peek().expect("projected grid row");
        let indent = first.grid_indent(width);
        let cell_width = first.grid_width(width, columns);
        let parent = workspace
            .displayed_explorer()
            .row(first.node_id)
            .and_then(|row| row.parent_id)
            .unwrap_or(cards.section_id())
            .to_owned();
        let mut last_node = first.node_id.to_owned();
        let mut cells = row![].spacing(12);
        for item in items.by_ref().take(grid_row.end - grid_row.start) {
            last_node = item.node_id.to_owned();
            cells = cells.push(outline_card(
                workspace,
                theme,
                item,
                cell_width,
                columns > 1,
                &targets,
                generation,
                false,
            ));
        }
        last_card_id = Some(last_node.clone());
        for frame in frames
            .iter_mut()
            .filter(|frame| frame.last_row == row_index)
        {
            frame.last_card_id = last_card_id.clone();
        }
        if let Some(parent) = &grid_row.add_to {
            cells = cells.push(add_control(parent, grid_row.depth));
        }
        let context_parent = parent.clone();
        let body = right_click::right_click_area(
            row![Space::new().width(indent), cells].width(Length::Fill),
            move |point| {
                ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyContextMenu {
                    node_id: context_parent.clone(),
                    point: Point::new(point.x, point.y),
                })
            },
        );
        grid = grid.push(hierarchy_drag::target(
            container(body).padding(iced::Padding {
                bottom: trailing,
                ..iced::Padding::ZERO
            }),
            None,
            &targets,
            move |bounds, point| {
                if !bounds.contains(point) {
                    return None;
                }
                workspace.preview_destination(
                    &last_node,
                    DragDestination::AfterSibling(last_node.clone()),
                )
            },
        ));
    }
    grid = grid.push(Space::new().height(window.bottom_padding));
    if window.rows.is_empty() {
        grid = grid.push(text("Add a document or group to start your outline.").size(13));
    }
    let sections = workspace
        .explorer()
        .root_rows()
        .fold(row![].spacing(4), |row, item| {
            let id = item.id.to_owned();
            let selected = cards.section_id() == item.id;
            row.push(
                button(text(item.title).size(14))
                    .padding([6, 12])
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::SetCardsSection(id),
                    ))
                    .style(move |_, status| mode_switch_button_style(theme, status, selected)),
            )
        });
    column![
        row![
            sections,
            Space::new().width(Length::Fill),
            harness_target::target(
                HarnessTarget::ManageMetadata,
                button(text("Fields…").size(13))
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::ManageSettings(SettingsCategory::Metadata)
                    ))
                    .style(move |_, status| components::button_style(
                        theme,
                        ButtonKind::Quiet,
                        interaction(status, false)
                    ))
            ),
        ]
        .align_y(iced::alignment::Vertical::Center),
        hierarchy_drag::surface(
            right_click::right_click_area(
                crate::scroll_gate::drop_none(
                    scrollable(
                        crate::card_frames::groups(
                            grid,
                            frames,
                            workspace.card_positions.clone(),
                            theme,
                        )
                        .map(Some)
                    )
                    .id(HarnessTarget::CardsList.id())
                    .width(Length::Fill)
                    .on_scroll(move |viewport| {
                        let offset = viewport.absolute_offset().y;
                        needs_cards_window_refresh(
                            offset,
                            viewport.bounds().height,
                            viewport.content_bounds().height,
                            coverage,
                        )
                        .then_some(ProjectSurfaceMessage::Project(
                            ProjectMessage::SetCardsScroll(offset),
                        ))
                    })
                    .height(Length::Fill),
                ),
                move |point| ProjectSurfaceMessage::Project(
                    ProjectMessage::OpenHierarchyContextMenu {
                        node_id: cards.section_id().to_owned(),
                        point: Point::new(point.x, point.y),
                    }
                )
            ),
            targets,
            workspace.hierarchy_drag_source().is_some(),
            true,
            |destination| ProjectSurfaceMessage::Project(ProjectMessage::PreviewHierarchyDrop {
                surface: crate::HierarchySurface::Cards,
                destination,
            }),
            ProjectSurfaceMessage::Project(ProjectMessage::LeaveHierarchySurface(
                crate::HierarchySurface::Cards
            )),
        ),
    ]
    .spacing(SPACING_12)
    .width(Length::Fill)
    .into()
}

#[allow(clippy::too_many_arguments)]
fn outline_card<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
    item: crate::CardItem<'a>,
    width: f32,
    horizontal: bool,
    targets: &hierarchy_drag::HoverTargets<DragDestination>,
    generation: u64,
    floating: bool,
) -> Element<'a, ProjectSurfaceMessage> {
    let drag_source = workspace.hierarchy_drag_source().map(str::to_owned);
    let drag_destination = workspace.hierarchy_drag_destination().cloned();
    let node_id = item.node_id.to_owned();
    let group = item.kind == HierarchyRowKind::Group;
    let title = text(item.title)
        .size(item.title_size())
        .font(item.title_font())
        .line_height(iced::Pixels(24.0))
        .wrapping(text::Wrapping::WordOrGlyph)
        .width(Length::Fill);
    let mut heading = row![].spacing(6).align_y(iced::alignment::Vertical::Center);
    heading = heading.push(title);
    heading = heading.push(
        text(word_count_label(item.words))
            .font(crate::cards_layout::CARD_FONT)
            .size(12)
            .color(theme.palette().secondary_text),
    );
    let drag_node = node_id.clone();
    let heading = hierarchy_drag::source_with_pointer(
        &node_id,
        heading,
        ProjectSurfaceMessage::Project(if group {
            ProjectMessage::ToggleCardsExpanded(node_id.clone())
        } else {
            ProjectMessage::SelectHierarchy {
                node_id: node_id.clone(),
                gesture: SelectionGesture::Replace,
            }
        }),
        (!group)
            .then(|| ProjectSurfaceMessage::Project(ProjectMessage::ActivateCard(node_id.clone()))),
        move |origin, bounds| {
            ProjectSurfaceMessage::Project(ProjectMessage::BeginCardDrag {
                source_id: drag_node.clone(),
                grab_offset: Point::new(origin.x - bounds.x + 12.0, origin.y - bounds.y + 12.0),
                width,
            })
        },
    );
    let heading: Element<'a, ProjectSurfaceMessage> = if group {
        row![
            harness_target::target_id(
                harness_target::card_disclosure_id(&node_id),
                button(
                    container(icon_sized(
                        if item.expanded {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        },
                        18
                    ))
                    .center(26)
                )
                .padding(0)
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::ToggleCardsExpanded(node_id.clone())
                ))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    interaction(status, false)
                ))
            ),
            heading
        ]
        .spacing(6)
        .into()
    } else {
        heading
    };
    let heading: Element<'a, ProjectSurfaceMessage> = if let Some((id, draft)) = workspace
        .hierarchy_rename()
        .filter(|(id, _)| *id == item.node_id)
    {
        hierarchy_drag::commit_on_click_away(
            sensor(
                text_input("Document or group name", draft)
                    .id(hierarchy_rename_input_id(id))
                    .on_input(|title| {
                        ProjectSurfaceMessage::Project(ProjectMessage::SetHierarchyRenameDraft(
                            title,
                        ))
                    })
                    .on_submit(ProjectSurfaceMessage::Project(
                        ProjectMessage::CommitHierarchyRename,
                    ))
                    .padding([6, 8]),
            )
            .key(id.to_owned())
            .on_show({
                let id = id.to_owned();
                move |_| ProjectSurfaceMessage::HierarchyRenameShown(id.clone())
            }),
            ProjectSurfaceMessage::Project(ProjectMessage::CommitHierarchyRename),
        )
    } else {
        heading
    };
    let expand = stationary_tooltip::tooltip(
        harness_target::target_id(
            iced::widget::Id::from(format!("card-details-{node_id}")),
            button(icon_sized(
                if item.details_expanded {
                    Icon::RestoreLayout
                } else {
                    Icon::FocusWriting
                },
                16,
            ))
            .padding(3)
            .on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::ToggleCardDetails(node_id.clone()),
            ))
            .style(move |_, status| {
                components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
            }),
        ),
        text(if item.details_expanded {
            "Collapse details"
        } else {
            "Expand details"
        })
        .size(12),
        components::surface(theme, Surface::Elevated, Interaction::Rest),
    );
    let mut header = row![
        container(heading)
            .width(Length::Fill)
            .height(item.heading_height(width))
            .clip(true)
    ]
    .spacing(4);
    if !group && (item.details_expanded || item.needs_expansion(width)) {
        header = header.push(expand);
    }
    let details = column![
        header,
        outline_fields(
            workspace,
            item.node_id,
            theme,
            width,
            item.synopsis_height(width),
            item.details_expanded,
        )
    ]
    .spacing(4);
    let card_content: Element<'a, ProjectSurfaceMessage> = details.into();
    let middle_active =
        drag_destination.as_ref() == Some(&DragDestination::IntoGroup(node_id.clone()));
    let source_active = drag_source.as_deref() == Some(node_id.as_str());
    let card_content = if source_active && !floating {
        container(
            Space::new()
                .height(item.row_height(width) - crate::project_workspace::CARDS_ROW_GAP - 24.0)
                .width(Length::Fill),
        )
        .into()
    } else {
        card_content
    };
    let card = container(crate::motion::resize_height(
        node_id.clone(),
        container(card_content)
            .height(item.row_height(width) - crate::project_workspace::CARDS_ROW_GAP - 24.0)
            .clip(true),
    ))
    .clip(true)
    .padding(SPACING_12)
    .width(Length::Fill)
    .style(move |_| {
        let mut style = iced::widget::container::Style {
            background: (!(group && item.expanded && !floating) && (item.depth == 0 || floating))
                .then_some(theme.palette().panel.into()),
            border: Border {
                color: theme.palette().divider,
                width: if group && item.expanded && !floating {
                    0.0
                } else {
                    1.0
                },
                radius: 5.0.into(),
            },
            ..Default::default()
        };
        if !floating && !group && (item.selected || source_active || middle_active) {
            style.border.color = theme.palette().accent;
            style.border.width = 1.0;
        }
        if group && item.selected {
            style.background = Some(theme.palette().control_hover.into());
        }
        if group && middle_active {
            style.border.color = Color::TRANSPARENT;
        }
        if !floating && (source_active || middle_active) {
            style.background = Some(theme.palette().accent_subtle.into());
        }
        if floating {
            style.background = Some(theme.palette().control_hover.into());
            style.border.color = theme.palette().strong_border;
            style.shadow = iced::Shadow {
                color: theme.palette().scrim.scale_alpha(0.4),
                offset: iced::Vector::new(2.0, 4.0),
                blur_radius: 10.0,
            };
        }
        style
    });
    if floating {
        return card.into();
    }
    let context_node = node_id.clone();
    let card = harness_target::target_id(
        harness_target::card_id(&node_id),
        right_click::right_click_area(card, move |point| {
            ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyContextMenu {
                node_id: context_node.clone(),
                point: Point::new(point.x, point.y),
            })
        }),
    );
    let kind = item.kind;
    let target_node = node_id.clone();
    let card = crate::motion::reflow(
        workspace.card_positions.clone(),
        node_id.clone(),
        generation,
        workspace.hierarchy_drag_source().is_none() && !source_active,
        card,
    );
    let card = hierarchy_drag::target_with_zone(card, None, targets, move |bounds, point| {
        let destination = cards_drop_destination(kind, &target_node, bounds, point, horizontal)?;
        let zone = cards_drop_zone(kind, &destination, bounds, horizontal);
        workspace
            .preview_destination(&target_node, destination)
            .map(|destination| (destination, zone))
    });
    column![
        card,
        Space::new().height(crate::project_workspace::CARDS_ROW_GAP)
    ]
    .width(width)
    .into()
}

fn cards_drop_zone(
    kind: HierarchyRowKind,
    destination: &DragDestination,
    bounds: iced::Rectangle,
    horizontal: bool,
) -> iced::Rectangle {
    let mut zone = bounds;
    if kind == HierarchyRowKind::Group {
        let edge = (bounds.height * 0.25).min(24.0);
        match destination {
            DragDestination::BeforeSibling(_) => zone.height = edge,
            DragDestination::AfterSibling(_) => {
                zone.y += bounds.height - edge;
                zone.height = edge;
            }
            _ => {
                zone.y += edge;
                zone.height -= edge * 2.0;
            }
        }
    } else if horizontal {
        zone.width /= 2.0;
        if matches!(destination, DragDestination::AfterSibling(_)) {
            zone.x += zone.width;
        }
    } else {
        zone.height /= 2.0;
        if matches!(destination, DragDestination::AfterSibling(_)) {
            zone.y += zone.height;
        }
    }
    zone
}

fn cards_drop_destination(
    kind: HierarchyRowKind,
    node_id: &str,
    bounds: iced::Rectangle,
    point: iced::Point,
    horizontal: bool,
) -> Option<DragDestination> {
    let row_gap = crate::project_workspace::CARDS_ROW_GAP;
    let hit_bounds = iced::Rectangle {
        x: bounds.x - 6.0,
        y: bounds.y - row_gap / 2.0,
        width: bounds.width + 12.0,
        height: bounds.height + row_gap,
    };
    if !hit_bounds.contains(point) {
        return None;
    }
    let before = if kind == HierarchyRowKind::Group {
        let edge = (bounds.height * 0.25).min(24.0);
        if point.y > bounds.y + edge && point.y < bounds.y + bounds.height - edge {
            return Some(DragDestination::IntoGroup(node_id.to_owned()));
        }
        point.y < bounds.center_y()
    } else if horizontal {
        point.x < bounds.center_x()
    } else {
        point.y < bounds.center_y()
    };
    Some(if before {
        DragDestination::BeforeSibling(node_id.to_owned())
    } else {
        DragDestination::AfterSibling(node_id.to_owned())
    })
}

fn search_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let preview = workspace.replacement_preview();
    let replacement = text_input(
        "Replace matches with",
        workspace.global_search().replacement(),
    )
    .id(global_replacement_input_id())
    .on_input(|replacement| {
        ProjectSurfaceMessage::Project(ProjectMessage::SetGlobalReplacement(replacement))
    })
    .padding([7, 8])
    .style(move |_, status| components::field_style(theme, field_interaction(status)));

    if !preview.uses_middle_pane() {
        let review = if workspace.global_search().is_complete()
            && !workspace.global_search().results().is_empty()
        {
            button(text("Review replacement")).on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::OpenReplacementPreview,
            ))
        } else {
            button(text("Review replacement"))
        };
        return container(
            column![
                page_title("Global Search"),
                replacement,
                text(
                    "Search for text in the sidebar, then review the matches you want to replace."
                )
                .size(13),
                review.style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Primary,
                    interaction(status, false)
                )),
            ]
            .spacing(14),
        )
        .padding(SPACING_24)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    let rows = preview
        .rows()
        .into_iter()
        .fold(column![].spacing(6), |rows, item| {
            let (label, navigation) = match item.kind {
                ReplacementPreviewRowKind::AllMatches => (
                    format!("All matches ({})", preview.included_match_ids().len()),
                    None,
                ),
                ReplacementPreviewRowKind::Document => (
                    workspace
                        .explorer()
                        .title_for_document(item.node_id)
                        .unwrap_or("Unavailable document")
                        .to_owned(),
                    None,
                ),
                ReplacementPreviewRowKind::Match => {
                    let snippet = format!(
                        "{}{}{}",
                        item.prefix.unwrap_or_default(),
                        item.matching_text.unwrap_or_default(),
                        item.suffix.unwrap_or_default()
                    );
                    (
                        format!(
                            "Before: {snippet}\nAfter: {}{}{}",
                            item.prefix.unwrap_or_default(),
                            workspace.global_search().replacement(),
                            item.suffix.unwrap_or_default()
                        ),
                        Some(item.node_id.to_owned()),
                    )
                }
            };
            let label = if item.check_state == ReplacementCheckState::Indeterminate {
                format!("{label} · partially selected")
            } else {
                label
            };
            let checked = item.check_state != ReplacementCheckState::Unselected;
            let node_id = item.node_id.to_owned();
            let control = checkbox(checked)
                .label(label)
                .size(16)
                .text_size(14)
                .width(Length::Fill)
                .on_toggle(move |included| {
                    ProjectSurfaceMessage::Project(ProjectMessage::SetReplacementIncluded {
                        node_id: node_id.clone(),
                        included,
                    })
                });
            let mut row_content = row![Space::new().width((item.depth * 18) as u32), control]
                .spacing(8)
                .align_y(iced::alignment::Vertical::Center);
            if let Some(match_id) = navigation {
                row_content = row_content.push(
                    button(text("Go to match").size(11))
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::NavigateGlobalSearchResult(match_id),
                        ))
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, false),
                            )
                        }),
                )
            }
            let row_content = if let Some(issue) = item.issue {
                row_content.push(text(format!("Skipped: {issue}")).size(11))
            } else {
                row_content
            };
            rows.push(row_content)
        });
    let validation = if preview.is_validating() {
        "Checking selected matches…".to_owned()
    } else if let Some(error) = preview.validation_error() {
        format!("Preview needs attention: {error}")
    } else if preview.is_revalidated() {
        String::new()
    } else {
        "The project or selection changed. Refresh the preview before applying.".to_owned()
    };
    let mut actions = row![].spacing(8);
    if !preview.is_revalidated() {
        actions = actions.push(button(text("Refresh preview")).on_press_maybe(
            (!preview.is_validating()).then_some(ProjectSurfaceMessage::Project(
                ProjectMessage::OpenReplacementPreview,
            )),
        ));
    }
    let apply = if preview.can_apply(workspace.project_revision()) {
        button(text("Apply replacement")).on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::ApplyReplacement,
        ))
    } else {
        button(text("Apply replacement"))
    };
    let mut content = column![
        row![
            page_title("Review replacements"),
            Space::new().width(Length::Fill),
            button(text("Close").size(12)).on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::CloseReplacementPreview
            ))
        ]
        .align_y(iced::alignment::Vertical::Center),
        replacement,
        text("Replaces document body text only; titles, synopsis, metadata, and notes stay as they are.").size(12),
    ].spacing(16);
    if !validation.is_empty() {
        content = content.push(text(validation).size(13));
    }
    container(
        content
            .push(actions.push(apply.style(move |_, status| {
                components::button_style(theme, ButtonKind::Primary, interaction(status, false))
            })))
            .push(crate::scroll_gate::smooth(
                scrollable(rows).height(Length::Fill),
            )),
    )
    .padding(SPACING_24)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn history_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let history = workspace.history();
    let timeline = history.timeline_window();
    let checkpoint_window = timeline.window;
    let checkpoints =
        timeline
            .checkpoints
            .into_iter()
            .fold(column![], |column, (checkpoint, heading)| {
                let checkpoint_id = checkpoint.checkpoint_id.clone();
                let selected = history.selected_checkpoint_id() == Some(checkpoint_id.as_str());
                let word_delta = history
                    .comparison()
                    .filter(|comparison| comparison.checkpoint_id == checkpoint_id)
                    .map(|comparison| {
                        let delta = comparison.word_count_delta();
                        format!(
                            " · {delta:+} {}",
                            if delta == 1 || delta == -1 {
                                "word"
                            } else {
                                "words"
                            }
                        )
                    })
                    .unwrap_or_default();
                let column = if let Some(heading) = heading {
                    column.push(
                        container(text(heading).size(11).color(theme.palette().secondary_text))
                            .height(Length::Fixed(
                                crate::project_workspace::HISTORY_TIMELINE_HEADING_HEIGHT,
                            ))
                            .align_y(iced::alignment::Vertical::Center),
                    )
                } else {
                    column
                };
                column
                    .push(harness_target::target_id(
                        harness_target::history_checkpoint_id(&checkpoint_id),
                        button(
                            column![
                                text(if checkpoint.name.is_some() {
                                    checkpoint.label()
                                } else if checkpoint.affected_document_ids.len() == 1 {
                                    let document = &checkpoint.affected_document_ids[0];
                                    let title = workspace
                                        .explorer()
                                        .node_id_for_document(document)
                                        .and_then(|id| workspace.explorer().title(id))
                                        .unwrap_or("Document");
                                    format!("{} · {title}", checkpoint.label())
                                } else if checkpoint.affected_document_ids.is_empty() {
                                    checkpoint.label()
                                } else {
                                    format!(
                                        "{} · {}",
                                        checkpoint.label(),
                                        checkpoint.affected_summary()
                                    )
                                })
                                .size(u32::from(UI_BODY.size))
                                .wrapping(text::Wrapping::None),
                                text(format!(
                                    "{}{}",
                                    checkpoint
                                        .recorded_at_unix_millis
                                        .map(crate::project_workspace::local_version_time)
                                        .unwrap_or_else(|| format!(
                                            "Version {}",
                                            checkpoint.sequence
                                        )),
                                    word_delta
                                ))
                                .size(u32::from(UI_COMPACT.size))
                                .color(theme.palette().secondary_text),
                            ]
                            .spacing(SPACING_4),
                        )
                        .padding(SPACING_8)
                        .width(Length::Fill)
                        .height(crate::project_workspace::HISTORY_CHECKPOINT_ROW_HEIGHT)
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::SelectHistoryCheckpoint(checkpoint_id),
                        ))
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, selected),
                            )
                        }),
                    ))
                    .push(
                        container(
                            Space::new()
                                .height(crate::project_workspace::HISTORY_TIMELINE_DIVIDER_HEIGHT),
                        )
                        .width(Length::Fill)
                        .style(move |_| iced::widget::container::Style {
                            background: Some(Background::Color(theme.palette().divider)),
                            ..Default::default()
                        }),
                    )
            });
    let checkpoints = if history.visible_checkpoints().next().is_none() {
        column![
            text("No saved versions")
                .size(13)
                .color(theme.palette().secondary_text)
        ]
    } else {
        column![
            Space::new().height(checkpoint_window.top_padding),
            checkpoints,
            Space::new().height(checkpoint_window.bottom_padding),
        ]
    };
    let restore: Element<'a, ProjectSurfaceMessage> = history
        .selected_checkpoint_id()
        .filter(|_| history.group_filter.is_none())
        .and_then(|checkpoint_id| {
            history
                .checkpoints()
                .iter()
                .find(|checkpoint| checkpoint.checkpoint_id == checkpoint_id)
                .map(|checkpoint| (checkpoint_id, checkpoint))
        })
        .map(|(checkpoint_id, _)| {
            let document_scope = history.active_document_filter().is_some();
            column![
                button(
                    text(if document_scope {
                        "Restore document…"
                    } else {
                        "Restore project…"
                    })
                    .size(12)
                )
                .on_press_maybe(
                    ((!document_scope || workspace.can_restore_history_document(checkpoint_id))
                        && !history
                            .preview()
                            .and_then(|preview| preview.project_changes.as_ref())
                            .is_some_and(|changes| changes.is_empty()))
                    .then_some(ProjectSurfaceMessage::Project(
                        ProjectMessage::RequestHistoryRestore {
                            checkpoint_id: checkpoint_id.to_owned(),
                        },
                    ))
                )
                .style(move |_, status| {
                    components::button_style(theme, ButtonKind::Primary, interaction(status, false))
                })
            ]
            .spacing(8)
            .into()
        })
        .unwrap_or_else(|| Space::new().height(0).into());
    let milestone_draft = history.named_snapshot_draft();
    let milestone_submit = ProjectSurfaceMessage::Project(ProjectMessage::RequestNamedSnapshot(
        milestone_draft.to_owned(),
    ));
    let can_create_milestone =
        !history.is_creating_named_snapshot() && !milestone_draft.trim().is_empty();
    let milestone = row![
        text_input("Milestone name", milestone_draft)
            .on_input_maybe((!history.is_creating_named_snapshot()).then_some(|name| {
                ProjectSurfaceMessage::Project(ProjectMessage::SetNamedSnapshotDraft(name))
            }))
            .on_submit_maybe(can_create_milestone.then_some(milestone_submit.clone()))
            .padding([6, 8])
            .width(Length::Fill),
        button(
            text(if history.is_creating_named_snapshot() {
                "Creating…"
            } else {
                "Create milestone"
            })
            .size(12)
        )
        .on_press_maybe(can_create_milestone.then_some(milestone_submit)),
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Center);
    let heading = if history.active_document_filter().is_some() {
        column![page_title("History")]
    } else {
        column![page_title("Project history")]
    };
    let load_more: Element<'a, ProjectSurfaceMessage> = if history.next_cursor().is_some() {
        button(text(if history.is_loading_more() {
            "Loading more…"
        } else {
            "Load more"
        }))
        .on_press_maybe(
            (!history.is_loading_more()).then_some(ProjectSurfaceMessage::LoadMoreHistory),
        )
        .into()
    } else {
        Space::new().height(0).into()
    };
    let comparison: Element<'a, ProjectSurfaceMessage> = match history.comparison() {
        Some(comparison) => crate::history_diff::view(comparison, theme),
        None if history.error().is_some() => Space::new().height(0).into(),
        None if history.preview().is_some() => {
            text("This document was not present in this version.")
                .size(12)
                .into()
        }
        None if history.selected_checkpoint_id().is_some() => {
            components::muted_label("Loading version comparison…").into()
        }
        None => components::muted_label("Select a version.").into(),
    };
    let comparison = if let Some(changes) = history
        .preview()
        .and_then(|preview| preview.project_changes.as_ref())
    {
        let mut content = column![
            text(
                history
                    .preview()
                    .map(|preview| format!(
                        "{} · {}",
                        preview.checkpoint.label(),
                        preview
                            .checkpoint
                            .recorded_at_unix_millis
                            .map(crate::project_workspace::local_version_time)
                            .unwrap_or_else(|| "Date unavailable".to_owned())
                    ))
                    .unwrap_or_default()
            )
            .size(13),
        ]
        .spacing(12);
        let changes = changes
            .iter()
            .filter(|change| {
                history
                    .group_filter
                    .as_ref()
                    .is_none_or(|group| change.path.iter().any(|heading| &heading.id == group))
            })
            .cloned()
            .collect::<Vec<_>>();
        if changes.is_empty() {
            content = content.push(text("No changes since this version."));
        }
        content = content.push(crate::history_diff::tree_view(
            &changes,
            &history.collapsed_sections,
            theme,
        ));
        content.into()
    } else {
        comparison
    };
    let error: Element<'a, ProjectSurfaceMessage> = history
        .error()
        .map(|error| {
            container(text(error).size(13).wrapping(text::Wrapping::WordOrGlyph))
                .padding(12)
                .width(Length::Fill)
                .style(move |_| components::status_style(theme, components::StatusKind::Error))
                .into()
        })
        .unwrap_or_else(|| Space::new().height(0).into());
    let maintenance: Element<'a, ProjectSurfaceMessage> = match history.maintenance() {
        HistoryMaintenanceStatus::Available => history
            .maintenance_message()
            .map(|message| text(message).size(12).into())
            .unwrap_or_else(|| Space::new().height(0).into()),
        HistoryMaintenanceStatus::Reinitializable { problem } => row![
            text(format!("History is unavailable: {problem}")).size(12),
            button(text("Reinitialize History").size(12)).on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::RequestHistoryReinitialize
            ))
        ]
        .spacing(8)
        .into(),
        HistoryMaintenanceStatus::Unavailable { problem, reason } => text(format!(
            "History is unavailable: {problem}. Reinitialization is blocked: {reason}"
        ))
        .size(12)
        .into(),
    };
    let mut locations = column![
        button(text("Entire project").size(13))
            .on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::SetHistoryDocumentFilter(None)
            ))
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                interaction(status, false)
            ))
    ];
    for item in workspace
        .explorer()
        .rows()
        .into_iter()
        .filter(|item| hierarchy_row_is_visible(workspace.explorer(), item.parent_id))
    {
        let id = item.id.to_owned();
        let is_group = item.kind != HierarchyRowKind::Document;
        let selected = item.document_id == history.active_document_filter() && !is_group
            || history.group_filter.as_deref() == Some(item.id);
        let select = if let Some(document) = item.document_id {
            ProjectMessage::SetHistoryDocumentFilter(Some(document.to_owned()))
        } else {
            ProjectMessage::SetHistoryGroupFilter(id.clone())
        };
        locations = locations.push(components::location_row(
            item.title.to_owned(),
            hierarchy_depth(workspace.explorer(), item.parent_id),
            is_group.then_some(item.expanded),
            selected,
            ProjectSurfaceMessage::Project(ProjectMessage::ToggleHierarchyExpanded(id)),
            ProjectSurfaceMessage::Project(select),
            theme,
        ));
    }
    let filter = crate::action_menu::panel(
        button(
            row![
                text(workspace.history_scope_label())
                    .size(13)
                    .width(Length::Fill),
                icon_sized(Icon::ChevronDown, 18)
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .on_press(())
        .padding([6, 4])
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
        })
        .into(),
        container(scrollable(locations).height(Length::Shrink))
            .max_height(360)
            .into(),
        theme,
        320.0,
    );
    let list = column![
        heading,
        filter,
        harness_target::target(
            HarnessTarget::HistoryTimeline,
            crate::scroll_gate::smooth(
                scrollable(checkpoints)
                    .on_scroll(|viewport| ProjectSurfaceMessage::Project(
                        ProjectMessage::SetHistoryScroll(viewport.absolute_offset().y)
                    ))
                    .height(Length::Fill)
            )
        ),
        load_more,
        if history.active_document_filter().is_none() {
            Element::from(milestone)
        } else {
            Element::from(Space::new().height(0))
        },
    ]
    .spacing(SPACING_12);
    let detail = column![
        error,
        maintenance,
        harness_target::target(
            HarnessTarget::HistoryComparison,
            crate::scroll_gate::smooth(scrollable(comparison).height(Length::Fill))
        ),
        restore,
    ]
    .spacing(SPACING_16);
    row![
        container(list)
            .padding([SPACING_24, SPACING_16])
            .width(280)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
        static_divider(theme),
        // center_view already paints the History canvas as Manuscript.
        container(detail)
            .padding(SPACING_24)
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}

fn deleted_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let deleted = workspace.recently_deleted();
    let items = deleted.items();
    let selected_item_id = deleted.selected_item_id();
    let rows = items.iter().fold(column![].spacing(6), |column, item| {
        let former = restore_location_label(workspace, item.former_location);
        let kind = match item.kind {
            HierarchyRowKind::Root => "Section",
            HierarchyRowKind::Group => "Group",
            HierarchyRowKind::Document => "Document",
        };
        let selected = selected_item_id == Some(item.node_id);
        column.push(
            button(
                column![
                    row![
                        text(item.title)
                            .size(14)
                            .width(Length::Fill)
                            .wrapping(text::Wrapping::WordOrGlyph),
                        text(kind).size(11).color(theme.palette().secondary_text),
                    ]
                    .align_y(iced::alignment::Vertical::Center),
                    text(format!("Former location: {former}")).size(11),
                ]
                .spacing(4),
            )
            .width(Length::Fill)
            .padding(10)
            .on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::SelectRecentlyDeleted(item.node_id.to_owned()),
            ))
            .style(move |_, status| {
                components::button_style(theme, ButtonKind::Quiet, interaction(status, selected))
            }),
        )
    });
    let list = if items.is_empty() {
        rows.push(components::muted_label("No deleted items"))
    } else {
        rows
    };
    let selected = items
        .iter()
        .find(|item| Some(item.node_id) == selected_item_id);
    let preview: Element<'a, ProjectSurfaceMessage> = match selected {
        Some(item) => {
            let using_fallback = item.former_location != item.restore_location;
            let restore_location = restore_location_label(workspace, item.restore_location);
            let content: Element<'a, ProjectSurfaceMessage> = match deleted.selected_preview() {
                Some(preview) => crate::scroll_gate::smooth(
                    scrollable(container(semantic_preview(preview.semantic, theme)).max_width(840))
                        .height(Length::Fill),
                ),
                None => column![
                    text(if item.preview_document_id.is_none() {
                        "This group has no documents to preview."
                    } else {
                        "A formatted preview is not available."
                    })
                    .size(14),
                ]
                .spacing(8)
                .into(),
            };
            column![
                row![
                    page_title(item.title)
                        .width(Length::Fill)
                        .wrapping(text::Wrapping::WordOrGlyph),
                    text("Read only")
                        .size(12)
                        .color(theme.palette().secondary_text),
                ]
                .spacing(12)
                .align_y(iced::alignment::Vertical::Center),
                container(content).width(Length::Fill).height(Length::Fill),
                row![
                    text(format!(
                        "Restore to {restore_location}{}",
                        if using_fallback {
                            " · original folder is unavailable"
                        } else {
                            ""
                        }
                    ))
                    .size(12)
                    .width(Length::Fill),
                    button(text("Restore item").size(12))
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::RestoreDeleted(item.node_id.to_owned()),
                        ))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Primary,
                            interaction(status, false),
                        )),
                ]
                .spacing(12)
                .align_y(iced::alignment::Vertical::Center),
            ]
            .spacing(20)
            .height(Length::Fill)
            .into()
        }
        None => text(if items.is_empty() {
            ""
        } else {
            "Select an item to preview."
        })
        .size(14)
        .color(theme.palette().secondary_text)
        .into(),
    };
    row![
        container(
            column![
                page_title("Recently Deleted"),
                crate::scroll_gate::smooth(scrollable(list).height(Length::Fill)),
            ]
            .spacing(18)
            .height(Length::Fill)
        )
        .padding([SPACING_24, SPACING_16])
        .width(320)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
        container(preview)
            .padding(SPACING_24)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| iced::widget::container::Style::default()),
    ]
    .height(Length::Fill)
    .into()
}

fn semantic_preview<'a>(
    document: &'a parchmint_editor_api::SemanticDocument,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    document
        .blocks()
        .iter()
        .fold(column![].spacing(10), |column, block| {
            column.push(semantic_preview_block(block, theme))
        })
        .into()
}

fn semantic_preview_block<'a>(
    block: &'a SemanticBlock,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let (size, block_font) = match block.kind() {
        // The deterministic renderer bundles Source Serif Regular only.
        // `semantic_preview_spans` still applies authored inline bold marks.
        SemanticBlockKind::Heading1 => (24, Font::with_name("Source Serif 4")),
        SemanticBlockKind::Heading2 => (20, Font::with_name("Source Serif 4")),
        SemanticBlockKind::Heading3 => (18, Font::with_name("Source Serif 4")),
        SemanticBlockKind::SceneBreak => return text("* * *").size(16).width(Length::Fill).into(),
        SemanticBlockKind::PageBreak => {
            return text("— page break —").size(12).width(Length::Fill).into();
        }
        _ => (16, Font::with_name("Source Serif 4")),
    };
    let content = rich_text(semantic_preview_spans(block, block_font, size, theme))
        .size(size)
        .width(Length::Fill);
    let content: Element<'a, ProjectSurfaceMessage> = match block.kind() {
        SemanticBlockKind::UnorderedListItem | SemanticBlockKind::OrderedListItem => {
            let marker = if block.kind() == SemanticBlockKind::UnorderedListItem {
                "•".to_owned()
            } else {
                "1.".to_owned()
            };
            row![
                Space::new().width(Length::Fixed((block.list_depth() * 18) as f32)),
                text(marker).size(size).width(20),
                content,
            ]
            .into()
        }
        _ => content.into(),
    };
    if block.kind() == SemanticBlockKind::BlockQuote {
        row![
            iced::widget::rule::vertical(2).style(move |iced_theme| {
                iced::widget::rule::Style {
                    color: theme.palette().border,
                    ..iced::widget::rule::default(iced_theme)
                }
            }),
            container(content).padding([6, 10]).width(Length::Fill),
        ]
        .into()
    } else {
        content
    }
}

fn semantic_preview_spans<'a>(
    block: &'a SemanticBlock,
    block_font: Font,
    size: u32,
    theme: ParchMintTheme,
) -> Vec<iced::widget::text::Span<'a>> {
    let characters = block.text().chars().collect::<Vec<_>>();
    let mut boundaries = vec![0, characters.len()];
    for mark in block.marks() {
        boundaries.push((mark.range().start().value() as usize).min(characters.len()));
        boundaries.push((mark.range().end().value() as usize).min(characters.len()));
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
        .windows(2)
        .filter_map(|window| {
            let start = window[0];
            let end = window[1];
            (start < end).then(|| {
                let active = block.marks().iter().filter(|mark| {
                    mark.range().start().value() as usize <= start
                        && end <= mark.range().end().value() as usize
                });
                let mut bold = false;
                let mut italic = false;
                let mut underline = false;
                let mut strikethrough = false;
                let mut small_caps = false;
                let mut reduced_size = false;
                let mut link = false;
                let mut font = block_font;
                let mut inline_size = None;
                for mark in active {
                    match mark.mark() {
                        SemanticInlineMark::Bold => bold = true,
                        SemanticInlineMark::Italic => italic = true,
                        SemanticInlineMark::Underline => underline = true,
                        SemanticInlineMark::Strikethrough => strikethrough = true,
                        SemanticInlineMark::SmallCaps => small_caps = true,
                        SemanticInlineMark::Superscript | SemanticInlineMark::Subscript => {
                            reduced_size = true
                        }
                        SemanticInlineMark::Link(_) => {
                            link = true;
                            underline = true;
                        }
                        SemanticInlineMark::FontFamily(family) => {
                            font.family = match family {
                                parchmint_editor_api::InlineFontFamily::Serif => {
                                    font::Family::Name("Source Serif 4")
                                }
                                parchmint_editor_api::InlineFontFamily::SansSerif => {
                                    font::Family::Name("Source Sans 3")
                                }
                                parchmint_editor_api::InlineFontFamily::Monospace => {
                                    font::Family::Monospace
                                }
                            }
                        }
                        SemanticInlineMark::FontSize(points) => {
                            inline_size = Some(f32::from(*points) * (4.0 / 3.0))
                        }
                    }
                }
                if bold {
                    font.weight = font::Weight::Bold;
                }
                if italic {
                    font.style = font::Style::Italic;
                }
                let mut content = characters[start..end].iter().collect::<String>();
                if small_caps {
                    content = content.to_uppercase();
                }
                let mut rendered = span(content)
                    .font(font)
                    .underline(underline)
                    .strikethrough(strikethrough);
                if let Some(size) = inline_size {
                    rendered = rendered.size(size);
                }
                if reduced_size || small_caps {
                    let base = inline_size.unwrap_or(size as f32);
                    let reduction = if reduced_size { 3.0 } else { 1.0 };
                    rendered = rendered.size((base - reduction).max(1.0));
                }
                if link {
                    rendered = rendered.color(theme.palette().accent);
                }
                rendered
            })
        })
        .collect()
}

fn restore_location_label(workspace: &ProjectWorkspace, location: &RestoreLocation) -> String {
    let id = match location {
        RestoreLocation::FormerParent(id) | RestoreLocation::SectionRoot(id) => id,
    };
    workspace
        .explorer()
        .title(id)
        .map(str::to_owned)
        .unwrap_or_else(|| id.clone())
}

fn export_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let export = workspace.export();
    let state = match export.state() {
        crate::ExportState::Ready => "Ready".to_owned(),
        crate::ExportState::ChoosingDestination => "Choose an export destination…".to_owned(),
        crate::ExportState::Planning => "Preparing your manuscript…".to_owned(),
        crate::ExportState::Exporting { completed, total } => {
            format!("Exporting document {completed} of {total}…")
        }
        crate::ExportState::Committing => "Saving the exported file…".to_owned(),
        crate::ExportState::Cancelling => "Cancelling export…".to_owned(),
        crate::ExportState::Succeeded { artifact } => {
            format!("Exported {}", artifact.display_name)
        }
        crate::ExportState::Cancelled => "Export cancelled".to_owned(),
        crate::ExportState::Failed(error) => error,
    };
    let title_setting = export.project_settings().emit_titles;
    let mut terminal_actions = row![].spacing(8);
    if export.can_cancel() {
        terminal_actions = terminal_actions.push(
            button(text("Cancel export"))
                .on_press(ProjectSurfaceMessage::Project(ProjectMessage::CancelExport)),
        );
    }
    if export.can_open_result() {
        terminal_actions = terminal_actions
            .push(
                button(text("Open")).on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::OpenExportResult,
                )),
            )
            .push(
                button(text("Reveal")).on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::RevealExportResult,
                )),
            );
    }
    let title_control: Element<'a, ProjectSurfaceMessage> = if export.can_configure() {
        harness_target::target(
            HarnessTarget::ExportTitles,
            pick_list(
                vec!["Default · include", "Include", "Exclude"],
                Some(export_setting_label(title_setting)),
                |value| {
                    ProjectSurfaceMessage::Project(ProjectMessage::SetExportTitleSetting(
                        match value {
                            "Include" => parchmint_domain::ProjectExportSetting::Enabled,
                            "Exclude" => parchmint_domain::ProjectExportSetting::Disabled,
                            _ => parchmint_domain::ProjectExportSetting::Inherit,
                        },
                    ))
                },
            )
            .width(Length::Fill),
        )
    } else {
        text(export_setting_label(title_setting)).into()
    };
    let page_break_control = checkbox(export.project_settings().starts_new_page)
        .label("Include manuscript page breaks")
        .on_toggle_maybe(export.can_configure().then_some(|enabled| {
            ProjectSurfaceMessage::Project(ProjectMessage::SetExportPageBreak(enabled))
        }));
    let output_controls = column![
        row![
            text("Include chapter titles")
                .size(16)
                .width(Length::FillPortion(3)),
            container(title_control).width(Length::FillPortion(2)),
        ]
        .spacing(32)
        .align_y(iced::alignment::Vertical::Center),
        page_break_control,
        checkbox(export.numbers_documents())
            .label("Number chapter headings")
            .on_toggle_maybe(export.can_configure().then_some(|enabled| {
                ProjectSurfaceMessage::Project(ProjectMessage::SetExportNumbering(enabled))
            })),
    ]
    .spacing(20);
    let state: Element<'a, ProjectSurfaceMessage> =
        if matches!(export.state(), crate::ExportState::Ready) {
            Space::new().height(0).into()
        } else {
            let kind = match export.state() {
                crate::ExportState::Succeeded { .. } => components::StatusKind::Success,
                crate::ExportState::Failed(_) => components::StatusKind::Error,
                crate::ExportState::Cancelled => components::StatusKind::Warning,
                _ => components::StatusKind::Saving,
            };
            container(text(state).size(14).wrapping(text::Wrapping::WordOrGlyph))
                .padding(12)
                .width(Length::Fill)
                .style(move |_| components::status_style(theme, kind))
                .into()
        };
    let output_file = row![
        container(
            text(
                export
                    .destination()
                    .unwrap_or("Choose where to save your manuscript")
            )
            .size(14)
            .wrapping(text::Wrapping::WordOrGlyph)
        )
        .width(Length::Fill)
        .padding([9, 12])
        .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
        harness_target::target(
            HarnessTarget::ExportBrowse,
            button(text("Browse…"))
                .padding([9, 14])
                .on_press_maybe(
                    export
                        .can_configure()
                        .then_some(ProjectSurfaceMessage::Project(
                            ProjectMessage::BrowseExportDestination
                        ))
                )
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Secondary,
                    interaction(status, false)
                )),
        ),
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Center);
    let content =
        container(
            column![
                column![
                    page_title("Export manuscript"),
                    text("Manuscript · HTML")
                        .size(14)
                        .color(theme.palette().secondary_text),
                ]
                .spacing(8),
                column![components::muted_label("Destination"), output_file].spacing(8),
                output_controls,
                state,
                row![
                    terminal_actions,
                    Space::new().width(Length::Fill),
                    harness_target::target(
                        HarnessTarget::ExportStart,
                        button(text("Export"))
                            .padding([10, 18])
                            .on_press_maybe(export.can_start().then_some(
                                ProjectSurfaceMessage::Project(ProjectMessage::StartExport)
                            ))
                            .style(move |_, status| components::button_style(
                                theme,
                                ButtonKind::Primary,
                                interaction(status, false)
                            )),
                    ),
                ]
                .spacing(12),
            ]
            .spacing(24),
        )
        .padding([32, 24])
        .width(Length::Fill)
        .max_width(760);
    container(crate::scroll_gate::smooth(
        scrollable(container(content).center_x(Length::Fill)).height(Length::Fill),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(iced::alignment::Horizontal::Center)
    .into()
}

fn export_setting_label(setting: parchmint_domain::ProjectExportSetting) -> &'static str {
    match setting {
        parchmint_domain::ProjectExportSetting::Inherit => "Default · include",
        parchmint_domain::ProjectExportSetting::Enabled => "Include",
        parchmint_domain::ProjectExportSetting::Disabled => "Exclude",
    }
}

fn settings_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let settings = workspace.settings();
    let category = settings.selected_category();
    let navigation =
        settings
            .categories()
            .into_iter()
            .fold(column![].spacing(SPACING_4), |column, item| {
                let column = if item.category == SettingsCategory::Appearance {
                    column.push(
                        text("Application")
                            .size(12)
                            .color(theme.palette().secondary_text),
                    )
                } else if item.category == SettingsCategory::Styles {
                    column.push(Space::new().height(12)).push(
                        text("This project")
                            .size(12)
                            .color(theme.palette().secondary_text),
                    )
                } else {
                    column
                };
                column.push(
                    button(text(item.label).size(13).font(if item.selected {
                        Font {
                            weight: font::Weight::Bold,
                            ..Font::DEFAULT
                        }
                    } else {
                        Font::DEFAULT
                    }))
                    .width(Length::Fill)
                    .padding([SPACING_8, SPACING_12])
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::SelectSettingsCategory(item.category),
                    ))
                    .style(move |_, status| {
                        flat_selection_button_style(theme, status, item.selected)
                    }),
                )
            });
    row![
        container(column![components::muted_label("Settings"), navigation,].spacing(SPACING_12),)
            .padding([SPACING_16, SPACING_12])
            .width(280)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
        static_divider(theme),
        container(crate::motion::enter(
            format!("{category:?}"),
            settings_content(workspace, category, theme, true)
        ))
        .padding([SPACING_24, SPACING_24])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
    ]
    .height(Length::Fill)
    .into()
}

fn metadata_section_heading(
    label: &'static str,
    target_index: usize,
    visible_on_cards: bool,
    active_target: Option<(usize, bool)>,
    targets: &hierarchy_drag::HoverTargets<(usize, bool)>,
    theme: ParchMintTheme,
) -> Element<'static, ProjectSurfaceMessage> {
    let indicator = (active_target == Some((target_index, visible_on_cards))).then_some(
        hierarchy_drag::DropIndicator {
            position: hierarchy_drag::DropIndicatorPosition::Into,
            color: theme.palette().selection_border,
        },
    );
    hierarchy_drag::target(
        container(components::muted_label(label))
            .padding([10, 4])
            .width(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
        indicator,
        targets,
        move |bounds, point| {
            bounds
                .contains(point)
                .then_some((target_index, visible_on_cards))
        },
    )
}

fn settings_content<'a>(
    workspace: &'a ProjectWorkspace,
    category: SettingsCategory,
    theme: ParchMintTheme,
    already_on_panel: bool,
) -> Element<'a, ProjectSurfaceMessage> {
    let settings = workspace.settings();
    match category {
        SettingsCategory::Shortcuts => {
            let mut controls = column![
                row![
                    page_title("Keyboard shortcuts"),
                    Space::new().width(Length::Fill),
                    button(text("Reset all").size(13))
                        .on_press_maybe((!settings.shortcuts_busy).then_some(
                            ProjectSurfaceMessage::Project(ProjectMessage::ResetAllShortcuts)
                        ))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false)
                        ))
                ]
                .align_y(iced::alignment::Vertical::Center),
                text_input("Search commands", &settings.shortcut_query)
                    .on_input(|value| ProjectSurfaceMessage::Project(
                        ProjectMessage::SetShortcutQuery(value)
                    ))
                    .padding(8)
                    .size(14),
            ]
            .spacing(12);
            if let Some(error) = &settings.shortcut_error {
                controls = controls.push(text(error).size(13).color(theme.palette().error));
            }
            let query = settings.shortcut_query.to_lowercase();
            let mut group = "";
            for command in parchmint_preferences::shortcut_commands() {
                if !format!("{} {}", command.label, command.group)
                    .to_lowercase()
                    .contains(&query)
                {
                    continue;
                }
                if group != command.group {
                    group = command.group;
                    controls = controls.push(
                        container(text(group).size(12).color(theme.palette().secondary_text))
                            .padding(iced::Padding {
                                top: 12.0,
                                ..Default::default()
                            }),
                    );
                }
                let binding =
                    parchmint_preferences::effective_shortcut(&command, &settings.keybindings);
                let recording = settings.shortcut_recording.as_deref() == Some(command.id);
                let mut entry = row![
                    text(command.label).size(14).width(Length::Fill),
                    button(
                        text(if recording {
                            "Press shortcut…".into()
                        } else {
                            binding
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_else(|| "Unassigned".into())
                        })
                        .size(13)
                    )
                    .width(180)
                    .on_press_maybe((!settings.shortcuts_busy).then_some(
                        ProjectSurfaceMessage::Project(ProjectMessage::RecordShortcut(
                            command.id.into()
                        ))
                    ))
                    .style(move |_, status| components::button_style(
                        theme,
                        ButtonKind::Secondary,
                        interaction(status, recording)
                    )),
                ]
                .spacing(8)
                .align_y(iced::alignment::Vertical::Center);
                let action = if recording {
                    ProjectMessage::CancelShortcutRecording
                } else {
                    ProjectMessage::ClearShortcut(command.id.into())
                };
                entry = entry.push(
                    button(text(if recording { "Cancel" } else { "Clear" }).size(12))
                        .width(58)
                        .on_press_maybe(
                            (!settings.shortcuts_busy && (recording || binding.is_some()))
                                .then_some(ProjectSurfaceMessage::Project(action)),
                        )
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, false),
                            )
                        }),
                );
                entry = entry.push(
                    button(text("Reset").size(12))
                        .width(58)
                        .on_press_maybe(
                            (!settings.shortcuts_busy
                                && settings.keybindings.contains_key(command.id))
                            .then_some(ProjectSurfaceMessage::Project(
                                ProjectMessage::ResetShortcut(command.id.into()),
                            )),
                        )
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, false),
                            )
                        }),
                );
                controls = controls.push(entry);
            }
            crate::scroll_gate::smooth(scrollable(controls).height(Length::Fill))
        }
        SettingsCategory::Appearance => {
            let choices = settings.appearance_choices().into_iter().fold(
                column![].spacing(SPACING_8),
                |column, mode| {
                    let name = match mode {
                        parchmint_preferences::AppearanceMode::System => "System",
                        parchmint_preferences::AppearanceMode::Light => "Light",
                        parchmint_preferences::AppearanceMode::Dark => "Dark",
                    };
                    column.push(
                        button(
                            row![
                                text(if settings.appearance() == mode {
                                    "◉"
                                } else {
                                    "○"
                                })
                                .size(22),
                                text(name)
                                    .width(Length::Fill)
                                    .size(u32::from(UI_HEADING.size))
                                    .font(Font {
                                        weight: font::Weight::Bold,
                                        ..Font::DEFAULT
                                    }),
                            ]
                            .align_y(iced::alignment::Vertical::Center)
                            .spacing(SPACING_12),
                        )
                        .width(Length::Fill)
                        .padding([SPACING_8, SPACING_12])
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::SetAppearance(mode),
                        ))
                        .style(move |_, status| {
                            flat_selection_button_style(
                                theme,
                                status,
                                settings.appearance() == mode,
                            )
                        }),
                    )
                },
            );
            crate::scroll_gate::smooth(
                scrollable(
                    column![
                        page_title("Appearance"),
                        container(choices).width(Length::Fill).max_width(540),
                        row![
                            components::muted_label("Application zoom"),
                            pick_list(
                                ["75%", "90%", "100%", "110%", "125%", "150%"].map(str::to_owned),
                                Some(format!("{}%", settings.ui_zoom_percent())),
                                |value: String| ProjectSurfaceMessage::Project(
                                    ProjectMessage::SetUiZoom(
                                        value.trim_end_matches('%').parse().unwrap_or(100)
                                    )
                                )
                            ),
                            button(text("Reset").size(13)).on_press_maybe(
                                (settings.ui_zoom_percent() != 100).then_some(
                                    ProjectSurfaceMessage::Project(ProjectMessage::SetUiZoom(100))
                                )
                            ),
                        ]
                        .spacing(12)
                        .align_y(iced::alignment::Vertical::Center),
                        iced::widget::checkbox(crate::motion::reduced())
                            .label("Reduce motion")
                            .on_toggle(|value| ProjectSurfaceMessage::Project(
                                ProjectMessage::SetReducedMotion(value)
                            )),
                    ]
                    .spacing(SPACING_12),
                )
                .height(Length::Fill),
            )
        }
        SettingsCategory::Metadata => {
            let targets = hierarchy_drag::targets();
            let active_target = settings.metadata_drag_target();
            let mut fields: Vec<_> = settings.metadata_fields().into_iter().enumerate().collect();
            fields.sort_by_key(|(_, field)| !field.visible_on_cards);
            let shown_count = fields
                .iter()
                .filter(|(_, field)| field.visible_on_cards)
                .count();
            let shown_end = fields
                .get(shown_count)
                .map_or(fields.len(), |(index, _)| *index);
            let field_count = fields.len();
            let metadata = fields.into_iter().enumerate().fold(
                column![
                    row![
                        Space::new().width(Length::Fill),
                        harness_target::target(
                            HarnessTarget::CreateMetadataField,
                            button(text("+ New").size(13))
                                .on_press(ProjectSurfaceMessage::Project(
                                    ProjectMessage::CreateMetadataField
                                ))
                                .style(move |_, status| {
                                    components::button_style(
                                        theme,
                                        ButtonKind::Quiet,
                                        interaction(status, false),
                                    )
                                })
                        ),
                    ]
                    .align_y(iced::alignment::Vertical::Center),
                    metadata_section_heading(
                        "Shown on collapsed cards",
                        0,
                        true,
                        active_target,
                        &targets,
                        theme,
                    ),
                ]
                .spacing(SPACING_8),
                |mut column, (display_index, (index, field))| {
                    if display_index == shown_count {
                        column = column.push(metadata_section_heading(
                            "Hidden until expanded",
                            shown_end,
                            false,
                            active_target,
                            &targets,
                            theme,
                        ));
                    }
                    let id = field.id.to_owned();
                    let selected = matches!(
                        settings.selected_detail(),
                        Some(SettingsDetail::MetadataField(selected_id)) if selected_id == field.id
                    );
                    let dragging = settings.metadata_drag_source() == Some(field.id);
                    let drag_id = id.clone();
                    let entry = hierarchy_drag::source(
                        format!("metadata-{id}"),
                        container(
                            row![
                                icon_sized(Icon::ReorderGrip, 18),
                                text(field.label).size(14),
                            ]
                            .spacing(8)
                            .align_y(iced::alignment::Vertical::Center),
                        )
                        .padding(SPACING_8)
                        .width(Length::Fill)
                        .style(move |_| {
                            if selected || dragging {
                                components::surface(theme, Surface::Panel, Interaction::Selected)
                            } else {
                                iced::widget::container::Style::default()
                            }
                        }),
                        ProjectSurfaceMessage::Project(ProjectMessage::SelectMetadataField(
                            id.clone(),
                        )),
                        None,
                        ProjectSurfaceMessage::Project(ProjectMessage::BeginMetadataFieldDrag(
                            drag_id,
                        )),
                    );
                    let row = row![
                        container(entry).width(Length::Fill),
                        harness_target::target(
                            HarnessTarget::DeleteMetadataField(index),
                            button(icon_sized(Icon::RecentlyDeleted, 16))
                                .padding(5)
                                .on_press(ProjectSurfaceMessage::Project(
                                    ProjectMessage::RequestDeleteMetadataField(id.clone())
                                ))
                                .style(move |_, status| components::button_style(
                                    theme,
                                    ButtonKind::Quiet,
                                    interaction(status, false)
                                ))
                        ),
                    ]
                    .spacing(SPACING_8);
                    let before = active_target == Some((index, field.visible_on_cards));
                    let after = active_target == Some((index + 1, field.visible_on_cards));
                    let indicator = (before || after).then_some(hierarchy_drag::DropIndicator {
                        position: if before {
                            hierarchy_drag::DropIndicatorPosition::Before
                        } else {
                            hierarchy_drag::DropIndicatorPosition::After
                        },
                        color: theme.palette().selection_border,
                    });
                    column.push(hierarchy_drag::target_with_zone(
                        container(row).width(Length::Fill),
                        indicator,
                        &targets,
                        move |bounds, point| {
                            if !bounds.contains(point) {
                                return None;
                            }
                            let before = point.y < bounds.center_y();
                            let zone = iced::Rectangle {
                                y: if before { bounds.y } else { bounds.center_y() },
                                height: bounds.height * 0.5,
                                ..bounds
                            };
                            Some(((index + usize::from(!before), field.visible_on_cards), zone))
                        },
                    ))
                },
            );
            let metadata = if shown_count == field_count {
                metadata.push(metadata_section_heading(
                    "Hidden until expanded",
                    field_count,
                    false,
                    active_target,
                    &targets,
                    theme,
                ))
            } else {
                metadata
            };
            let detail = match settings.selected_detail() {
                Some(SettingsDetail::MetadataField(id)) => settings
                    .metadata_field(id)
                    .map(|field| metadata_field_detail(field, theme)),
                Some(SettingsDetail::NewMetadataField) => settings
                    .new_metadata_field_label()
                    .map(|label| metadata_field_creation_detail(label, theme)),
                _ => None,
            }
            .unwrap_or_else(|| {
                text(if settings.metadata_fields().is_empty() {
                    "Add fields for viewpoint, setting, or draft status."
                } else {
                    "Select a field."
                })
                .size(13)
                .into()
            });
            let metadata = hierarchy_drag::surface(
                crate::scroll_gate::smooth(scrollable(metadata).height(Length::Fill)),
                targets,
                settings.metadata_drag_source().is_some(),
                true,
                |destination| {
                    ProjectSurfaceMessage::Project(ProjectMessage::SetMetadataFieldDragTarget(
                        destination,
                    ))
                },
                ProjectSurfaceMessage::Project(ProjectMessage::CancelMetadataFieldDrag),
            );
            column![
                row![
                    container(metadata)
                        .padding(12)
                        .style(move |_| components::surface(
                            theme,
                            Surface::Sidebar,
                            Interaction::Rest
                        ))
                        .width(256)
                        .height(Length::Fill),
                    static_divider(theme),
                    container(
                        column![crate::scroll_gate::smooth(
                            scrollable(detail).height(Length::Fill)
                        )]
                        .spacing(10),
                    )
                    .padding(12)
                    .width(Length::Fill)
                    .max_width(920)
                    .height(Length::Fill)
                    .style(move |_| {
                        // The Settings page already paints this entire region as Panel.
                        if already_on_panel {
                            iced::widget::container::Style::default()
                        } else {
                            components::surface(theme, Surface::Panel, Interaction::Rest)
                        }
                    }),
                ]
                .spacing(0)
                .height(Length::Fill),
            ]
            .spacing(SPACING_12)
            .height(Length::Fill)
            .into()
        }
        SettingsCategory::Styles => {
            let mut last_custom = None;
            let styles = settings.styles().into_iter().fold(
                column![harness_target::target(
                    HarnessTarget::CreateStyle,
                    button(text("+ New").size(13))
                        .width(80)
                        .height(32)
                        .padding(4)
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false)
                        ))
                        .on_press(ProjectSurfaceMessage::Project(ProjectMessage::CreateStyle))
                ),]
                .spacing(6),
                |column, style| {
                    let custom = !style.role.is_reserved();
                    let column = if last_custom != Some(custom) {
                        last_custom = Some(custom);
                        column.push(components::muted_label(if custom {
                            "Custom"
                        } else {
                            "Built-in"
                        }))
                    } else {
                        column
                    };
                    let selected = matches!(settings.selected_detail(),
                        Some(SettingsDetail::Style(id)) if id == style.id);
                    let entry = button(text(style.display_name).size(14))
                        .width(Length::Fill)
                        .padding(SPACING_8)
                        .on_press(ProjectSurfaceMessage::Project(ProjectMessage::SelectStyle(
                            style.id.to_owned(),
                        )))
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, selected),
                            )
                        });
                    let mut line = row![entry].align_y(iced::alignment::Vertical::Center);
                    if !style.role.is_reserved() {
                        line = line.push(stationary_tooltip::tooltip(
                            button(icon_sized(Icon::RecentlyDeleted, 18))
                                .padding(5)
                                .on_press(ProjectSurfaceMessage::Project(
                                    ProjectMessage::RequestDeleteStyle(style.id.to_owned()),
                                ))
                                .style(move |_, status| {
                                    components::button_style(
                                        theme,
                                        ButtonKind::Quiet,
                                        interaction(status, false),
                                    )
                                }),
                            text("Delete style").size(12),
                            components::surface(theme, Surface::Elevated, Interaction::Rest),
                        ));
                    }
                    column.push(line)
                },
            );
            let detail = match settings.selected_detail() {
                Some(SettingsDetail::Style(id)) => settings
                    .style(id)
                    .map(|style| style_detail(settings, style, theme)),
                _ => None,
            }
            .unwrap_or_else(|| components::muted_label("Select a style.").into());
            column![
                row![
                    container(crate::scroll_gate::smooth(
                        scrollable(styles).height(Length::Fill)
                    ))
                    .padding(12)
                    .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest))
                    .width(232)
                    .height(Length::Fill),
                    static_divider(theme),
                    container(detail)
                        .padding(12)
                        .width(Length::Fill)
                        .max_width(920)
                        .height(Length::Fill)
                        .style(move |_| {
                            // The modal is Elevated, so its detail still needs a Panel fill.
                            if already_on_panel {
                                iced::widget::container::Style::default()
                            } else {
                                components::surface(theme, Surface::Panel, Interaction::Rest)
                            }
                        }),
                ]
                .spacing(0)
                .height(Length::Fill),
            ]
            .spacing(SPACING_12)
            .height(Length::Fill)
            .into()
        }
        SettingsCategory::Dictionaries => {
            let dictionaries = settings.dictionaries();
            let scopes = dictionaries
                .scopes()
                .into_iter()
                .fold(row![].spacing(8), |row, scope| {
                    row.push(
                        button(text(scope.label).size(13))
                            .padding([8, 10])
                            .on_press_maybe(scope.available.then_some(
                                ProjectSurfaceMessage::Project(
                                    ProjectMessage::SelectDictionaryScope(scope.scope),
                                ),
                            ))
                            .style(move |_, status| {
                                components::button_style(
                                    theme,
                                    ButtonKind::Quiet,
                                    interaction(status, scope.selected),
                                )
                            }),
                    )
                });
            let query = dictionaries.query().to_lowercase();
            let matches: Vec<_> = dictionaries
                .words()
                .unwrap_or_default()
                .iter()
                .filter(|word| word.to_lowercase().contains(&query))
                .collect();
            let words: Element<'a, ProjectSurfaceMessage> = if matches.is_empty() {
                text(if dictionaries.words().is_none() {
                    "Loading global words…"
                } else if query.is_empty() {
                    "No words added"
                } else {
                    "No matching words"
                })
                .size(13)
                .into()
            } else {
                crate::scroll_gate::smooth(
                    scrollable(
                        matches
                            .into_iter()
                            .fold(column![].spacing(6), |column, word| {
                                column.push(
                                    row![
                                        text(word).width(Length::Fill).size(14),
                                        button(text("Remove").size(12)).on_press(
                                            ProjectSurfaceMessage::Project(
                                                ProjectMessage::RemoveDictionaryWord(word.clone())
                                            )
                                        )
                                    ]
                                    .spacing(8)
                                    .align_y(iced::alignment::Vertical::Center),
                                )
                            }),
                    )
                    .height(Length::Fill),
                )
            };
            container(
                column![
                    page_title("Dictionaries"),
                    text(format!("Language · {}", dictionaries.language())).size(12),
                    scopes,
                    text(
                        if dictionaries.selected_scope() == crate::DictionaryScope::Project {
                            "Saved with this project."
                        } else {
                            "All projects on this device."
                        }
                    )
                    .size(13),
                    row![
                        text_input("Enter a dictionary word", dictionaries.word_draft())
                            .on_input(|word| ProjectSurfaceMessage::Project(
                                ProjectMessage::EditDictionaryWord(word)
                            ))
                            .on_submit(ProjectSurfaceMessage::Project(
                                ProjectMessage::AddDictionaryWord
                            )),
                        button(text("Add word")).on_press_maybe(
                            (dictionaries.words().is_some()
                                && !dictionaries.word_draft().trim().is_empty())
                            .then_some(ProjectSurfaceMessage::Project(
                                ProjectMessage::AddDictionaryWord
                            ))
                        )
                    ]
                    .spacing(8)
                    .align_y(iced::alignment::Vertical::Center),
                    text_input("Search dictionary words", dictionaries.query()).on_input(|query| {
                        ProjectSurfaceMessage::Project(ProjectMessage::SetDictionaryQuery(query))
                    }),
                    words,
                ]
                .spacing(SPACING_12)
                .height(Length::Fill),
            )
            .width(Length::Fill)
            .max_width(640)
            .height(Length::Fill)
            .into()
        }
    }
}

fn metadata_field_creation_detail<'a>(
    label: &'a str,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let name = sensor(
        text_input("Name this field", label)
            .padding([8, 10])
            .size(14)
            .style(move |_, status| components::field_style(theme, field_interaction(status)))
            .id(metadata_field_name_input_id())
            .on_input(|value| {
                ProjectSurfaceMessage::Project(ProjectMessage::SetNewMetadataFieldLabel(value))
            })
            .on_submit(ProjectSurfaceMessage::Project(
                ProjectMessage::CommitNewMetadataField,
            )),
    )
    .key("metadata-field-creation")
    .on_show(|_| ProjectSurfaceMessage::MetadataFieldCreationShown);
    column![
        text("New metadata field").size(18),
        name,
        row![
            button(text("Cancel"))
                .padding([8, 14])
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    interaction(status, false)
                ))
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::CancelNewMetadataField
                )),
            button(text("Add field"))
                .padding([8, 14])
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Primary,
                    interaction(status, false)
                ))
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::CommitNewMetadataField
                )),
        ]
        .spacing(8),
    ]
    .spacing(10)
    .into()
}

fn metadata_field_detail<'a>(
    field: crate::MetadataFieldSummary<'a>,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let id = field.id.to_owned();
    let description = field.description.unwrap_or_default().to_owned();
    let default_value = field.default_value.unwrap_or_default().to_owned();
    let label = field.label.to_owned();
    let label_input = text_input("Name", field.label)
        .id(metadata_field_name_input_id())
        .padding([7, 10])
        .size(14)
        .style(move |_, status| components::field_style(theme, field_interaction(status)))
        .on_input({
            let id = id.clone();
            let description = description.clone();
            let default_value = default_value.clone();
            move |value| {
                ProjectSurfaceMessage::Project(ProjectMessage::UpdateMetadataField {
                    field_id: id.clone(),
                    label: value,
                    description: (!description.is_empty()).then_some(description.clone()),
                    applicability: field.applicability,
                    text_kind: field.text_kind,
                    default_value: (!default_value.is_empty()).then_some(default_value.clone()),
                    visible_on_cards: field.visible_on_cards,
                })
            }
        });
    let description_input = text_input("Description", &description)
        .padding([7, 10])
        .size(14)
        .style(move |_, status| components::field_style(theme, field_interaction(status)))
        .on_input({
            let id = id.clone();
            let label = label.clone();
            let default_value = default_value.clone();
            move |value| {
                ProjectSurfaceMessage::Project(ProjectMessage::UpdateMetadataField {
                    field_id: id.clone(),
                    label: label.clone(),
                    description: (!value.is_empty()).then_some(value),
                    applicability: field.applicability,
                    text_kind: field.text_kind,
                    default_value: (!default_value.is_empty()).then_some(default_value.clone()),
                    visible_on_cards: field.visible_on_cards,
                })
            }
        });
    let default_input = text_input("Default value", &default_value)
        .padding([7, 10])
        .size(14)
        .style(move |_, status| components::field_style(theme, field_interaction(status)))
        .on_input({
            let id = id.clone();
            let label = label.clone();
            let description = description.clone();
            move |value| {
                ProjectSurfaceMessage::Project(ProjectMessage::UpdateMetadataField {
                    field_id: id.clone(),
                    label: label.clone(),
                    description: (!description.is_empty()).then_some(description.clone()),
                    applicability: field.applicability,
                    text_kind: field.text_kind,
                    default_value: (!value.is_empty()).then_some(value),
                    visible_on_cards: field.visible_on_cards,
                })
            }
        });
    let applicability = components::anchored_select(
        MetadataFieldApplicability::ALL.to_vec(),
        Some(field.applicability),
        {
            let label = label.clone();
            let description = description.clone();
            let default_value = default_value.clone();
            let id = id.clone();
            move |choice| {
                ProjectSurfaceMessage::Project(ProjectMessage::UpdateMetadataField {
                    field_id: id.clone(),
                    label: label.clone(),
                    description: (!description.is_empty()).then_some(description.clone()),
                    applicability: choice,
                    text_kind: field.text_kind,
                    default_value: (!default_value.is_empty()).then_some(default_value.clone()),
                    visible_on_cards: field.visible_on_cards,
                })
            }
        },
        theme,
    );
    let kind = components::anchored_select(
        MetadataFieldTextKind::ALL.to_vec(),
        Some(field.text_kind),
        {
            let id = id.clone();
            move |choice| {
                ProjectSurfaceMessage::Project(ProjectMessage::UpdateMetadataField {
                    field_id: id.clone(),
                    label: label.clone(),
                    description: (!description.is_empty()).then_some(description.clone()),
                    applicability: field.applicability,
                    text_kind: choice,
                    default_value: (!default_value.is_empty()).then_some(default_value.clone()),
                    visible_on_cards: field.visible_on_cards,
                })
            }
        },
        theme,
    );
    column![
        components::muted_label("Field name"),
        label_input,
        components::muted_label("Description · optional"),
        description_input,
        components::muted_label("Default value · optional"),
        default_input,
        components::muted_label("Applies to"),
        applicability,
        components::muted_label("Text format"),
        kind,
    ]
    .spacing(8)
    .into()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StyleChoice {
    id: Option<String>,
    label: String,
}

impl std::fmt::Display for StyleChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

fn style_detail<'a>(
    settings: &'a crate::SettingsState,
    style: crate::StyleSummary<'a>,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let style_id = style.id.to_owned();
    let properties = settings.resolved_style_properties(style.id);
    let choices: Vec<_> = std::iter::once(StyleChoice {
        id: None,
        label: "No inheritance".into(),
    })
    .chain(
        settings
            .styles()
            .into_iter()
            .filter(|candidate| candidate.id != style.id)
            .map(|candidate| StyleChoice {
                id: Some(candidate.id.to_owned()),
                label: candidate.display_name.to_owned(),
            }),
    )
    .collect();
    let selected = choices
        .iter()
        .find(|choice| choice.id.as_deref() == style.inherits)
        .cloned();
    let inheritance = harness_target::target(
        HarnessTarget::StyleInheritance,
        pick_list(choices, selected, {
            let id = style_id.clone();
            move |choice: StyleChoice| {
                ProjectSurfaceMessage::Project(ProjectMessage::SetStyleInheritance {
                    style_id: id.clone(),
                    inherits: choice.id,
                })
            }
        })
        .width(Length::Fill)
        .text_size(13)
        .padding([5, 8]),
    );
    let reset: Element<'a, ProjectSurfaceMessage> = if style.role.is_reserved() {
        button(text("Reset to defaults").size(12))
            .padding([5, 8])
            .on_press(ProjectSurfaceMessage::Project(ProjectMessage::ResetStyle(
                style_id.clone(),
            )))
            .style(move |_, status| {
                components::button_style(theme, ButtonKind::Secondary, interaction(status, false))
            })
            .into()
    } else {
        Space::new().into()
    };
    let mut content = column![
        row![
            text("Style name")
                .size(12)
                .color(theme.palette().secondary_text),
            Space::new().width(Length::Fill),
            reset
        ]
        .align_y(iced::alignment::Vertical::Center),
        text_input("Display name", style.display_name)
            .padding([5, 8])
            .size(14)
            .style(move |_, status| components::field_style(theme, field_interaction(status)))
            .on_input({
                let id = style_id.clone();
                move |display_name| {
                    ProjectSurfaceMessage::Project(ProjectMessage::RenameStyle {
                        style_id: id.clone(),
                        display_name,
                    })
                }
            }),
        components::muted_label("Inherits from"),
        inheritance,
    ]
    .spacing(8);
    let mut groups = Vec::new();
    for property in StyleProperty::ALL {
        if !groups.contains(&property.group()) {
            groups.push(property.group());
        }
    }
    for heading in groups {
        content = content
            .push(Space::new().height(8))
            .push(text(heading).size(16));
        let mut fields = Vec::new();
        for &property in StyleProperty::ALL
            .iter()
            .filter(|property| property.group() == heading)
        {
            let saved = style_property_value(&properties, property);
            let draft = settings.style_property_draft(&style_id, property);
            let value = draft.unwrap_or(&saved);
            let id = style_id.clone();
            let options = property.input().choices();
            let control: Element<'a, ProjectSurfaceMessage> = if !options.is_empty() {
                let choices = options
                    .iter()
                    .map(|(value, label)| StyleChoice {
                        id: Some((*value).into()),
                        label: (*label).into(),
                    })
                    .collect::<Vec<_>>();
                let selected = choices
                    .iter()
                    .find(|choice| choice.id.as_deref() == Some(value))
                    .cloned();
                pick_list(choices, selected, move |choice: StyleChoice| {
                    ProjectSurfaceMessage::Project(ProjectMessage::SetStyleProperty {
                        style_id: id.clone(),
                        property,
                        value: choice.id.unwrap_or_default(),
                    })
                })
                .text_size(13)
                .padding([5, 8])
                .width(Length::Fill)
                .into()
            } else {
                let commit = ProjectSurfaceMessage::Project(ProjectMessage::SetStyleProperty {
                    style_id: id.clone(),
                    property,
                    value: value.to_owned(),
                });
                let field = text_input("Default", value)
                    .id(format!("style-property-{property:?}"))
                    .size(14)
                    .padding([5, 8])
                    .style(move |_, status| {
                        components::field_style(theme, field_interaction(status))
                    })
                    .on_input(move |value| {
                        ProjectSurfaceMessage::Project(ProjectMessage::EditStyleProperty {
                            style_id: id.clone(),
                            property,
                            value,
                        })
                    })
                    .on_submit(commit.clone());
                hierarchy_drag::commit_on_click_away_maybe(field, draft.is_some().then_some(commit))
            };
            let mut label = row![
                text(property.label())
                    .size(12)
                    .color(theme.palette().secondary_text)
            ]
            .align_y(iced::alignment::Vertical::Center);
            if !style.role.is_reserved() && !property.value(style.properties).is_empty() {
                label = label.push(Space::new().width(Length::Fill)).push(
                    button(text("Reset").size(11))
                        .padding([0, 4])
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::SetStyleProperty {
                                style_id: style_id.clone(),
                                property,
                                value: String::new(),
                            },
                        ))
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, false),
                            )
                        }),
                );
            }
            fields.push(container(column![label, control].spacing(5)).width(Length::Fill));
        }
        let mut fields = fields.into_iter();
        while let Some(left) = fields.next() {
            let mut pair = row![left].spacing(16);
            if let Some(right) = fields.next() {
                pair = pair.push(right);
            } else {
                pair = pair.push(Space::new().width(Length::Fill));
            }
            content = content.push(pair);
        }
    }
    let family = match properties.font_family.as_deref() {
        Some("Source Sans 3") => "Source Sans 3",
        Some("Monospace" | "monospace") => "monospace",
        _ => "Source Serif 4",
    };
    let preview = container(
        column![
            components::muted_label("Preview"),
            iced::widget::rich_text::<(), _, _, _>([iced::widget::span(
                "The light reached the last page."
            )
            .underline(
                properties
                    .text_decoration
                    .is_some_and(|value| value.underline())
            )
            .strikethrough(
                properties
                    .text_decoration
                    .is_some_and(|value| value.strikethrough())
            )
            .font(Font {
                weight: if properties.weight.unwrap_or(400) >= 600 {
                    font::Weight::Bold
                } else {
                    font::Weight::Normal
                },
                style: if properties.italic.unwrap_or(false) {
                    font::Style::Italic
                } else {
                    font::Style::Normal
                },
                ..Font::with_name(family)
            })
            .size(
                properties
                    .font_size_points
                    .unwrap_or(18.0)
                    .clamp(10.0, 36.0)
            )]),
        ]
        .spacing(10),
    )
    .padding(10)
    .width(Length::Fill)
    .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest));
    column![
        preview,
        crate::scroll_gate::smooth(
            scrollable(container(content).padding(iced::Padding {
                right: 16.0,
                ..iced::Padding::ZERO
            }))
            .height(Length::Fill)
        )
    ]
    .spacing(8)
    .height(Length::Fill)
    .into()
}

fn style_property_value(
    properties: &parchmint_domain::StyleProperties,
    property: StyleProperty,
) -> String {
    property.value(properties)
}

fn recovery_modal<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let recovery = workspace.recovery();
    let mut summary = column![text("Documents with unsaved changes").size(14)].spacing(8);
    for document in workspace.recovery_summary() {
        summary =
            summary.push(text(document.display_title.unwrap_or("Untitled document")).size(15));
    }
    summary = summary.push(
        button(
            text(if recovery.details_expanded() {
                "Hide technical details"
            } else {
                "Show technical details"
            })
            .size(12),
        )
        .on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::ToggleRecoveryDetails,
        )),
    );
    if recovery.details_expanded() {
        summary = summary
            .push(text(format!("{} recovery records", recovery.accepted_records())).size(12));
        for document in workspace.recovery_summary() {
            summary = summary.push(
                text(format!(
                    "{} · revision {}",
                    document.display_title.unwrap_or("Untitled document"),
                    document.revision
                ))
                .size(12),
            );
        }
        if let Some(isolation) = recovery.isolation() {
            summary = summary.push(text(format!("Isolated records: {isolation}")).size(12));
        }
    }
    if let Some(error) = recovery.error() {
        summary = summary.push(text(format!("Recovery could not complete: {error}")).size(13));
    }
    let mut recover = button(text(if recovery.is_resolving() {
        "Resolving…"
    } else {
        "Recover changes"
    }));
    let mut discard = button(text("Open last saved version"));
    if !recovery.is_resolving() && recovery.error().is_none() {
        recover = recover.on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::AcceptRecovery,
        ));
        discard = discard.on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::DiscardRecovery,
        ));
    }
    let mut actions = row![
        discard.style(move |_, status| components::button_style(
            theme,
            ButtonKind::Secondary,
            interaction(status, recovery.is_resolving())
        )),
        recover.style(move |_, status| components::button_style(
            theme,
            ButtonKind::Primary,
            interaction(status, recovery.is_resolving())
        )),
    ]
    .spacing(10);
    if recovery.error().is_some() && !recovery.is_resolving() {
        actions = actions.push(
            button(text("Retry")).on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::RetryRecovery,
            )),
        );
    }
    container(
        column![
            page_title("Unsaved changes found"),
            text("Recover your newer edits, or open the last saved version and discard the unsaved changes.").size(16),

            container(crate::scroll_gate::smooth(
                scrollable(summary).width(Length::Fill).spacing(12),
            ))
                .max_height(260)
                .padding(16)
                .width(Length::Fill)
                .style(move |_| iced::widget::container::Style {
                    background: Some(Background::Color(theme.palette().sidebar)),
                    ..Default::default()
                }),
            Space::new().height(4),
            row![Space::new().width(Length::Fill), actions]
                .spacing(12)
                .align_y(iced::alignment::Vertical::Center),
        ]
        .spacing(18),
    )
    .padding(24)
    .width(620)
    .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Rest))
    .into()
}

fn state_center<'a>(
    title: &'a str,
    detail: &'a str,
    _theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    column![text(title).size(22), text(detail).size(14)]
        .spacing(14)
        .into()
}

#[allow(clippy::too_many_arguments)]
fn inline_outline_field<'a>(
    workspace: &ProjectWorkspace,
    node: &str,
    field: Option<&str>,
    value: &'a str,
    placeholder: &'a str,
    size: u16,
    height: f32,
    editor: Option<Element<'a, ProjectSurfaceMessage>>,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let node_id = node.to_owned();
    let field_id = field.map(str::to_owned);
    if workspace.outline_field_is_editing(node, field)
        && let Some(editor) = editor
    {
        hierarchy_drag::commit_on_click_away(
            editor,
            ProjectSurfaceMessage::Project(ProjectMessage::EndOutlineField { node_id, field_id }),
        )
    } else {
        button(
            text(if value.is_empty() { placeholder } else { value })
                .size(u32::from(size))
                .font(crate::cards_layout::CARD_FONT)
                .line_height(iced::Pixels(if size == 14 { 20.0 } else { 18.0 }))
                .wrapping(text::Wrapping::WordOrGlyph)
                .width(Length::Fill)
                .color(if value.is_empty() || field.is_some() {
                    theme.palette().secondary_text
                } else {
                    theme.palette().primary_text
                }),
        )
        .width(Length::Fill)
        .height(height)
        .padding([2, 3])
        .on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::BeginOutlineField { node_id, field_id },
        ))
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
        })
        .into()
    }
}

fn outline_fields<'a>(
    workspace: &'a ProjectWorkspace,
    selected: &'a str,
    theme: ParchMintTheme,
    width: f32,
    synopsis_height: f32,
    full: bool,
) -> Element<'a, ProjectSurfaceMessage> {
    let selected_id = selected.to_owned();
    let inspector = workspace.inspector();
    let metadata_items = inspector
        .metadata_items(selected)
        .into_iter()
        .filter(|item| full || item.visible_on_cards)
        .collect::<Vec<_>>();
    let has_metadata = !metadata_items.is_empty();
    let expanded_group = full
        && workspace
            .explorer()
            .row(selected)
            .is_some_and(|row| row.kind == HierarchyRowKind::Group);
    let metadata_columns =
        crate::cards_layout::metadata_columns(width, expanded_group, metadata_items.len());
    let field_width = crate::cards_layout::metadata_width(width, expanded_group, metadata_columns);
    let metadata = metadata_items
        .into_iter()
        .map(|item| {
            let node_id = selected_id.clone();
            let field_id = item.field_id.to_owned();
            let value = item.effective_value.unwrap_or_default();
            let height = if full {
                crate::cards_layout::metadata_height(value, field_width)
            } else {
                22.0
            };
            let multiline = item.text_kind == MetadataFieldTextKind::Multiline;
            let done = ProjectSurfaceMessage::Project(ProjectMessage::EndOutlineField {
                node_id: selected.to_owned(),
                field_id: Some(item.field_id.to_owned()),
            });
            let editor = workspace
                .outline_field_is_editing(selected, Some(item.field_id))
                .then(|| workspace.metadata_editor(selected, item.field_id))
                .flatten()
                .map(|content| {
                    text_editor(content)
                        .id("outline-metadata")
                        .placeholder("—")
                        .on_action(move |action| {
                            ProjectSurfaceMessage::Project(ProjectMessage::EditMetadata {
                                node_id: node_id.clone(),
                                field_id: field_id.clone(),
                                action,
                            })
                        })
                        .key_binding(move |press| {
                            if press.key
                                == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                                || (!multiline
                                    && press.key
                                        == iced::keyboard::Key::Named(
                                            iced::keyboard::key::Named::Enter,
                                        ))
                            {
                                Some(text_editor::Binding::Custom(done.clone()))
                            } else {
                                text_editor::Binding::from_key_press(press)
                            }
                        })
                        .size(13)
                        .padding([2, 3])
                        .height(height)
                        .style(move |_, status| multiline_field_style(theme, status))
                        .into()
                });
            let value = inline_outline_field(
                workspace,
                selected,
                Some(item.field_id),
                value,
                "—",
                13,
                height,
                editor,
                theme,
            );
            let value = harness_target::target_id(
                format!("metadata-{selected}-{}", item.field_id).into(),
                value,
            );
            column![
                container(
                    text(item.label)
                        .size(12)
                        .color(theme.palette().secondary_text)
                )
                .height(Length::Shrink),
                value,
            ]
            .spacing(2)
            .into()
        })
        .collect::<Vec<Element<'a, ProjectSurfaceMessage>>>();
    let mut metadata_rows = column![].spacing(6);
    let mut fields = metadata.into_iter().peekable();
    while fields.peek().is_some() {
        let mut line = row![].spacing(8);
        for field in fields.by_ref().take(metadata_columns) {
            line = line.push(container(field).width(field_width));
        }
        metadata_rows = metadata_rows.push(line);
    }
    let metadata = metadata_rows;
    let synopsis_id = selected_id.clone();
    let done = ProjectSurfaceMessage::Project(ProjectMessage::EndOutlineField {
        node_id: selected.to_owned(),
        field_id: None,
    });
    let synopsis_editor = workspace.outline_field_is_editing(selected, None).then(|| {
        text_editor(
            workspace
                .synopsis_editor(selected)
                .expect("every live hierarchy node has a synopsis editor"),
        )
        .id(HarnessTarget::InspectorSynopsis.id())
        .placeholder("What happens here?")
        .key_binding(move |press| {
            if press.key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                return Some(text_editor::Binding::Custom(done.clone()));
            }
            text_editor::Binding::from_key_press(press)
        })
        .on_action(move |action| {
            ProjectSurfaceMessage::Project(ProjectMessage::EditSynopsis {
                node_id: synopsis_id.clone(),
                action,
            })
        })
        .padding([2, 3])
        .size(14)
        .height(Length::Fixed(synopsis_height))
        .style(move |_, status| multiline_field_style(theme, status))
        .into()
    });
    let synopsis = inline_outline_field(
        workspace,
        selected,
        None,
        workspace
            .explorer()
            .row(selected)
            .map_or("", |item| item.synopsis),
        "What happens here?",
        14,
        synopsis_height,
        synopsis_editor,
        theme,
    );
    let synopsis = harness_target::target_id(format!("synopsis-{selected}").into(), synopsis);
    if has_metadata {
        if expanded_group {
            let metadata_width =
                field_width * metadata_columns as f32 + (metadata_columns - 1) as f32 * 8.0;
            row![
                container(synopsis)
                    .padding(iced::Padding {
                        top: 18.0,
                        ..Default::default()
                    })
                    .width(Length::FillPortion(1)),
                container(metadata).width(metadata_width)
            ]
            .spacing(12)
            .into()
        } else {
            column![synopsis, metadata].spacing(8).into()
        }
    } else {
        synopsis
    }
}

fn inspector<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
    width: u32,
    _expanded: [bool; 3],
    compact: bool,
) -> Element<'a, ProjectSurfaceMessage> {
    let selected = workspace.inspector_node_id();
    let content = if let Some(selected) = selected {
        let title = workspace.explorer().title(selected).unwrap_or("Untitled");
        let heading: Element<'a, ProjectSurfaceMessage> = if compact {
            text(format!("Notes · {title}")).size(12).into()
        } else {
            harness_target::target(
                HarnessTarget::InspectorTitle,
                column![
                    text("Notes").size(14).font(Font {
                        weight: font::Weight::Semibold,
                        ..Font::DEFAULT
                    }),
                    components::muted_label(title),
                ]
                .spacing(2),
            )
        };
        let editor = workspace.editor();
        let mut notes = column![].spacing(SPACING_8);
        let threads = editor.inspector_comments();
        let document_choices = matches!(
            editor.inspector_context(),
            crate::InspectorContext::Group { .. }
        )
        .then(|| workspace.history_document_choices());
        let mut count = 0;
        for thread in threads {
            let thread_id = thread.id().to_owned();
            let quote = comment_anchor_summary(thread.anchor());
            let document_title = document_choices.as_ref().map(|choices| {
                choices
                    .iter()
                    .find(|(id, _)| *id == thread.document_id())
                    .map_or("Document", |(_, title)| *title)
                    .to_owned()
            });
            for message in thread.messages() {
                count += 1;
                let message_id = message.id().to_owned();
                let selected_note = editor.selected_comment() == Some(thread_id.as_str());
                let editing = editor.editing_comment_message()
                    == Some((thread_id.as_str(), message_id.as_str()))
                    && !editor.editing_note_in_hover();
                let note_body = crate::iced_editor_surface::note_message_body(
                    &thread_id,
                    message,
                    editor,
                    theme,
                    !editor.editing_note_in_hover(),
                )
                .map(ProjectSurfaceMessage::EditorCenter);
                let note_body: Element<'a, ProjectSurfaceMessage> = if editing {
                    note_body
                } else {
                    container(note_body).max_height(72).clip(true).into()
                };
                let edit = stationary_tooltip::tooltip(
                    harness_target::target_id(
                        iced::widget::Id::from(format!("note-edit-{message_id}")),
                        button(icon_sized(Icon::Rename, 15))
                            .padding(4)
                            .on_press(ProjectSurfaceMessage::EditorCenter(
                                EditorCenterMessage::Workspace(
                                    EditorMessage::BeginEditCommentMessage {
                                        thread_id: thread_id.clone(),
                                        message_id: message_id.clone(),
                                        body: message.body().to_owned(),
                                    },
                                ),
                            ))
                            .style(move |_, status| {
                                components::button_style(
                                    theme,
                                    ButtonKind::Quiet,
                                    interaction(status, false),
                                )
                            }),
                    ),
                    text("Edit note").size(12),
                    components::surface(theme, Surface::Elevated, Interaction::Rest),
                );
                let delete = stationary_tooltip::tooltip(
                    harness_target::target_id(
                        iced::widget::Id::from(format!("note-delete-{message_id}")),
                        button(icon_sized(Icon::RecentlyDeleted, 15))
                            .padding(4)
                            .on_press(ProjectSurfaceMessage::EditorCenter(
                                EditorCenterMessage::Workspace(EditorMessage::RequestDeleteNote {
                                    thread_id: thread_id.clone(),
                                    message_id: message_id.clone(),
                                }),
                            ))
                            .style(move |_, status| {
                                components::button_style(
                                    theme,
                                    ButtonKind::Quiet,
                                    interaction(status, false),
                                )
                            }),
                    ),
                    text("Delete note").size(12),
                    components::surface(theme, Surface::Elevated, Interaction::Rest),
                );
                let mut card = column![
                    row![
                        text(document_title.clone().unwrap_or_else(|| "Note".to_owned()))
                            .size(12)
                            .color(theme.palette().secondary_text)
                            .width(Length::Fill),
                        edit,
                        delete,
                    ]
                    .spacing(2)
                    .align_y(iced::alignment::Vertical::Center),
                    container(
                        text(quote.clone())
                            .size(12)
                            .color(theme.palette().secondary_text)
                    )
                    .max_height(26)
                    .clip(true),
                    note_body,
                ]
                .spacing(6);
                if editor.pending_delete_note() == Some((thread_id.as_str(), message_id.as_str())) {
                    card = card.push(
                        column![
                            text("Delete this note?").size(12),
                            row![
                                button("Delete")
                                    .on_press(ProjectSurfaceMessage::EditorCenter(
                                        EditorCenterMessage::Workspace(
                                            EditorMessage::ConfirmDeleteNote
                                        )
                                    ))
                                    .style(move |_, status| components::button_style(
                                        theme,
                                        ButtonKind::Destructive,
                                        interaction(status, false)
                                    )),
                                button("Cancel")
                                    .on_press(ProjectSurfaceMessage::EditorCenter(
                                        EditorCenterMessage::Workspace(
                                            EditorMessage::CancelDeleteNote
                                        )
                                    ))
                                    .style(move |_, status| components::button_style(
                                        theme,
                                        ButtonKind::Quiet,
                                        interaction(status, false)
                                    )),
                            ]
                            .spacing(6),
                        ]
                        .spacing(4),
                    );
                }
                notes =
                    notes.push(
                        mouse_area(container(card).padding(10).width(Length::Fill).style(
                            move |_| {
                                components::surface(
                                    theme,
                                    Surface::Panel,
                                    if selected_note {
                                        Interaction::Selected
                                    } else {
                                        Interaction::Rest
                                    },
                                )
                            },
                        ))
                        .on_press(ProjectSurfaceMessage::EditorCenter(
                            EditorCenterMessage::Workspace(EditorMessage::SelectComment(
                                thread_id.clone(),
                            )),
                        )),
                    );
            }
        }
        let sections: Element<'a, ProjectSurfaceMessage> = if count == 0 {
            text("No notes yet")
                .size(13)
                .color(theme.palette().secondary_text)
                .into()
        } else {
            notes.into()
        };
        column![
            heading,
            crate::scroll_gate::smooth(scrollable(sections).height(Length::Fill)),
        ]
        .spacing(12)
        .height(Length::Fill)
    } else {
        column![
            components::muted_label("Notes"),
            components::muted_label("Open a document to view its notes."),
        ]
        .spacing(10)
    };
    focus::f6_region(
        F6Region::Inspector,
        container(content)
            .padding(12)
            .width(width)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
    )
}

fn status_bar<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
    explorer_visible: bool,
    inspector_visible: bool,
    show_companion: bool,
) -> Element<'a, ProjectSurfaceMessage> {
    let link_label = workspace
        .editor()
        .hovered_link
        .as_deref()
        .filter(|_| show_companion)
        .map(|target| {
            if let Some(id) = target.strip_prefix("parchmint://document/") {
                workspace
                    .explorer()
                    .breadcrumb_for_document(id)
                    .map(|path| path.join(" > "))
                    .unwrap_or_else(|| "Unavailable document".into())
            } else {
                target.to_owned()
            }
        });
    let label = match workspace.save().state() {
        SaveState::SavedThrough(_) => "Saved".to_owned(),
        SaveState::Dirty { .. } => "Unsaved changes".to_owned(),
        SaveState::Saving { .. } => "Saving changes".to_owned(),
        SaveState::Error(_) => "Couldn't save changes".to_owned(),
    };
    let editor_status = workspace.editor().status_bar();
    let active_count = if show_companion {
        match editor_status.current_count() {
            StatusCount::Selection(words) => format!("Selection · {}", word_count_label(words)),
            StatusCount::ActiveDocument(words) => format!("Document · {}", word_count_label(words)),
        }
    } else {
        format!(
            "Selected · {}",
            word_count_label(workspace.selected_outline_words())
        )
    };
    let explorer_control: Element<'a, ProjectSurfaceMessage> = if show_companion {
        status_pane_button(
            theme,
            Icon::ExplorerPane,
            HarnessTarget::ToggleExplorer,
            explorer_visible,
            if explorer_visible {
                "Hide Explorer"
            } else {
                "Show Explorer"
            },
            Some(ProjectSurfaceMessage::ToggleExplorer),
        )
    } else {
        Space::new().width(0).into()
    };
    let comments_control: Element<'a, ProjectSurfaceMessage> = if show_companion {
        status_pane_button(
            theme,
            Icon::Comment,
            HarnessTarget::ToggleInspector,
            inspector_visible,
            if inspector_visible {
                "Hide notes"
            } else {
                "Show notes"
            },
            Some(ProjectSurfaceMessage::ToggleInspector),
        )
    } else {
        Space::new().width(0).into()
    };
    let mut content = row![]
        .align_y(iced::alignment::Vertical::Center)
        .spacing(14);
    if show_companion {
        content = content.push(explorer_control);
    }
    let content = content
        .push(text(active_count).size(12))
        .push(
            text(format!(
                "Manuscript · {}",
                word_count_label(editor_status.manuscript_total())
            ))
            .size(12),
        )
        .push(
            container(
                iced::widget::rich_text::<(), _, _, _>([
                    iced::widget::span(link_label.clone().unwrap_or_default())
                        .color(theme.palette().accent)
                        .underline(true),
                    iced::widget::span(if link_label.is_some() {
                        "   Ctrl+click to follow"
                    } else {
                        ""
                    })
                    .color(theme.palette().secondary_text),
                ])
                .size(12),
            )
            .width(Length::Fill)
            .clip(true),
        )
        .push(text(label).size(12));
    let content = if show_companion {
        content.push(comments_control)
    } else {
        content
    };
    focus::f6_region(
        F6Region::StatusBar,
        column![
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(move |_| iced::widget::container::Style {
                    background: Some(Background::Color(theme.palette().divider)),
                    ..Default::default()
                }),
            container(content)
                .padding([2, if show_companion { 12 } else { 16 }])
                .width(Length::Fill)
                .height(Length::Fixed(f32::from(STATUS_HEIGHT) - 1.0))
                .align_y(iced::alignment::Vertical::Center)
                .style(move |_| components::surface(theme, Surface::Status, Interaction::Rest)),
        ],
    )
}

fn status_pane_button<'a>(
    theme: ParchMintTheme,
    icon: Icon,
    target: HarnessTarget,
    selected: bool,
    label: &'static str,
    message: Option<ProjectSurfaceMessage>,
) -> Element<'a, ProjectSurfaceMessage> {
    let command = match target {
        HarnessTarget::ToggleExplorer => "view.explorer",
        HarnessTarget::ToggleInspector => "view.comments",
        _ => "",
    };
    stationary_tooltip::tooltip(
        harness_target::target(
            target,
            button(container(icon_sized(icon, 18)).center(Length::Fill))
                .width(32)
                .height(22)
                .padding(0)
                .on_press_maybe(message)
                .style(move |_, status| {
                    components::button_style(
                        theme,
                        ButtonKind::Quiet,
                        interaction(status, selected),
                    )
                }),
        ),
        container(text(components::tooltip_label(label, command)).size(12)).padding([4, 6]),
        components::surface(theme, Surface::Elevated, Interaction::Rest),
    )
}

fn modal_view<'a>(
    modal: ProjectModal,
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    if let ProjectModal::CommentThread { thread_id } = &modal {
        let content = workspace.editor().comment_thread(thread_id).map_or_else(
            || text("Note deleted").into(),
            |thread| {
                crate::iced_editor_surface::comment_thread_card(
                    "Note",
                    comment_anchor_summary(thread.anchor()),
                    thread,
                    workspace.editor(),
                    theme,
                )
                .map(ProjectSurfaceMessage::EditorCenter)
            },
        );
        return container(
            column![
                scrollable(content).height(Length::Shrink),
                button("Done")
                    .on_press(ProjectSurfaceMessage::Project(ProjectMessage::DismissModal)),
            ]
            .spacing(16),
        )
        .padding(20)
        .width(440)
        .max_height(500)
        .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Rest))
        .into();
    }
    if let ProjectModal::HistoryRestoring { scope } = &modal {
        return container(
            text(match scope {
                crate::HistoryRestoreScope::EntireProject => "Restoring project…",
                crate::HistoryRestoreScope::Document { .. } => "Restoring document…",
            })
            .size(18),
        )
        .padding(24)
        .width(360)
        .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Rest))
        .into();
    }
    if let ProjectModal::ManageSettings(category) = modal {
        let action = |label: &'static str, message, kind| {
            button(text(label).size(13))
                .padding([6, 16])
                .style(move |_, status| {
                    components::button_style(theme, kind, interaction(status, false))
                })
                .on_press(ProjectSurfaceMessage::Project(message))
        };
        if workspace.settings_cancel_pending {
            return container(
                column![
                    text("Discard changes?").size(18),
                    text("Your changes in this dialog will be discarded.")
                        .size(13)
                        .color(theme.palette().secondary_text),
                    row![
                        Space::new().width(Length::Fill),
                        focus::region(
                            focus::modal_cancel_id(),
                            action(
                                "Keep editing",
                                ProjectMessage::KeepEditingSettings,
                                ButtonKind::Secondary
                            )
                        ),
                        focus::region(
                            focus::modal_confirm_id(),
                            action(
                                "Discard",
                                ProjectMessage::ConfirmDiscardSettings,
                                ButtonKind::Primary
                            )
                        ),
                    ]
                    .spacing(8),
                ]
                .spacing(12),
            )
            .padding(16)
            .width(400)
            .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Rest))
            .into();
        }
        return container(
            column![
                settings_content(workspace, category, theme, false),
                row![
                    Space::new().width(Length::Fill),
                    focus::region(
                        focus::modal_cancel_id(),
                        action(
                            "Cancel",
                            ProjectMessage::DismissModal,
                            ButtonKind::Secondary
                        )
                    ),
                    focus::region(
                        focus::modal_confirm_id(),
                        action(
                            "Save",
                            ProjectMessage::SaveSettingsManager,
                            ButtonKind::Primary
                        )
                    ),
                ]
                .spacing(8)
                .align_y(iced::alignment::Vertical::Center),
            ]
            .spacing(8),
        )
        .padding(10)
        .width(1160)
        .height(660)
        .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Rest))
        .into();
    }
    if let ProjectModal::SaveBeforeClosing { title, .. } = &modal {
        return container(
            column![
                text("Save before closing?").size(18),
                text(format!(
                    "Choose a name and location for “{title}”, or discard this draft."
                ))
                .size(13),
                row![
                    focus::region(
                        iced::widget::Id::new("draft-discard"),
                        button(text("Don’t Save"))
                            .on_press(ProjectSurfaceMessage::Project(
                                ProjectMessage::DiscardClosingDraft
                            ))
                            .style(move |_, status| components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, false)
                            ))
                    ),
                    Space::new().width(Length::Fill),
                    focus::region(
                        focus::modal_cancel_id(),
                        button(text("Cancel"))
                            .on_press(ProjectSurfaceMessage::Project(ProjectMessage::DismissModal))
                            .style(move |_, status| components::button_style(
                                theme,
                                ButtonKind::Secondary,
                                interaction(status, false)
                            ))
                    ),
                    focus::region(
                        focus::modal_confirm_id(),
                        button(text("Save…"))
                            .on_press(ProjectSurfaceMessage::Project(
                                ProjectMessage::SaveClosingDraft
                            ))
                            .style(move |_, status| components::button_style(
                                theme,
                                ButtonKind::Primary,
                                interaction(status, false)
                            ))
                    ),
                ]
                .spacing(8),
            ]
            .spacing(16),
        )
        .padding(20)
        .width(480)
        .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Rest))
        .into();
    }
    if let ProjectModal::FileDraft {
        title, parent_id, ..
    } = &modal
    {
        let locations = workspace.draft_tree().into_iter().enumerate().fold(
            column![].spacing(2),
            |rows, (index, (id, label, depth, has_children, expanded))| {
                let selected = id == *parent_id;
                rows.push(focus::region(
                    iced::widget::Id::from(format!("draft-location-{index}")),
                    components::location_row(
                        label,
                        depth,
                        has_children.then_some(expanded),
                        selected,
                        ProjectSurfaceMessage::Project(ProjectMessage::ToggleDraftFolder(
                            id.clone(),
                        )),
                        ProjectSurfaceMessage::Project(ProjectMessage::SetDraftParent(id)),
                        theme,
                    ),
                ))
            },
        );
        return container(
            column![
                text("Save document").size(18),
                text_input("Document name", title)
                    .id(HarnessTarget::DraftTitle.id())
                    .on_input(|title| ProjectSurfaceMessage::Project(
                        ProjectMessage::SetDraftTitle(title)
                    ))
                    .on_submit(ProjectSurfaceMessage::Project(
                        ProjectMessage::ConfirmFileDraft
                    ))
                    .padding([9, 10])
                    .style(move |_, status| components::field_style(
                        theme,
                        field_interaction(status)
                    )),
                components::muted_label("Location"),
                container(scrollable(locations).height(Length::Shrink)).max_height(220),
                row![
                    Space::new().width(Length::Fill),
                    focus::region(
                        focus::modal_cancel_id(),
                        button(text("Cancel"))
                            .on_press(ProjectSurfaceMessage::Project(ProjectMessage::DismissModal))
                            .style(move |_, status| components::button_style(
                                theme,
                                ButtonKind::Secondary,
                                interaction(status, false)
                            ))
                    ),
                    focus::region(
                        focus::modal_confirm_id(),
                        button(text("Save"))
                            .on_press_maybe((!title.trim().is_empty()).then_some(
                                ProjectSurfaceMessage::Project(ProjectMessage::ConfirmFileDraft)
                            ))
                            .style(move |_, status| components::button_style(
                                theme,
                                ButtonKind::Primary,
                                interaction(status, false)
                            ))
                    ),
                ]
                .spacing(8)
            ]
            .spacing(12),
        )
        .padding(20)
        .width(480)
        .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Rest))
        .into();
    }
    if let ProjectModal::Error { title, detail } = &modal {
        return container(
            column![
                text(title.clone()).size(18),
                text(detail.clone()).size(13),
                row![
                    Space::new().width(Length::Fill),
                    focus::region(
                        focus::modal_cancel_id(),
                        button(text("Dismiss"))
                            .on_press(ProjectSurfaceMessage::Project(ProjectMessage::DismissModal))
                            .style(move |_, status| components::button_style(
                                theme,
                                ButtonKind::Secondary,
                                interaction(status, false)
                            )),
                    ),
                ]
                .spacing(10)
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fixed(520.0))
        .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Error))
        .into();
    }
    let (title, detail) = match &modal {
        ProjectModal::HistoryRestore {
            checkpoint_label,
            affected_summary,
            scope: crate::HistoryRestoreScope::Document { .. },
            ..
        } => (
            "Restore document?",
            format!(
                "Restore “{affected_summary}” to “{checkpoint_label}”? Only its text, formatting, and notes change."
            ),
        ),
        ProjectModal::HistoryRestore {
            checkpoint_label,
            ..
        } => (
            "Restore project?",
            format!(
                "Replace the entire project with “{checkpoint_label}”?"
            ),
        ),
        ProjectModal::DeleteMetadataField { field_id } => (
            "Delete metadata field",
            format!("Remove “{}” and its values from every applicable hierarchy item.", workspace.settings().metadata_field(field_id).map(|field| field.label).unwrap_or("this field")),
        ),
        ProjectModal::DeleteStyle { style_id } => (
            "Delete custom style",
            format!("Remove “{}”. Text using it will fall back to an available style.", workspace.settings().style(style_id).map(|style| style.display_name).unwrap_or("this style")),
        ),
        ProjectModal::ReinitializeHistory => (
            "Reinitialize History",
            "Preserve the damaged History store when possible, then create a new empty History. Project documents are not changed.".to_owned(),
        ),
        ProjectModal::CommentThread { .. } | ProjectModal::ManageSettings(_) | ProjectModal::HistoryRestoring { .. } | ProjectModal::SaveBeforeClosing { .. } | ProjectModal::FileDraft { .. } | ProjectModal::Error { .. } => unreachable!("special modals return above"),
    };
    container(
        column![
            text(title).size(18),
            text(detail).size(13),
            row![
                Space::new().width(Length::Fill),
                focus::region(
                    focus::modal_cancel_id(),
                    button(text("Cancel"))
                        .on_press(ProjectSurfaceMessage::Project(ProjectMessage::DismissModal))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Secondary,
                            interaction(status, false)
                        )),
                ),
                focus::region(
                    focus::modal_confirm_id(),
                    button(text(match &modal {
                        ProjectModal::HistoryRestore {
                            scope: crate::HistoryRestoreScope::Document { .. },
                            ..
                        } => "Restore document",
                        ProjectModal::HistoryRestore { .. } => "Restore project",
                        ProjectModal::DeleteMetadataField { .. } => "Delete field",
                        ProjectModal::DeleteStyle { .. } => "Delete style",
                        ProjectModal::ReinitializeHistory => "Reinitialize History",
                        ProjectModal::SaveBeforeClosing { .. }
                        | ProjectModal::HistoryRestoring { .. }
                        | ProjectModal::CommentThread { .. }
                        | ProjectModal::ManageSettings(_)
                        | ProjectModal::FileDraft { .. }
                        | ProjectModal::Error { .. } => "Dismiss",
                    }))
                    .on_press(ProjectSurfaceMessage::Project(match modal {
                        ProjectModal::HistoryRestore { .. } =>
                            ProjectMessage::ConfirmHistoryRestore,
                        ProjectModal::DeleteMetadataField { .. } =>
                            ProjectMessage::ConfirmDeleteMetadataField,
                        ProjectModal::DeleteStyle { .. } => ProjectMessage::ConfirmDeleteStyle,
                        ProjectModal::ReinitializeHistory =>
                            ProjectMessage::ConfirmHistoryReinitialize,
                        ProjectModal::SaveBeforeClosing { .. }
                        | ProjectModal::HistoryRestoring { .. }
                        | ProjectModal::CommentThread { .. }
                        | ProjectModal::ManageSettings(_)
                        | ProjectModal::FileDraft { .. }
                        | ProjectModal::Error { .. } => ProjectMessage::DismissModal,
                    }))
                    .style(move |_, status| components::button_style(
                        theme,
                        ButtonKind::Destructive,
                        interaction(status, false)
                    )),
                )
            ]
            .spacing(10)
        ]
        .spacing(8),
    )
    .padding(16)
    .width(Length::Fixed(520.0))
    .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Rest))
    .into()
}

/// Tint selected navigation; the caller draws its underline.
fn flat_selection_button_style(
    theme: ParchMintTheme,
    status: iced::widget::button::Status,
    selected: bool,
) -> iced::widget::button::Style {
    let state = interaction(status, selected);
    let mut style = components::button_style(theme, ButtonKind::Quiet, state);
    if state == Interaction::Selected {
        style.text_color = theme.palette().accent;
    }
    style
}

#[cfg(test)]
mod tests {
    use crate::ProjectFixture;
    use iced::{Settings, Size, Task, Theme, executor};
    use iced_test::{Emulator, Instruction, Simulator};
    use iced_test::{
        emulator::Mode,
        instruction::{Interaction, Mouse, Target},
        program::Program,
    };
    use parchmint_application::{DocumentSnapshot, DocumentVisibility, EditorRevision};
    use parchmint_domain::{
        DocumentId, MetadataApplicability, MetadataFieldDefinition, MetadataFieldId,
        MetadataTextKind, NodeId, Project, ProjectCommand, ProjectId, apply_project_command,
    };
    use parchmint_editor_api::{BlockId, CanonicalComment, CommentId, EditorSelection};
    use parchmint_preferences::ResolvedAppearance;
    use parchmint_ui_api::ProjectSnapshot;

    use super::*;
    use crate::EditorPane;

    #[test]
    fn cards_scroll_rebuilds_before_the_viewport_leaves_mounted_rows() {
        let coverage = CardsWindowCoverage {
            mounted_start: 400.0,
            mounted_end: 1_700.0,
            has_rows_before: true,
            has_rows_after: true,
            projected_scroll: 800.0,
        };
        let refresh = |offset| needs_cards_window_refresh(offset, 600.0, 3_000.0, coverage);
        assert!(!refresh(850.0));
        assert!(refresh(495.0));
        assert!(refresh(1_010.0));
        assert!(needs_cards_window_refresh(
            0.0,
            600.0,
            3_000.0,
            CardsWindowCoverage {
                mounted_start: 0.0,
                mounted_end: 1_100.0,
                has_rows_before: false,
                has_rows_after: true,
                ..coverage
            }
        ));
        assert!(needs_cards_window_refresh(
            2_400.0,
            600.0,
            3_000.0,
            CardsWindowCoverage {
                mounted_start: 2_000.0,
                mounted_end: 3_000.0,
                has_rows_before: true,
                has_rows_after: false,
                projected_scroll: 2_200.0,
            }
        ));
    }

    #[test]
    fn previews_preserve_inline_fonts_when_combined_with_other_marks() {
        use parchmint_editor_api::{DocumentPosition, InlineFontFamily, SemanticMarkRange};

        for (mark, expected_size) in [
            (SemanticInlineMark::Bold, 32.0),
            (SemanticInlineMark::SmallCaps, 31.0),
            (SemanticInlineMark::Superscript, 29.0),
        ] {
            let range = EditorSelection::new(DocumentPosition::from(0), DocumentPosition::from(4));
            let block = SemanticBlock::new(
                BlockId::from_bytes([1; 16]),
                SemanticBlockKind::Paragraph,
                None,
                "text",
                vec![
                    SemanticMarkRange::new(range, SemanticInlineMark::FontSize(24)),
                    SemanticMarkRange::new(
                        range,
                        SemanticInlineMark::FontFamily(InlineFontFamily::Monospace),
                    ),
                    SemanticMarkRange::new(range, mark),
                ],
            );
            let spans = semantic_preview_spans(
                &block,
                Font::with_name("Source Serif 4"),
                16,
                ParchMintTheme::new(ResolvedAppearance::Light),
            );
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].size, Some(iced::Pixels(expected_size)));
            assert_eq!(spans[0].font.unwrap().family, font::Family::Monospace);
        }
    }

    #[test]
    fn cards_show_labeled_metadata_in_columns_below_the_synopsis() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            cards_center(
                &workspace,
                ParchMintTheme::new(ResolvedAppearance::Light),
                None,
            ),
        );

        assert!(simulator.find("Point of view").is_ok());
        assert!(
            simulator
                .find(iced::widget::Id::from("synopsis-chapter-one".to_owned()))
                .is_ok()
        );
        let synopsis = simulator
            .find(iced::widget::Id::from("synopsis-chapter-one".to_owned()))
            .unwrap()
            .bounds();
        let first = simulator
            .find(iced::widget::Id::from(
                "metadata-chapter-one-field-17".to_owned(),
            ))
            .unwrap()
            .bounds();
        let second = simulator
            .find(iced::widget::Id::from(
                "metadata-chapter-one-field-18".to_owned(),
            ))
            .unwrap()
            .bounds();
        assert!(first.y >= synopsis.y + synopsis.height);
        assert_eq!(first.y, second.y);
        assert!(second.x >= first.x + first.width);
    }

    #[test]
    fn creation_placeholder_is_shorter_than_a_document_card() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let mut surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1440.0, 1200.0),
            cards_center(
                &workspace,
                ParchMintTheme::new(ResolvedAppearance::Light),
                None,
            ),
        );
        let card = surface
            .find(harness_target::card_id("chapter-three"))
            .unwrap()
            .bounds();
        let half = surface
            .find(HarnessTarget::OverviewAdd.id())
            .unwrap()
            .bounds();
        assert!(half.height >= 64.0 && half.height < card.height);
        assert!((half.width * 2.0 + 1.0 - card.width).abs() < 1.0);
    }

    #[test]
    fn brand_opens_projects_with_or_without_the_explorer() {
        for expanded in [false, true] {
            let mut surface = Simulator::with_size(
                Settings::default(),
                Size::new(320.0, 60.0),
                project_selector(
                    "Project",
                    expanded,
                    240,
                    ParchMintTheme::new(ResolvedAppearance::Dark),
                ),
            );
            surface.click(HarnessTarget::ProjectMenu.id()).unwrap();
            assert!(
                surface
                    .into_messages()
                    .any(|message| message == ProjectSurfaceMessage::ShowProjectChooser)
            );
        }
    }

    #[test]
    fn cards_expand_independently_in_place_and_keep_all_fields_editable() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let generation = workspace.cards().motion_generation();
        for id in ["chapter-one", "chapter-two"] {
            workspace.update(ProjectMessage::ToggleCardDetails(id.into()));
        }
        assert_eq!(workspace.cards().motion_generation(), generation);
        assert!(workspace.modal().is_none());
        let items = workspace.cards().items();
        for id in ["chapter-one", "chapter-two"] {
            let item = items.iter().find(|item| item.node_id == id).unwrap();
            assert!(item.details_expanded);
            assert!(item.row_height(420.0) > crate::cards_layout::CARD_HEIGHT);
        }
        let mut surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1280.0, 1200.0),
            cards_center(
                &workspace,
                ParchMintTheme::new(ResolvedAppearance::Light),
                None,
            ),
        );
        assert!(surface.find("Point of view").is_ok());
        surface
            .click(iced::widget::Id::from("card-details-chapter-one"))
            .unwrap();
        assert!(surface.into_messages().any(|message| message
            == ProjectSurfaceMessage::Project(ProjectMessage::ToggleCardDetails(
                "chapter-one".into()
            ))));
        workspace.update(ProjectMessage::ToggleCardDetails("chapter-one".into()));
        let items = workspace.cards().items();
        assert!(
            !items
                .iter()
                .find(|item| item.node_id == "chapter-one")
                .unwrap()
                .details_expanded
        );
        assert!(
            items
                .iter()
                .find(|item| item.node_id == "chapter-two")
                .unwrap()
                .details_expanded
        );
    }

    #[test]
    fn group_heading_toggles_while_synopsis_click_edits_only_that_field() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(840.0, 900.0),
            cards_center(&workspace, theme, None),
        );
        simulator.click("Part One").unwrap();
        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            vec![ProjectSurfaceMessage::Project(
                ProjectMessage::ToggleCardsExpanded("part-one".into())
            )]
        );
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(840.0, 900.0),
            cards_center(&workspace, theme, None),
        );
        simulator
            .click(iced::widget::Id::from("synopsis-part-one".to_owned()))
            .unwrap();
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(messages.iter().any(|message| matches!(message, ProjectSurfaceMessage::Project(ProjectMessage::BeginOutlineField { node_id, field_id: None }) if node_id == "part-one")));
        assert!(!messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::ToggleCardsExpanded(_))
        )));
    }

    #[test]
    fn cards_drop_zones_follow_grid_direction_and_distinguish_group_nesting() {
        let bounds = iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(300.0, 160.0));
        let destination = |kind, x, y, horizontal| {
            cards_drop_destination(kind, "chapter", bounds, iced::Point::new(x, y), horizontal)
        };
        assert_eq!(
            destination(HierarchyRowKind::Document, 70.0, 80.0, true),
            Some(DragDestination::BeforeSibling("chapter".into()))
        );
        assert_eq!(
            destination(HierarchyRowKind::Document, 230.0, 80.0, true),
            Some(DragDestination::AfterSibling("chapter".into()))
        );
        assert_eq!(
            destination(HierarchyRowKind::Document, 305.0, 80.0, true),
            Some(DragDestination::AfterSibling("chapter".into()))
        );
        assert_eq!(
            destination(HierarchyRowKind::Document, 150.0, 40.0, false),
            Some(DragDestination::BeforeSibling("chapter".into()))
        );
        assert_eq!(
            destination(HierarchyRowKind::Group, 150.0, 12.0, true),
            Some(DragDestination::BeforeSibling("chapter".into()))
        );
        assert_eq!(
            destination(HierarchyRowKind::Group, 150.0, 80.0, true),
            Some(DragDestination::IntoGroup("chapter".into()))
        );
        assert_eq!(
            destination(HierarchyRowKind::Group, 150.0, 148.0, true),
            Some(DragDestination::AfterSibling("chapter".into()))
        );
        assert_eq!(
            destination(HierarchyRowKind::Document, 320.0, 80.0, true),
            None
        );
    }

    #[test]
    fn cards_leave_room_for_the_scrollbar() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        for width in [420.0, 840.0, 1_440.0] {
            let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                Settings::default(),
                Size::new(width, 250.0),
                cards_center(
                    &workspace,
                    ParchMintTheme::new(ResolvedAppearance::Light),
                    None,
                ),
            );
            let group = simulator.find(harness_target::card_id("part-one")).unwrap();
            assert!((group.bounds().width - group.visible_bounds().unwrap().width).abs() < 1.0);
        }
    }

    #[test]
    fn cards_pack_siblings_in_columns_with_indented_groups() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        for width in [420.0, 840.0, 1_440.0] {
            let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                Settings::default(),
                Size::new(width, 900.0),
                cards_center(
                    &workspace,
                    ParchMintTheme::new(ResolvedAppearance::Light),
                    None,
                ),
            );
            let mut bounds = |id| {
                simulator
                    .find(harness_target::card_id(id))
                    .unwrap()
                    .visible_bounds()
                    .unwrap()
            };
            let group = bounds("part-one");
            let first = bounds("chapter-one");
            let second = bounds("chapter-two");
            let item = workspace
                .cards()
                .items()
                .into_iter()
                .find(|item| item.node_id == "part-one")
                .unwrap();
            assert!(
                (group.height
                    - (item.row_height(group.width) - crate::project_workspace::CARDS_ROW_GAP))
                    .abs()
                    < 1.0,
                "group measurement must match its rendered header: {} versus {}",
                group.height,
                item.row_height(group.width)
            );
            assert!(first.y >= group.y + group.height);
            assert!(first.width >= 230.0);
            assert!(second.x + second.width <= width - SPACING_16);
            if crate::cards_layout::column_count(width - 44.0) > 1 {
                assert_eq!(second.y, first.y);
                assert!(second.x >= first.x + first.width);
            } else {
                assert!(second.y >= first.y + first.height);
                assert_eq!(first.x, second.x);
            }
            assert_eq!(first.width, second.width);
        }
    }

    #[test]
    fn selecting_cards_preserves_their_width_and_spacing() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        workspace.update(ProjectMessage::BeginHierarchyRename("chapter-two".into()));
        workspace.update(ProjectMessage::SetHierarchyRenameDraft(
            "A long chapter title that should wrap without shifting when selected".into(),
        ));
        workspace.update(ProjectMessage::CommitHierarchyRename);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".into(),
            gesture: SelectionGesture::Replace,
        });
        let geometry = |workspace: &ProjectWorkspace| {
            let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                Settings::default(),
                Size::new(420.0, 900.0),
                cards_center(
                    workspace,
                    ParchMintTheme::new(ResolvedAppearance::Light),
                    None,
                ),
            );
            ["chapter-one", "chapter-two", "chapter-three"].map(|id| {
                simulator
                    .find(harness_target::card_id(id))
                    .unwrap()
                    .bounds()
            })
        };
        let before = geometry(&workspace);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-two".into(),
            gesture: SelectionGesture::Replace,
        });
        let after = geometry(&workspace);
        for (old, new) in before.iter().zip(after) {
            assert_eq!(old.width, new.width);
        }
        assert!(after[2].y >= after[1].y + after[1].height);
    }

    #[test]
    fn cards_stack_metadata_below_synopsis_after_long_field_edits() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let before = workspace.cards().item_window(2, 792.0).rows[1].height;
        let synopsis = format!(
            "{}\n\nShe finally opens the letter — and understands.",
            "Mara follows the river through the old town, searching for the missing archivist. "
                .repeat(6)
        );
        let value =
            "An unreliable narrator remembering the journey differently each time. ".repeat(4);
        workspace.update(ProjectMessage::SetSynopsis {
            node_id: "chapter-one".into(),
            synopsis: synopsis.clone(),
        });
        workspace.update(ProjectMessage::SetMetadataValue {
            node_id: "chapter-one".into(),
            field_id: "field-17".into(),
            value: value.clone(),
        });
        assert!(workspace.cards().item_window(2, 792.0).rows[1].height >= before);
        for (width, appearance) in [
            (420.0, ResolvedAppearance::Light),
            (840.0, ResolvedAppearance::Light),
            (840.0, ResolvedAppearance::Dark),
        ] {
            let theme = ParchMintTheme::new(appearance);
            let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                Settings::default(),
                Size::new(width, 900.0),
                cards_center(&workspace, theme, None),
            );
            let synopsis_bounds = simulator
                .find(iced::widget::Id::from("synopsis-chapter-one".to_owned()))
                .unwrap()
                .bounds();
            let metadata_bounds = simulator
                .find(iced::widget::Id::from(
                    "metadata-chapter-one-field-17".to_owned(),
                ))
                .unwrap()
                .bounds();
            let card = simulator
                .find(harness_target::card_id("chapter-one"))
                .unwrap()
                .bounds();
            assert!(synopsis_bounds.height > 40.0);
            assert!(metadata_bounds.height > 18.0);
            assert!(card.height >= crate::cards_layout::CARD_HEIGHT);
            assert!(metadata_bounds.y >= synopsis_bounds.y + synopsis_bounds.height);
            assert!(metadata_bounds.y + metadata_bounds.height <= card.y + card.height - SPACING_8);
            if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
                simulator
                    .snapshot(&theme.iced_theme())
                    .unwrap()
                    .matches_image(
                        std::path::PathBuf::from(root)
                            .join(format!("full-cards-{width}-{appearance:?}")),
                    )
                    .unwrap();
            }
        }
    }

    #[test]
    fn overview_has_split_creation_controls_and_hides_explorer() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".into(),
            gesture: SelectionGesture::Replace,
        });
        interact(&workspace, RibbonDestination::Cards, |surface| {
            assert!(surface.find(HarnessTarget::OverviewAdd.id()).is_ok());
            assert!(
                surface
                    .find(iced::widget::Id::from("synopsis-chapter-one".to_owned()))
                    .is_ok()
            );
            assert!(surface.find("Explorer").is_err());
            assert!(surface.find(HarnessTarget::InspectorTitle.id()).is_err());
            assert!(surface.find("+ New").is_err());
        });
    }

    #[test]
    fn cards_context_menu_uses_the_shared_hierarchy_selection() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(840.0, 700.0),
            cards_center(
                &workspace,
                ParchMintTheme::new(ResolvedAppearance::Light),
                None,
            ),
        );
        let bounds = simulator
            .find(harness_target::card_id("chapter-one"))
            .unwrap()
            .visible_bounds()
            .unwrap();
        simulator.point_at(bounds.center());
        simulator.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Right,
        ))]);
        let messages = simulator.into_messages().collect();
        assert!(apply_project_messages(&mut workspace, messages).is_empty());
        assert_eq!(workspace.hierarchy_context_menu(), Some("chapter-one"));
        assert_eq!(workspace.explorer().selected_ids(), ["chapter-one"]);
    }

    #[test]
    fn document_context_menu_opens_exact_history_and_fits_near_window_edge() {
        let _motion = crate::motion::SettledMotion::new();
        let (mut workspace, ids) = production_workspace();
        workspace.update(ProjectMessage::OpenHierarchyContextMenu {
            node_id: ids.live_node,
            point: Point::new(1438.0, 898.0),
        });
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1440.0, 900.0),
            hierarchy_context_overlay(
                &workspace,
                Space::new().width(Length::Fill).height(Length::Fill).into(),
                ParchMintTheme::new(ResolvedAppearance::Light),
                1440.0,
                900.0,
            ),
        );
        for label in [
            "Open",
            "Open beside",
            "History",
            "Rename",
            "Copy",
            "Cut",
            "Delete",
        ] {
            let bounds = simulator.find(label).unwrap().visible_bounds().unwrap();
            assert!(
                bounds.x >= 0.0
                    && bounds.y >= 0.0
                    && bounds.x + bounds.width <= 1440.0
                    && bounds.y + bounds.height <= 900.0
            );
        }
        simulator.click("History").unwrap();
        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            [ProjectSurfaceMessage::OpenDocumentHistory(
                ids.live_document
            )]
        );

        workspace.update(ProjectMessage::OpenHierarchyContextMenu {
            node_id: ids.group,
            point: Point::new(200.0, 200.0),
        });
        let mut group = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1440.0, 900.0),
            hierarchy_context_overlay(
                &workspace,
                Space::new().width(Length::Fill).height(Length::Fill).into(),
                ParchMintTheme::new(ResolvedAppearance::Light),
                1440.0,
                900.0,
            ),
        );
        assert!(group.find("History").is_err());
    }

    #[test]
    fn overview_group_body_and_background_have_context_menus() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        for region in 0..3 {
            let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                Settings::default(),
                Size::new(840.0, 700.0),
                cards_center(
                    &workspace,
                    ParchMintTheme::new(ResolvedAppearance::Light),
                    None,
                ),
            );
            let bounds = simulator
                .find(harness_target::card_id("chapter-one"))
                .unwrap()
                .visible_bounds()
                .unwrap();
            let point = match region {
                0 => iced::Point::new(17.0, bounds.center_y()),
                1 => iced::Point::new(832.0, 650.0),
                _ => iced::Point::new(1.0, 1.0),
            };
            simulator.point_at(point);
            simulator.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Right,
            ))]);
            let menus = simulator
                .into_messages()
                .filter_map(|message| match message {
                    ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyContextMenu {
                        node_id,
                        ..
                    }) => Some(node_id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                menus,
                vec![if region == 0 {
                    "part-one"
                } else {
                    "manuscript"
                }]
            );
        }
    }

    #[test]
    fn navigation_rail_has_one_clickable_target_per_destination() {
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(48.0, 720.0),
            navigation_rail(
                RibbonDestination::Editor,
                ParchMintTheme::new(ResolvedAppearance::Light),
            ),
        );
        let destinations = [
            RibbonDestination::Editor,
            RibbonDestination::Cards,
            RibbonDestination::History,
            RibbonDestination::RecentlyDeleted,
            RibbonDestination::Export,
            RibbonDestination::Settings,
        ];
        let mut bottom = 0.0;
        for destination in destinations {
            let target = HarnessTarget::Ribbon(destination).id();
            let bounds = simulator.find(target.clone()).unwrap().bounds();
            assert!(bounds.width >= 32.0 && bounds.height >= 32.0);
            assert!(bounds.y >= bottom);
            bottom = bounds.y + bounds.height;
            assert!(bounds.x + bounds.width <= 48.0);
            simulator.click(target).unwrap();
        }
        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            destinations.map(ProjectSurfaceMessage::Navigate)
        );
    }

    #[test]
    fn reference_shell_layout_uses_the_1440_desktop_columns() {
        let layout = ShellLayout::for_window(1_440, 900);

        assert_eq!(layout.ribbon().height(), 44);
        assert_eq!(layout.status_bar().height(), 26);
        assert_eq!(layout.explorer().width(), 280);
        assert_eq!(layout.inspector().width(), 0);
        assert_eq!(layout.center().width(), 1_160);
    }

    #[test]
    fn settings_center_uses_the_application_canvas_role() {
        assert_eq!(
            destination_canvas_surface(RibbonDestination::Settings),
            Surface::Application
        );
    }

    #[cfg(feature = "visual-verification")]
    #[test]
    fn verification_center_geometry_matches_the_reference_shell() {
        assert_eq!(
            verification_center_geometry(RibbonDestination::Editor),
            VerificationCenterGeometry {
                width: 840,
                height: 816,
            }
        );
        assert_eq!(
            verification_center_geometry(RibbonDestination::GlobalSearch),
            VerificationCenterGeometry {
                width: 760,
                height: 848,
            }
        );
    }

    #[test]
    fn destination_shells_keep_only_the_chrome_in_their_reference_composition() {
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let cards = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let mut cards_surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &cards,
                RibbonDestination::Cards,
                theme,
                text("Mounted editor child").into(),
            ),
        );
        assert!(
            cards_surface
                .find(HarnessTarget::Ribbon(RibbonDestination::Cards).id())
                .is_ok()
        );
        assert!(cards_surface.find("Explorer").is_err());
        assert!(
            cards_surface
                .find(HarnessTarget::InspectorTitle.id())
                .is_err()
        );
        assert!(cards_surface.find("Document History").is_err());
        assert!(cards_surface.find("+ Document").is_err());
        assert!(cards_surface.find("Copy").is_err());
        assert!(cards_surface.find("P · C").is_err());

        let search = ProjectWorkspace::from_fixture(ProjectFixture::GlobalSearch);
        let mut search_surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &search,
                RibbonDestination::GlobalSearch,
                theme,
                text("Mounted editor child").into(),
            ),
        );
        assert!(search_surface.find("←  Search").is_ok());
        assert!(search_surface.find("Replace with").is_err());
        assert!(
            search_surface
                .find(HarnessTarget::InspectorTitle.id())
                .is_err()
        );
        assert!(search_surface.find("1 match in 1 document").is_ok());
        assert!(search_surface.find("Chapter One").is_ok());
        assert!(search_surface.find("1 match").is_ok());
    }

    #[test]
    fn cards_surface_edits_the_selected_authoritative_fields() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".into(),
            gesture: SelectionGesture::Replace,
        });
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Cards,
                theme,
                text("Mounted editor child").into(),
            ),
        );

        assert!(simulator.find("Chapter One").is_ok());
    }

    #[test]
    fn empty_global_search_keeps_only_search_controls() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::GlobalSearch);
        workspace.update(ProjectMessage::SetGlobalSearchQuery(String::new()));
        let _ = interact(&workspace, RibbonDestination::Editor, |surface| {
            assert!(surface.find(global_search_query_input_id()).is_ok());
            assert!(surface.find(global_replacement_input_id()).is_err());
            assert!(surface.find("0 matches in 0 documents").is_err());
        });
        workspace.update(ProjectMessage::SetGlobalSearchQuery("river".into()));
        let _ = interact(&workspace, RibbonDestination::Editor, |surface| {
            assert!(surface.find(global_replacement_input_id()).is_err());
        });
        workspace.update(ProjectMessage::ToggleGlobalReplace);
        let _ = interact(&workspace, RibbonDestination::Editor, |surface| {
            assert!(surface.find(global_replacement_input_id()).is_ok());
        });
    }

    #[test]
    fn search_group_match_counts_keep_singular_and_plural_labels() {
        assert_eq!(search_match_count_label(1), "1 match");
        assert_eq!(search_match_count_label(2), "2 matches");
        assert_eq!(search_match_count_label(4), "4 matches");
    }

    #[test]
    fn global_search_summary_uses_singular_document_label_for_one_document() {
        assert_eq!(
            global_search_result_count_label(1, 1),
            "1 match in 1 document"
        );
        assert_eq!(
            global_search_result_count_label(2, 1),
            "2 matches in 1 document"
        );
        assert_eq!(
            global_search_result_count_label(2, 2),
            "2 matches in 2 documents"
        );
    }

    #[test]
    fn recovery_is_an_opaque_action_overlay_without_editor_sidebars() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::ErrorRecovery);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Mounted editor child").into(),
            ),
        );
        assert!(simulator.find("Unsaved changes found").is_ok());
        assert!(simulator.find("Recover changes").is_ok());
        assert!(simulator.find("Open last saved version").is_ok());
        assert!(simulator.find("Choose which version to open.").is_err());
        assert!(simulator.find("Explorer").is_err());
        assert!(simulator.find("Inspector").is_err());
        assert!(simulator.find("Document History").is_err());
        assert!(
            simulator
                .find(format!(
                    "{} words",
                    workspace.editor().status_bar().manuscript_total()
                ))
                .is_err()
        );
    }

    #[test]
    fn inspector_omits_empty_comment_sections() {
        let (mut workspace, ids) = production_workspace();
        workspace.update(ProjectMessage::ToggleHierarchyExpanded(ids.group));
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: ids.live_node,
            gesture: SelectionGesture::Replace,
        });
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );
        assert!(simulator.find("Comments").is_err());
        assert!(
            simulator
                .find("Select text, then choose Comment in the toolbar.")
                .is_err()
        );
        assert!(simulator.find("New comment").is_err());
        assert!(simulator.find("Add at selection").is_err());
        assert!(simulator.find("Add to document").is_err());
    }

    #[test]
    fn selected_comment_is_rendered_first_in_the_read_only_inspector_index() {
        let (mut workspace, ids) = production_workspace();
        workspace.update(ProjectMessage::ToggleHierarchyExpanded(ids.group));
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: ids.live_node,
            gesture: SelectionGesture::Replace,
        });
        let selected = CommentId::from_bytes([0x22; 16]);
        workspace.editor_mut().reconcile_document_comments(
            &ids.live_document,
            &[
                CanonicalComment::new(
                    CommentId::from_bytes([0x11; 16]),
                    EditorSelection::new(1.into(), 4.into()),
                    "Earlier thread",
                    BlockId::from_bytes([0x33; 16]),
                ),
                CanonicalComment::new(
                    selected,
                    EditorSelection::new(5.into(), 8.into()),
                    "A long selected comment that must leave room for its unresolved status",
                    BlockId::from_bytes([0x33; 16]),
                ),
            ],
        );
        let selected_id = id_string(selected.as_bytes());
        workspace
            .editor_mut()
            .update(EditorMessage::SelectComment(selected_id.clone()));
        assert_eq!(
            workspace.editor().selected_comment(),
            Some(selected_id.as_str())
        );
        assert_ne!(
            workspace.editor().inspector_comments()[0].id(),
            selected_id.as_str()
        );

        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut layout = ShellLayout::for_window(1440, 900);
        layout.set_inspector_visible(true);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            native_project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
                &layout,
                [true; 3],
            ),
        );
        let comment = simulator
            .find("A long selected comment that must leave room for its unresolved status")
            .unwrap()
            .bounds();
        assert!(comment.x + comment.width <= 1_440.0);
        assert!(simulator.find(HarnessTarget::CommentReply.id()).is_err());
        let message_id = workspace
            .editor()
            .comment_thread(&selected_id)
            .unwrap()
            .messages()[0]
            .id()
            .to_owned();
        simulator
            .click(iced::widget::Id::from(format!("note-edit-{message_id}")))
            .unwrap();
        assert!(
            matches!(simulator.into_messages().collect::<Vec<_>>().as_slice(),
            [ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                EditorMessage::BeginEditCommentMessage { thread_id, message_id: edited, .. }
            ))] if thread_id == &selected_id && edited == &message_id)
        );
        workspace
            .editor_mut()
            .update(EditorMessage::BeginEditHoveredNote {
                thread_id: selected_id,
                message_id,
                body: "A long selected comment that must leave room for its unresolved status"
                    .into(),
            });
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            native_project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
                &layout,
                [true; 3],
            ),
        );
        assert!(simulator.find(HarnessTarget::CommentEdit.id()).is_err());
    }

    #[test]
    fn comment_inspector_stays_read_only_when_the_editor_composer_is_open() {
        let (mut workspace, ids) = production_workspace();
        workspace.update(ProjectMessage::ToggleHierarchyExpanded(ids.group));
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: ids.live_node,
            gesture: SelectionGesture::Replace,
        });
        workspace
            .editor_mut()
            .update(EditorMessage::BeginCommentAtSelection {
                pane: EditorPane::Primary,
                anchor_bounds: crate::Rect::new(24.0, 36.0, 30.0, 14.0),
            });
        assert!(
            workspace
                .editor()
                .comment_composer(EditorPane::Primary)
                .is_some()
        );
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );

        assert!(simulator.find("Comments").is_err());
        assert!(simulator.find("New comment").is_err());
        assert!(simulator.find("Add at selection").is_err());
    }

    #[test]
    fn production_projections_render_in_the_persistent_shell_in_light_and_dark() {
        let (mut workspace, ids) = production_workspace();
        workspace.update(ProjectMessage::ToggleHierarchyExpanded(ids.group.clone()));
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: ids.live_node.clone(),
            gesture: SelectionGesture::Replace,
        });

        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            for destination in [
                RibbonDestination::Cards,
                RibbonDestination::RecentlyDeleted,
                RibbonDestination::Settings,
            ] {
                let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                    Settings::default(),
                    Size::new(1_440.0, 900.0),
                    project_surface(
                        &workspace,
                        destination,
                        theme,
                        text("Mounted editor child").into(),
                    ),
                );
                match destination {
                    RibbonDestination::Cards => {
                        assert!(
                            simulator
                                .find(HarnessTarget::Ribbon(RibbonDestination::Cards).id())
                                .is_ok()
                        );
                        assert!(simulator.find("Part One").is_ok());
                        assert!(simulator.find("Opening Scene").is_ok());
                        assert!(
                            simulator
                                .find(iced::widget::Id::from(format!(
                                    "synopsis-{}",
                                    ids.live_node
                                )))
                                .is_ok()
                        );
                        assert!(
                            workspace
                                .inspector()
                                .metadata_items(&ids.live_node)
                                .iter()
                                .any(|field| field.effective_value == Some("Final"))
                        );
                    }
                    RibbonDestination::RecentlyDeleted => {
                        assert!(simulator.find("Discarded Part").is_ok());
                        assert!(simulator.find("Deleted document contents").is_err());
                        assert!(simulator.find("Restore item").is_ok());
                        assert!(simulator.find("Manuscript").is_err());
                    }
                    RibbonDestination::Settings => {
                        assert!(simulator.find("System").is_ok());
                        assert!(simulator.find("Manuscript").is_err());
                    }
                    _ => unreachable!("the test enumerates destination-specific projections"),
                }
                let snapshot = simulator
                    .snapshot(&theme.iced_theme())
                    .expect("production project surface snapshot");
                assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
            }
        }
    }

    #[test]
    fn recently_deleted_surface_keeps_the_primary_restore_action_visible() {
        let (workspace, ids) = production_workspace();
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::RecentlyDeleted,
                theme,
                text("Mounted editor child").into(),
            ),
        );
        assert!(simulator.find("Restore to Manuscript").is_ok());
        assert!(simulator.find("Deleted item preview").is_err());
        simulator
            .click("Restore item")
            .expect("visible restore action");
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert_eq!(
            messages,
            [ProjectSurfaceMessage::Project(
                ProjectMessage::RestoreDeleted(ids.deleted_node,)
            )]
        );
    }

    #[test]
    fn deleted_empty_group_can_be_restored_without_a_document_preview() {
        let group = NodeId::from_bytes([42; 16]);
        let mut project = Project::new(ProjectId::from_bytes([41; 16]));
        project
            .nodes
            .try_insert_group(group, NodeId::manuscript_root(), 0, "Empty group")
            .unwrap();
        let project = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::delete_node_at(group, 123),
        )
        .unwrap()
        .project;
        let workspace = ProjectWorkspace::from_snapshot(&ProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        });
        interact(&workspace, RibbonDestination::History, |surface| {
            assert!(surface.find("Project history").is_ok());
            assert!(
                surface
                    .find("Restoring replaces the entire current project.")
                    .is_err()
            );
        });
        interact(&workspace, RibbonDestination::Settings, |surface| {
            assert!(surface.find("Appearance").is_ok());
        });
        let messages = interact(&workspace, RibbonDestination::RecentlyDeleted, |surface| {
            assert!(
                surface
                    .find("This group has no documents to preview.")
                    .is_ok()
            );
            surface.click("Restore item").unwrap();
        });
        assert_eq!(
            messages,
            [ProjectSurfaceMessage::Project(
                ProjectMessage::RestoreDeleted(id_string(group.as_bytes()))
            )]
        );
    }

    #[test]
    fn expanded_recovery_keeps_actions_visible_with_many_documents() {
        let _motion = crate::motion::SettledMotion::new();
        crate::visual_verification::load_test_fonts();
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::ErrorRecovery);
        let ticket = workspace.begin_task(crate::ProjectTask::ReconcileRecovery);
        workspace.accept_completion(crate::ProjectTaskCompletion::for_ticket(
            ticket,
            crate::ProjectTaskPayload::RecoveryAvailable {
                accepted_records: 50,
                affected_documents: (0..50).map(|i| (format!("document-{i}"), 8)).collect(),
                isolation: None,
            },
        ));
        workspace.update(ProjectMessage::ToggleRecoveryDetails);
        let theme = ParchMintTheme::new(ResolvedAppearance::Dark);
        let mut surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1280.0, 720.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );
        for label in ["Open last saved version", "Recover changes"] {
            let bounds = surface
                .find(label)
                .unwrap()
                .visible_bounds()
                .expect("visible action");
            assert!(bounds.y >= 0.0 && bounds.y + bounds.height <= 720.0);
        }
        if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
            assert!(
                surface
                    .snapshot(&theme.iced_theme())
                    .unwrap()
                    .matches_image(std::path::PathBuf::from(root).join("recovery-many-documents"))
                    .unwrap()
            );
        }
        surface.click("Recover changes").unwrap();
        assert_eq!(
            surface.into_messages().collect::<Vec<_>>(),
            [ProjectSurfaceMessage::Project(
                ProjectMessage::AcceptRecovery
            )]
        );
    }

    #[test]
    fn history_surface_exposes_typed_numbered_changed_lines() {
        fn semantic(lines: &[&str]) -> parchmint_editor_api::SemanticDocument {
            parchmint_editor_api::SemanticDocument::new(
                lines
                    .iter()
                    .enumerate()
                    .map(|(index, text)| {
                        parchmint_editor_api::SemanticBlock::new(
                            parchmint_editor_api::BlockId::from_bytes([index as u8; 16]),
                            parchmint_editor_api::SemanticBlockKind::Paragraph,
                            None,
                            *text,
                            Vec::new(),
                        )
                    })
                    .collect(),
            )
        }

        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::History);
        let checkpoint = workspace.history().checkpoints()[0].clone();
        let checkpoint_id = checkpoint.checkpoint_id.clone();
        let _ = workspace.update(ProjectMessage::SelectHistoryCheckpoint(
            checkpoint_id.clone(),
        ));
        let ticket = workspace.begin_task(crate::ProjectTask::PreviewHistory {
            checkpoint_id: checkpoint_id.clone(),
        });
        let checkpoint_document = crate::HistoryDocumentPreview {
            document_id: "chapter-one".to_owned(),
            canonical_path: "documents/chapter-one.html".to_owned(),
            semantic: semantic(&["The blue house", "Keep", "Remove me"]),
        };
        let current_document = crate::HistoryCurrentDocument {
            document_id: "chapter-one".to_owned(),
            title: "Chapter One".to_owned(),
            body: "<p>The green house</p><p>Keep</p><p>Added one</p><p>Added two</p>".to_owned(),
            semantic: semantic(&["The green house", "Keep", "Added one", "Added two"]),
        };
        let comparison = crate::project_workspace::compare_history_documents(
            &checkpoint_id,
            &checkpoint_document,
            &current_document,
        );
        assert!(
            workspace.accept_completion(crate::ProjectTaskCompletion::for_ticket(
                ticket,
                crate::ProjectTaskPayload::HistoryPreviewReady {
                    preview: Box::new(crate::HistoryPreviewData {
                        checkpoint,
                        resource_paths: vec!["documents/chapter-one.html".to_owned()],
                        project_changes: Some(vec![comparison.clone()]),
                        document: Some(checkpoint_document),
                    }),
                    current_document: Some(current_document),
                    comparison: Some(comparison),
                },
            ))
        );
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::History,
                theme,
                text("Mounted editor child").into(),
            ),
        );
        assert!(simulator.find("Checkpoint").is_ok());
        assert!(simulator.find("Current project").is_ok());
        assert!(simulator.find("− 1").is_ok());
        assert!(simulator.find("+ 1").is_ok());
        assert!(simulator.find("The blue house").is_ok());
        assert!(simulator.find("The green house").is_ok());
        assert!(
            simulator
                .find(format!(
                    "{} · +2 words",
                    crate::project_workspace::local_version_time(
                        workspace
                            .history()
                            .comparison()
                            .and_then(|comparison| workspace
                                .history()
                                .checkpoints()
                                .iter()
                                .find(|checkpoint| checkpoint.checkpoint_id
                                    == comparison.checkpoint_id))
                            .unwrap()
                            .recorded_at_unix_millis
                            .unwrap()
                    )
                ))
                .is_ok()
        );
    }

    #[test]
    fn explorer_header_controls_share_a_center_line() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        for width in [220.0, 320.0] {
            let mut simulator = Simulator::with_size(
                Settings::default(),
                Size::new(width, 700.0),
                explorer_rail(&workspace, ParchMintTheme::new(ResolvedAppearance::Light)),
            );
            let title = simulator.find("Explorer").unwrap().bounds();
            assert!(simulator.find("+ New").is_err());
            let root_row = simulator
                .find(harness_target::explorer_row_id("manuscript"))
                .unwrap()
                .bounds();
            let first_document = simulator
                .find(harness_target::explorer_row_id("chapter-one"))
                .unwrap()
                .bounds();
            let second_document = simulator
                .find(harness_target::explorer_row_id("chapter-two"))
                .unwrap()
                .bounds();
            assert_eq!(root_row.height, EXPLORER_ROW_EXTENT);
            assert_eq!(first_document.height, EXPLORER_ROW_EXTENT);
            assert_eq!(second_document.height, EXPLORER_ROW_EXTENT);
            assert_eq!(second_document.y - first_document.y, EXPLORER_ROW_EXTENT);
            {
                let bounds = simulator
                    .find(HarnessTarget::ExplorerSearch.id())
                    .unwrap()
                    .bounds();
                assert!(bounds.width >= 32.0 && bounds.height >= 32.0);
                assert!((bounds.center_y() - title.center_y()).abs() < 1.0);
                assert!(bounds.x + bounds.width <= width);
            }
        }
    }

    #[test]
    fn explorer_keeps_mutation_commands_in_the_context_menu() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut selection_surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );
        assert!(selection_surface.find("+ Document").is_err());
        assert!(selection_surface.find("+ Group").is_err());
        assert!(selection_surface.find("Copy").is_err());
        assert!(selection_surface.find("Cut").is_err());
        assert!(selection_surface.find("Delete").is_err());
        drop(selection_surface);

        workspace.update(ProjectMessage::CopySelection);
        let mut clipboard_surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );
        assert!(clipboard_surface.find("Paste").is_err());
    }

    #[test]
    fn overview_add_menu_creates_at_its_own_parent() {
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(400.0, 300.0),
            overview_add("part-one", false, theme),
        );
        simulator
            .click(iced::widget::Id::from("overview-document-part-one"))
            .unwrap();
        assert!(simulator.into_messages().any(|message| matches!(message, ProjectSurfaceMessage::Project(ProjectMessage::RequestCreateHierarchy { parent_id, kind: HierarchyItemKind::Document }) if parent_id == "part-one")));
    }

    #[test]
    fn hierarchy_context_overlay_exposes_only_applicable_actions() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::OpenHierarchyContextMenu {
            node_id: "part-one".to_owned(),
            point: Point::new(1_420.0, 880.0),
        });
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );
        assert!(simulator.find("New document").is_ok());
        assert!(simulator.find("New group").is_ok());
        assert!(simulator.find("Open beside").is_err());
        assert!(simulator.find("Rename").is_ok());
        assert!(simulator.find("Rename item").is_err());
        assert!(simulator.find("Copy").is_ok());
        assert!(simulator.find("Cut").is_ok());
        assert!(simulator.find("Delete").is_ok());
    }

    #[test]
    fn hierarchy_context_actions_publish_their_project_commands() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::OpenHierarchyContextMenu {
            node_id: "part-one".to_owned(),
            point: Point::default(),
        });
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );

        simulator
            .click("New document")
            .expect("create document action");
        simulator.click("New group").expect("create group action");
        simulator.click("Rename").expect("rename action");
        simulator.click("Copy").expect("copy action");
        simulator.click("Cut").expect("cut action");
        simulator.click("Delete").expect("delete action");

        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::RequestCreateHierarchy {
                parent_id,
                kind: HierarchyItemKind::Document,
            }) if parent_id == "part-one"
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::RequestCreateHierarchy {
                parent_id,
                kind: HierarchyItemKind::Group,
            }) if parent_id == "part-one"
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::BeginHierarchyRename(node_id))
                if node_id == "part-one"
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::CopySelection)
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::CutSelection)
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::DeleteSelection)
        )));
    }

    #[test]
    fn inline_hierarchy_rename_captures_typing_and_commits_on_click_away() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::BeginHierarchyRename(
            "chapter-one".to_owned(),
        ));
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Mounted editor child").into(),
            ),
        );
        let rename = simulator.find("Chapter One").expect("inline rename input");
        simulator.point_at(rename.visible_bounds().expect("rename bounds").center());
        assert!(
            simulator
                .simulate(iced_test::simulator::click())
                .contains(&iced::event::Status::Captured)
        );
        assert_eq!(
            simulator.typewrite(" revised"),
            iced::event::Status::Captured
        );
        // This point is outside the Explorer field. The wrapper must emit the
        // same reducer command used by Enter rather than leave a stale draft.
        let outside = iced::Point::new(1_000.0, 820.0);
        simulator.point_at(outside);
        simulator.simulate([
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: outside }),
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
        ]);

        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::SetHierarchyRenameDraft(value))
                if value == "Chapter One revised"
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::CommitHierarchyRename)
        )));
        assert!(
            messages
                .iter()
                .all(|message| matches!(message, ProjectSurfaceMessage::Project(_)))
        );
        assert_eq!(
            apply_project_messages(&mut workspace, messages),
            [crate::ProjectEffect::CommitNodeTitle {
                node_id: "chapter-one".to_owned(),
                title: "Chapter One revised".to_owned(),
            }]
        );
    }

    #[test]
    fn document_row_drop_destinations_are_reorder_only() {
        let bounds = iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(160.0, 40.0));
        for point in [
            iced::Point::new(80.0, 2.0),
            iced::Point::new(80.0, 20.0),
            iced::Point::new(80.0, 38.0),
        ] {
            assert!(matches!(
                hierarchy_row_destination(HierarchyRowKind::Document, "chapter-two", bounds, point),
                Some(DragDestination::BeforeSibling(_)) | Some(DragDestination::AfterSibling(_))
            ));
        }
    }

    #[test]
    fn hierarchy_context_delete_action_targets_the_right_clicked_document() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::OpenHierarchyContextMenu {
            node_id: "chapter-one".to_owned(),
            point: Point::new(300.0, 220.0),
        });
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );
        simulator.click("Delete").expect("delete action");

        for message in simulator.into_messages() {
            if let ProjectSurfaceMessage::Project(message) = message {
                let effects = workspace.update(message);
                if !effects.is_empty() {
                    assert_eq!(
                        effects,
                        vec![crate::ProjectEffect::DeleteHierarchy(vec![
                            "chapter-one".to_owned()
                        ])]
                    );
                    return;
                }
            }
        }
        panic!("context-menu delete must dispatch a hierarchy deletion");
    }

    #[test]
    fn explorer_document_click_opens_a_replaceable_preview_and_selects_the_document() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );

        simulator.click("Chapter Two").expect("document row click");

        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::PreviewHierarchyNode(node_id))
                if node_id == "chapter-two"
        )));
        let effects = apply_project_messages(&mut workspace, messages);
        assert_eq!(
            effects,
            [crate::ProjectEffect::OpenDocumentInPrimary(
                "chapter-two".to_owned()
            )]
        );
        assert_eq!(workspace.explorer().selected_ids(), ["chapter-two"]);
        assert_eq!(
            workspace
                .editor()
                .pane(crate::EditorPane::Primary)
                .active_document(),
            Some("chapter-two")
        );
        assert!(workspace.editor().pane(crate::EditorPane::Primary).tabs()[1].is_preview());
    }

    #[test]
    fn explorer_and_overview_disclosures_are_independent() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        workspace.update(ProjectMessage::ToggleHierarchyExpanded("part-one".into()));
        assert!(!workspace.explorer().row("part-one").unwrap().expanded);
        let messages = interact(&workspace, RibbonDestination::Cards, |surface| {
            assert!(surface.find(harness_target::card_id("chapter-one")).is_ok());
            surface
                .click(harness_target::card_disclosure_id("part-one"))
                .unwrap();
        });
        apply_project_messages(&mut workspace, messages);
        assert!(!workspace.explorer().row("part-one").unwrap().expanded);
        interact(&workspace, RibbonDestination::Cards, |surface| {
            assert!(
                surface
                    .find(harness_target::card_id("chapter-one"))
                    .is_err()
            );
        });
        workspace.update(ProjectMessage::ToggleHierarchyExpanded("part-one".into()));
        assert!(workspace.explorer().row("part-one").unwrap().expanded);
        interact(&workspace, RibbonDestination::Cards, |surface| {
            assert!(
                surface
                    .find(harness_target::card_id("chapter-one"))
                    .is_err()
            );
        });
    }

    #[test]
    fn explorer_click_below_drag_threshold_selects_without_starting_a_drag() {
        let program = ExplorerDragProgram;
        let (sender, mut events) = iced_test::futures::futures::channel::mpsc::channel(8);
        let mut emulator =
            Emulator::new(sender, &program, Mode::Immediate, Size::new(1_440.0, 900.0));
        let _ = iced_test::futures::futures::executor::block_on(
            iced_test::futures::futures::StreamExt::next(&mut events),
        );

        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Press {
                button: iced::mouse::Button::Left,
                target: Some(Target::Text("Chapter One".to_owned())),
            },
        );
        // This production-target move remains at the press point, below the
        // four-pixel threshold, before the button is released.
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Move(Target::Text("Chapter One".to_owned())),
        );
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Release {
                button: iced::mouse::Button::Left,
                target: Some(Target::Text("Chapter One".to_owned())),
            },
        );

        let (state, _) = emulator.into_state();
        assert!(state.effects.is_empty());
        assert_eq!(state.workspace.explorer().selected_ids(), ["chapter-one"]);
        assert!(state.workspace.hierarchy_drag_source().is_none());
    }

    struct ExplorerDragProgram;

    struct ExplorerDragState {
        workspace: ProjectWorkspace,
        editor_slots: crate::iced_editor_surface::EditorHostSlots,
        effects: Vec<crate::ProjectEffect>,
    }

    impl Program for ExplorerDragProgram {
        type State = ExplorerDragState;
        type Message = ProjectSurfaceMessage;
        type Theme = Theme;
        type Renderer = iced::Renderer;
        type Executor = executor::Default;

        fn name() -> &'static str {
            "explorer_drag_test"
        }

        fn settings(&self) -> Settings {
            Settings::default()
        }

        fn window(&self) -> Option<iced::window::Settings> {
            None
        }

        fn boot(&self) -> (Self::State, Task<Self::Message>) {
            (
                ExplorerDragState {
                    workspace: ProjectWorkspace::from_fixture(ProjectFixture::Explorer),
                    editor_slots: Default::default(),
                    effects: Vec::new(),
                },
                Task::none(),
            )
        }

        fn update(&self, state: &mut Self::State, message: Self::Message) -> Task<Self::Message> {
            let effects = match message {
                ProjectSurfaceMessage::Project(message) => state.workspace.update(message),
                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::HierarchyDropTarget(
                    pane,
                )) => state
                    .workspace
                    .update(ProjectMessage::SetDragDestination(Some(
                        DragDestination::EditorPane(pane),
                    ))),
                ProjectSurfaceMessage::EditorCenter(
                    EditorCenterMessage::ClearHierarchyDropTarget(pane),
                ) => state.workspace.update(ProjectMessage::ClearDragDestination(
                    DragDestination::EditorPane(pane),
                )),
                _ => Vec::new(),
            };
            state.effects.extend(effects);
            Task::none()
        }

        fn view<'a>(
            &self,
            state: &'a Self::State,
            _window: iced::window::Id,
        ) -> Element<'a, Self::Message, Self::Theme, Self::Renderer> {
            let theme = ParchMintTheme::new(ResolvedAppearance::Light);
            let editor = crate::iced_editor_surface::editor_center_surface(
                state.workspace.editor(),
                theme,
                &state.editor_slots,
                None,
            )
            .map(ProjectSurfaceMessage::EditorCenter);
            project_surface(&state.workspace, RibbonDestination::Editor, theme, editor)
        }

        fn theme(&self, _state: &Self::State, _window: iced::window::Id) -> Option<Self::Theme> {
            Some(Theme::Light)
        }
    }

    fn run_drag_instruction(
        emulator: &mut Emulator<ExplorerDragProgram>,
        program: &ExplorerDragProgram,
        events: &mut iced_test::futures::futures::channel::mpsc::Receiver<
            iced_test::emulator::Event<ExplorerDragProgram>,
        >,
        mouse: Mouse,
    ) {
        emulator.run(program, Instruction::Interact(Interaction::Mouse(mouse)));
        loop {
            match iced_test::futures::futures::executor::block_on(
                iced_test::futures::futures::StreamExt::next(events),
            ) {
                Some(iced_test::emulator::Event::Action(action)) => {
                    emulator.perform(program, action)
                }
                Some(iced_test::emulator::Event::Ready) => return,
                Some(iced_test::emulator::Event::Failed(instruction)) => {
                    panic!("continuous drag instruction failed: {instruction:?}")
                }
                None => panic!("continuous drag emulator stopped before becoming ready"),
            }
        }
    }

    #[test]
    fn style_detail_keeps_inheritance_and_property_controls_inside_a_narrow_panel() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::SettingsAppearance);
        let style_id = id_string(parchmint_domain::StyleCatalog::body_id().as_bytes());
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        for edited in [false, true] {
            if edited {
                workspace.update(ProjectMessage::EditStyleProperty {
                    style_id: style_id.clone(),
                    property: StyleProperty::FontFamily,
                    value: "Source Sans 3".into(),
                });
            }
            let settings = workspace.settings();
            let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                Settings::default(),
                Size::new(520.0, 1_200.0),
                style_detail(settings, settings.style(&style_id).unwrap(), theme),
            );
            assert!(simulator.find("Apply").is_err());
            for label in ["Inherits from", "Font family"] {
                let bounds = simulator.find(label).unwrap().bounds();
                assert!(bounds.width > 0.0, "{label} must remain visible");
                assert!(bounds.x + bounds.width <= 520.0, "{label} is clipped");
            }
        }
    }

    #[test]
    fn rendered_settings_keep_project_managers_out_of_navigation() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::SettingsAppearance);
        interact(&workspace, RibbonDestination::Settings, |settings| {
            assert!(settings.find("Settings").is_ok());
            assert!(settings.find("Appearance").is_ok());
            assert!(settings.find("Dictionaries").is_ok());
            assert!(settings.find("Styles").is_err());
            assert!(settings.find("Metadata fields").is_err());
        });
    }

    #[test]
    fn rendered_synopsis_first_edit_retains_a_newer_local_draft_during_reconcile() {
        let node = NodeId::from_bytes([0x51; 16]);
        let document = DocumentId::from_bytes([0x52; 16]);
        let mut project = Project::new(ProjectId::from_bytes([0x50; 16]));
        project
            .nodes
            .try_insert_document(node, document, NodeId::manuscript_root(), 0, "Chapter One")
            .expect("test document");
        let mut snapshot = ProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        };
        let node_id = id_string(node.as_bytes());
        let mut workspace = ProjectWorkspace::from_snapshot(&snapshot);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: node_id.clone(),
            gesture: SelectionGesture::Replace,
        });

        workspace.update(ProjectMessage::BeginOutlineField {
            node_id: node_id.clone(),
            field_id: None,
        });
        let type_at_synopsis = |workspace: &ProjectWorkspace, value| {
            let theme = ParchMintTheme::new(ResolvedAppearance::Light);
            let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                Settings::default(),
                Size::new(1_440.0, 900.0),
                project_surface(
                    workspace,
                    RibbonDestination::Cards,
                    theme,
                    text("Editor").into(),
                ),
            );
            assert!(simulator.find("Metadata").is_err());
            simulator
                .click(HarnessTarget::InspectorSynopsis.id())
                .expect("synopsis input");
            assert_eq!(simulator.typewrite(value), iced::event::Status::Captured);
            simulator.into_messages().collect::<Vec<_>>()
        };

        let messages = type_at_synopsis(&workspace, "a");
        let effects = apply_project_messages(&mut workspace, messages);
        assert!(matches!(
            effects.as_slice(),
            [crate::ProjectEffect::CommitSynopsis { synopsis, .. }] if synopsis == "a"
        ));
        // iced_test remounts the widget tree between simulated input batches,
        // so its second keystroke cannot retain the live text-editor focus.
        // Apply the exact reducer action emitted by that second key before an
        // earlier persistence completion is reconciled.
        let effects = workspace.update(ProjectMessage::EditSynopsis {
            node_id: node_id.clone(),
            action: text_editor::Action::Edit(text_editor::Edit::Insert('b')),
        });
        assert!(matches!(
            effects.as_slice(),
            [crate::ProjectEffect::CommitSynopsis { synopsis, .. }] if synopsis == "ab"
        ));

        snapshot
            .project
            .nodes
            .get_mut(node)
            .expect("test node")
            .synopsis = "a".to_owned();
        workspace.reconcile_snapshot(&snapshot);
        assert_eq!(
            workspace
                .synopsis_editor(&node_id)
                .expect("local synopsis editor")
                .text(),
            "ab"
        );

        snapshot
            .project
            .nodes
            .get_mut(node)
            .expect("test node")
            .synopsis = "ab".to_owned();
        workspace.reconcile_snapshot(&snapshot);
        assert_eq!(
            workspace
                .synopsis_editor(&node_id)
                .expect("acknowledged synopsis editor")
                .text(),
            "ab"
        );
    }

    #[test]
    fn editor_center_payload_survives_project_surface_composition() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let editor_child: Element<'static, EditorCenterMessage> = button(text("Editor payload"))
            .on_press(EditorCenterMessage::SetReplaceDraft {
                pane: crate::EditorPane::Primary,
                value: "river".into(),
            })
            .into();
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &workspace,
                RibbonDestination::Editor,
                theme,
                editor_child.map(ProjectSurfaceMessage::EditorCenter),
            ),
        );
        simulator
            .click("Editor payload")
            .expect("mounted center payload action");
        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            [ProjectSurfaceMessage::EditorCenter(
                EditorCenterMessage::SetReplaceDraft {
                    pane: crate::EditorPane::Primary,
                    value: "river".into(),
                },
            )]
        );
    }

    #[test]
    fn rendered_organize_cards_and_editor_flow_share_selection_and_mutation_intent() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);

        let messages = interact(&workspace, RibbonDestination::Editor, |explorer| {
            explorer
                .click("Chapter One")
                .expect("visible hierarchy item");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());
        assert_eq!(workspace.explorer().selected_ids(), ["chapter-one"]);

        let messages = interact(&workspace, RibbonDestination::Cards, |cards| {
            assert!(cards.find("Chapter One").is_ok());
            assert!(
                cards
                    .find(iced::widget::Id::from("synopsis-chapter-one".to_owned()))
                    .is_ok()
            );
        });
        assert!(messages.is_empty());

        let messages = interact(&workspace, RibbonDestination::Editor, |explorer| {
            let target = explorer
                .find("Chapter One")
                .expect("visible hierarchy item");
            explorer.point_at(
                target
                    .visible_bounds()
                    .expect("visible hierarchy item bounds")
                    .center(),
            );
            explorer.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Right,
            ))]);
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());
        assert_eq!(workspace.hierarchy_context_menu(), Some("chapter-one"));

        let messages = interact(&workspace, RibbonDestination::Editor, |menu| {
            menu.click("Delete").expect("visible hierarchy mutation");
        });
        assert_eq!(
            apply_project_messages(&mut workspace, messages),
            [crate::ProjectEffect::DeleteHierarchy(vec![
                "chapter-one".to_owned()
            ])]
        );

        let messages = interact(&workspace, RibbonDestination::Editor, |editor| {
            editor
                .click("Chapter One")
                .expect("first visible editor navigation click");
            editor
                .click("Chapter One")
                .expect("second visible editor navigation click");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());
        assert_eq!(
            workspace
                .editor()
                .pane(crate::EditorPane::Primary)
                .active_document(),
            Some("chapter-one")
        );
    }

    #[test]
    fn global_search_exposes_every_match_and_counts_documents_outside_the_window() {
        let _motion = crate::motion::SettledMotion::new();
        crate::visual_verification::load_test_fonts();
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::ShowGlobalSearch);
        workspace.update(ProjectMessage::SetGlobalSearchQuery("river".to_owned()));
        let ticket = workspace.begin_task(crate::ProjectTask::GlobalSearch {
            generation: workspace.global_search().query_generation(),
        });
        let results = (0..120)
            .map(|index| crate::GlobalSearchResult {
                document_id: if index < 100 {
                    "chapter-one"
                } else {
                    "chapter-two"
                }
                .to_owned(),
                match_id: format!("river-{index}"),
                prefix: format!("Passage {index}: the harbor bells rang beside the "),
                matching_text: "river".to_owned(),
                suffix: "\nA path led toward the old lighthouse and the sea.".to_owned(),
                indexed_revision: workspace.project_revision(),
            })
            .collect();
        assert!(
            workspace.accept_completion(crate::ProjectTaskCompletion::for_ticket(
                ticket,
                crate::ProjectTaskPayload::SearchBatch {
                    results,
                    finished: true
                },
            ))
        );
        let messages = interact(&workspace, RibbonDestination::GlobalSearch, |search| {
            assert!(search.find("120 matches in 2 documents").is_ok());
            search
                .click(HarnessTarget::GlobalSearchMatch(2).id())
                .expect("the third match must be clickable");
        });
        assert!(
            matches!(apply_project_messages(&mut workspace, messages).as_slice(),
            [crate::ProjectEffect::NavigateSearchResult { match_id, .. }] if match_id == "river-2")
        );
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                Settings::default(),
                Size::new(1_280.0, 720.0),
                project_surface(
                    &workspace,
                    RibbonDestination::GlobalSearch,
                    theme,
                    text("Mounted editor child").into(),
                ),
            );
            simulator
                .click(HarnessTarget::GlobalSearchMatch(2).id())
                .unwrap();
            if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
                assert!(
                    simulator
                        .snapshot(&theme.iced_theme())
                        .unwrap()
                        .matches_image(
                            std::path::PathBuf::from(root)
                                .join(format!("search-results-{appearance:?}"))
                        )
                        .unwrap()
                );
            }
        }
    }

    #[test]
    fn rendered_global_replace_flow_revalidates_before_one_typed_apply_effect() {
        let _motion = crate::motion::SettledMotion::new();
        crate::visual_verification::load_test_fonts();
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);

        let messages = interact(&workspace, RibbonDestination::Editor, |explorer| {
            explorer
                .click(HarnessTarget::ExplorerSearch.id())
                .expect("visible global search control");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());

        let messages = interact(&workspace, RibbonDestination::GlobalSearch, |search| {
            search
                .click(global_search_query_input_id())
                .expect("search query input");
            assert_ne!(search.typewrite("river"), iced::event::Status::Ignored);
        });
        let search_effects = apply_project_messages(&mut workspace, messages);
        assert!(matches!(
            search_effects.last(),
            Some(crate::ProjectEffect::SearchProject { query, .. }) if query == "river"
        ));

        interact(&workspace, RibbonDestination::GlobalSearch, |search| {
            assert!(search.find("Searching…").is_ok());
            assert!(search.find("0 matches in 0 documents").is_err());
        });

        let search_ticket = workspace.begin_task(crate::ProjectTask::GlobalSearch {
            generation: workspace.global_search().query_generation(),
        });
        assert!(
            workspace.accept_completion(crate::ProjectTaskCompletion::for_ticket(
                search_ticket,
                crate::ProjectTaskPayload::SearchBatch {
                    results: vec![crate::GlobalSearchResult {
                        document_id: "chapter-one".to_owned(),
                        match_id: format!(
                            "chapter-one:0:Body:0:5:{}",
                            workspace.project_revision()
                        ),
                        prefix: "beside the ".to_owned(),
                        matching_text: "river".to_owned(),
                        suffix: ", the path".to_owned(),
                        indexed_revision: workspace.project_revision(),
                    }],
                    finished: true,
                },
            ))
        );

        workspace.update(ProjectMessage::ToggleGlobalReplace);
        let messages = interact(&workspace, RibbonDestination::GlobalSearch, |search| {
            search
                .click(global_replacement_input_id())
                .expect("replacement input");
            assert_ne!(search.typewrite("shore"), iced::event::Status::Ignored);
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());

        let messages = interact(&workspace, RibbonDestination::GlobalSearch, |search| {
            search
                .click("Review 1 replacement")
                .expect("visible replacement preview action");
        });
        assert!(matches!(
            apply_project_messages(&mut workspace, messages).as_slice(),
            [crate::ProjectEffect::BuildReplacementPreview { replacement, .. }] if replacement == "shore"
        ));
        let preview_ticket = workspace.begin_task(crate::ProjectTask::ReplacementPreview);
        assert!(
            workspace.accept_completion(crate::ProjectTaskCompletion::for_ticket(
                preview_ticket,
                crate::ProjectTaskPayload::ReplacementPreviewReady,
            ))
        );

        let messages = interact(&workspace, RibbonDestination::Editor, |preview| {
            assert!(preview.find("Chapter One").is_ok());
            assert!(preview.find("chapter-one").is_err());
            assert!(preview.find(HarnessTarget::InspectorTitle.id()).is_err());
            assert!(preview.find("Select all").is_err());
            assert!(preview.find("Select none").is_err());
            assert!(preview.find("Refresh preview").is_err());
            if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
                assert!(
                    preview
                        .snapshot(&ParchMintTheme::new(ResolvedAppearance::Light).iced_theme())
                        .unwrap()
                        .matches_image(std::path::PathBuf::from(root).join("replacement-review"))
                        .unwrap()
                );
            }
            preview
                .click("All matches (1)")
                .expect("visible preview selection control");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());
        let messages = interact(&workspace, RibbonDestination::Editor, |preview| {
            preview
                .click("All matches (0)")
                .expect("visible preview selection control");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());

        workspace.update(ProjectMessage::MarkDirty(2));
        let messages = interact(&workspace, RibbonDestination::Editor, |preview| {
            assert!(
                preview
                    .find("The project or selection changed. Refresh the preview before applying.")
                    .is_ok()
            );
            preview
                .click("Refresh preview")
                .expect("visible stale-preview revalidation action");
        });
        assert!(matches!(
            apply_project_messages(&mut workspace, messages).as_slice(),
            [crate::ProjectEffect::BuildReplacementPreview {
                captured_project_revision: 2,
                replacement,
                ..
            }] if replacement == "shore"
        ));
        let revalidation_ticket = workspace.begin_task(crate::ProjectTask::ReplacementPreview);
        assert!(
            workspace.accept_completion(crate::ProjectTaskCompletion::for_ticket(
                revalidation_ticket,
                crate::ProjectTaskPayload::ReplacementPreviewReady,
            ))
        );

        let messages = interact(&workspace, RibbonDestination::Editor, |preview| {
            preview
                .click("Apply replacement")
                .expect("visible apply replacement action");
        });
        assert!(matches!(
            apply_project_messages(&mut workspace, messages).as_slice(),
            [crate::ProjectEffect::ApplyGlobalReplacement {
                captured_project_revision: 2,
                included_match_ids,
                replacement,
            }] if included_match_ids == &["chapter-one:0:Body:0:5:1"] && replacement == "shore"
        ));
    }

    #[test]
    fn rendered_history_restore_confirms_before_emitting_its_project_effect() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::History);

        let messages = interact(&workspace, RibbonDestination::History, |history| {
            history.click("Draft Two").expect("visible checkpoint");
        });
        assert_eq!(
            apply_project_messages(&mut workspace, messages),
            [crate::ProjectEffect::PreviewHistory(
                "snapshot-draft-two".to_owned()
            )]
        );
        assert_eq!(
            workspace.history().selected_checkpoint_id(),
            Some("snapshot-draft-two")
        );

        let messages = interact(&workspace, RibbonDestination::History, |history| {
            history
                .click("Restore project…")
                .expect("visible restore action");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());
        assert!(matches!(
            workspace.modal(),
            Some(ProjectModal::HistoryRestore { .. })
        ));

        let messages = interact(&workspace, RibbonDestination::History, |confirmation| {
            assert!(
                confirmation
                    .find("Replace the entire project with “Draft Two”?")
                    .is_ok()
            );
            confirmation
                .click("Restore project")
                .expect("visible restore confirmation");
        });
        assert_eq!(
            apply_project_messages(&mut workspace, messages),
            [crate::ProjectEffect::RestoreHistory {
                checkpoint_id: "snapshot-draft-two".to_owned(),
                scope: crate::HistoryRestoreScope::EntireProject,
            }]
        );
        interact(&workspace, RibbonDestination::History, |busy| {
            assert!(busy.find("Restoring project…").is_ok());
            assert!(busy.find("Cancel").is_err());
            assert!(busy.find("Restore project").is_err());
        });
    }

    #[test]
    fn inspector_does_not_render_comment_mutation_controls() {
        let (mut workspace, ids) = production_workspace();
        workspace.update(ProjectMessage::ToggleHierarchyExpanded(ids.group));
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: ids.live_node,
            gesture: SelectionGesture::Replace,
        });
        let messages = interact(&workspace, RibbonDestination::Editor, |editor| {
            assert!(editor.find(HarnessTarget::CommentDraft.id()).is_err());
            assert!(editor.find("Add to document").is_err());
        });
        assert!(messages.is_empty());
    }

    #[test]
    fn rendered_recovery_acceptance_transitions_back_to_the_workspace() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::ErrorRecovery);

        let messages = interact(&workspace, RibbonDestination::Editor, |recovery| {
            recovery
                .click("Recover changes")
                .expect("visible recovery action");
        });
        assert_eq!(
            apply_project_messages(&mut workspace, messages),
            [crate::ProjectEffect::FocusRecoveredEditor]
        );
        assert!(workspace.recovery().is_resolving());

        let ticket = workspace.begin_task(crate::ProjectTask::AcceptRecovery);
        assert!(
            workspace.accept_completion(crate::ProjectTaskCompletion::for_ticket(
                ticket,
                crate::ProjectTaskPayload::RecoveryAccepted { revision: 2 },
            ))
        );
        assert_eq!(workspace.content_state(), &ContentState::Ready);
        let messages = interact(&workspace, RibbonDestination::Editor, |editor| {
            assert!(editor.find("Explorer").is_ok());
            assert!(editor.find("Unsaved changes found").is_err());
        });
        assert!(messages.is_empty());
    }

    fn interact(
        workspace: &ProjectWorkspace,
        destination: RibbonDestination,
        interaction: impl FnOnce(&mut Simulator<'_, ProjectSurfaceMessage>),
    ) -> Vec<ProjectSurfaceMessage> {
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                workspace,
                destination,
                theme,
                text("Mounted editor child").into(),
            ),
        );
        interaction(&mut simulator);
        simulator.into_messages().collect()
    }

    fn apply_project_messages(
        workspace: &mut ProjectWorkspace,
        messages: Vec<ProjectSurfaceMessage>,
    ) -> Vec<crate::ProjectEffect> {
        messages
            .into_iter()
            .flat_map(|message| match message {
                ProjectSurfaceMessage::Project(message) => workspace.update(message),
                unexpected => {
                    panic!("project flow emitted unexpected surface message: {unexpected:?}")
                }
            })
            .collect()
    }

    struct ProductionIds {
        group: String,
        live_node: String,
        live_document: String,
        deleted_node: String,
    }

    fn production_workspace() -> (ProjectWorkspace, ProductionIds) {
        let group = NodeId::from_bytes([3; 16]);
        let live_node = NodeId::from_bytes([4; 16]);
        let live_document = DocumentId::from_bytes([5; 16]);
        let deleted_group = NodeId::from_bytes([6; 16]);
        let deleted_node = NodeId::from_bytes([7; 16]);
        let deleted_document = DocumentId::from_bytes([8; 16]);
        let field = MetadataFieldId::from_bytes([9; 16]);
        let mut project = Project::new(ProjectId::from_bytes([1; 16]));
        project
            .metadata
            .upsert(MetadataFieldDefinition {
                id: field,
                label: "Status".into(),
                description: Some("Draft state".into()),
                applicability: MetadataApplicability::Documents,
                text_kind: MetadataTextKind::SingleLine,
                default_value: Some("Draft".into()),
                visible_on_cards: true,
            })
            .unwrap();
        project
            .nodes
            .try_insert_group(group, NodeId::manuscript_root(), 0, "Part One")
            .unwrap();
        project
            .nodes
            .try_insert_document(live_node, live_document, group, 0, "Opening Scene")
            .unwrap();
        project
            .nodes
            .try_insert_group(
                deleted_group,
                NodeId::manuscript_root(),
                1,
                "Discarded Part",
            )
            .unwrap();
        project
            .nodes
            .try_insert_document(
                deleted_node,
                deleted_document,
                deleted_group,
                0,
                "Discarded Scene",
            )
            .unwrap();
        let live = project.nodes.get_mut(live_node).unwrap();
        live.synopsis = "The opening synopsis.".into();
        live.metadata.insert(field, "Final".into());
        let project = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::delete_node_at(deleted_group, 123),
        )
        .unwrap()
        .project;
        let snapshot = ProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents: vec![
                DocumentSnapshot {
                    comments: Vec::new(),
                    document_id: live_document,
                    body: "one two three".into(),
                    revision: EditorRevision::from(3),
                    visibility: DocumentVisibility::Open,
                },
                DocumentSnapshot {
                    comments: Vec::new(),
                    document_id: deleted_document,
                    body: "<p data-block-id=\"08080808080808080808080808080808\">formatted deleted preview</p>".into(),
                    revision: EditorRevision::from(2),
                    visibility: DocumentVisibility::Closed,
                },
            ],
            styles_css: String::new(),
        };
        (
            ProjectWorkspace::from_snapshot(&snapshot),
            ProductionIds {
                group: id_string(group.as_bytes()),
                live_node: id_string(live_node.as_bytes()),
                live_document: id_string(live_document.as_bytes()),
                deleted_node: id_string(deleted_group.as_bytes()),
            },
        )
    }

    fn id_string(bytes: &[u8; 16]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
