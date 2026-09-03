//! Private Iced composition for production and deterministic project workspaces.

use iced::widget::{
    Space, button, checkbox, column, container, mouse_area, opaque, rich_text, row, scrollable,
    sensor, span, stack, text, text_editor, text_input,
};
use iced::{Background, Border, Color, Element, Font, Length, Theme, border, font};
use parchmint_editor_api::{SemanticBlock, SemanticBlockKind, SemanticInlineMark};
use parchmint_ui_api::HistoryMaintenanceStatus;
use std::collections::BTreeMap;

use crate::{
    CARDS_CARD_CONTENT_HEIGHT, CARDS_DROP_STRIP_HEIGHT, CommentAnchor, ContentState,
    DragDestination, EditorMessage, EditorPane, F6Region, FocusTarget, HarnessTarget,
    HierarchyItemKind, HierarchyRowKind, InspectorSection, MetadataFieldApplicability,
    MetadataFieldTextKind, Point, ProjectFixture, ProjectMessage, ProjectModal, ProjectWorkspace,
    ReplacementCheckState, ReplacementPreviewRowKind, RestoreLocation, RibbonDestination,
    SaveState, SelectionGesture, SettingsCategory, SettingsDetail, ShellLayout, SidebarSurface,
    StatusCount, StyleProperty,
    components::{self, ButtonKind, Interaction, Surface},
    design_tokens::{
        ParchMintTheme, RIBBON_HEIGHT, SPACING_4, SPACING_8, SPACING_12, SPACING_16, SPACING_24,
        SPACING_32, STATUS_HEIGHT, UI_BODY, UI_COMPACT, UI_HEADING, UI_LABEL, UI_PAGE_TITLE,
        UI_TAB,
    },
    focus, harness_target, hierarchy_drag,
    iced_editor_surface::EditorCenterMessage,
    icons::{Icon, icon, icon_sized},
    right_click, stationary_tooltip,
};

/// Typed output from the reusable project surface. The native integrator maps
/// these directly to its existing project, editor, and shell reducers.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectSurfaceMessage {
    Project(ProjectMessage),
    EditorCenter(EditorCenterMessage),
    Navigate(RibbonDestination),
    Focus(FocusTarget),
    ToggleExplorer,
    ToggleInspector,
    ToggleFocusedPane,
    ToggleInspectorSection(InspectorSection),
    BeginResize(SidebarPanel),
    LoadMoreHistory,
    /// The newly-created Explorer rename field has entered the rendered tree.
    HierarchyRenameShown(String),
    /// The transient metadata-field name control is ready to receive typing.
    MetadataFieldCreationShown,
    /// The Inspector's intentionally quiet title was explicitly activated for
    /// renaming and can now safely receive focus.
    InspectorRenameShown(String),
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

pub(crate) fn inspector_title_input_id() -> iced::widget::Id {
    HarnessTarget::InspectorTitle.id()
}

// Divider lines belong to the adjacent sidebar's reference width; they must
// not shrink the manuscript allocation between the 280 px Explorer and 320 px
// Inspector columns.
const SIDEBAR_SPLITTER_WIDTH: u32 = 1;
const HIERARCHY_CONTEXT_MENU_WIDTH: f32 = 168.0;

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
    let ribbon = ribbon(project_title, destination, theme);
    let center = center_view(workspace, destination, theme, editor_child);
    // Cards shares the persistent authoring shell from its reference board;
    // the remaining project destinations retain their dedicated full-area
    // compositions.
    let recovering = matches!(workspace.content_state(), ContentState::Recovery);
    let shows_explorer = !recovering
        && matches!(
            destination,
            RibbonDestination::Editor | RibbonDestination::Cards | RibbonDestination::GlobalSearch
        );
    let shows_inspector = !recovering
        && matches!(
            destination,
            RibbonDestination::Editor | RibbonDestination::Cards | RibbonDestination::GlobalSearch
        );
    let shows_status = recovering
        || matches!(
            destination,
            RibbonDestination::Editor | RibbonDestination::Cards
        );
    let mut body = row![].height(Length::Fill);
    if shows_explorer && layout.explorer_is_visible() {
        let rail_width = if destination == RibbonDestination::GlobalSearch {
            360
        } else {
            layout.explorer_width()
        };
        body = body.push(left_rail(
            workspace,
            theme,
            rail_width.saturating_sub(SIDEBAR_SPLITTER_WIDTH),
        ));
        body = body.push(sidebar_splitter(SidebarPanel::Explorer, theme));
    }
    body = body.push(center);
    if shows_inspector && layout.inspector_is_visible() {
        body = body.push(sidebar_splitter(SidebarPanel::Inspector, theme));
        body = body.push(inspector(
            workspace,
            theme,
            layout
                .inspector_width()
                .saturating_sub(SIDEBAR_SPLITTER_WIDTH),
            inspector_expansion,
            false,
        ));
    }
    let mut content = column![ribbon, body]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill);
    if shows_status {
        content = content.push(if recovering {
            recovery_status_bar(workspace, theme)
        } else {
            status_bar(
                workspace,
                theme,
                shows_explorer && layout.explorer_is_visible(),
                shows_inspector && layout.inspector_is_visible(),
            )
        });
    } else if let Some(footer) = destination_footer(destination, theme) {
        content = content.push(footer);
    }
    let base: Element<'a, ProjectSurfaceMessage> = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Application, Interaction::Rest))
        .into();
    // Context menus belong to the project surface, not the Explorer rail.
    // This lets a menu extend over the editor when there is room, and keeps
    // its anchor in the same window coordinate space as the right-click.
    let base = hierarchy_context_overlay(workspace, base, theme, layout.requested_width() as f32);
    if matches!(workspace.content_state(), ContentState::Recovery) {
        stack![
            base,
            opaque(
                container(recovery_modal(workspace, theme))
                    // The recovery sheet sits slightly above the midpoint in the
                    // reference chrome, leaving room for the status silhouette.
                    .padding(iced::Padding {
                        top: 0.0,
                        right: 0.0,
                        bottom: 80.0,
                        left: 0.0,
                    })
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
        stack![
            base,
            opaque(
                container(modal_view(modal, theme))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(|_| iced::widget::container::Style {
                        background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.55))),
                        ..Default::default()
                    })
            )
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        base
    }
}

fn destination_footer(
    destination: RibbonDestination,
    theme: ParchMintTheme,
) -> Option<Element<'static, ProjectSurfaceMessage>> {
    let label = match destination {
        RibbonDestination::History => "Earlier project versions are saved here automatically.",
        RibbonDestination::RecentlyDeleted => "Deleted items remain recoverable through History.",
        _ => return None,
    };
    Some(
        container(text(label).size(12))
            .padding([7, 12])
            .width(Length::Fill)
            .height(Length::Fixed(f32::from(STATUS_HEIGHT)))
            .style(move |_| components::surface(theme, Surface::Status, Interaction::Rest))
            .into(),
    )
}

fn sidebar_splitter(
    panel: SidebarPanel,
    theme: ParchMintTheme,
) -> Element<'static, ProjectSurfaceMessage> {
    mouse_area(
        container(Space::new().width(1).height(Length::Fill))
            .width(SIDEBAR_SPLITTER_WIDTH)
            .height(Length::Fill)
            .style(move |_| iced::widget::container::Style {
                background: Some(Background::Color(theme.palette().divider)),
                ..Default::default()
            }),
    )
    .on_press(ProjectSurfaceMessage::BeginResize(panel))
    .interaction(iced::mouse::Interaction::ResizingHorizontally)
    .into()
}

fn ribbon<'a>(
    project_title: &'a str,
    destination: RibbonDestination,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let mode_switch = [
        ("Editor", RibbonDestination::Editor),
        ("Cards", RibbonDestination::Cards),
    ]
    .into_iter()
    .fold(row![].spacing(0), |modes, (label, item)| {
        let selected = matches!(
            (destination, item),
            (
                RibbonDestination::Editor | RibbonDestination::GlobalSearch,
                RibbonDestination::Editor
            ) | (RibbonDestination::Cards, RibbonDestination::Cards)
        );
        modes.push(harness_target::target(
            HarnessTarget::Ribbon(item),
            button(text(label).size(12))
                .padding([6, 12])
                .height(32)
                .on_press(ProjectSurfaceMessage::Navigate(item))
                .style(move |_, status| mode_switch_button_style(theme, status, selected)),
        ))
    });
    let mode_switch = focus::f6_region(
        F6Region::ModeSwitch,
        container(mode_switch)
            .style(move |_| mode_switch_container_style(theme))
            .padding(1),
    );

    let destinations = [
        (Icon::History, "History", RibbonDestination::History),
        (
            Icon::RecentlyDeleted,
            "Recently Deleted",
            RibbonDestination::RecentlyDeleted,
        ),
        (Icon::Export, "Export", RibbonDestination::Export),
        (Icon::Settings, "Settings", RibbonDestination::Settings),
    ];
    let utilities =
        destinations
            .into_iter()
            .fold(row![].spacing(4), |row, (icon_kind, label, item)| {
                let selected = item == destination;
                let control: Element<'a, ProjectSurfaceMessage> = harness_target::target(
                    HarnessTarget::Ribbon(item),
                    column![
                        button(icon(icon_kind))
                            .width(42)
                            .height(38)
                            .padding([5, 0])
                            .on_press(ProjectSurfaceMessage::Navigate(item))
                            .style(move |_, status| flat_selection_button_style(
                                theme, status, selected
                            )),
                        container(Space::new().height(2))
                            .width(42)
                            .height(2)
                            .style(move |_| ribbon_indicator_style(theme, selected)),
                    ]
                    .spacing(0),
                );
                row.push(stationary_tooltip::tooltip(
                    control,
                    container(text(label).size(12)).padding([4, 6]),
                    components::surface(theme, Surface::Elevated, Interaction::Rest),
                ))
            });
    let title = row![icon(Icon::Project), text(project_title).size(16),]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center)
        .width(230);
    container(
        row![
            title,
            mode_switch,
            Space::new().width(Length::Fill),
            utilities,
        ]
        .spacing(16)
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding([6, 18])
    .width(Length::Fill)
    .height(Length::Fixed(f32::from(RIBBON_HEIGHT)))
    .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest))
    .into()
}

fn mode_switch_container_style(theme: ParchMintTheme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(theme.palette().application)),
        border: Border {
            color: theme.palette().border,
            width: 1.0,
            radius: 5.0.into(),
        },
        ..Default::default()
    }
}

fn mode_switch_button_style(
    theme: ParchMintTheme,
    status: iced::widget::button::Status,
    selected: bool,
) -> iced::widget::button::Style {
    let mut style = components::button_style(theme, ButtonKind::Quiet, interaction(status, false));
    if selected {
        style.background = Some(Background::Color(theme.palette().accent_subtle));
        style.text_color = theme.palette().accent;
        style.border = Border {
            color: theme.palette().accent,
            width: 1.0,
            radius: 3.0.into(),
        };
    }
    style
}

fn left_rail<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
    width: u32,
) -> Element<'a, ProjectSurfaceMessage> {
    let content = match workspace.sidebar_surface() {
        SidebarSurface::Explorer => explorer_rail(workspace, theme),
        SidebarSurface::GlobalSearch => global_search_rail(workspace, theme),
    };
    container(content)
        .padding(SPACING_12)
        .width(width)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest))
        .into()
}

fn explorer_rail<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let explorer = workspace.explorer();
    let rows = explorer
        .rows()
        .into_iter()
        .filter(|item| hierarchy_row_is_visible(explorer, item.parent_id))
        .fold(column![].spacing(SPACING_4), |column, item| {
            let depth = hierarchy_depth(explorer, item.parent_id);
            let disclosure: Element<'a, ProjectSurfaceMessage> = match item.kind {
                HierarchyRowKind::Root => button(icon_sized(
                    if item.expanded {
                        Icon::ExplorerFolderOpen
                    } else {
                        Icon::ExplorerFolderClosed
                    },
                    16,
                ))
                .padding(SPACING_4)
                .width(20)
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::ToggleHierarchyExpanded(item.id.to_owned()),
                ))
                .style(move |_, status| {
                    components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
                })
                .into(),
                HierarchyRowKind::Group => container(icon_sized(
                    if item.expanded {
                        Icon::ExplorerFolderOpen
                    } else {
                        Icon::ExplorerFolderClosed
                    },
                    16,
                ))
                .width(20)
                .align_x(iced::alignment::Horizontal::Center)
                .into(),
                HierarchyRowKind::Document => Space::new().width(20).into(),
            };
            let title = if item.cut_pending {
                format!("{}  (cut)", item.title)
            } else {
                item.title.to_owned()
            };
            let is_renaming = workspace
                .hierarchy_rename()
                .is_some_and(|(node_id, _)| node_id == item.id);
            let hierarchy_press = (!is_renaming
                && matches!(
                    item.kind,
                    HierarchyRowKind::Group | HierarchyRowKind::Document
                ))
            .then(|| match item.kind {
                HierarchyRowKind::Document => ProjectSurfaceMessage::Project(
                    ProjectMessage::PreviewHierarchyNode(item.id.to_owned()),
                ),
                HierarchyRowKind::Group => ProjectSurfaceMessage::Project(
                    ProjectMessage::SelectAndToggleHierarchyExpanded(item.id.to_owned()),
                ),
                HierarchyRowKind::Root => unreachable!("roots retain their disclosure control"),
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
                                ProjectSurfaceMessage::Project(
                                    ProjectMessage::SetHierarchyRenameDraft(title),
                                )
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
                                    background: Some(Background::Color(
                                        theme.palette().accent_subtle,
                                    )),
                                    ..Default::default()
                                }
                            } else {
                                iced::widget::container::Style::default()
                            }
                        }),
                )
                .interaction(iced::mouse::Interaction::Pointer);
                if item.kind == HierarchyRowKind::Root {
                    hierarchy_drag::source(
                        row,
                        ProjectSurfaceMessage::Project(ProjectMessage::SelectHierarchy {
                            node_id: item.id.to_owned(),
                            gesture: SelectionGesture::Replace,
                        }),
                        None,
                        ProjectSurfaceMessage::Project(ProjectMessage::BeginHierarchyDrag {
                            source_id: item.id.to_owned(),
                            gesture: SelectionGesture::Replace,
                        }),
                        ProjectSurfaceMessage::Project(ProjectMessage::CommitHierarchyDrag),
                    )
                } else {
                    row.into()
                }
            };
            let item_row: Element<'a, ProjectSurfaceMessage> =
                row![Space::new().width((depth * 14) as f32), disclosure, select]
                    .spacing(1)
                    .align_y(iced::alignment::Vertical::Center)
                    .into();
            let item_row = match hierarchy_press {
                Some(on_press) => hierarchy_drag::source(
                    item_row,
                    on_press,
                    (item.kind == HierarchyRowKind::Document).then(|| {
                        ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyNode(
                            item.id.to_owned(),
                        ))
                    }),
                    ProjectSurfaceMessage::Project(ProjectMessage::BeginHierarchyDrag {
                        source_id: item.id.to_owned(),
                        gesture: SelectionGesture::Replace,
                    }),
                    ProjectSurfaceMessage::Project(ProjectMessage::CommitHierarchyDrag),
                ),
                None => item_row,
            };
            let node_id = item.id.to_owned();
            let drag_destination = workspace.hierarchy_drag_destination();
            let indicator = hierarchy_row_indicator(item.kind, &node_id, drag_destination, theme);
            let kind = item.kind;
            let target_id = node_id.clone();
            let row_body = hierarchy_drag::target(
                container(item_row).width(Length::Fill).style(move |_| {
                    if indicator.is_some() {
                        components::surface(theme, Surface::Panel, Interaction::Selected)
                    } else {
                        iced::widget::container::Style::default()
                    }
                }),
                indicator,
                move |bounds, point| hierarchy_row_destination(kind, &target_id, bounds, point),
                |target| {
                    ProjectSurfaceMessage::Project(ProjectMessage::SetDragDestination(Some(target)))
                },
                |target| {
                    ProjectSurfaceMessage::Project(ProjectMessage::ClearDragDestination(target))
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
    let creation_menu: Element<'a, ProjectSurfaceMessage> = if workspace
        .explorer_creation_menu_open()
    {
        workspace
            .explorer_creation_parent_id()
            .map(|parent_id| {
                let parent_title = explorer.title(parent_id).unwrap_or("Manuscript");
                let document_parent = parent_id.to_owned();
                let group_parent = parent_id.to_owned();
                container(
                    column![
                        text(format!("Add to {parent_title}")).size(11),
                        row![
                            explorer_creation_action(
                                "Document",
                                ProjectSurfaceMessage::Project(
                                    ProjectMessage::RequestCreateHierarchy {
                                        parent_id: document_parent,
                                        kind: HierarchyItemKind::Document,
                                    },
                                ),
                                theme,
                            ),
                            explorer_creation_action(
                                "Group",
                                ProjectSurfaceMessage::Project(
                                    ProjectMessage::RequestCreateHierarchy {
                                        parent_id: group_parent,
                                        kind: HierarchyItemKind::Group,
                                    },
                                ),
                                theme,
                            ),
                        ]
                        .spacing(6),
                    ]
                    .spacing(6),
                )
                .padding(8)
                .style(move |_| components::surface(theme, Surface::Panel, Interaction::Focused))
                .into()
            })
            .unwrap_or_else(|| Space::new().height(0).into())
    } else {
        Space::new().height(0).into()
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
            text("EXPLORER").size(12),
            Space::new().width(Length::Fill),
            harness_target::target(
                HarnessTarget::ExplorerAdd,
                stationary_tooltip::tooltip(
                    button(text("+ New").size(12))
                        .padding([4, 7])
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::ToggleExplorerCreationMenu
                        ))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, workspace.explorer_creation_menu_open())
                        )),
                    container(text("Create a document or group").size(12)).padding([4, 6]),
                    components::surface(theme, Surface::Elevated, Interaction::Rest),
                ),
            ),
            harness_target::target(
                HarnessTarget::ExplorerSearch,
                stationary_tooltip::tooltip(
                    button(text("⌕").size(20))
                        .padding([2, 5])
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::ShowGlobalSearch
                        ))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false)
                        )),
                    container(text("Global Search").size(12)).padding([4, 6]),
                    components::surface(theme, Surface::Elevated, Interaction::Rest),
                )
            )
        ]
        .spacing(4),
        creation_menu,
        selection_shelf,
        scrollable(rows)
            .id(explorer_scroll_id())
            .height(Length::Fill),
    ]
    .spacing(8)
    .height(Length::Fill);
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

// Cards use transparent hover targets and one visible insertion indicator.
// This preserves a generous direct-manipulation target without drawing every
// possible drop boundary when a drag begins.
fn hierarchy_drop_strip<'a>(
    target: DragDestination,
    id: iced::widget::Id,
    dragging: bool,
    current: Option<&DragDestination>,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    if !dragging {
        return Space::new().height(CARDS_DROP_STRIP_HEIGHT).into();
    }
    let active = current == Some(&target);
    harness_target::target_id(
        id,
        mouse_area(
            container(Space::new().height(CARDS_DROP_STRIP_HEIGHT))
                .width(Length::Fill)
                .style(move |_| {
                    if active {
                        components::surface(theme, Surface::Panel, Interaction::Selected)
                    } else {
                        iced::widget::container::Style::default()
                    }
                }),
        )
        .on_enter(ProjectSurfaceMessage::Project(
            ProjectMessage::SetDragDestination(Some(target)),
        ))
        .on_release(ProjectSurfaceMessage::Project(
            ProjectMessage::CommitHierarchyDrag,
        ))
        .interaction(iced::mouse::Interaction::Grabbing),
    )
}

fn hierarchy_context_overlay<'a>(
    workspace: &'a ProjectWorkspace,
    content: Element<'a, ProjectSurfaceMessage>,
    theme: ParchMintTheme,
    window_width: f32,
) -> Element<'a, ProjectSurfaceMessage> {
    let Some(node_id) = workspace.hierarchy_context_menu() else {
        return content;
    };
    let Some(node) = workspace.explorer().row(node_id) else {
        return content;
    };
    let id = node.id.to_owned();
    let mut actions = column![].spacing(6);
    if node.kind == HierarchyRowKind::Document {
        actions = actions
            .push(context_menu_button(
                "Open",
                ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyNode(id.clone())),
                theme,
            ))
            .push(context_menu_button(
                "Open in companion",
                ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyNodeInCompanion(
                    id.clone(),
                )),
                theme,
            ));
    } else {
        actions = actions
            .push(context_menu_button(
                "Create document",
                ProjectSurfaceMessage::Project(ProjectMessage::RequestCreateHierarchy {
                    parent_id: id.clone(),
                    kind: HierarchyItemKind::Document,
                }),
                theme,
            ))
            .push(context_menu_button(
                "Create group",
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
    let top = point.y().max(0.0);
    stack![
        mouse_area(content).on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::CloseHierarchyContextMenu,
        )),
        container(opaque(
            container(actions)
                .padding(6)
                .width(HIERARCHY_CONTEXT_MENU_WIDTH)
                .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest,)),
        ))
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
    button(text(label).size(12))
        .padding([6, 8])
        .width(Length::Fill)
        .on_press(message)
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
        })
}

fn explorer_creation_action<'a>(
    label: &'static str,
    message: ProjectSurfaceMessage,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    mouse_area(
        container(text(label).size(12))
            .padding([6, 8])
            .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
    )
    .on_press(message)
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

fn comment_action<'a>(
    label: &'static str,
    message: ProjectSurfaceMessage,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    mouse_area(
        container(text(label).size(12))
            .padding([6, 8])
            .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
    )
    .on_press(message)
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

fn comment_anchor_summary(anchor: &CommentAnchor) -> String {
    match anchor {
        CommentAnchor::Range { quote, .. } => format!("“{quote}”"),
        CommentAnchor::Position { quote, .. } if quote.is_empty() => "At cursor".to_owned(),
        CommentAnchor::Position { quote, .. } => format!("At cursor · “{quote}”"),
        CommentAnchor::Document { .. } => "Document comment".to_owned(),
        CommentAnchor::Orphaned { quote, .. } => format!("Anchor needs attention · “{quote}”"),
    }
}

fn hierarchy_row_is_visible<'a>(
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

fn hierarchy_depth<'a>(
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
    let controls = row![
        stationary_tooltip::tooltip(
            button(text("←  Search").size(u32::from(UI_HEADING.size)))
                .on_press(ProjectSurfaceMessage::Project(ProjectMessage::ShowExplorer))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    interaction(status, false)
                )),
            container(text("Back to Explorer").size(12)).padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        ),
        Space::new().width(Length::Fill),
        stationary_tooltip::tooltip(
            button(
                text(if search.case_sensitive() {
                    "Aa ✓"
                } else {
                    "Aa"
                })
                .size(12)
            )
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
            button(text(if search.whole_word() { "W ✓" } else { "W" }).size(12))
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
    .spacing(SPACING_8);
    let mut grouped = BTreeMap::<&str, Vec<&crate::GlobalSearchResult>>::new();
    for result in search.windowed_results() {
        grouped
            .entry(result.document_id.as_str())
            .or_default()
            .push(result);
    }
    let document_count = grouped.len();
    let results = grouped.into_iter().fold(
        column![].spacing(SPACING_8),
        |column, (document_id, matches)| {
            let title = workspace
                .explorer()
                .title_for_document(document_id)
                .unwrap_or(document_id);
            let match_count = matches.len();
            let rows = matches
                .into_iter()
                .take(2)
                .fold(column![].spacing(3), |rows, result| {
                    let active = search.active_match_id() == Some(result.match_id.as_str());
                    let highlight = if active {
                        theme.palette().search_match_active
                    } else {
                        theme.palette().search_match
                    };
                    let snippet_spans: Vec<iced::widget::text::Span<'a>> = vec![
                        span(result.prefix.as_str()),
                        span(result.matching_text.as_str())
                            .font(Font {
                                weight: font::Weight::Bold,
                                ..Font::default()
                            })
                            .background(highlight),
                        span(result.suffix.as_str()),
                    ];
                    let snippet = container(rich_text(snippet_spans).size(12))
                        .padding([2, 3])
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
                    rows.push(
                        button(snippet)
                            .padding([5, 6])
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
                });
            column.push(
                column![
                    row![
                        text(title).size(u32::from(UI_LABEL.size)),
                        Space::new().width(Length::Fill),
                        text(search_match_count_label(match_count))
                            .size(u32::from(UI_COMPACT.size))
                            .color(theme.palette().secondary_text),
                    ]
                    .align_y(iced::alignment::Vertical::Center),
                    rows,
                ]
                .spacing(3),
            )
        },
    );
    let result_count = search.results().len();
    let results = if search.results().is_empty() {
        results.push(
            text(if search.query().is_empty() {
                "Type to search every project document."
            } else {
                "No matches yet."
            })
            .size(12),
        )
    } else {
        column![
            Space::new().height(search.result_window_start() as f32 * 44.0),
            results,
            Space::new().height(search.result_window_bottom_padding()),
        ]
    };
    let replace = text_input("Replace with (optional)", search.replacement())
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
            text(if result_count == 1 {
                "Review 1 replacement".to_owned()
            } else {
                format!("Review {result_count} replacements")
            })
            .size(13),
        )
        .on_press_maybe(
            (search.is_complete() && !search.results().is_empty()).then_some(
                ProjectSurfaceMessage::Project(ProjectMessage::OpenReplacementPreview),
            ),
        )
        .padding([7, 14])
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Primary, interaction(status, false))
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
            text(format!(
                "{} · {} of {} · results stay open here",
                search.query(),
                index + 1,
                result_count,
            ))
            .size(11)
            .color(theme.palette().secondary_text)
            .into()
        })
        .unwrap_or_else(|| Space::new().height(0).into());
    column![
        controls,
        query,
        row![replace, replace_action].spacing(8),
        text("Searches titles, synopsis, and metadata; review changes document body text only.")
            .size(11)
            .color(theme.palette().secondary_text),
        text(global_search_result_count_label(
            result_count,
            document_count
        ))
        .size(12),
        active_trail,
        scrollable(results)
            .on_scroll(|viewport| ProjectSurfaceMessage::Project(
                ProjectMessage::SetGlobalSearchScroll(viewport.absolute_offset().y)
            ))
            .height(Length::Fill)
    ]
    .spacing(SPACING_12)
    .height(Length::Fill)
    .into()
}

fn global_search_query_input_id() -> iced::widget::Id {
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
) -> Element<'a, ProjectSurfaceMessage> {
    let content = match workspace.content_state() {
        ContentState::Loading => {
            state_center("Loading project", "Project content is loading.", theme)
        }
        ContentState::Empty => state_center(
            "No content",
            "Create a manuscript section from Explorer.",
            theme,
        ),
        ContentState::Error(error) => state_center("Project needs attention", error, theme),
        ContentState::Recovery => recovery_backdrop(workspace, theme),
        ContentState::Ready => match destination {
            RibbonDestination::Editor if workspace.replacement_preview().uses_middle_pane() => {
                search_center(workspace, theme)
            }
            RibbonDestination::Editor => editor_child,
            RibbonDestination::Cards => cards_center(workspace, theme),
            RibbonDestination::GlobalSearch => editor_child,
            RibbonDestination::History => history_center(workspace, theme),
            RibbonDestination::RecentlyDeleted => deleted_center(workspace, theme),
            RibbonDestination::Export => export_center(workspace, theme),
            RibbonDestination::Settings => settings_center(workspace, theme),
        },
    };
    // Each destination owns its own reference composition. Applying an outer
    // frame here made History, Deleted, Settings, and Export look like cards
    // inside the workspace instead of full-area application screens.
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

fn cards_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let cards = workspace.cards();
    let section_title = workspace
        .explorer()
        .title(cards.section_id())
        .unwrap_or("Project");
    let sections = workspace.explorer().root_ids().into_iter().fold(
        row![].spacing(SPACING_12),
        |sections, section_id| {
            let title = workspace
                .explorer()
                .title(section_id)
                .unwrap_or("Section")
                .to_owned();
            let selected = section_id == cards.section_id();
            let section_id = section_id.to_owned();
            sections.push(
                button(
                    text(title)
                        .size(u32::from(UI_TAB.size))
                        .line_height(UI_TAB.line_height),
                )
                .padding(0)
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::SetCardsSection(section_id),
                ))
                .style(move |_, status| text_tab_button_style(theme, status, selected)),
            )
        },
    );
    let drag_source = workspace.hierarchy_drag_source().map(str::to_owned);
    let drag_destination = workspace.hierarchy_drag_destination().cloned();
    let card_window = cards.item_window();
    let items = cards
        .windowed_items()
        .into_iter()
        .fold(column![].spacing(0), |column, item| {
            // Cards retain one stable row extent, but each configured field
            // remains a separate, labelled chip. Colour accelerates scanning;
            // the label and value continue to carry the meaning.
            let metadata = item.metadata.into_iter().fold(
                row![].spacing(SPACING_4),
                |metadata, (_, label, value)| {
                    metadata.push(card_metadata_chip(label, value, theme))
                },
            );
            let node_id = item.node_id.to_owned();
            let disclosure: Element<'a, ProjectSurfaceMessage> = match item.kind {
                HierarchyRowKind::Group => container(icon_sized(
                    if item.expanded {
                        Icon::ExplorerFolderOpen
                    } else {
                        Icon::ExplorerFolderClosed
                    },
                    16,
                ))
                .width(28)
                .align_x(iced::alignment::Horizontal::Center)
                .into(),
                _ => Space::new().width(28).into(),
            };
            let group = item.kind == HierarchyRowKind::Group;
            let title = compact_card_projection(item.title, 72);
            let synopsis = compact_card_projection(item.synopsis, 104);
            let card_content = column![
                row![
                    disclosure,
                    text(title)
                        .size(if group {
                            u32::from(UI_HEADING.size)
                        } else {
                            u32::from(UI_BODY.size)
                        })
                        .font(Font {
                            weight: if group {
                                font::Weight::Bold
                            } else if item.selected {
                                font::Weight::Semibold
                            } else {
                                font::Weight::Normal
                            },
                            ..Font::DEFAULT
                        })
                        .width(Length::Fill),
                ]
                .align_y(iced::alignment::Vertical::Center),
                text(synopsis)
                    .size(u32::from(UI_BODY.size))
                    .color(theme.palette().secondary_text)
                    .wrapping(iced::widget::text::Wrapping::None),
                metadata,
            ]
            .spacing(SPACING_4);
            let before = DragDestination::BeforeSibling(node_id.clone());
            let after = DragDestination::AfterSibling(node_id.clone());
            let middle = if item.kind == HierarchyRowKind::Group {
                DragDestination::IntoGroup(node_id.clone())
            } else {
                after.clone()
            };
            let middle_active = drag_destination.as_ref() == Some(&middle);
            let source_active = drag_source.as_deref() == Some(node_id.as_str());
            let drag_state: Element<'a, ProjectSurfaceMessage> = if source_active {
                text("Moving").size(11).into()
            } else {
                Space::new().width(0).into()
            };
            let drop_state: Element<'a, ProjectSurfaceMessage> = if middle_active {
                text(if group { "Drop inside" } else { "Drop after" })
                    .size(11)
                    .into()
            } else {
                Space::new().height(0).into()
            };
            let card_body = mouse_area(row![
                Space::new().width((item.depth * 18) as f32),
                container(
                    column![
                        row![card_content, drag_state].align_y(iced::alignment::Vertical::Center),
                        drop_state,
                    ]
                    .spacing(if middle_active { SPACING_4 } else { 0.0 })
                )
                .padding(if group {
                    [SPACING_12, SPACING_16]
                } else {
                    [SPACING_8, SPACING_16]
                })
                .height(Length::Fixed(CARDS_CARD_CONTENT_HEIGHT))
                .width(Length::Fill)
                .style(move |_| {
                    let mut style =
                        components::surface(theme, Surface::Elevated, Interaction::Rest);
                    if source_active || middle_active || item.selected {
                        style.background = Some(Background::Color(theme.palette().accent_subtle));
                        style.border = Border {
                            color: if item.selected {
                                theme.palette().focus_ring
                            } else {
                                theme.palette().accent
                            },
                            width: 1.0,
                            radius: 4.0.into(),
                        };
                    }
                    style
                }),
            ])
            .on_enter(ProjectSurfaceMessage::Project(
                ProjectMessage::SetDragDestination(Some(middle)),
            ));
            // Cards deliberately share Explorer's group interaction: a group
            // is a structural disclosure, so one click expands or collapses
            // it. Documents keep a selection click and activate on double
            // click, leaving Inspector as the single editing surface.
            let press = match item.kind {
                HierarchyRowKind::Group | HierarchyRowKind::Root => ProjectSurfaceMessage::Project(
                    ProjectMessage::SelectAndToggleHierarchyExpanded(node_id.clone()),
                ),
                HierarchyRowKind::Document => {
                    ProjectSurfaceMessage::Project(ProjectMessage::SelectHierarchy {
                        node_id: node_id.clone(),
                        gesture: SelectionGesture::Replace,
                    })
                }
            };
            let card: Element<'a, ProjectSurfaceMessage> = hierarchy_drag::source(
                card_body,
                press,
                (item.kind == HierarchyRowKind::Document).then(|| {
                    ProjectSurfaceMessage::Project(ProjectMessage::ActivateCard(node_id.clone()))
                }),
                ProjectSurfaceMessage::Project(ProjectMessage::BeginHierarchyDrag {
                    source_id: node_id.clone(),
                    gesture: SelectionGesture::Replace,
                }),
                ProjectSurfaceMessage::Project(ProjectMessage::CommitHierarchyDrag),
            );
            let card = harness_target::target_id(harness_target::card_id(&node_id), card);
            column
                .push(hierarchy_drop_strip(
                    before,
                    harness_target::card_drop_before_id(&node_id),
                    drag_source.is_some(),
                    drag_destination.as_ref(),
                    theme,
                ))
                .push(card)
                .push(
                    container(Space::new().height(1))
                        .width(Length::Fill)
                        .style(move |_| iced::widget::container::Style {
                            background: Some(Background::Color(theme.palette().divider)),
                            ..Default::default()
                        }),
                )
                .push(hierarchy_drop_strip(
                    after,
                    harness_target::card_drop_after_id(&node_id),
                    drag_source.is_some(),
                    drag_destination.as_ref(),
                    theme,
                ))
        });
    let items = if cards.visible_item_count() == 0 {
        items.push(text("This section has no cards yet.").size(13))
    } else {
        column![
            Space::new().height(card_window.top_padding),
            items,
            Space::new().height(card_window.bottom_padding),
        ]
        .spacing(0)
    };
    let content =
        column![
            text(section_title)
                .size(u32::from(UI_HEADING.size))
                .line_height(UI_HEADING.line_height),
            text("Manuscript outline")
                .size(u32::from(UI_COMPACT.size))
                .color(theme.palette().secondary_text),
            sections,
            scrollable(items)
                .id(HarnessTarget::CardsList.id())
                .on_scroll(|viewport| ProjectSurfaceMessage::Project(
                    ProjectMessage::SetCardsScroll(viewport.absolute_offset().y)
                ))
                .height(Length::Fill),
        ]
        .spacing(SPACING_12);
    let content = container(content)
        .padding([SPACING_32 + SPACING_4, SPACING_24])
        .width(Length::Fill)
        .height(Length::Fill);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .into()
}

/// Collapses multiline, user-authored Card content into a bounded one-line
/// preview without splitting a Unicode code point. Inspector remains the
/// complete source of title, Synopsis, and metadata values.
fn compact_card_projection(value: &str, maximum_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let Some((truncate_at, _)) = compact.char_indices().nth(maximum_chars) else {
        return compact;
    };
    format!("{}…", &compact[..truncate_at])
}

/// Uses a stable field-name hash so the same metadata field receives the same
/// theme-aware tint in every project. Field values deliberately do not affect
/// the colour: their text is the content authors scan and edit.
fn card_metadata_chip<'a>(
    label: &'a str,
    value: Option<&'a str>,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let (background, border) = card_metadata_chip_colors(theme, label);
    let value = compact_card_projection(value.unwrap_or("—"), 24);
    container(
        text(format!("{label}: {value}"))
            .size(u32::from(UI_COMPACT.size))
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .padding([2.0, SPACING_4])
    .style(move |_| iced::widget::container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            color: border,
            width: 1.0,
            radius: 3.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn card_metadata_chip_colors(theme: ParchMintTheme, label: &str) -> (Color, Color) {
    let palette = theme.palette();
    let tints = [
        palette.accent,
        palette.success,
        palette.warning,
        palette.saving,
        palette.comment_active,
        palette.comment_resolved,
    ];
    let tint = tints[(stable_metadata_label_hash(label) as usize) % tints.len()];
    (Color { a: 0.16, ..tint }, Color { a: 0.55, ..tint })
}

fn stable_metadata_label_hash(label: &str) -> u64 {
    // FNV-1a is sufficient for selecting from this small visual palette and,
    // unlike the standard-library hasher, remains stable across processes.
    let normalized = label
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    normalized
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
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
        return column![
            text("Global Search").size(22),
            replacement,
            text("Finish the streamed project search before reviewing replacements.").size(13),
            review.style(move |_, status| components::button_style(
                theme,
                ButtonKind::Primary,
                interaction(status, false)
            )),
        ]
        .spacing(14)
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
                ReplacementPreviewRowKind::Document => (item.node_id.to_owned(), None),
                ReplacementPreviewRowKind::Match => {
                    let snippet = format!(
                        "{}{}{}",
                        item.prefix.unwrap_or_default(),
                        item.matching_text.unwrap_or_default(),
                        item.suffix.unwrap_or_default()
                    );
                    (snippet, Some(item.node_id.to_owned()))
                }
            };
            let state_label = match item.check_state {
                ReplacementCheckState::Selected => "selected",
                ReplacementCheckState::Unselected => "not selected",
                ReplacementCheckState::Indeterminate => "partially selected",
            };
            let checked = item.check_state != ReplacementCheckState::Unselected;
            let node_id = item.node_id.to_owned();
            let control = checkbox(checked)
                .label(format!("{state_label}: {label}"))
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
        "Selected matches are revalidated and ready to apply atomically.".to_owned()
    } else {
        "Selection changed. Revalidate before applying.".to_owned()
    };
    let revalidate = if preview.is_validating() {
        button(text("Revalidate selection"))
    } else {
        button(text("Revalidate selection")).on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::OpenReplacementPreview,
        ))
    };
    let apply = if preview.can_apply(workspace.project_revision()) {
        button(text("Apply replacement")).on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::ApplyReplacement,
        ))
    } else {
        button(text("Apply replacement"))
    };
    column![
        row![
            text("Replace Preview").size(22),
            Space::new().width(Length::Fill),
            button(text("Close").size(12)).on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::CloseReplacementPreview
            ))
        ]
        .align_y(iced::alignment::Vertical::Center),
        replacement,
        text(format!(
            "{} selected match{} ready to replace",
            preview.included_match_ids().len(),
            if preview.included_match_ids().len() == 1 {
                ""
            } else {
                "es"
            },
        ))
        .size(13),
        text(validation).size(13),
        row![
            button(text("Select all").size(12)).on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::SelectAllReplacementMatches
            )),
            button(text("Select none").size(12)).on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::SelectNoReplacementMatches
            )),
            revalidate,
            apply,
        ]
        .spacing(8),
        scrollable(rows).height(Length::Fill),
    ]
    .spacing(14)
    .into()
}

fn history_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let history = workspace.history();
    let checkpoint_window = history.checkpoint_window();
    let checkpoints = history
        .visible_checkpoints()
        .skip(checkpoint_window.start)
        .take(
            checkpoint_window
                .end
                .saturating_sub(checkpoint_window.start),
        )
        .fold(column![], |column, checkpoint| {
            let checkpoint_id = checkpoint.checkpoint_id.clone();
            let selected = history.selected_checkpoint_id() == Some(checkpoint_id.as_str());
            let word_delta = history
                .comparison()
                .filter(|comparison| comparison.checkpoint_id == checkpoint_id)
                .map(|comparison| format!(" · {:+} words", comparison.word_count_delta()))
                .unwrap_or_default();
            let column = if let Some(heading) = history.timeline_heading(&checkpoint_id) {
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
                .push(
                    column![
                        harness_target::target_id(
                            harness_target::history_checkpoint_id(&checkpoint_id),
                            button(text(checkpoint.label()).size(u32::from(UI_BODY.size)))
                                .padding([SPACING_4, 0.0])
                                .width(Length::Fill)
                                .on_press(ProjectSurfaceMessage::Project(
                                    ProjectMessage::SelectHistoryCheckpoint(checkpoint_id.clone(),),
                                ))
                                .style(move |_, status| components::button_style(
                                    theme,
                                    ButtonKind::Quiet,
                                    interaction(status, selected),
                                )),
                        ),
                        text(format!(
                            "{} · {} · Version {}{}",
                            checkpoint.category.label(),
                            checkpoint.affected_summary(),
                            checkpoint.sequence,
                            word_delta,
                        ))
                        .size(u32::from(UI_COMPACT.size))
                        .color(theme.palette().secondary_text),
                    ]
                    .spacing(SPACING_4)
                    .padding(SPACING_8)
                    .height(Length::Fixed(
                        crate::project_workspace::HISTORY_CHECKPOINT_ROW_HEIGHT,
                    )),
                )
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
    let checkpoints = column![
        Space::new().height(checkpoint_window.top_padding),
        checkpoints,
        Space::new().height(checkpoint_window.bottom_padding),
    ];
    let restore: Element<'a, ProjectSurfaceMessage> = history
        .selected_checkpoint_id()
        .and_then(|checkpoint_id| {
            history
                .checkpoints()
                .iter()
                .find(|checkpoint| checkpoint.checkpoint_id == checkpoint_id)
                .map(|checkpoint| (checkpoint_id, checkpoint.label()))
        })
        .map(|(checkpoint_id, label)| {
            button(text(format!("Restore “{label}”")).size(12))
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::RequestHistoryRestore {
                        checkpoint_id: checkpoint_id.to_owned(),
                    },
                ))
                .style(move |_, status| {
                    components::button_style(theme, ButtonKind::Primary, interaction(status, false))
                })
                .into()
        })
        .unwrap_or_else(|| Space::new().height(0).into());
    let milestone_draft = history.named_snapshot_draft();
    let milestone_submit = ProjectSurfaceMessage::Project(ProjectMessage::RequestNamedSnapshot(
        milestone_draft.to_owned(),
    ));
    let milestone = row![
        text_input("Milestone name", milestone_draft)
            .on_input(
                |name| ProjectSurfaceMessage::Project(ProjectMessage::SetNamedSnapshotDraft(name))
            )
            .on_submit(milestone_submit.clone())
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
        .on_press_maybe((!history.is_creating_named_snapshot()).then_some(milestone_submit)),
    ]
    .spacing(8);
    let active_document = workspace
        .editor()
        .pane(workspace.editor().focused_pane())
        .active_document()
        .map(str::to_owned);
    let filter: Element<'a, ProjectSurfaceMessage> = match history.active_document_filter() {
        Some(_) => button(text("Show all documents").size(12))
            .on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::SetHistoryDocumentFilter(None),
            ))
            .into(),
        None => active_document
            .map(|document_id| {
                button(text("Filter to current document").size(12))
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::SetHistoryDocumentFilter(Some(document_id)),
                    ))
                    .into()
            })
            .unwrap_or_else(|| Space::new().height(0).into()),
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
        Some(comparison) => {
            let summary = comparison.change_summary();
            container(
                column![
                    text(format!("Comparison · {}", comparison.document_title)).size(15),
                    row![
                        container(
                            column![
                                text("Checkpoint").size(13),
                                history_comparison_side(&comparison.lines, true, theme),
                            ]
                            .spacing(6)
                        )
                        .width(Length::Fill),
                        container(
                            column![
                                text("Current").size(13),
                                history_comparison_side(&comparison.lines, false, theme),
                            ]
                            .spacing(6)
                        )
                        .width(Length::Fill),
                    ]
                    .spacing(16),
                    text(format!(
                        "{} added · {} removed · {} modified lines",
                        summary.added_lines, summary.removed_lines, summary.modified_lines
                    ))
                    .size(12),
                ]
                .spacing(8),
            )
            .into()
        }
        None if history.error().is_some() => Space::new().height(0).into(),
        None if history.preview().is_some() => {
            text("This checkpoint has no version of the current document to compare.")
                .size(12)
                .into()
        }
        None if history.selected_checkpoint_id().is_some() => {
            text("Loading checkpoint comparison…").size(12).into()
        }
        None => text("Select a checkpoint to compare it with the current document.")
            .size(12)
            .into(),
    };
    let error: Element<'a, ProjectSurfaceMessage> = history
        .error()
        .map(|error| text(error).size(12).into())
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
    let list =
        column![
            text("Writing timeline")
                .size(u32::from(UI_PAGE_TITLE.size))
                .line_height(UI_PAGE_TITLE.line_height)
                .font(Font {
                    weight: font::Weight::Semibold,
                    ..Font::with_name(UI_PAGE_TITLE.family)
                }),
            text("Milestones and recoverable project versions")
                .size(u32::from(UI_BODY.size))
                .color(theme.palette().secondary_text),
            milestone,
            filter,
            scrollable(checkpoints)
                .on_scroll(|viewport| ProjectSurfaceMessage::Project(
                    ProjectMessage::SetHistoryScroll(viewport.absolute_offset().y)
                ))
                .height(Length::Fill),
            load_more,
        ]
        .spacing(SPACING_12);
    let detail = column![
        text("Checkpoint details")
            .size(u32::from(UI_PAGE_TITLE.size))
            .line_height(UI_PAGE_TITLE.line_height)
            .font(Font {
                weight: font::Weight::Semibold,
                ..Font::with_name(UI_PAGE_TITLE.family)
            }),
        text("Compare a version with the current document or restore the whole project.")
            .size(u32::from(UI_BODY.size))
            .color(theme.palette().secondary_text),
        error,
        maintenance,
        scrollable(comparison).height(Length::Fill),
        row![Space::new().width(Length::Fill), restore].spacing(12),
    ]
    .spacing(SPACING_16);
    row![
        container(list)
            .padding(SPACING_16)
            .width(420)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
        container(Space::new().width(SIDEBAR_SPLITTER_WIDTH))
            .width(SIDEBAR_SPLITTER_WIDTH)
            .height(Length::Fill)
            .style(move |_| iced::widget::container::Style {
                background: Some(Background::Color(theme.palette().divider)),
                ..Default::default()
            }),
        container(detail)
            .padding([SPACING_24, SPACING_32])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest)),
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
    let ordered_items = items
        .iter()
        .filter(|item| Some(item.node_id) == selected_item_id)
        .chain(
            items
                .iter()
                .filter(|item| Some(item.node_id) != selected_item_id),
        );
    let rows = ordered_items.fold(column![].spacing(6), |column, item| {
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
                        text(item.title).size(14),
                        Space::new().width(Length::Fill),
                        text(kind).size(11),
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
        rows.push(text("No deleted content is available.").size(13))
    } else {
        rows
    };
    let preview: Element<'a, ProjectSurfaceMessage> = match deleted.selected_preview() {
        Some(preview) => {
            let (node_id, using_fallback, former_location) = items
                .iter()
                .find(|item| Some(item.node_id) == selected_item_id)
                .map(|item| {
                    (
                        item.node_id.to_owned(),
                        matches!(item.restore_location, RestoreLocation::SectionRoot(_)),
                        restore_location_label(workspace, item.former_location),
                    )
                })
                .expect("selected deleted item has a presentation row");
            let mut actions = row![
                Space::new().width(Length::Fill),
                button(text("Restore item").size(12))
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::RestoreDeleted(node_id.clone()),
                    ))
                    .style(move |_, status| components::button_style(
                        theme,
                        ButtonKind::Primary,
                        interaction(status, false),
                    )),
            ]
            .spacing(8);
            if using_fallback {
                actions = actions.push(
                    button(text("Use section root").size(12))
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::UseRestoreFallback(node_id),
                        ))
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Secondary,
                                interaction(status, true),
                            )
                        }),
                );
            }
            container(
                column![
                    text("Deleted document contents").size(16).font(Font {
                        weight: font::Weight::Bold,
                        ..Font::DEFAULT
                    }),
                    container(
                        column![
                            row![
                                text(preview.title).size(14),
                                Space::new().width(Length::Fill),
                                text("Read only").size(12),
                            ],
                            scrollable(semantic_preview(preview.semantic, theme))
                                .height(Length::Fill),
                        ]
                        .spacing(12)
                        .height(Length::Fill)
                    )
                    .padding(16)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_| components::surface(
                        theme,
                        Surface::Manuscript,
                        Interaction::Rest,
                    )),
                    row![
                        container(
                            text(format!(
                            "Read-only preview · restoring returns the item to {former_location}."
                        ))
                            .size(12)
                        )
                        .padding(12)
                        .width(Length::Fill)
                        .style(move |_| iced::widget::container::Style {
                            background: Some(Background::Color(theme.palette().sidebar)),
                            ..Default::default()
                        }),
                        actions,
                    ]
                    .spacing(12),
                ]
                .spacing(14)
                .height(Length::Fill),
            )
            .padding(0)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest))
            .into()
        }
        None if selected_item_id.is_some() => container(
            column![
                text("Formatted preview unavailable").size(18),
                text("This deleted document is no longer available to preview.").size(12),
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest))
        .into(),
        None => container(text("Select a deleted item to inspect it.").size(13))
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest))
            .into(),
    };
    row![
        container(
            column![
                text("Recently Deleted").size(24),
                text("Select an item to preview and restore it.").size(14),
                scrollable(list).height(Length::Fill),
            ]
            .spacing(18)
            .height(Length::Fill)
        )
        .padding(16)
        .width(420)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
        container(
            column![
                text("Deleted item preview").size(24),
                text(
                    items
                        .iter()
                        .find(|item| Some(item.node_id) == selected_item_id)
                        .map(|item| item.title)
                        .unwrap_or("Select an item to preview"),
                )
                .size(14),
                preview,
            ]
            .spacing(18)
            .height(Length::Fill),
        )
        .padding([20, 30])
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

fn history_comparison_side(
    lines: &[crate::HistoryComparisonLine],
    before: bool,
    theme: ParchMintTheme,
) -> Element<'static, ProjectSurfaceMessage> {
    let rows = lines
        .iter()
        .fold(column![].spacing(0), |column, comparison| {
            let line = if before {
                comparison.before.as_ref()
            } else {
                comparison.after.as_ref()
            };
            let Some(line) = line else {
                return column.push(Space::new().height(26));
            };
            let changed_kind = if before {
                crate::HistoryComparisonSpanKind::Removed
            } else {
                crate::HistoryComparisonSpanKind::Added
            };
            // `rich_text` does not implement Iced's semantic operation. Keep
            // these as ordinary zero-gap text widgets so changed words remain
            // discoverable to assistive and headless UI tooling.
            let content = line.spans.iter().fold(
                row![].spacing(0).width(Length::Fill),
                |content, span_data| {
                    let color = match span_data.kind {
                        crate::HistoryComparisonSpanKind::Added => theme.palette().success,
                        crate::HistoryComparisonSpanKind::Removed => theme.palette().destructive,
                        crate::HistoryComparisonSpanKind::Unchanged => theme.palette().primary_text,
                    };
                    content.push(text(span_data.text.clone()).size(13).color(color))
                },
            );
            let changed = line.spans.iter().any(|span| span.kind == changed_kind);
            column.push(
                container(
                    row![
                        text(line.line_number.to_string()).size(12).width(30),
                        content,
                    ]
                    .spacing(8),
                )
                .padding([4, 8])
                .width(Length::Fill)
                .style(move |_| {
                    if changed {
                        iced::widget::container::Style {
                            background: Some(Background::Color(if before {
                                theme.palette().error_subtle
                            } else {
                                theme.palette().success_subtle
                            })),
                            ..Default::default()
                        }
                    } else {
                        iced::widget::container::Style::default()
                    }
                }),
            )
        });
    rows.into()
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
        container(content)
            .padding([6, 10])
            .width(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest))
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
                    }
                }
                let mut font = block_font;
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
                if reduced_size || small_caps {
                    rendered = rendered.size(if reduced_size {
                        size.saturating_sub(3)
                    } else {
                        size.saturating_sub(1)
                    });
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
        crate::ExportState::Planning => "Planning export…".to_owned(),
        crate::ExportState::Exporting { completed, total } => {
            format!("Exporting {completed}/{total}")
        }
        crate::ExportState::Committing => "Committing output…".to_owned(),
        crate::ExportState::Cancelling => "Cancelling export…".to_owned(),
        crate::ExportState::Succeeded { artifact } => {
            format!("Exported {}", artifact.display_name)
        }
        crate::ExportState::Cancelled => "Export cancelled".to_owned(),
        crate::ExportState::Failed(error) => error,
    };
    let title_setting = export.project_settings().emit_titles;
    let next_title_setting = match title_setting {
        parchmint_domain::ProjectExportSetting::Inherit => {
            parchmint_domain::ProjectExportSetting::Enabled
        }
        parchmint_domain::ProjectExportSetting::Enabled => {
            parchmint_domain::ProjectExportSetting::Disabled
        }
        parchmint_domain::ProjectExportSetting::Disabled => {
            parchmint_domain::ProjectExportSetting::Inherit
        }
    };
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
    let title_control = button(
        row![
            text(export_setting_label(title_setting)).size(14),
            Space::new().width(Length::Fill),
            text("⌄").size(14),
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .padding([9, 12])
    .on_press_maybe(export.can_start().then_some(ProjectSurfaceMessage::Project(
        ProjectMessage::SetExportTitleSetting(next_title_setting),
    )))
    .style(move |_, status| {
        components::button_style(theme, ButtonKind::Secondary, interaction(status, false))
    });
    let page_break_control = button(
        row![
            text(export_setting_label(
                if export.project_settings().starts_new_page {
                    parchmint_domain::ProjectExportSetting::Enabled
                } else {
                    parchmint_domain::ProjectExportSetting::Disabled
                },
            ))
            .size(14),
            Space::new().width(Length::Fill),
            text("⌄").size(14),
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .padding([9, 12])
    .on_press_maybe(export.can_start().then_some(ProjectSurfaceMessage::Project(
        ProjectMessage::SetExportPageBreak(!export.project_settings().starts_new_page),
    )))
    .style(move |_, status| {
        components::button_style(theme, ButtonKind::Secondary, interaction(status, false))
    });
    let output_controls = column![
        text("MANUSCRIPT OUTPUT").size(12),
        row![
            column![
                text("Title emission").size(16),
                text("Controls headings emitted from manuscript node titles.").size(12),
            ]
            .spacing(4)
            .width(Length::FillPortion(3)),
            container(title_control).width(Length::FillPortion(2)),
        ]
        .spacing(32)
        .align_y(iced::alignment::Vertical::Center),
        Space::new().height(12),
        row![
            column![
                text("Page breaks").size(16),
                text("Preserves authored page-break atoms in the manuscript.").size(12),
            ]
            .spacing(4)
            .width(Length::FillPortion(3)),
            container(page_break_control).width(Length::FillPortion(2)),
        ]
        .spacing(32)
        .align_y(iced::alignment::Vertical::Center),
        Space::new().height(20),
        checkbox(export.numbers_documents())
            .label("Number chapter headings")
            .on_toggle_maybe(export.can_start().then_some(|enabled| {
                ProjectSurfaceMessage::Project(ProjectMessage::SetExportNumbering(enabled))
            })),
    ]
    .spacing(10);
    let state: Element<'a, ProjectSurfaceMessage> =
        if matches!(export.state(), crate::ExportState::Ready) {
            Space::new().height(0).into()
        } else {
            text(state).size(12).into()
        };
    let summary = column![
        text("SUMMARY").size(12),
        container(column![
            text("Entire Manuscript").size(18),
            text("HTML · UTF-8").size(14),
            Space::new().height(8),
            text("Uses project title-emission and page-break settings.").size(13),
            text("Excludes Synopsis, metadata, comments, and Research.").size(13),
        ].spacing(10))
        .padding(16)
        .height(Length::Fixed(160.0))
        .style(move |_| iced::widget::container::Style {
            background: Some(Background::Color(theme.palette().sidebar)),
            ..Default::default()
        }),
        container(text("Export does not change project content. Existing output is replaced only after confirmation.").size(12))
            .padding(16)
            .style(move |_| iced::widget::container::Style {
                background: Some(Background::Color(theme.palette().sidebar)),
                ..Default::default()
            }),
        state,
    ]
    .spacing(10);
    let output_file =
        row![
            text_input("Output file", export.output_name())
                .on_input_maybe(export.can_start().then_some(|value| {
                    ProjectSurfaceMessage::Project(ProjectMessage::SetExportOutputName(value))
                }))
                .padding([9, 12])
                .width(Length::Fill)
                .style(move |_, status| components::field_style(theme, field_interaction(status))),
            harness_target::target(
                HarnessTarget::ExportBrowse,
                button(text("Browse…"))
                    .padding([9, 14])
                    .on_press_maybe((!export.can_cancel()).then_some(
                        ProjectSurfaceMessage::Project(ProjectMessage::BrowseExportDestination)
                    ))
                    .style(move |_, status| components::button_style(
                        theme,
                        ButtonKind::Secondary,
                        interaction(status, false)
                    )),
            ),
        ]
        .spacing(8);
    let content =
        container(
            column![
                text("Export manuscript").size(28),
                text("Create one self-contained HTML export of the entire Manuscript.").size(16),
                row![
                    container(output_controls)
                        .padding([16, 0])
                        .width(Length::FillPortion(1))
                        .style(|_| iced::widget::container::Style::default()),
                    column![
                        text("OUTPUT FILE").size(12),
                        output_file,
                        container(summary)
                            .padding([16, 0])
                            .style(|_| iced::widget::container::Style::default()),
                    ]
                    .spacing(12)
                    .width(Length::FillPortion(1))
                ]
                .spacing(32),
                row![
                    Space::new().width(Length::Fill),
                    button(text("Cancel"))
                        .padding([10, 18])
                        .on_press_maybe(export.can_cancel().then_some(
                            ProjectSurfaceMessage::Project(ProjectMessage::CancelExport)
                        ))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Secondary,
                            interaction(status, false)
                        )),
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
                    terminal_actions,
                ]
                .spacing(12),
            ]
            .spacing(20),
        )
        .padding([40, 0])
        .width(1100)
        .height(Length::Fill);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .into()
}

fn export_setting_label(setting: parchmint_domain::ProjectExportSetting) -> &'static str {
    match setting {
        parchmint_domain::ProjectExportSetting::Inherit => "Project default",
        parchmint_domain::ProjectExportSetting::Enabled => "Include",
        parchmint_domain::ProjectExportSetting::Disabled => "Exclude",
    }
}

fn settings_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let settings = workspace.settings();
    let choices = settings.appearance_choices().into_iter().fold(
        column![].spacing(SPACING_8),
        |column, mode| {
            let (name, detail) = match mode {
                parchmint_preferences::AppearanceMode::System => {
                    ("System", "Follow the operating system")
                }
                parchmint_preferences::AppearanceMode::Light => {
                    ("Light", "Keep this appearance when the system changes")
                }
                parchmint_preferences::AppearanceMode::Dark => {
                    ("Dark", "Keep this appearance when the system changes")
                }
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
                        text(name).size(u32::from(UI_HEADING.size)).font(Font {
                            weight: font::Weight::Bold,
                            ..Font::DEFAULT
                        }),
                        Space::new().width(40),
                        text(detail)
                            .size(u32::from(UI_BODY.size))
                            .color(theme.palette().secondary_text),
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
                    flat_selection_button_style(theme, status, settings.appearance() == mode)
                }),
            )
        },
    );
    let metadata = settings.metadata_fields().into_iter().enumerate().fold(
        column![
            row![
                text("Fields").size(16),
                Space::new().width(Length::Fill),
                button(text("+ New field").size(12))
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::CreateMetadataField
                    ))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Primary,
                            interaction(status, false),
                        )
                    }),
            ]
            .align_y(iced::alignment::Vertical::Center),
            text("Choose a field to edit its type, default, and card visibility.").size(11),
        ]
        .spacing(SPACING_8),
        |column, (index, field)| {
            let id = field.id.to_owned();
            let delete_id = id.clone();
            let selected = matches!(
                settings.selected_detail(),
                Some(SettingsDetail::MetadataField(selected_id)) if selected_id == field.id
            );
            let summary = format!(
                "{} · {}{}",
                metadata_applicability_label(field.applicability),
                match field.text_kind {
                    MetadataFieldTextKind::SingleLine => "Single line",
                    MetadataFieldTextKind::Multiline => "Multiline",
                },
                if field.visible_on_cards {
                    " · Cards"
                } else {
                    ""
                },
            );
            let dragging = settings.metadata_drag_source() == Some(field.id);
            let target = settings.metadata_drag_target() == Some(index);
            let drag_handle = stationary_tooltip::tooltip(
                mouse_area(
                    container(text("⠿").size(16))
                        .width(22)
                        .align_x(iced::alignment::Horizontal::Center),
                )
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::BeginMetadataFieldDrag(id.clone()),
                ))
                .on_release(ProjectSurfaceMessage::Project(
                    ProjectMessage::CommitMetadataFieldDrag,
                ))
                .interaction(iced::mouse::Interaction::Grabbing),
                container(text("Drag to reorder").size(12)).padding([4, 6]),
                components::surface(theme, Surface::Elevated, Interaction::Rest),
            );
            let row = row![
                drag_handle,
                button(
                    column![
                        text(field.label).size(13),
                        text(summary).size(11),
                        text(field.description.unwrap_or("No description")).size(11),
                    ]
                    .spacing(SPACING_4),
                )
                .width(Length::Fill)
                .padding(SPACING_8)
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::SelectMetadataField(id)
                ))
                .style(move |_, status| {
                    components::button_style(
                        theme,
                        ButtonKind::Quiet,
                        interaction(status, selected),
                    )
                }),
                stationary_tooltip::tooltip(
                    button(icon_sized(Icon::RecentlyDeleted, 16))
                        .padding(5)
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::RequestDeleteMetadataField(delete_id),
                        ))
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false),
                        )),
                    container(text("Delete field").size(12)).padding([4, 6]),
                    components::surface(theme, Surface::Elevated, Interaction::Rest),
                ),
            ]
            .spacing(SPACING_8);
            column.push(
                mouse_area(container(row).width(Length::Fill).style(move |_| {
                    if target || dragging {
                        components::surface(theme, Surface::Panel, Interaction::Selected)
                    } else {
                        iced::widget::container::Style::default()
                    }
                }))
                .on_enter(ProjectSurfaceMessage::Project(
                    ProjectMessage::SetMetadataFieldDragTarget(index),
                ))
                .on_release(ProjectSurfaceMessage::Project(
                    ProjectMessage::CommitMetadataFieldDrag,
                )),
            )
        },
    );
    let styles = settings.styles().into_iter().fold(
        column![
            button(text("Create custom style"))
                .on_press(ProjectSurfaceMessage::Project(ProjectMessage::CreateStyle))
        ]
        .spacing(6),
        |column, style| {
            let id = style.id.to_owned();
            let delete_id = id.clone();
            let reserved = style.role.is_reserved();
            let mut row_content = row![
                button(
                    row![
                        column![
                            text(style.display_name).size(13),
                            text(if style.role.is_reserved() {
                                "Reserved"
                            } else {
                                "Custom"
                            })
                            .size(11)
                        ]
                        .spacing(2),
                        Space::new().width(Length::Fill),
                        text(style.inherits.unwrap_or("No inheritance")).size(11),
                    ]
                    .spacing(14),
                )
                .width(Length::Fill)
                .padding(SPACING_8)
                .on_press(ProjectSurfaceMessage::Project(ProjectMessage::SelectStyle(
                    id,
                )))
                .style(move |_, status| {
                    components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
                })
            ]
            .align_y(iced::alignment::Vertical::Center);
            if !reserved {
                row_content = row_content.push(button(text("Trash").size(11)).on_press(
                    ProjectSurfaceMessage::Project(ProjectMessage::RequestDeleteStyle(delete_id)),
                ));
            }
            column.push(row_content)
        },
    );
    let category = settings.selected_category();
    let appearance_heading = match settings.appearance() {
        parchmint_preferences::AppearanceMode::System => "System appearance",
        parchmint_preferences::AppearanceMode::Light => "Light appearance",
        parchmint_preferences::AppearanceMode::Dark => "Dark appearance",
    };
    let navigation =
        settings
            .categories()
            .into_iter()
            .fold(column![].spacing(SPACING_4), |column, item| {
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
    let content: Element<'a, ProjectSurfaceMessage> = match category {
        SettingsCategory::Appearance => column![
            text(appearance_heading)
                .size(u32::from(UI_PAGE_TITLE.size))
                .line_height(UI_PAGE_TITLE.line_height)
                .font(Font {
                    weight: font::Weight::Semibold,
                    ..Font::with_name(UI_PAGE_TITLE.family)
                }),
            text("Application setting · applies to every open ParchMint window.")
                .size(u32::from(UI_BODY.size))
                .color(theme.palette().secondary_text),
            Space::new().height(8),
            container(choices).width(540),
            Space::new().height(12),
            text("Operating system changes Light → Dark; every open ParchMint window updates immediately.").size(14),
        ]
        .spacing(SPACING_12)
        .into(),
        SettingsCategory::General => column![
            text("General")
                .size(u32::from(UI_PAGE_TITLE.size))
                .line_height(UI_PAGE_TITLE.line_height),
            text("No general settings are available.").size(13),
        ]
        .spacing(SPACING_12)
        .into(),
        SettingsCategory::Metadata => {
            let detail = match settings.selected_detail() {
                Some(SettingsDetail::MetadataField(id)) => settings
                    .metadata_field(id)
                    .map(|field| metadata_field_detail(field, theme)),
                Some(SettingsDetail::NewMetadataField) => settings
                    .new_metadata_field_label()
                    .map(metadata_field_creation_detail),
                _ => None,
            }
            .unwrap_or_else(|| {
                text("Select a metadata field to edit its details.")
                    .size(12)
                    .into()
            });
            column![
                text("Metadata fields")
                    .size(u32::from(UI_PAGE_TITLE.size))
                    .line_height(UI_PAGE_TITLE.line_height),
                row![
                    container(scrollable(metadata).height(Length::Fill))
                        .width(Length::FillPortion(2))
                        .height(Length::Fill),
                    container(
                        column![text("Metadata field details").size(16), scrollable(detail)]
                            .spacing(10),
                    )
                    .padding(0)
                    .width(Length::FillPortion(3))
                    .height(Length::Fill)
                    .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
                ]
                .spacing(SPACING_24)
                .height(Length::Fill),
            ]
            .spacing(SPACING_12)
            .height(Length::Fill)
            .into()
        }
        SettingsCategory::Styles => {
            let detail = match settings.selected_detail() {
                Some(SettingsDetail::Style(id)) => settings
                    .style(id)
                    .map(|style| style_detail(settings, style, theme)),
                _ => None,
            }
            .unwrap_or_else(|| text("Select a style to edit its details.").size(12).into());
            column![
                text("Styles")
                    .size(u32::from(UI_PAGE_TITLE.size))
                    .line_height(UI_PAGE_TITLE.line_height),
                row![
                    container(scrollable(styles).height(Length::Fill))
                        .width(Length::FillPortion(2))
                        .height(Length::Fill),
                    container(column![text("Style details").size(16), scrollable(detail)].spacing(10))
                        .padding(0)
                        .width(Length::FillPortion(3))
                        .height(Length::Fill)
                        .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
                ]
                .spacing(SPACING_24)
                .height(Length::Fill),
            ]
            .spacing(SPACING_12)
            .height(Length::Fill)
            .into()
        }
        SettingsCategory::Dictionaries => {
            let dictionaries = settings.dictionaries();
            let scopes =
                dictionaries
                    .scopes()
                    .into_iter()
                    .fold(column![].spacing(6), |column, scope| {
                        column.push(
                            button(text(scope.label).size(13))
                                .width(Length::Fill)
                                .padding([8, 10])
                                .on_press_maybe(scope.available.then_some(
                                    ProjectSurfaceMessage::Project(
                                        ProjectMessage::SelectDictionaryScope(scope.scope),
                                    ),
                                ))
                                .style(move |_, status| {
                                    components::button_style(
                                        theme,
                                        ButtonKind::Secondary,
                                        interaction(status, scope.selected),
                                    )
                                }),
                        )
                    });
            let words: Element<'a, ProjectSurfaceMessage> = match dictionaries.words() {
                Some([]) => text("No project dictionary words.").size(12).into(),
                Some(words) => {
                    scrollable(words.iter().fold(column![].spacing(4), |column, word| {
                        column.push(text(word).size(13))
                    }))
                    .height(Length::Fill)
                    .into()
                }
                None => text("This dictionary is unavailable in the project workspace.")
                    .size(12)
                    .into(),
            };
            column![
                text("Dictionaries")
                    .size(u32::from(UI_PAGE_TITLE.size))
                    .line_height(UI_PAGE_TITLE.line_height),
                text(format!("Language · {}", dictionaries.language())).size(12),
                scopes,
                words,
            ]
            .spacing(SPACING_12)
            .into()
        }
    };
    row![
        container(
            column![
                text("SETTINGS").size(12),
                text("Project and application")
                    .size(11)
                    .color(theme.palette().secondary_text),
                navigation,
            ]
            .spacing(SPACING_12),
        )
        .padding([SPACING_16, SPACING_12])
        .width(280)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
        container(Space::new().width(SIDEBAR_SPLITTER_WIDTH))
            .width(SIDEBAR_SPLITTER_WIDTH)
            .height(Length::Fill)
            .style(move |_| iced::widget::container::Style {
                background: Some(Background::Color(theme.palette().divider)),
                ..Default::default()
            }),
        container(content)
            .padding([SPACING_24, SPACING_24])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest)),
    ]
    .height(Length::Fill)
    .into()
}

fn metadata_applicability_label(value: MetadataFieldApplicability) -> &'static str {
    match value {
        MetadataFieldApplicability::Groups => "Groups",
        MetadataFieldApplicability::Documents => "Documents",
        MetadataFieldApplicability::GroupsAndDocuments => "Groups and documents",
    }
}

fn metadata_field_creation_detail<'a>(label: &'a str) -> Element<'a, ProjectSurfaceMessage> {
    let name = sensor(
        text_input("Name this field", label)
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
        text("New metadata field").size(15),
        text("Start with a clear name. You can choose where it appears and how it behaves after adding it.")
            .size(12),
        name,
        row![
            button(text("Cancel")).on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::CancelNewMetadataField
            )),
            button(text("Add field")).on_press(ProjectSurfaceMessage::Project(
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
    let make_update = |label: String,
                       description: Option<String>,
                       applicability,
                       text_kind,
                       default_value,
                       visible_on_cards| {
        ProjectMessage::UpdateMetadataField {
            field_id: field.id.to_owned(),
            label,
            description,
            applicability,
            text_kind,
            default_value,
            visible_on_cards,
        }
    };
    let id = field.id.to_owned();
    let description = field.description.unwrap_or_default().to_owned();
    let default_value = field.default_value.unwrap_or_default().to_owned();
    let label = field.label.to_owned();
    let label_input = text_input("Name", field.label)
        .id(metadata_field_name_input_id())
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
    let description_input = text_input("Description", &description).on_input({
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
    let default_input = text_input("Default value", &default_value).on_input({
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
    let applicability = [
        MetadataFieldApplicability::Groups,
        MetadataFieldApplicability::Documents,
        MetadataFieldApplicability::GroupsAndDocuments,
    ]
    .into_iter()
    .fold(row![].spacing(5), |row, applicability| {
        let label = label.clone();
        let description = description.clone();
        let default_value = default_value.clone();
        let selected = field.applicability == applicability;
        row.push(
            button(text(metadata_applicability_label(applicability)))
                .on_press(ProjectSurfaceMessage::Project(make_update(
                    label,
                    (!description.is_empty()).then_some(description),
                    applicability,
                    field.text_kind,
                    (!default_value.is_empty()).then_some(default_value),
                    field.visible_on_cards,
                )))
                .style(move |_, status| {
                    components::button_style(
                        theme,
                        ButtonKind::Secondary,
                        interaction(status, selected),
                    )
                }),
        )
    });
    let kind = [
        MetadataFieldTextKind::SingleLine,
        MetadataFieldTextKind::Multiline,
    ]
    .into_iter()
    .fold(row![].spacing(5), |row, text_kind| {
        let label = label.clone();
        let description = description.clone();
        let default_value = default_value.clone();
        let selected = field.text_kind == text_kind;
        row.push(
            button(text(match text_kind {
                MetadataFieldTextKind::SingleLine => "Single line",
                MetadataFieldTextKind::Multiline => "Multiline",
            }))
            .on_press(ProjectSurfaceMessage::Project(make_update(
                label,
                (!description.is_empty()).then_some(description),
                field.applicability,
                text_kind,
                (!default_value.is_empty()).then_some(default_value),
                field.visible_on_cards,
            )))
            .style(move |_, status| {
                components::button_style(
                    theme,
                    ButtonKind::Secondary,
                    interaction(status, selected),
                )
            }),
        )
    });
    column![
        text("Metadata field details").size(15),
        label_input,
        description_input,
        default_input,
        text("Applies to").size(12),
        applicability,
        text("Text kind").size(12),
        kind,
        checkbox(field.visible_on_cards)
            .label("Visible on cards")
            .on_toggle({
                let label = label.clone();
                let description = description.clone();
                let default_value = default_value.clone();
                move |visible_on_cards| {
                    ProjectSurfaceMessage::Project(make_update(
                        label.clone(),
                        (!description.is_empty()).then_some(description.clone()),
                        field.applicability,
                        field.text_kind,
                        (!default_value.is_empty()).then_some(default_value.clone()),
                        visible_on_cards,
                    ))
                }
            }),
        button(text("Delete metadata field")).on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::RequestDeleteMetadataField(id)
        )),
    ]
    .spacing(8)
    .into()
}

fn style_detail<'a>(
    settings: &'a crate::SettingsState,
    style: crate::StyleSummary<'a>,
    _theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let style_id = style.id.to_owned();
    let properties = [
        StyleProperty::FontFamily,
        StyleProperty::FontSizePoints,
        StyleProperty::Weight,
        StyleProperty::Italic,
        StyleProperty::Alignment,
        StyleProperty::FirstLineIndentPoints,
        StyleProperty::LeftIndentPoints,
        StyleProperty::RightIndentPoints,
        StyleProperty::LineSpacing,
        StyleProperty::SpaceBeforePoints,
        StyleProperty::SpaceAfterPoints,
        StyleProperty::KeepWithNext,
        StyleProperty::PageBreakBefore,
    ];
    let property_controls =
        properties
            .into_iter()
            .fold(column![].spacing(6), |column, property| {
                let value = style_property_value(style.properties, property);
                let id = style_id.clone();
                column.push(
                    row![
                        text(property.label()).width(190),
                        text(value).width(120),
                        text_input("Enter new value", "").on_input(move |value| {
                            ProjectSurfaceMessage::Project(ProjectMessage::SetStyleProperty {
                                style_id: id.clone(),
                                property,
                                value,
                            })
                        })
                    ]
                    .spacing(8),
                )
            });
    let inheritance = settings
        .styles()
        .into_iter()
        .filter(|candidate| candidate.id != style.id)
        .fold(
            row![
                button(text("No inheritance")).on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::SetStyleInheritance {
                        style_id: style_id.clone(),
                        inherits: None
                    }
                ))
            ]
            .spacing(5),
            |row, candidate| {
                let id = style_id.clone();
                row.push(button(text(candidate.display_name)).on_press(
                    ProjectSurfaceMessage::Project(ProjectMessage::SetStyleInheritance {
                        style_id: id,
                        inherits: Some(candidate.id.to_owned()),
                    }),
                ))
            },
        );
    let mut content = column![
        text("Style details").size(15),
        text_input("Display name", style.display_name).on_input({
            let id = style_id.clone();
            move |display_name| {
                ProjectSurfaceMessage::Project(ProjectMessage::RenameStyle {
                    style_id: id.clone(),
                    display_name,
                })
            }
        }),
        text(if style.role.is_reserved() {
            "Reserved style (cannot be deleted)"
        } else {
            "Custom style"
        })
        .size(12),
        text("Inherits from").size(12),
        inheritance,
        text("Properties").size(12),
        property_controls,
    ]
    .spacing(8);
    if !style.role.is_reserved() {
        content = content.push(button(text("Delete custom style")).on_press(
            ProjectSurfaceMessage::Project(ProjectMessage::RequestDeleteStyle(style_id)),
        ));
    }
    content.into()
}

fn style_property_value(
    properties: &parchmint_domain::StyleProperties,
    property: StyleProperty,
) -> String {
    match property {
        StyleProperty::FontFamily => properties.font_family.clone().unwrap_or_default(),
        StyleProperty::FontSizePoints => properties
            .font_size_points
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::Weight => properties
            .weight
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::Italic => properties
            .italic
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::Alignment => properties
            .alignment
            .map(|value| format!("{value:?}"))
            .unwrap_or_default(),
        StyleProperty::FirstLineIndentPoints => properties
            .first_line_indent_points
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::LeftIndentPoints => properties
            .left_indent_points
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::RightIndentPoints => properties
            .right_indent_points
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::LineSpacing => properties
            .line_spacing
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::SpaceBeforePoints => properties
            .space_before_points
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::SpaceAfterPoints => properties
            .space_after_points
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::KeepWithNext => properties
            .keep_with_next
            .map(|value| value.to_string())
            .unwrap_or_default(),
        StyleProperty::PageBreakBefore => properties
            .page_break_before
            .map(|value| value.to_string())
            .unwrap_or_default(),
    }
}

fn recovery_backdrop<'a>(
    _workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    container(
        column![
            Space::new().height(120),
            text("ParchMint recovered newer edits before opening the workspace.").size(16),
        ]
        .spacing(0),
    )
    .padding(24)
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Top)
    .style(move |_| components::surface(theme, Surface::Application, Interaction::Rest))
    .into()
}

fn recovery_modal<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let recovery = workspace.recovery();
    let mut summary = column![
        text(format!(
            "{} recovered record{}",
            recovery.accepted_records(),
            if recovery.accepted_records() == 1 {
                ""
            } else {
                "s"
            }
        ))
        .size(16),
    ]
    .spacing(6);
    for document in workspace.recovery_summary() {
        summary = summary.push(
            text(format!(
                "{} · editor revision {}",
                document.display_title.unwrap_or("Recovered document"),
                document.revision
            ))
            .size(13),
        );
    }
    if let Some(isolation) = recovery.isolation() {
        summary = summary.push(text(format!("Some records were isolated: {isolation}")).size(13));
    }
    if let Some(error) = recovery.error() {
        summary = summary.push(text(format!("Recovery could not complete: {error}")).size(13));
    }
    let mut recover = button(text(if recovery.is_resolving() {
        "Resolving…"
    } else {
        "Recover changes"
    }));
    let mut discard = button(text("Discard"));
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
            row![
                icon_sized(Icon::History, 28),
                text("Recovered changes are ready")
                    .size(28)
                    .font(Font {
                        weight: font::Weight::Bold,
                        ..Font::DEFAULT
                    })
            ]
                .spacing(16)
                .align_y(iced::alignment::Vertical::Center),
            text("ParchMint found newer edits than the last completed save. Review the recovery summary before continuing.").size(16),
            text("RECOVERY SUMMARY").size(12),
            container(summary)
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

fn inspector<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
    width: u32,
    expanded: [bool; 3],
    compact: bool,
) -> Element<'a, ProjectSurfaceMessage> {
    let selected = workspace.inspector_node_id();
    let content = if let Some(selected) = selected {
        let title = workspace.explorer().title(selected).unwrap_or("Untitled");
        let selected_id = selected.to_owned();
        let document_selected = workspace
            .explorer()
            .row(selected)
            .is_some_and(|row| row.kind == HierarchyRowKind::Document);
        let inspector = workspace.inspector();
        let metadata = inspector.metadata_items(selected).into_iter().fold(
            column![].spacing(SPACING_8),
            |column, item| {
                let node_id = selected_id.clone();
                let field_id = item.field_id.to_owned();
                let value = item.effective_value.unwrap_or_default();
                column.push(
                    row![
                        text(item.label).size(u32::from(UI_LABEL.size)).width(88),
                        text_input("—", value)
                            .on_input(move |value| ProjectSurfaceMessage::Project(
                                ProjectMessage::SetMetadataValue {
                                    node_id: node_id.clone(),
                                    field_id: field_id.clone(),
                                    value,
                                }
                            ))
                            .padding([7, 9])
                            .width(Length::Fill)
                            .style(move |_, status| components::field_style(
                                theme,
                                field_interaction(status)
                            )),
                    ]
                    .spacing(SPACING_8)
                    .align_y(iced::alignment::Vertical::Center),
                )
            },
        );
        let synopsis_id = selected_id.clone();
        let synopsis = text_editor(
            workspace
                .synopsis_editor(selected)
                .expect("every live hierarchy node has a synopsis editor"),
        )
        .id(HarnessTarget::InspectorSynopsis.id())
        .placeholder("No synopsis")
        .on_action(move |action| {
            ProjectSurfaceMessage::Project(ProjectMessage::EditSynopsis {
                node_id: synopsis_id.clone(),
                action,
            })
        })
        .padding(12)
        .size(14)
        .height(Length::Fixed(80.0))
        .style(move |_, status| multiline_field_style(theme, status));
        let title: Element<'a, ProjectSurfaceMessage> = if compact {
            text(format!("INSPECTOR · {title}")).size(12).into()
        } else if workspace.inspector_title_rename_node_id() == Some(selected) {
            let title_id = selected_id.clone();
            sensor(
                text_input("Untitled", title)
                    .id(inspector_title_input_id())
                    .on_input(move |title| {
                        ProjectSurfaceMessage::Project(ProjectMessage::RenameNode {
                            node_id: title_id.clone(),
                            title,
                        })
                    })
                    .on_submit(ProjectSurfaceMessage::Project(
                        ProjectMessage::CommitInspectorTitleRename,
                    ))
                    .padding([6, 8])
                    .style(move |_, status| {
                        components::field_style(theme, field_interaction(status))
                    }),
            )
            .key(selected_id.clone())
            .on_show({
                let node_id = selected_id.clone();
                move |_| ProjectSurfaceMessage::InspectorRenameShown(node_id.clone())
            })
            .into()
        } else {
            let node_id = selected_id.clone();
            harness_target::target(
                HarnessTarget::InspectorTitle,
                stationary_tooltip::tooltip(
                    button(text(format!("INSPECTOR · {title}")).size(12))
                        .width(Length::Fill)
                        .padding(0)
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::BeginInspectorTitleRename(node_id),
                        ))
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, false),
                            )
                        }),
                    container(text("Rename title").size(12)).padding([4, 6]),
                    components::surface(theme, Surface::Elevated, Interaction::Rest),
                ),
            )
        };
        let editor = workspace.editor();
        let mut comments = column![].spacing(SPACING_8);
        let threads = editor.inspector_comments();
        let has_threads = !threads.is_empty();
        if !has_threads {
            comments = comments.push(
                column![
                    text("No comments").size(13),
                    text("Add a document comment below, or select text to anchor one.").size(12),
                ]
                .spacing(2),
            );
        }
        for (thread_index, thread) in threads.into_iter().enumerate() {
            let thread_id = thread.id().to_owned();
            let selected_thread = editor.selected_comment() == Some(thread_id.as_str());
            let state = if thread.resolved() {
                "Resolved"
            } else {
                "Unresolved"
            };
            let state_color = if thread.resolved() {
                theme.palette().comment_resolved
            } else {
                theme.palette().comment_active
            };
            let root = thread.messages().first();
            let root_body = root.map(|message| message.body()).unwrap_or("Comment");
            let anchor_summary = comment_anchor_summary(thread.anchor());
            let mut thread_summary = row![
                column![
                    text(root_body).size(u32::from(UI_BODY.size)),
                    text(anchor_summary)
                        .size(u32::from(UI_COMPACT.size))
                        .color(theme.palette().secondary_text),
                ]
                .spacing(SPACING_4),
                Space::new().width(Length::Fill),
            ]
            .spacing(SPACING_8);
            if selected_thread {
                thread_summary = thread_summary.push(
                    text("Selected").size(u32::from(UI_LABEL.size)).font(Font {
                        weight: font::Weight::Semibold,
                        ..Font::DEFAULT
                    }),
                );
            }
            thread_summary = thread_summary.push(
                text(state)
                    .size(u32::from(UI_LABEL.size))
                    .font(Font {
                        weight: font::Weight::Semibold,
                        ..Font::DEFAULT
                    })
                    .color(state_color),
            );
            let mut card = column![
                button(thread_summary)
                    .width(Length::Fill)
                    .padding([4, 0])
                    .on_press(ProjectSurfaceMessage::EditorCenter(
                        EditorCenterMessage::Workspace(EditorMessage::SelectComment(
                            thread_id.clone()
                        ))
                    ))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false),
                        )
                    })
            ]
            .spacing(SPACING_8);
            if let Some(root) = root {
                let root_id = root.id().to_owned();
                if editor.editing_comment_message() == Some((thread_id.as_str(), root_id.as_str()))
                {
                    let edit_thread = thread_id.clone();
                    card = card
                        .push(
                            text_editor(
                                editor
                                    .comment_reply_draft(&thread_id)
                                    .expect("every rendered comment thread has a reply draft"),
                            )
                            .placeholder("Edit comment message")
                            .on_action(move |action| {
                                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                                    EditorMessage::EditCommentReplyDraft {
                                        thread_id: edit_thread.clone(),
                                        action,
                                    },
                                ))
                            })
                            .height(Length::Fixed(72.0))
                            .style(move |_, status| multiline_field_style(theme, status)),
                        )
                        .push(
                            row![
                                button(text("Save edit")).on_press(
                                    ProjectSurfaceMessage::EditorCenter(
                                        EditorCenterMessage::Workspace(
                                            EditorMessage::SaveEditedCommentMessage {
                                                thread_id: thread_id.clone(),
                                                message_id: root_id.clone(),
                                            },
                                        ),
                                    ),
                                ),
                                button(text("Cancel edit")).on_press(
                                    ProjectSurfaceMessage::EditorCenter(
                                        EditorCenterMessage::Workspace(
                                            EditorMessage::CancelEditCommentMessage,
                                        ),
                                    ),
                                ),
                            ]
                            .spacing(6),
                        );
                } else {
                    card = card.push(
                        row![
                            button(text("Edit message")).on_press(
                                ProjectSurfaceMessage::EditorCenter(
                                    EditorCenterMessage::Workspace(
                                        EditorMessage::BeginEditCommentMessage {
                                            thread_id: thread_id.clone(),
                                            message_id: root_id.clone(),
                                            body: root.body().to_owned(),
                                        },
                                    ),
                                ),
                            ),
                            button(text("Delete message")).on_press(
                                ProjectSurfaceMessage::EditorCenter(
                                    EditorCenterMessage::Workspace(
                                        EditorMessage::DeleteCommentMessage {
                                            thread_id: thread_id.clone(),
                                            message_id: root_id,
                                        },
                                    ),
                                ),
                            ),
                        ]
                        .spacing(6),
                    );
                }
            }
            let replies_collapsed = editor.comment_replies_collapsed(&thread_id);
            let reply_count = thread.messages().len().saturating_sub(1);
            if reply_count > 0 {
                card = card.push(
                    button(text(if replies_collapsed {
                        format!("Expand {reply_count} replies")
                    } else {
                        format!("Collapse {reply_count} replies")
                    }))
                    .on_press(ProjectSurfaceMessage::EditorCenter(
                        EditorCenterMessage::Workspace(EditorMessage::ToggleCommentReplies {
                            thread_id: thread_id.clone(),
                            collapsed: !replies_collapsed,
                        }),
                    )),
                );
            }
            for reply in thread
                .messages()
                .iter()
                .skip(1)
                .filter(|_| !replies_collapsed)
            {
                let reply_id = reply.id().to_owned();
                if editor.editing_comment_message() == Some((thread_id.as_str(), reply_id.as_str()))
                {
                    let edit_thread = thread_id.clone();
                    card = card
                        .push(
                            text_editor(
                                editor
                                    .comment_reply_draft(&thread_id)
                                    .expect("every rendered comment thread has a reply draft"),
                            )
                            .placeholder("Edit comment message")
                            .on_action(move |action| {
                                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                                    EditorMessage::EditCommentReplyDraft {
                                        thread_id: edit_thread.clone(),
                                        action,
                                    },
                                ))
                            })
                            .height(Length::Fixed(72.0))
                            .style(move |_, status| multiline_field_style(theme, status)),
                        )
                        .push(
                            row![
                                button(text("Save edit")).on_press(
                                    ProjectSurfaceMessage::EditorCenter(
                                        EditorCenterMessage::Workspace(
                                            EditorMessage::SaveEditedCommentMessage {
                                                thread_id: thread_id.clone(),
                                                message_id: reply_id.clone(),
                                            },
                                        ),
                                    ),
                                ),
                                button(text("Cancel edit")).on_press(
                                    ProjectSurfaceMessage::EditorCenter(
                                        EditorCenterMessage::Workspace(
                                            EditorMessage::CancelEditCommentMessage,
                                        ),
                                    ),
                                ),
                            ]
                            .spacing(6),
                        );
                } else {
                    card = card.push(
                        row![
                            text(format!("Reply: {}", reply.body())).size(12),
                            button(text("Edit message")).on_press(
                                ProjectSurfaceMessage::EditorCenter(
                                    EditorCenterMessage::Workspace(
                                        EditorMessage::BeginEditCommentMessage {
                                            thread_id: thread_id.clone(),
                                            message_id: reply_id.clone(),
                                            body: reply.body().to_owned(),
                                        },
                                    ),
                                ),
                            ),
                            button(text("Delete message")).on_press(
                                ProjectSurfaceMessage::EditorCenter(
                                    EditorCenterMessage::Workspace(
                                        EditorMessage::DeleteCommentMessage {
                                            thread_id: thread_id.clone(),
                                            message_id: reply_id,
                                        },
                                    ),
                                ),
                            ),
                        ]
                        .spacing(6),
                    );
                }
            }
            if let CommentAnchor::Orphaned {
                quote,
                context_before,
                context_after,
                ..
            } = thread.anchor()
            {
                card = card.push(
                    container(
                        column![
                            text("Anchor needs attention")
                                .size(12)
                                .font(Font {
                                    weight: font::Weight::Bold,
                                    ..Font::DEFAULT
                                })
                                .color(theme.palette().comment_orphaned),
                            text(format!("{context_before}[{quote}]{context_after}"))
                                .size(12)
                                .color(theme.palette().secondary_text),
                            row![
                                button(text("Reattach to selection")).on_press(
                                    ProjectSurfaceMessage::EditorCenter(
                                        EditorCenterMessage::Workspace(
                                            EditorMessage::ReattachComment(thread_id.clone())
                                        )
                                    )
                                ),
                                button(text("Make document comment")).on_press(
                                    ProjectSurfaceMessage::EditorCenter(
                                        EditorCenterMessage::Workspace(
                                            EditorMessage::ConvertCommentToDocument(
                                                thread_id.clone()
                                            )
                                        )
                                    )
                                ),
                            ]
                            .spacing(6),
                        ]
                        .spacing(6),
                    )
                    .padding(8)
                    .style(move |_| components::surface(theme, Surface::Panel, Interaction::Error)),
                );
            }
            if editor
                .editing_comment_message()
                .is_none_or(|(editing_thread, _)| editing_thread != thread_id)
            {
                let reply_id = thread_id.clone();
                let reply_editor = text_editor(
                    editor
                        .comment_reply_draft(&thread_id)
                        .expect("every rendered comment thread has a reply draft"),
                )
                .placeholder("Reply to thread")
                .on_action(move |action| {
                    ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                        EditorMessage::EditCommentReplyDraft {
                            thread_id: reply_id.clone(),
                            action,
                        },
                    ))
                })
                .height(Length::Fixed(68.0));
                let reply_editor = if thread_index == 0 {
                    reply_editor.id(HarnessTarget::CommentReply.id())
                } else {
                    reply_editor
                };
                card = card.push(
                    reply_editor.style(move |_, status| multiline_field_style(theme, status)),
                );
            }
            card = card.push(
                row![
                    comment_action(
                        "Reply",
                        ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                            EditorMessage::SubmitCommentReply {
                                thread_id: thread_id.clone(),
                            },
                        )),
                        theme,
                    ),
                    comment_action(
                        if thread.resolved() {
                            "Reopen"
                        } else {
                            "Resolve"
                        },
                        ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                            EditorMessage::ToggleCommentResolved {
                                thread_id: thread_id.clone(),
                                resolved: !thread.resolved(),
                            },
                        )),
                        theme,
                    ),
                    comment_action(
                        "Delete thread",
                        ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                            EditorMessage::RequestDeleteCommentThread(thread_id.clone()),
                        )),
                        theme,
                    ),
                ]
                .spacing(SPACING_8),
            );
            if editor.pending_delete_comment() == Some(thread_id.as_str()) {
                card = card.push(
                    container(
                        row![
                            text("Delete this thread?").size(12),
                            comment_action(
                                "Confirm delete",
                                ProjectSurfaceMessage::EditorCenter(
                                    EditorCenterMessage::Workspace(
                                        EditorMessage::ConfirmDeleteCommentThread,
                                    ),
                                ),
                                theme,
                            ),
                            comment_action(
                                "Cancel",
                                ProjectSurfaceMessage::EditorCenter(
                                    EditorCenterMessage::Workspace(
                                        EditorMessage::CancelDeleteCommentThread,
                                    ),
                                ),
                                theme,
                            ),
                        ]
                        .spacing(6)
                        .align_y(iced::alignment::Vertical::Center),
                    )
                    .padding(8)
                    .style(move |_| components::surface(theme, Surface::Panel, Interaction::Error)),
                );
            }
            comments = comments
                .push(
                    container(card)
                        .width(Length::Fill)
                        .padding(8)
                        .style(move |_| {
                            if selected_thread {
                                components::surface(theme, Surface::Panel, Interaction::Selected)
                            } else {
                                iced::widget::container::Style::default()
                            }
                        }),
                )
                .push(
                    container(Space::new().height(1))
                        .width(Length::Fill)
                        .style(move |_| iced::widget::container::Style {
                            background: Some(Background::Color(theme.palette().divider)),
                            ..Default::default()
                        }),
                );
        }
        // A document-level comment is always available: authors often need to
        // leave a note about a chapter before selecting specific prose.
        comments = comments.push(
            text("New comment")
                .size(u32::from(UI_LABEL.size))
                .font(Font {
                    weight: font::Weight::Semibold,
                    ..Font::DEFAULT
                }),
        );
        comments = comments.push(
            text_editor(editor.comment_draft())
                .id(HarnessTarget::CommentDraft.id())
                .placeholder("Write a comment")
                .on_action(|action| {
                    ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                        EditorMessage::EditCommentDraft(action),
                    ))
                })
                .height(Length::Fixed(84.0))
                .style(move |_, status| multiline_field_style(theme, status)),
        );
        comments = comments.push(
            row![
                comment_action(
                    "Add at selection",
                    ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                        EditorMessage::CreateComment {
                            document_level: false,
                        },
                    )),
                    theme,
                ),
                comment_action(
                    "Add to document",
                    ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                        EditorMessage::CreateComment {
                            document_level: true,
                        },
                    )),
                    theme,
                ),
            ]
            .spacing(6),
        );
        if let Some(feedback) = editor.comment_feedback() {
            comments = comments.push(text(feedback).size(12));
        }
        // Comment mutation belongs beside the text it affects. Keep this
        // Inspector section as a compact document index: clicking an entry
        // navigates to its anchor, while the anchored editor popover owns all
        // drafting and thread actions.
        let _legacy_comment_controls = comments;
        let mut comments = column![].spacing(SPACING_8);
        let threads = editor.inspector_comments();
        if threads.is_empty() {
            comments = comments.push(
                column![
                    text("No comments").size(13),
                    text("Select text, then choose Add Comment from its context menu.")
                        .size(12)
                        .color(theme.palette().secondary_text),
                ]
                .spacing(2),
            );
        }
        for thread in threads {
            let thread_id = thread.id().to_owned();
            let selected_thread = editor.selected_comment() == Some(thread_id.as_str());
            let state = if thread.resolved() {
                "Resolved"
            } else {
                "Unresolved"
            };
            let state_color = if thread.resolved() {
                theme.palette().comment_resolved
            } else {
                theme.palette().comment_active
            };
            let root_body = thread
                .messages()
                .first()
                .map_or("Comment", crate::CommentMessageView::body);
            let summary = row![
                column![
                    text(root_body).size(u32::from(UI_BODY.size)),
                    text(comment_anchor_summary(thread.anchor()))
                        .size(u32::from(UI_COMPACT.size))
                        .color(theme.palette().secondary_text),
                ]
                .spacing(SPACING_4),
                Space::new().width(Length::Fill),
                text(state)
                    .size(u32::from(UI_LABEL.size))
                    .font(Font {
                        weight: font::Weight::Semibold,
                        ..Font::DEFAULT
                    })
                    .color(state_color),
            ]
            .spacing(SPACING_8);
            comments = comments.push(
                container(
                    button(summary)
                        .width(Length::Fill)
                        .padding([6, 4])
                        .on_press(ProjectSurfaceMessage::EditorCenter(
                            EditorCenterMessage::Workspace(EditorMessage::SelectComment(thread_id)),
                        ))
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, false),
                            )
                        }),
                )
                .width(Length::Fill)
                .padding(6)
                .style(move |_| {
                    if selected_thread {
                        components::surface(theme, Surface::Panel, Interaction::Selected)
                    } else {
                        iced::widget::container::Style::default()
                    }
                }),
            );
        }
        let [synopsis_expanded, metadata_expanded, comments_expanded] = expanded;
        let mut sections = column![
            button(
                row![
                    text(if synopsis_expanded { "⌄" } else { "›" }).size(12),
                    text("SYNOPSIS").size(12),
                ]
                .spacing(6),
            )
            .on_press(ProjectSurfaceMessage::ToggleInspectorSection(
                InspectorSection::Synopsis,
            ))
            .padding(0)
            .style(move |_, status| {
                components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
            }),
        ]
        .spacing(8);
        if synopsis_expanded {
            sections = sections.push(synopsis);
        }
        sections = sections.push(
            button(
                row![
                    text(if metadata_expanded { "⌄" } else { "›" }).size(12),
                    text("METADATA").size(12),
                ]
                .spacing(6),
            )
            .on_press(ProjectSurfaceMessage::ToggleInspectorSection(
                InspectorSection::Metadata,
            ))
            .padding(0)
            .style(move |_, status| {
                components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
            }),
        );
        if metadata_expanded {
            sections = sections.push(metadata);
        }
        if document_selected && editor.production_comments_enabled() {
            let comment_summary = if editor.inspector_unresolved_comment_count() > 0 {
                format!(
                    "· {} unresolved",
                    editor.inspector_unresolved_comment_count()
                )
            } else {
                format!("· {} comments", editor.inspector_comment_count())
            };
            sections = sections.push(
                button(
                    row![
                        text(if comments_expanded { "⌄" } else { "›" }).size(12),
                        text("COMMENTS").size(12),
                        text(comment_summary)
                            .size(u32::from(UI_COMPACT.size))
                            .color(theme.palette().secondary_text),
                    ]
                    .spacing(6),
                )
                .on_press(ProjectSurfaceMessage::ToggleInspectorSection(
                    InspectorSection::Comments,
                ))
                .padding(0)
                .style(move |_, status| {
                    components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
                }),
            );
            if comments_expanded {
                sections = sections.push(comments);
            }
        }
        column![title, scrollable(sections).height(Length::Fill),]
            .spacing(12)
            .height(Length::Fill)
    } else {
        column![
            text("Inspector").size(12),
            text("No selection").size(13),
            text("Select a group or document in Explorer or Cards to inspect its synopsis and metadata.")
                .size(12),
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
) -> Element<'a, ProjectSurfaceMessage> {
    let label = match workspace.save().state() {
        SaveState::SavedThrough(_) => "Saved".to_owned(),
        SaveState::Dirty { .. } => "Unsaved changes".to_owned(),
        SaveState::Saving { .. } => "Saving changes".to_owned(),
        SaveState::Error(_) => "Couldn't save changes".to_owned(),
    };
    let editor_status = workspace.editor().status_bar();
    let active_count = match editor_status.current_count() {
        StatusCount::Selection(words) => format!("Selection · {words} words"),
        StatusCount::ActiveDocument(words) => format!("Document · {words} words"),
    };
    let pane_focus_action: Element<'a, ProjectSurfaceMessage> = if workspace
        .editor()
        .pane(EditorPane::Companion)
        .is_populated()
    {
        let panes_focused = !explorer_visible && !inspector_visible;
        stationary_tooltip::tooltip(
            button(
                text(if panes_focused {
                    "Restore panes"
                } else {
                    "Focus pane"
                })
                .size(12),
            )
            .padding([3, 6])
            .on_press(ProjectSurfaceMessage::ToggleFocusedPane)
            .style(move |_, status| {
                components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
            }),
            container(
                text(if panes_focused {
                    "Restore the Explorer and Inspector"
                } else {
                    "Temporarily hide the Explorer and Inspector"
                })
                .size(12),
            )
            .padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        )
    } else {
        Space::new().width(0).into()
    };
    let content = row![
        stationary_tooltip::tooltip(
            button(sidebar_toggle_glyph(theme, true))
                .on_press(ProjectSurfaceMessage::ToggleExplorer)
                .padding([4, 0])
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    interaction(status, explorer_visible)
                )),
            container(
                text(if explorer_visible {
                    "Hide Explorer"
                } else {
                    "Show Explorer"
                })
                .size(12)
            )
            .padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        ),
        text(active_count).size(12),
        text(format!(
            "Manuscript · {} words",
            editor_status.manuscript_total()
        ))
        .size(12),
        pane_focus_action,
        Space::new().width(Length::Fill),
        text(label).size(12),
        stationary_tooltip::tooltip(
            button(sidebar_toggle_glyph(theme, false))
                .on_press(ProjectSurfaceMessage::ToggleInspector)
                .padding([4, 0])
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    interaction(status, inspector_visible)
                )),
            container(
                text(if inspector_visible {
                    "Hide Inspector"
                } else {
                    "Show Inspector"
                })
                .size(12)
            )
            .padding([4, 6]),
            components::surface(theme, Surface::Elevated, Interaction::Rest),
        ),
    ]
    .align_y(iced::alignment::Vertical::Center)
    .spacing(14);
    focus::f6_region(
        F6Region::StatusBar,
        container(content)
            .padding([6, 12])
            .width(Length::Fill)
            .height(Length::Fixed(f32::from(STATUS_HEIGHT)))
            .style(move |_| components::surface(theme, Surface::Status, Interaction::Rest)),
    )
}

/// Compact sidebar affordance matching the two-pane controls in the Penpot
/// status bar. `left` mirrors the divider for the Explorer versus Inspector.
fn sidebar_toggle_glyph<'a>(
    theme: ParchMintTheme,
    left: bool,
) -> Element<'a, ProjectSurfaceMessage> {
    let divider = container(Space::new()).width(1).height(13).style(move |_| {
        iced::widget::container::Style {
            background: Some(Background::Color(theme.palette().secondary_text)),
            ..Default::default()
        }
    });
    let narrow = container(Space::new())
        .width(if left { 4 } else { 8 })
        .height(13);
    let wide = container(Space::new())
        .width(if left { 7 } else { 4 })
        .height(13);
    container(if left {
        row![narrow, divider, wide]
    } else {
        row![wide, divider, narrow]
    })
    .padding(2)
    .style(move |_| iced::widget::container::Style {
        border: Border {
            color: theme.palette().secondary_text,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// Recovery preserves the reference status-chrome silhouette without exposing
/// pane toggles for sidebars that are intentionally absent from the recovery
/// workspace.
fn recovery_status_bar<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let word_count = workspace.editor().status_bar().manuscript_total();
    let content = row![
        icon_sized(Icon::Project, 14),
        text(format!("{word_count} words")).size(12),
        Space::new().width(Length::Fill),
        icon_sized(Icon::History, 14),
    ]
    .align_y(iced::alignment::Vertical::Center)
    .spacing(12);
    focus::f6_region(
        F6Region::StatusBar,
        container(content)
            .padding([6, 12])
            .width(Length::Fill)
            .height(Length::Fixed(f32::from(STATUS_HEIGHT)))
            .style(move |_| components::surface(theme, Surface::Status, Interaction::Rest)),
    )
}

fn modal_view<'a>(
    modal: ProjectModal,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
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
            ..
        } => (
            "Restore project history",
            format!(
                "Restore “{checkpoint_label}”? This replaces the entire current project. That version recorded changes to {affected_summary}."
            ),
        ),
        ProjectModal::DeleteMetadataField { field_id } => (
            "Delete metadata field",
            format!("Remove {field_id} and its values from every applicable hierarchy item."),
        ),
        ProjectModal::DeleteStyle { style_id } => (
            "Delete custom style",
            format!("Remove {style_id}. Text using it will fall back to an available style."),
        ),
        ProjectModal::ReinitializeHistory => (
            "Reinitialize History",
            "Preserve the damaged History store when possible, then create a new empty History. Project documents are not changed.".to_owned(),
        ),
        ProjectModal::Error { .. } => unreachable!("error modals return above"),
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
                    button(text("Confirm"))
                        .on_press(ProjectSurfaceMessage::Project(match modal {
                            ProjectModal::HistoryRestore { .. } =>
                                ProjectMessage::ConfirmHistoryRestore,
                            ProjectModal::DeleteMetadataField { .. } =>
                                ProjectMessage::ConfirmDeleteMetadataField,
                            ProjectModal::DeleteStyle { .. } => ProjectMessage::ConfirmDeleteStyle,
                            ProjectModal::ReinitializeHistory =>
                                ProjectMessage::ConfirmHistoryReinitialize,
                            ProjectModal::Error { .. } => ProjectMessage::DismissModal,
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
    .style(move |_| components::surface(theme, Surface::Dialog, Interaction::Focused))
    .into()
}

fn interaction(status: iced::widget::button::Status, selected: bool) -> Interaction {
    match status {
        iced::widget::button::Status::Active if selected => Interaction::Selected,
        iced::widget::button::Status::Active => Interaction::Rest,
        iced::widget::button::Status::Hovered => Interaction::Hovered,
        iced::widget::button::Status::Pressed => Interaction::Pressed,
        iced::widget::button::Status::Disabled => Interaction::Disabled,
    }
}

/// Reference navigation uses a tint and an underline for selection rather than
/// the generic outlined control treatment used by form actions.
fn flat_selection_button_style(
    theme: ParchMintTheme,
    status: iced::widget::button::Status,
    selected: bool,
) -> iced::widget::button::Style {
    let mut style = components::button_style(theme, ButtonKind::Quiet, interaction(status, false));
    if selected && matches!(status, iced::widget::button::Status::Active) {
        style.background = Some(Background::Color(theme.palette().accent_subtle));
        style.text_color = theme.palette().accent;
        style.border = Border::default();
    }
    style
}

/// The Cards section picker is navigation embedded in content, rather than a
/// separate control strip. A selected section therefore changes text color
/// without becoming a filled pill.
fn text_tab_button_style(
    theme: ParchMintTheme,
    status: iced::widget::button::Status,
    selected: bool,
) -> iced::widget::button::Style {
    let mut style = components::button_style(theme, ButtonKind::Quiet, interaction(status, false));
    if selected && matches!(status, iced::widget::button::Status::Active) {
        style.text_color = theme.palette().accent;
        style.border = Border::default();
    }
    style
}

fn ribbon_indicator_style(theme: ParchMintTheme, selected: bool) -> iced::widget::container::Style {
    if selected {
        iced::widget::container::Style {
            background: Some(Background::Color(theme.palette().accent)),
            ..Default::default()
        }
    } else {
        iced::widget::container::Style::default()
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

fn multiline_field_style(
    theme: ParchMintTheme,
    status: iced::widget::text_editor::Status,
) -> iced::widget::text_editor::Style {
    let interaction = match status {
        iced::widget::text_editor::Status::Active => Interaction::Rest,
        iced::widget::text_editor::Status::Hovered => Interaction::Hovered,
        iced::widget::text_editor::Status::Focused { .. } => Interaction::Focused,
        iced::widget::text_editor::Status::Disabled => Interaction::Disabled,
    };
    let field = components::field_style(theme, interaction);
    iced::widget::text_editor::Style {
        background: field.background,
        border: field.border,
        placeholder: field.placeholder,
        value: field.value,
        selection: field.selection,
    }
}

pub(crate) fn fixture_surface(workspace: &ProjectWorkspace) -> Element<'static, ProjectMessage> {
    let ribbon = container(text(
        "Editor    Cards    History    Recently Deleted    Export    Settings",
    ))
    .padding([10, 16])
    .width(Length::Fill)
    .height(52)
    .style(toolbar_style);

    let sidebar = container(
        column![
            text(match workspace.sidebar_surface() {
                SidebarSurface::Explorer => "Explorer                         Search",
                SidebarSurface::GlobalSearch => "Back to Explorer        Global Search",
            })
            .size(16),
            text(sidebar_text(workspace)).size(13),
        ]
        .spacing(14),
    )
    .padding(16)
    .width(280)
    .height(Length::Fill)
    .style(sidebar_style);

    let main = container(
        column![
            text(main_title(workspace)).size(22),
            text(main_text(workspace)).size(14),
        ]
        .spacing(18),
    )
    .padding(24)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(main_style);

    let inspector = container(
        column![
            text("Inspector").size(16),
            text("Synopsis").size(13),
            text(
                workspace
                    .explorer()
                    .synopsis("chapter-one")
                    .unwrap_or("No selection")
                    .to_owned(),
            )
            .size(12),
            text("Metadata\nPoint of view    first person").size(12),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(320)
    .height(Length::Fill)
    .style(sidebar_style);

    let workspace_row = row![sidebar, main, inspector].height(Length::Fill);
    let status = container(text(status_text(workspace)).size(12))
        .padding([7, 12])
        .width(Length::Fill)
        .height(32)
        .style(status_style);

    container(column![ribbon, workspace_row, status])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(workspace_style)
        .into()
}

fn sidebar_text(workspace: &ProjectWorkspace) -> String {
    match workspace.sidebar_surface() {
        SidebarSurface::Explorer => {
            "▾ Manuscript\n  ▾ Part One\n      Chapter One\n      Chapter Two\n    Chapter Three\n▾ Research\n    Research Notes"
                .to_owned()
        }
        SidebarSurface::GlobalSearch => format!(
            "Query: {}\nAa   Whole word\n\nChapter One\n… beside the river, the path …",
            workspace.global_search().query()
        ),
    }
}

fn main_title(workspace: &ProjectWorkspace) -> &'static str {
    match workspace.fixture() {
        ProjectFixture::Explorer => "Chapter One",
        ProjectFixture::Cards => "Cards · Manuscript",
        ProjectFixture::GlobalSearch => "Replace Preview",
        ProjectFixture::History => "Project History",
        ProjectFixture::RecentlyDeleted => "Recently Deleted",
        ProjectFixture::SettingsAppearance => "Settings · Appearance",
        ProjectFixture::Export => "Export",
        ProjectFixture::ErrorRecovery => "Recover unsaved changes",
    }
}

fn main_text(workspace: &ProjectWorkspace) -> String {
    match workspace.fixture() {
        ProjectFixture::Explorer => {
            "The river narrowed beyond the old stone bridge.\n\nA complete editor surface remains mounted in the project shell."
                .to_owned()
        }
        ProjectFixture::Cards => "Part One\n\n  Chapter One\n  A first-person opening beside the river.\n\n  Chapter Two\n\nChapter Three"
            .to_owned(),
        ProjectFixture::GlobalSearch => {
            "☒ Manuscript\n  ☑ Chapter One\n    ☑ river — first match\n    ☑ river — second match\n  ☐ Chapter Two"
                .to_owned()
        }
        ProjectFixture::History => "Today\n\nDraft Two · Named snapshot\nAutosave · Chapter One\n\nCheckpoint                         Current\nThe narrow river                   The winding river"
            .to_owned(),
        ProjectFixture::RecentlyDeleted => "Deleted Part\nFormer location: Part One\n\nFormatted preview\nThe complete deleted subtree is available to restore."
            .to_owned(),
        ProjectFixture::SettingsAppearance => "Appearance\n\n◉ System    ○ Light    ○ Dark\n\nSystem follows the operating-system appearance while ParchMint is running."
            .to_owned(),
        ProjectFixture::Export => format!(
            "Scope                 Entire Manuscript\nOutput                {}\nTitles and page breaks Inherit\nNumber documents       {}\n\nExport",
            workspace.export().output_name(),
            if workspace.export().numbers_documents() {
                "On"
            } else {
                "Off"
            }
        ),
        ProjectFixture::ErrorRecovery => match workspace.content_state() {
            ContentState::Recovery => "ParchMint can replay valid unsaved edits on top of the last completed autosave.\n\nRecover edits    Open last saved"
                .to_owned(),
            ContentState::Empty => "No content yet".to_owned(),
            ContentState::Loading => "Loading project…".to_owned(),
            ContentState::Error(error) => format!("The project needs attention\n\n{error}"),
            ContentState::Ready => "Recovered edits are ready in the editor.".to_owned(),
        },
    }
}

fn status_text(workspace: &ProjectWorkspace) -> String {
    format!(
        "Explorer shown    Inspector shown                                  {:?}",
        workspace.save().state()
    )
}

fn workspace_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.base.text),
        ..container::Style::default()
    }
}

fn toolbar_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        border: border::color(palette.background.strong.color).width(1),
        ..container::Style::default()
    }
}

fn sidebar_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.weak.text),
        border: border::color(palette.background.strong.color).width(1),
        ..container::Style::default()
    }
}

fn main_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        ..container::Style::default()
    }
}

fn status_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.strong.color)),
        text_color: Some(palette.background.strong.text),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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

    fn assert_fixture_hash(fixture: ProjectFixture, theme: &Theme, appearance: ResolvedAppearance) {
        let workspace = ProjectWorkspace::from_fixture(fixture);
        let stem = workspace.fixture_reference(appearance);
        let mut simulator = Simulator::<ProjectMessage>::with_size(
            crate::visual_verification::visual_settings(),
            Size::new(1_440.0, 900.0),
            fixture_surface(&workspace),
        );
        let snapshot = simulator
            .snapshot(theme)
            .expect("headless project snapshot");
        let renderer = format!("{snapshot:?}");
        assert!(
            renderer.contains("renderer: \"tiny-skia\""),
            "headless fixture requires the pinned tiny-skia renderer: {renderer}"
        );
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(stem);
        assert!(
            snapshot.matches_hash(&base).expect("compare fixture hash"),
            "project fixture hash changed for {stem}"
        );
        assert!(
            base.with_file_name(format!("{stem}-tiny-skia.sha256"))
                .is_file(),
            "checked-in tiny-skia fixture hash is required for {stem}"
        );
    }

    #[test]
    fn card_metadata_chip_colours_are_stable_for_normalized_field_labels() {
        let light = ParchMintTheme::new(ResolvedAppearance::Light);
        let dark = ParchMintTheme::new(ResolvedAppearance::Dark);

        assert_eq!(
            stable_metadata_label_hash("Point of view"),
            stable_metadata_label_hash("  point   OF\tview ")
        );
        assert_eq!(
            card_metadata_chip_colors(light, "Point of view"),
            card_metadata_chip_colors(light, "point OF view")
        );
        assert_ne!(
            card_metadata_chip_colors(light, "Point of view"),
            card_metadata_chip_colors(dark, "Point of view"),
            "chip tints must follow the active theme"
        );
    }

    #[test]
    fn cards_render_labelled_metadata_chips_without_using_colour_as_the_label() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            cards_center(&workspace, ParchMintTheme::new(ResolvedAppearance::Light)),
        );

        assert!(simulator.find("Point of view: first person").is_ok());
    }

    #[test]
    fn every_requirement_linked_project_view_renders_in_light_and_dark() {
        for fixture in [
            ProjectFixture::Explorer,
            ProjectFixture::Cards,
            ProjectFixture::GlobalSearch,
            ProjectFixture::History,
            ProjectFixture::RecentlyDeleted,
            ProjectFixture::SettingsAppearance,
            ProjectFixture::Export,
            ProjectFixture::ErrorRecovery,
        ] {
            assert_fixture_hash(fixture, &Theme::Light, ResolvedAppearance::Light);
            assert_fixture_hash(fixture, &Theme::Dark, ResolvedAppearance::Dark);
        }
    }

    #[test]
    fn semantic_project_shell_renders_cards_and_recovery_in_both_appearances() {
        for (fixture, destination) in [
            (ProjectFixture::Cards, RibbonDestination::Cards),
            (ProjectFixture::ErrorRecovery, RibbonDestination::Editor),
        ] {
            for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
                let workspace = ProjectWorkspace::from_fixture(fixture);
                let theme = ParchMintTheme::new(appearance);
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
                let snapshot = simulator
                    .snapshot(&theme.iced_theme())
                    .expect("headless semantic project shell snapshot");
                assert!(
                    format!("{snapshot:?}").contains("renderer: \"tiny-skia\""),
                    "headless project surface requires tiny-skia"
                );
            }
        }
    }

    #[test]
    fn cards_keep_hierarchy_mutation_in_the_explorer() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
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

        assert!(simulator.find("Edit").is_err());
        assert!(simulator.find("New group").is_err());
        assert!(simulator.find("New document").is_err());
    }

    #[test]
    fn compact_card_projection_preserves_unicode_and_collapses_multiline_values() {
        assert_eq!(
            compact_card_projection("  A\nlong\tSynopsis  ", 64),
            "A long Synopsis"
        );
        assert_eq!(compact_card_projection("éclair", 1), "é…");
    }

    #[test]
    fn reference_shell_layout_uses_the_1440_desktop_columns() {
        let layout = ShellLayout::for_window(1_440, 900);

        assert_eq!(layout.ribbon().height(), 52);
        assert_eq!(layout.status_bar().height(), 32);
        assert_eq!(layout.explorer().width(), 280);
        assert_eq!(layout.inspector().width(), 320);
        assert_eq!(layout.center().width(), 840);
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
        assert!(cards_surface.find("Manuscript outline").is_ok());
        assert!(cards_surface.find("EXPLORER").is_ok());
        assert!(
            cards_surface
                .find(HarnessTarget::InspectorTitle.id())
                .is_ok()
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
        assert!(search_surface.find("Replace with (optional)").is_ok());
        assert!(
            search_surface
                .find(HarnessTarget::InspectorTitle.id())
                .is_ok()
        );
        assert!(search_surface.find("1 match in 1 document").is_ok());
        assert!(search_surface.find("Chapter One").is_ok());
        assert!(search_surface.find("1 match").is_ok());
    }

    #[test]
    fn cards_surface_renders_the_selected_authoritative_inspector() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
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

        for content in [
            "Chapter One",
            "SYNOPSIS",
            "A first-person opening beside the river.",
            "METADATA",
            "Point of view",
            "first person",
            "Location",
        ] {
            assert!(
                simulator.find(content).is_ok(),
                "Cards Inspector shows {content}"
            );
        }
        let snapshot = simulator
            .snapshot(&theme.iced_theme())
            .expect("Cards Inspector headless render");
        assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
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
        assert!(simulator.find("Recovered changes are ready").is_ok());
        assert!(simulator.find("Recover changes").is_ok());
        assert!(simulator.find("Discard").is_ok());
        assert!(
            simulator
                .find("ParchMint recovered newer edits before opening the workspace.")
                .is_ok()
        );
        assert!(simulator.find("EXPLORER").is_err());
        assert!(simulator.find("Inspector").is_err());
        assert!(simulator.find("Document History").is_err());
        assert!(
            simulator
                .find(format!(
                    "{} words",
                    workspace.editor().status_bar().manuscript_total()
                ))
                .is_ok()
        );
    }

    #[test]
    fn comment_inspector_explains_that_creation_starts_from_selected_text() {
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
        assert!(simulator.find("No comments").is_ok());
        assert!(
            simulator
                .find("Select text, then choose Add Comment from its context menu.")
                .is_ok()
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
                    "Selected thread",
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
        assert_eq!(
            workspace.editor().inspector_comments()[0].id(),
            selected_id.as_str()
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
        assert!(simulator.find("Selected thread").is_ok());
        assert!(simulator.find("Edit message").is_err());
        assert!(simulator.find("Reply to thread").is_err());
        let snapshot = simulator
            .snapshot(&theme.iced_theme())
            .expect("selected comment Inspector render");
        assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
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

        assert!(simulator.find("No comments").is_ok());
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
                        assert!(simulator.find("Manuscript outline").is_ok());
                        assert!(simulator.find("Part One").is_ok());
                        assert!(simulator.find("Opening Scene").is_ok());
                        assert!(simulator.find("The opening synopsis.").is_ok());
                        assert!(simulator.find("Final").is_ok());
                    }
                    RibbonDestination::RecentlyDeleted => {
                        assert!(simulator.find("Discarded Part").is_ok());
                        assert!(simulator.find("Deleted document contents").is_ok());
                        assert!(simulator.find("Restore item").is_ok());
                        assert!(simulator.find("Manuscript").is_err());
                    }
                    RibbonDestination::Settings => {
                        assert!(
                            simulator
                                .find("Operating system changes Light → Dark; every open ParchMint window updates immediately.")
                                .is_ok()
                        );
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
    fn history_and_export_keep_their_reference_column_labels() {
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        for (fixture, destination, left_label, right_label) in [
            (
                ProjectFixture::History,
                RibbonDestination::History,
                "Milestones and recoverable project versions",
                "Checkpoint details",
            ),
            (
                ProjectFixture::Export,
                RibbonDestination::Export,
                "MANUSCRIPT OUTPUT",
                "OUTPUT FILE",
            ),
        ] {
            let workspace = ProjectWorkspace::from_fixture(fixture);
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
            assert!(simulator.find(left_label).is_ok());
            assert!(simulator.find(right_label).is_ok());
        }
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
        assert!(simulator.find("Current").is_ok());
        assert!(simulator.find("1").is_ok());
        assert!(simulator.find("blue").is_ok());
        assert!(simulator.find("green").is_ok());
        assert!(
            simulator
                .find("Named snapshot · 1 document · Version 2 · +2 words")
                .is_ok()
        );
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
    fn explorer_add_menu_publishes_contextual_creation_commands() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        workspace.update(ProjectMessage::ToggleExplorerCreationMenu);

        let messages = interact(&workspace, RibbonDestination::Editor, |explorer| {
            assert!(explorer.find("Add to Part One").is_ok());
            explorer
                .click("Document")
                .expect("visible document creation action");
            explorer
                .click("Group")
                .expect("visible group creation action");
        });

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
    }

    #[test]
    fn hierarchy_context_overlay_exposes_only_applicable_actions() {
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
        assert!(simulator.find("Create document").is_ok());
        assert!(simulator.find("Create group").is_ok());
        assert!(simulator.find("Open in companion").is_err());
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
            .click("Create document")
            .expect("create document action");
        simulator
            .click("Create group")
            .expect("create group action");
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
    fn explorer_and_cards_group_clicks_toggle_the_shared_hierarchy() {
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);

        let mut explorer = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        let mut explorer_surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &explorer,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );
        explorer_surface.click("Part One").expect("Explorer group");
        let explorer_messages = explorer_surface.into_messages().collect::<Vec<_>>();
        assert!(explorer_messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::SelectAndToggleHierarchyExpanded(node_id))
                if node_id == "part-one"
        )));
        assert!(apply_project_messages(&mut explorer, explorer_messages).is_empty());
        assert!(
            !explorer
                .explorer()
                .rows()
                .into_iter()
                .find(|row| row.id == "part-one")
                .expect("Explorer group row")
                .expanded
        );

        let mut explorer_surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            project_surface(
                &explorer,
                RibbonDestination::Editor,
                theme,
                text("Editor").into(),
            ),
        );
        explorer_surface
            .click("Part One")
            .expect("Explorer group click");
        let explorer_messages = explorer_surface.into_messages().collect::<Vec<_>>();
        assert!(explorer_messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::SelectAndToggleHierarchyExpanded(node_id))
                if node_id == "part-one"
        )));
        assert!(apply_project_messages(&mut explorer, explorer_messages).is_empty());
        assert!(
            explorer
                .explorer()
                .rows()
                .into_iter()
                .find(|row| row.id == "part-one")
                .expect("Explorer group row")
                .expanded
        );

        let mut cards = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let mut cards_surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            cards_center(&cards, theme),
        );
        cards_surface.click("Part One").expect("Cards group");
        let card_messages = cards_surface.into_messages().collect::<Vec<_>>();
        assert!(card_messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::SelectAndToggleHierarchyExpanded(node_id))
                if node_id == "part-one"
        )));
        assert!(apply_project_messages(&mut cards, card_messages).is_empty());
        assert_eq!(cards.explorer().selected_ids(), ["part-one"]);
        assert!(
            !cards
                .explorer()
                .rows()
                .into_iter()
                .find(|row| row.id == "part-one")
                .expect("Cards group row")
                .expanded
        );

        let mut cards_surface = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_440.0, 900.0),
            cards_center(&cards, theme),
        );
        cards_surface
            .click("Part One")
            .expect("second Cards group click");
        let card_messages = cards_surface.into_messages().collect::<Vec<_>>();
        assert!(card_messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::SelectAndToggleHierarchyExpanded(node_id))
                if node_id == "part-one"
        )));
        assert!(apply_project_messages(&mut cards, card_messages).is_empty());
        assert!(
            cards
                .explorer()
                .rows()
                .into_iter()
                .find(|row| row.id == "part-one")
                .expect("Cards group row")
                .expanded
        );
    }

    #[test]
    fn continuous_explorer_drag_uses_one_production_lifecycle_and_blank_release_cancels() {
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
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Move(Target::Text("Research".to_owned())),
        );
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Release {
                button: iced::mouse::Button::Left,
                target: Some(Target::Text("Research".to_owned())),
            },
        );

        // Continue in this exact Emulator/cache. A release after leaving every
        // target cannot commit the destination from a prior hover.
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Press {
                button: iced::mouse::Button::Left,
                target: Some(Target::Text("Chapter One".to_owned())),
            },
        );
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Move(Target::Text("Research".to_owned())),
        );
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Move(Target::Point(iced::Point::new(700.0, 15.0))),
        );
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Release {
                button: iced::mouse::Button::Left,
                target: Some(Target::Point(iced::Point::new(700.0, 15.0))),
            },
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
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Move(Target::Point(iced::Point::new(700.0, 400.0))),
        );
        run_drag_instruction(
            &mut emulator,
            &program,
            &mut events,
            Mouse::Release {
                button: iced::mouse::Button::Left,
                target: Some(Target::Point(iced::Point::new(700.0, 400.0))),
            },
        );
        let (state, _) = emulator.into_state();
        let moves = state
            .effects
            .iter()
            .filter(|effect| matches!(effect, crate::ProjectEffect::MoveHierarchy { .. }))
            .collect::<Vec<_>>();
        assert_eq!(moves.len(), 1);
        assert!(matches!(
            moves[0],
            crate::ProjectEffect::MoveHierarchy { node_ids, destination }
                if node_ids == &vec!["chapter-one".to_owned()]
                    && destination == &DragDestination::IntoGroup("research".to_owned())
        ));
        assert!(
            !state.effects.iter().any(|effect| matches!(
                effect,
                crate::ProjectEffect::OpenDocumentInPrimary(document_id)
                    if document_id == "chapter-one"
            )),
            "reselecting the active document must not remount its preview"
        );
        assert_eq!(
            state
                .workspace
                .editor()
                .pane(crate::EditorPane::Primary)
                .active_document(),
            Some("chapter-one")
        );
        assert!(state.workspace.hierarchy_drag_source().is_none());
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
                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::CommitHierarchyDrop) => {
                    state.workspace.update(ProjectMessage::CommitHierarchyDrag)
                }
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
    fn rendered_cards_drag_starts_after_movement_and_reorders_at_a_card_target() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        let theme = ParchMintTheme::new(ResolvedAppearance::Light);
        let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
            Settings::default(),
            Size::new(1_000.0, 700.0),
            cards_center(&workspace, theme),
        );
        let source = simulator.find("Chapter One").expect("rendered card source");
        let source_position = source.visible_bounds().expect("card bounds").center();
        let target = simulator.find("Chapter Two").expect("rendered card target");
        let target_position = target.visible_bounds().expect("target bounds").center();
        simulator.point_at(source_position);
        simulator.simulate([iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Left,
        ))]);
        simulator.point_at(target_position);
        simulator.simulate([
            iced::Event::Mouse(iced::mouse::Event::CursorMoved {
                position: target_position,
            }),
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left,
            )),
        ]);
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::BeginHierarchyDrag { source_id, .. })
                if source_id == "chapter-one"
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::SetDragDestination(Some(
                DragDestination::AfterSibling(node_id)
            ))) if node_id == "chapter-two"
        )));
        let effects = apply_project_messages(&mut workspace, messages);
        assert!(matches!(
            effects.as_slice(),
            [crate::ProjectEffect::MoveHierarchy { node_ids, destination: actual }]
                if node_ids == &vec!["chapter-one".to_owned()]
                    && actual == &DragDestination::AfterSibling("chapter-two".to_owned())
        ));
    }

    #[test]
    fn rendered_settings_keep_details_in_their_own_categories() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::SettingsAppearance);

        let messages = interact(&workspace, RibbonDestination::Settings, |settings| {
            assert!(settings.find("SETTINGS").is_ok());
            settings
                .click("Metadata fields")
                .expect("metadata navigation");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());
        assert_eq!(
            workspace.settings().selected_category(),
            SettingsCategory::Metadata
        );

        let messages = interact(&workspace, RibbonDestination::Settings, |settings| {
            settings.click("Point of view").expect("metadata field");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());
        let _metadata = interact(&workspace, RibbonDestination::Settings, |settings| {
            assert!(settings.find("Metadata field details").is_ok());
            assert!(settings.find("SETTINGS").is_ok());
        });

        for category in [SettingsCategory::Dictionaries, SettingsCategory::General] {
            let messages = interact(&workspace, RibbonDestination::Settings, |settings| {
                settings
                    .click(category.label())
                    .expect("rendered settings category");
            });
            assert!(apply_project_messages(&mut workspace, messages).is_empty());
            let messages = interact(&workspace, RibbonDestination::Settings, |settings| {
                assert!(settings.find("SETTINGS").is_ok());
                assert!(settings.find("Metadata field details").is_err());
            });
            assert!(messages.is_empty());
        }
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

        let type_at_synopsis = |workspace: &ProjectWorkspace, value| {
            let theme = ParchMintTheme::new(ResolvedAppearance::Light);
            let mut simulator = Simulator::<ProjectSurfaceMessage>::with_size(
                Settings::default(),
                Size::new(1_440.0, 900.0),
                project_surface(
                    workspace,
                    RibbonDestination::Editor,
                    theme,
                    text("Editor").into(),
                ),
            );
            let heading = simulator.find("SYNOPSIS").expect("rendered synopsis");
            let bounds = heading.visible_bounds().expect("synopsis heading bounds");
            simulator.point_at(iced::Point::new(
                bounds.x + bounds.width - 20.0,
                bounds.y + 30.0,
            ));
            assert_eq!(
                simulator.simulate(iced_test::simulator::click()).first(),
                Some(&iced::event::Status::Captured)
            );
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
            for projection in [
                "Chapter One",
                "SYNOPSIS",
                "A first-person opening beside the river.",
            ] {
                assert!(
                    cards.find(projection).is_ok(),
                    "Cards and Inspector keep the selected hierarchy projection: {projection}"
                );
            }
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
    fn rendered_global_replace_flow_revalidates_before_one_typed_apply_effect() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);

        let messages = interact(&workspace, RibbonDestination::Editor, |explorer| {
            explorer.click("⌕").expect("visible global search control");
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

        let search_ticket = workspace.begin_task(crate::ProjectTask::GlobalSearch {
            generation: workspace.global_search().query_generation(),
        });
        assert!(
            workspace.accept_completion(crate::ProjectTaskCompletion::for_ticket(
                search_ticket,
                crate::ProjectTaskPayload::SearchBatch {
                    results: vec![crate::GlobalSearchResult {
                        document_id: "chapter-one".to_owned(),
                        match_id: "chapter-one-river".to_owned(),
                        prefix: "beside the ".to_owned(),
                        matching_text: "river".to_owned(),
                        suffix: ", the path".to_owned(),
                        indexed_revision: workspace.project_revision(),
                    }],
                    finished: true,
                },
            ))
        );

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
            preview
                .click("Select none")
                .expect("visible preview selection control");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());
        let messages = interact(&workspace, RibbonDestination::Editor, |preview| {
            preview
                .click("Select all")
                .expect("visible preview selection control");
        });
        assert!(apply_project_messages(&mut workspace, messages).is_empty());

        workspace.update(ProjectMessage::MarkDirty(2));
        let messages = interact(&workspace, RibbonDestination::Editor, |preview| {
            assert!(
                preview
                    .find("Selection changed. Revalidate before applying.")
                    .is_ok()
            );
            preview
                .click("Revalidate selection")
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
            }] if included_match_ids == &["chapter-one-river"] && replacement == "shore"
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
                .click("Restore “Draft Two”")
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
                    .find("Restore “Draft Two”? This replaces the entire current project. That version recorded changes to 1 document.")
                    .is_ok()
            );
            confirmation
                .click("Confirm")
                .expect("visible restore confirmation");
        });
        assert_eq!(
            apply_project_messages(&mut workspace, messages),
            [crate::ProjectEffect::RestoreHistory {
                checkpoint_id: "snapshot-draft-two".to_owned(),
                scope: crate::HistoryRestoreScope::EntireProject,
            }]
        );
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
            assert!(editor.find("EXPLORER").is_ok());
            assert!(editor.find("Recovered changes are ready").is_err());
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

    fn apply_editor_messages(
        workspace: &mut ProjectWorkspace,
        messages: Vec<ProjectSurfaceMessage>,
    ) -> Vec<crate::EditorEffect> {
        messages
            .into_iter()
            .map(|message| match message {
                ProjectSurfaceMessage::EditorCenter(message) => message,
                unexpected => {
                    panic!("editor flow emitted unexpected surface message: {unexpected:?}")
                }
            })
            .flat_map(|message| message.workspace_messages())
            .flat_map(|message| workspace.editor_mut().update(message))
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
