//! Private Iced composition for production and deterministic project workspaces.

use iced::widget::{
    Space, button, checkbox, column, container, mouse_area, opaque, rich_text, row, scrollable,
    span, stack, text, text_editor, text_input, tooltip,
};
use iced::{Background, Border, Color, Element, Font, Length, Theme, border, font};
use parchmint_editor_api::{SemanticBlock, SemanticBlockKind, SemanticInlineMark};
use parchmint_ui_api::HistoryMaintenanceStatus;
use std::collections::BTreeMap;

use crate::{
    CommentAnchor, ContentState, DragDestination, EditorMessage, F6Region, FocusTarget,
    HierarchyItemKind, HierarchyRowKind, InspectorSection, MetadataFieldApplicability,
    MetadataFieldTextKind, ProjectFixture, ProjectMessage, ProjectModal, ProjectWorkspace,
    ReplacementCheckState, ReplacementPreviewRowKind, RestoreLocation, RibbonDestination,
    SaveState, SelectionGesture, SettingsCategory, SettingsDetail, ShellLayout, SidebarSurface,
    StatusCount, StyleProperty,
    components::{self, ButtonKind, Interaction, Surface},
    design_tokens::{ParchMintTheme, RIBBON_HEIGHT, STATUS_HEIGHT},
    focus,
    iced_editor_surface::EditorCenterMessage,
    icons::{Icon, icon, icon_sized},
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
    ToggleInspectorSection(InspectorSection),
    OpenContextualHistory,
    BeginResize(SidebarPanel),
    ResizePointer(f32),
    EndResize,
    LoadMoreHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SidebarPanel {
    Explorer,
    Editor,
    Inspector,
}

// Divider lines belong to the adjacent sidebar's reference width; they must
// not shrink the manuscript allocation between the 280 px Explorer and 320 px
// Inspector columns.
const SIDEBAR_SPLITTER_WIDTH: u32 = 1;

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
        "ParchMint",
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
    project_title: &'static str,
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
    let body = mouse_area(body)
        .on_move(|point| ProjectSurfaceMessage::ResizePointer(point.x))
        .on_release(ProjectSurfaceMessage::EndResize);
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
        RibbonDestination::History => {
            "History is app-managed; Git terminology and object IDs are never shown."
        }
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
            .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
    )
    .on_press(ProjectSurfaceMessage::BeginResize(panel))
    .interaction(iced::mouse::Interaction::ResizingHorizontally)
    .into()
}

fn ribbon(
    project_title: &'static str,
    destination: RibbonDestination,
    theme: ParchMintTheme,
) -> Element<'static, ProjectSurfaceMessage> {
    let destinations = [
        (Icon::Editor, "Editor", RibbonDestination::Editor),
        (Icon::Cards, "Cards", RibbonDestination::Cards),
        (Icon::History, "History", RibbonDestination::History),
        (
            Icon::RecentlyDeleted,
            "Recently Deleted",
            RibbonDestination::RecentlyDeleted,
        ),
        (Icon::Export, "Export", RibbonDestination::Export),
        (Icon::Settings, "Settings", RibbonDestination::Settings),
    ];
    let buttons =
        destinations
            .into_iter()
            .fold(row![].spacing(4), |row, (icon_kind, label, item)| {
                let selected = item == destination;
                let control: Element<'static, ProjectSurfaceMessage> = column![
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
                .spacing(0)
                .into();
                row.push(tooltip(
                    control,
                    container(text(label).size(12)).padding([4, 6]),
                    tooltip::Position::Bottom,
                ))
            });
    let buttons = focus::f6_region(F6Region::ModeSwitch, buttons);
    let title = row![icon(Icon::Project), text(project_title).size(16),]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center)
        .width(230);
    container(
        row![title, buttons]
            .spacing(16)
            .align_y(iced::alignment::Vertical::Center),
    )
    .padding([6, 18])
    .width(Length::Fill)
    .height(Length::Fixed(f32::from(RIBBON_HEIGHT)))
    .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest))
    .into()
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
        .padding(12)
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
        .fold(column![].spacing(1), |column, item| {
            let depth = hierarchy_depth(explorer, item.parent_id);
            let disclosure: Element<'a, ProjectSurfaceMessage> = match item.kind {
                HierarchyRowKind::Root | HierarchyRowKind::Group => button(
                    text(if item.expanded { "▾" } else { "▸" })
                        .size(12)
                        .align_x(iced::alignment::Horizontal::Center),
                )
                .padding(2)
                .width(20)
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::ToggleHierarchyExpanded(item.id.to_owned()),
                ))
                .style(move |_, status| {
                    components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
                })
                .into(),
                HierarchyRowKind::Document => container(text("·").size(12))
                    .width(20)
                    .align_x(iced::alignment::Horizontal::Center)
                    .into(),
            };
            let title = if item.cut_pending {
                format!("{}  (cut)", item.title)
            } else {
                item.title.to_owned()
            };
            let select: Element<'a, ProjectSurfaceMessage> = mouse_area(
                container(text(title).size(13))
                    .padding([5, 6])
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
            .on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::SelectHierarchy {
                    node_id: item.id.to_owned(),
                    gesture: SelectionGesture::Replace,
                },
            ))
            .on_double_click(ProjectSurfaceMessage::Project(
                if item.kind == HierarchyRowKind::Document {
                    ProjectMessage::OpenHierarchyNode(item.id.to_owned())
                } else {
                    ProjectMessage::ToggleHierarchyExpanded(item.id.to_owned())
                },
            ))
            .interaction(iced::mouse::Interaction::Pointer)
            .into();
            let item_row = row![Space::new().width((depth * 14) as f32), disclosure, select]
                .spacing(1)
                .align_y(iced::alignment::Vertical::Center);
            let node_id = item.id.to_owned();
            let row_target = mouse_area(container(item_row).width(Length::Fill))
                .on_right_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::OpenHierarchyContextMenu(node_id.clone()),
                ))
                .on_double_click(ProjectSurfaceMessage::Project(
                    if item.kind == HierarchyRowKind::Document {
                        ProjectMessage::OpenHierarchyNode(node_id.clone())
                    } else {
                        ProjectMessage::ToggleHierarchyExpanded(node_id.clone())
                    },
                ));
            column.push(row_target)
        });
    let rail = column![
        row![
            text("EXPLORER").size(12),
            Space::new().width(Length::Fill),
            button(text("⌕").size(20))
                .padding([2, 5])
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::ShowGlobalSearch
                ))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Quiet,
                    interaction(status, false)
                ))
        ]
        .spacing(4),
        scrollable(rows).height(Length::Fill),
    ]
    .spacing(8)
    .height(Length::Fill);
    hierarchy_context_overlay(workspace, focus::f6_region(F6Region::Explorer, rail), theme)
}

fn hierarchy_drop_strip<'a>(
    target: DragDestination,
    dragging: bool,
    current: Option<&DragDestination>,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    if !dragging {
        return Space::new().height(1).into();
    }
    let active = current == Some(&target);
    mouse_area(
        container(Space::new().height(if active { 5 } else { 3 }))
            .width(Length::Fill)
            .style(move |_| {
                components::surface(
                    theme,
                    Surface::Panel,
                    if active {
                        Interaction::Selected
                    } else {
                        Interaction::Rest
                    },
                )
            }),
    )
    .on_enter(ProjectSurfaceMessage::Project(
        ProjectMessage::SetDragDestination(Some(target)),
    ))
    .on_release(ProjectSurfaceMessage::Project(
        ProjectMessage::CommitHierarchyDrag,
    ))
    .interaction(iced::mouse::Interaction::Grabbing)
    .into()
}

fn hierarchy_context_overlay<'a>(
    workspace: &'a ProjectWorkspace,
    content: Element<'a, ProjectSurfaceMessage>,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let Some(node_id) = workspace.hierarchy_context_menu() else {
        return content;
    };
    let Some(node) = workspace.explorer().row(node_id) else {
        return content;
    };
    let id = node.id.to_owned();
    let mut actions = column![
        row![
            text("Item actions").size(13),
            Space::new().width(Length::Fill),
            button(text("×")).on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::CloseHierarchyContextMenu,
            )),
        ],
        text_input("Rename item", node.title)
            .on_input({
                let id = id.clone();
                move |title| {
                    ProjectSurfaceMessage::Project(ProjectMessage::RenameNode {
                        node_id: id.clone(),
                        title,
                    })
                }
            })
            .padding([6, 8]),
    ]
    .spacing(6);
    if node.kind == HierarchyRowKind::Document {
        actions = actions
            .push(
                button(text("Open")).on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::OpenHierarchyNode(id.clone()),
                )),
            )
            .push(
                button(text("Open in companion")).on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::OpenHierarchyNodeInCompanion(id.clone()),
                )),
            );
    } else {
        actions = actions
            .push(
                button(text("Create document")).on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::RequestCreateHierarchy {
                        parent_id: id.clone(),
                        kind: HierarchyItemKind::Document,
                    },
                )),
            )
            .push(
                button(text("Create group")).on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::RequestCreateHierarchy {
                        parent_id: id.clone(),
                        kind: HierarchyItemKind::Group,
                    },
                )),
            );
    }
    if node.kind != HierarchyRowKind::Root {
        actions = actions
            .push(
                button(text("Copy")).on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::CopySelection,
                )),
            )
            .push(
                button(text("Cut"))
                    .on_press(ProjectSurfaceMessage::Project(ProjectMessage::CutSelection)),
            )
            .push(
                button(text("Delete")).on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::DeleteSelection,
                )),
            );
    }
    stack![
        content,
        container(opaque(container(actions).padding(10).width(220).style(
            move |_| components::surface(theme, Surface::Dialog, Interaction::Focused,)
        ),))
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Top),
    ]
    .into()
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
        .on_input(|query| {
            ProjectSurfaceMessage::Project(ProjectMessage::SetGlobalSearchQuery(query))
        })
        .padding([7, 8])
        .style(move |_, status| components::field_style(theme, field_interaction(status)));
    let controls = row![
        button(text("←  Search").size(16))
            .on_press(ProjectSurfaceMessage::Project(ProjectMessage::ShowExplorer))
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                interaction(status, false)
            )),
        Space::new().width(Length::Fill),
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
    ]
    .align_y(iced::alignment::Vertical::Center)
    .spacing(8);
    let mut grouped = BTreeMap::<&str, Vec<&crate::GlobalSearchResult>>::new();
    for result in search.windowed_results() {
        grouped
            .entry(result.document_id.as_str())
            .or_default()
            .push(result);
    }
    let document_count = grouped.len();
    let results =
        grouped
            .into_iter()
            .fold(column![].spacing(8), |column, (document_id, matches)| {
                let title = workspace
                    .explorer()
                    .title_for_document(document_id)
                    .unwrap_or(document_id);
                let match_count = matches.len();
                let rows =
                    matches
                        .into_iter()
                        .take(2)
                        .fold(column![].spacing(3), |rows, result| {
                            let highlight = Color {
                                a: 0.28,
                                ..theme.palette().accent
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
                            let snippet = rich_text(snippet_spans).size(12);
                            rows.push(
                                button(snippet)
                                    .padding([5, 6])
                                    .width(Length::Fill)
                                    .on_press(ProjectSurfaceMessage::Project(
                                        ProjectMessage::NavigateGlobalSearchResult(
                                            result.match_id.clone(),
                                        ),
                                    ))
                                    .style(move |_, status| {
                                        components::button_style(
                                            theme,
                                            ButtonKind::Quiet,
                                            interaction(status, false),
                                        )
                                    }),
                            )
                        });
                column.push(
                    column![
                        row![
                            text(title).size(12),
                            Space::new().width(Length::Fill),
                            text(search_match_count_label(match_count)).size(12),
                        ]
                        .align_y(iced::alignment::Vertical::Center),
                        rows,
                    ]
                    .spacing(3),
                )
            });
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
        .on_input(|replacement| {
            ProjectSurfaceMessage::Project(ProjectMessage::SetGlobalReplacement(replacement))
        })
        .padding([7, 8])
        .width(Length::Fill)
        .style(move |_, status| components::field_style(theme, field_interaction(status)));
    let replace_action = button(text("Replace").size(13))
        .on_press_maybe(
            (search.is_complete() && !search.results().is_empty()).then_some(
                ProjectSurfaceMessage::Project(ProjectMessage::OpenReplacementPreview),
            ),
        )
        .padding([7, 14])
        .style(move |_, status| {
            components::button_style(theme, ButtonKind::Primary, interaction(status, false))
        });
    column![
        controls,
        query,
        row![replace, replace_action].spacing(8),
        text(if result_count == 1 {
            "1 match in 1 document".to_owned()
        } else {
            format!("{result_count} matches in {document_count} documents")
        })
        .size(12),
        scrollable(results)
            .on_scroll(|viewport| ProjectSurfaceMessage::Project(
                ProjectMessage::SetGlobalSearchScroll(viewport.absolute_offset().y)
            ))
            .height(Length::Fill)
    ]
    .spacing(12)
    .height(Length::Fill)
    .into()
}

fn search_match_count_label(count: usize) -> String {
    format!("{count} {}", if count == 1 { "match" } else { "matches" })
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
    let drag_source = workspace.hierarchy_drag_source().map(str::to_owned);
    let drag_destination = workspace.hierarchy_drag_destination().cloned();
    let items = cards.items().into_iter().filter(|item| item.visible).fold(
        column![].spacing(0),
        |column, item| {
            let metadata = item.metadata.into_iter().fold(
                column![].spacing(2),
                |metadata, (_, label, value)| {
                    metadata.push(
                        row![
                            text(label).size(11),
                            text(" · ").size(11),
                            text(value.unwrap_or("—")).size(12),
                        ]
                        .spacing(0),
                    )
                },
            );
            let node_id = item.node_id.to_owned();
            let disclosure: Element<'a, ProjectSurfaceMessage> = match item.kind {
                HierarchyRowKind::Group => button(text(if item.expanded { "▾" } else { "▸" }))
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::ToggleHierarchyExpanded(node_id.clone()),
                    ))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, item.expanded),
                        )
                    })
                    .into(),
                _ => Space::new().width(28).into(),
            };
            // Cards are a scan-friendly projection of the hierarchy. Direct
            // mutation remains available through Explorer and Inspector; putting
            // text fields into every row made this destination read like a form
            // instead of the compact manuscript overview in the reference.
            let card_content = column![
                row![
                    disclosure,
                    text(item.title).size(16).width(Length::Fill),
                    Space::new().width(Length::Fill),
                    container(metadata).padding([0, 8]).width(220),
                ]
                .align_y(iced::alignment::Vertical::Center),
                text(item.synopsis).size(13),
            ]
            .spacing(6);
            let before = DragDestination::BeforeSibling(node_id.clone());
            let after = DragDestination::AfterSibling(node_id.clone());
            let middle = if item.kind == HierarchyRowKind::Group {
                DragDestination::IntoGroup(node_id.clone())
            } else {
                after.clone()
            };
            let middle_active = drag_destination.as_ref() == Some(&middle);
            let card = mouse_area(row![
                Space::new().width((item.depth * 18) as f32),
                container(card_content)
                    .padding([10, 16])
                    .width(Length::Fill)
                    .style(move |_| {
                        if middle_active {
                            components::surface(theme, Surface::Panel, Interaction::Selected)
                        } else {
                            iced::widget::container::Style::default()
                        }
                    }),
            ])
            .on_enter(ProjectSurfaceMessage::Project(
                ProjectMessage::SetDragDestination(Some(middle)),
            ))
            .on_release(ProjectSurfaceMessage::Project(
                ProjectMessage::CommitHierarchyDrag,
            ))
            .on_right_press(ProjectSurfaceMessage::Project(
                ProjectMessage::OpenHierarchyContextMenu(node_id.clone()),
            ))
            .on_double_click(ProjectSurfaceMessage::Project(
                if item.kind == HierarchyRowKind::Document {
                    ProjectMessage::ActivateCard(node_id.clone())
                } else {
                    ProjectMessage::ToggleHierarchyExpanded(node_id.clone())
                },
            ));
            column
                .push(hierarchy_drop_strip(
                    before,
                    drag_source.is_some(),
                    drag_destination.as_ref(),
                    theme,
                ))
                .push(card)
                .push(hierarchy_drop_strip(
                    after,
                    drag_source.is_some(),
                    drag_destination.as_ref(),
                    theme,
                ))
        },
    );
    let items = if cards.items().iter().all(|item| !item.visible) {
        items.push(text("This section has no cards yet.").size(13))
    } else {
        items
    };
    let content = column![
        text(section_title).size(16),
        text("Manuscript outline").size(12),
        scrollable(items).height(Length::Fill),
    ]
    .spacing(12);
    let content = container(hierarchy_context_overlay(workspace, content.into(), theme))
        .padding([36, 24])
        .width(Length::Fill)
        .height(Length::Fill);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .into()
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
        "Revalidating selected matches against the captured project revision…".to_owned()
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
            "Captured project revision {} · search generation {} · {} included matches",
            preview.captured_project_revision(),
            preview.captured_query_generation(),
            preview.included_match_ids().len(),
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
    let checkpoints =
        history
            .windowed_checkpoints()
            .fold(column![].spacing(6), |column, checkpoint| {
                let checkpoint_id = checkpoint.checkpoint_id.clone();
                let selected = history.selected_checkpoint_id() == Some(checkpoint_id.as_str());
                column.push(
                    container(
                        column![
                            button(text(checkpoint.label()).size(14))
                                .padding([4, 0])
                                .width(Length::Fill)
                                .on_press(ProjectSurfaceMessage::Project(
                                    ProjectMessage::SelectHistoryCheckpoint(checkpoint_id.clone())
                                ))
                                .style(move |_, status| components::button_style(
                                    theme,
                                    ButtonKind::Quiet,
                                    interaction(status, selected),
                                )),
                            text(format!(
                                "{} · {}",
                                checkpoint.category.label(),
                                checkpoint.affected_summary(),
                            ))
                            .size(11),
                        ]
                        .spacing(3),
                    )
                    .padding(8)
                    .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
                )
            });
    let checkpoints = column![
        Space::new().height(history.checkpoint_window_start() as f32 * 72.0),
        checkpoints,
        Space::new().height(history.checkpoint_window_bottom_padding()),
    ];
    let restore: Element<'a, ProjectSurfaceMessage> = history
        .selected_checkpoint_id()
        .map(|checkpoint_id| {
            button(text("Restore selected checkpoint").size(12))
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
            text("History").size(24),
            text("All project checkpoints").size(14),
            scrollable(checkpoints)
                .on_scroll(|viewport| ProjectSurfaceMessage::Project(
                    ProjectMessage::SetHistoryScroll(viewport.absolute_offset().y)
                ))
                .height(Length::Fill),
            load_more,
        ]
        .spacing(12);
    let detail = column![
        text("Checkpoint details").size(24),
        text("Select a checkpoint to compare it with the current document.").size(14),
        error,
        maintenance,
        scrollable(comparison).height(Length::Fill),
        row![
            container(text("Read-only comparison · restoring creates a new checkpoint.").size(12))
                .padding(12)
                .width(Length::Fill)
                .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
            restore,
        ]
        .spacing(12),
    ]
    .spacing(14);
    row![
        container(list)
            .padding(16)
            .width(420)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
        container(detail)
            .padding([20, 30])
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
                        container(text(format!(
                            "Read-only preview · restoring returns the item to {former_location}."
                        ))
                        .size(12))
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
                text("No canonical document snapshot is available for this deleted item.").size(12),
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
    let output_file = row![
        text_input("Output file", export.output_name())
            .on_input_maybe(export.can_start().then_some(|value| {
                ProjectSurfaceMessage::Project(ProjectMessage::SetExportOutputName(value))
            }))
            .padding([9, 12])
            .width(Length::Fill)
            .style(move |_, status| components::field_style(theme, field_interaction(status))),
        button(text("Browse…"))
            .padding([9, 14])
            .on_press_maybe(
                (!export.can_cancel()).then_some(ProjectSurfaceMessage::Project(
                    ProjectMessage::BrowseExportDestination
                ))
            )
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Secondary,
                interaction(status, false)
            )),
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
                    button(text("Export"))
                        .padding([10, 18])
                        .on_press_maybe(
                            export.can_start().then_some(ProjectSurfaceMessage::Project(
                                ProjectMessage::StartExport
                            ))
                        )
                        .style(move |_, status| components::button_style(
                            theme,
                            ButtonKind::Primary,
                            interaction(status, false)
                        )),
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
    let choices =
        settings
            .appearance_choices()
            .into_iter()
            .fold(column![].spacing(8), |column, mode| {
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
                            text(name).size(16).font(Font {
                                weight: font::Weight::Bold,
                                ..Font::DEFAULT
                            }),
                            Space::new().width(40),
                            text(detail).size(14),
                        ]
                        .align_y(iced::alignment::Vertical::Center)
                        .spacing(12),
                    )
                    .width(Length::Fill)
                    .padding([5, 12])
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::SetAppearance(mode),
                    ))
                    .style(move |_, status| {
                        flat_selection_button_style(theme, status, settings.appearance() == mode)
                    }),
                )
            });
    let metadata =
        settings.metadata_fields().into_iter().enumerate().fold(
            column![button(text("Create metadata field")).on_press(
                ProjectSurfaceMessage::Project(ProjectMessage::CreateMetadataField)
            )]
            .spacing(6),
            |column, (index, field)| {
                let id = field.id.to_owned();
                let move_up_id = id.clone();
                let move_down_id = id.clone();
                let delete_id = id.clone();
                let column = column.push(
                    button(
                        row![
                            column![
                                text(field.label).size(13),
                                text(field.description.unwrap_or("No description")).size(11)
                            ]
                            .spacing(2),
                            Space::new().width(Length::Fill),
                            text(metadata_applicability_label(field.applicability)).size(11),
                        ]
                        .spacing(14),
                    )
                    .width(Length::Fill)
                    .padding(10)
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::SelectMetadataField(id),
                    ))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Secondary,
                            interaction(status, false),
                        )
                    }),
                );
                column.push(
                    row![
                        text("⠿").size(14),
                        button(text("↑")).on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::ReorderMetadataField {
                                field_id: move_up_id,
                                target_index: index.saturating_sub(1),
                            }
                        )),
                        button(text("↓")).on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::ReorderMetadataField {
                                field_id: move_down_id,
                                target_index: index + 1,
                            }
                        )),
                        Space::new().width(Length::Fill),
                        button(text("Trash").size(11)).on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::RequestDeleteMetadataField(delete_id)
                        )),
                    ]
                    .spacing(6),
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
                text("⠿").size(14),
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
                .padding(10)
                .on_press(ProjectSurfaceMessage::Project(ProjectMessage::SelectStyle(
                    id,
                )))
                .style(move |_, status| {
                    components::button_style(
                        theme,
                        ButtonKind::Secondary,
                        interaction(status, false),
                    )
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
    let detail = match settings.selected_detail() {
        Some(SettingsDetail::MetadataField(id)) => settings
            .metadata_field(id)
            .map(|field| metadata_field_detail(field, theme)),
        Some(SettingsDetail::Style(id)) => settings
            .style(id)
            .map(|style| style_detail(settings, style, theme)),
        None => None,
    }
    .unwrap_or_else(|| {
        text("Select a metadata field or style to edit its details.")
            .size(12)
            .into()
    });
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
            .fold(column![].spacing(4), |column, item| {
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
                    .padding([10, 12])
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::SelectSettingsCategory(item.category),
                    ))
                    .style(move |_, status| {
                        flat_selection_button_style(theme, status, item.selected)
                    }),
                )
            });
    if category == SettingsCategory::Appearance {
        return row![
            container(column![text("PROJECT SETTINGS").size(12), navigation].spacing(18))
                .padding([16, 12])
                .width(280)
                .height(Length::Fill)
                .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest)),
            container(column![
                text(appearance_heading).size(24),
                text("Appearance").size(20),
                text("Choose the application appearance. Project styles and export do not change.").size(15),
                Space::new().height(8),
                container(choices).width(540),
                Space::new().height(12),
                text("Operating system changes Light → Dark; every open ParchMint window updates immediately.").size(14),
                text("Application preference only — project prose, history, saves, and export are unchanged.").size(13),
            ]
            .spacing(12))
            .padding([20, 24])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest)),
        ]
        .height(Length::Fill)
        .into();
    }
    let list: Element<'a, ProjectSurfaceMessage> = match category {
        SettingsCategory::Appearance => {
            unreachable!("appearance uses its dedicated settings layout")
        }
        SettingsCategory::General => text("No project-backed general settings are available.")
            .size(13)
            .into(),
        SettingsCategory::Metadata => column![
            text("Metadata fields").size(16),
            scrollable(metadata).height(Length::Fill)
        ]
        .spacing(10)
        .into(),
        SettingsCategory::Styles => column![
            text("Styles").size(16),
            scrollable(styles).height(Length::Fill)
        ]
        .spacing(10)
        .into(),
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
                text("Dictionaries").size(16),
                text(format!("Language · {}", dictionaries.language())).size(12),
                scopes,
                words,
            ]
            .spacing(10)
            .into()
        }
    };
    column![
        text("Settings").size(22),
        text("Appearance changes every open window and does not enter project history.").size(13),
        row![
            container(navigation)
                .padding(12)
                .width(160)
                .height(Length::Fill)
                .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest,)),
            container(list)
                .padding(12)
                .width(Length::FillPortion(2))
                .height(Length::Fill)
                .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest,)),
            container(column![text("Details").size(16), scrollable(detail)].spacing(10))
                .padding(12)
                .width(Length::FillPortion(3))
                .height(Length::Fill)
                .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest,)),
        ]
        .spacing(16)
        .height(Length::Fill),
    ]
    .spacing(14)
    .into()
}

fn metadata_applicability_label(value: MetadataFieldApplicability) -> &'static str {
    match value {
        MetadataFieldApplicability::Groups => "Groups",
        MetadataFieldApplicability::Documents => "Documents",
        MetadataFieldApplicability::GroupsAndDocuments => "Groups and documents",
    }
}

fn metadata_field_detail<'a>(
    field: crate::MetadataFieldSummary<'a>,
    _theme: ParchMintTheme,
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
    let label_input = text_input("Label", field.label).on_input({
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
        row.push(
            button(text(metadata_applicability_label(applicability))).on_press(
                ProjectSurfaceMessage::Project(make_update(
                    label,
                    (!description.is_empty()).then_some(description),
                    applicability,
                    field.text_kind,
                    (!default_value.is_empty()).then_some(default_value),
                    field.visible_on_cards,
                )),
            ),
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
            ))),
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
            column![].spacing(7),
            |column, item| {
                let node_id = selected_id.clone();
                let field_id = item.field_id.to_owned();
                let value = item.effective_value.unwrap_or_default();
                column.push(
                    row![
                        text(item.label).size(12).width(88),
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
                    .spacing(8)
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
        } else {
            let title_id = selected_id;
            text_input("Untitled", title)
                .on_input(move |title| {
                    ProjectSurfaceMessage::Project(ProjectMessage::RenameNode {
                        node_id: title_id.clone(),
                        title,
                    })
                })
                .padding([6, 8])
                .style(move |_, status| components::field_style(theme, field_interaction(status)))
                .into()
        };
        let editor = workspace.editor();
        let mut comments = column![].spacing(8);
        let threads = editor.inspector_comments();
        let has_threads = !threads.is_empty();
        if !has_threads {
            comments = comments.push(
                column![
                    text("No comments").size(13),
                    text("Right-click selected text to add a comment.").size(12),
                ]
                .spacing(2),
            );
        }
        for thread in threads {
            let thread_id = thread.id().to_owned();
            let state = if thread.resolved() {
                "Resolved"
            } else {
                "Unresolved"
            };
            let root = thread.messages().first();
            let mut card = column![
                button(text(format!(
                    "{state}: {}",
                    root.map(|message| message.body()).unwrap_or("Comment")
                )))
                .on_press(ProjectSurfaceMessage::EditorCenter(
                    EditorCenterMessage::Workspace(EditorMessage::SelectComment(thread_id.clone()))
                ))
            ]
            .spacing(6);
            if let Some(root) = root {
                let root_id = root.id().to_owned();
                if editor.editing_comment_message() == Some((thread_id.as_str(), root_id.as_str()))
                {
                    let edit_thread = thread_id.clone();
                    card = card
                        .push(
                            text_input(
                                "Edit comment message",
                                editor.comment_reply_draft(&thread_id),
                            )
                            .on_input(move |body| {
                                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                                    EditorMessage::SetCommentReplyDraft {
                                        thread_id: edit_thread.clone(),
                                        body,
                                    },
                                ))
                            }),
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
                            text_input(
                                "Edit comment message",
                                editor.comment_reply_draft(&thread_id),
                            )
                            .on_input(move |body| {
                                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                                    EditorMessage::SetCommentReplyDraft {
                                        thread_id: edit_thread.clone(),
                                        body,
                                    },
                                ))
                            }),
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
                card = card
                    .push(
                        text(format!(
                            "Orphaned anchor: {context_before}[{quote}]{context_after}"
                        ))
                        .size(12),
                    )
                    .push(button(text("Reattach to selection")).on_press(
                        ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                            EditorMessage::ReattachComment(thread_id.clone()),
                        )),
                    ))
                    .push(button(text("Convert to document comment")).on_press(
                        ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                            EditorMessage::ConvertCommentToDocument(thread_id.clone()),
                        )),
                    ));
            }
            if editor
                .editing_comment_message()
                .is_none_or(|(editing_thread, _)| editing_thread != thread_id)
            {
                let reply_id = thread_id.clone();
                card = card.push(
                    text_input("Reply to thread", editor.comment_reply_draft(&thread_id)).on_input(
                        move |body| {
                            ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                                EditorMessage::SetCommentReplyDraft {
                                    thread_id: reply_id.clone(),
                                    body,
                                },
                            ))
                        },
                    ),
                );
            }
            card = card.push(
                row![
                    button(text("Reply")).on_press(ProjectSurfaceMessage::EditorCenter(
                        EditorCenterMessage::Workspace(EditorMessage::SubmitCommentReply {
                            thread_id: thread_id.clone()
                        })
                    )),
                    button(text(if thread.resolved() {
                        "Reopen"
                    } else {
                        "Resolve"
                    }))
                    .on_press(ProjectSurfaceMessage::EditorCenter(
                        EditorCenterMessage::Workspace(EditorMessage::ToggleCommentResolved {
                            thread_id: thread_id.clone(),
                            resolved: !thread.resolved()
                        })
                    )),
                    button(text("Delete thread")).on_press(ProjectSurfaceMessage::EditorCenter(
                        EditorCenterMessage::Workspace(EditorMessage::RequestDeleteCommentThread(
                            thread_id.clone()
                        ))
                    )),
                ]
                .spacing(6),
            );
            comments = comments.push(container(card).padding(8));
        }
        // An empty inspector is a status view, not a permanently open comment
        // composer or action tray. Comment creation remains available from the
        // editor selection context; once a thread or draft exists, retain the
        // normal editor controls here.
        if has_threads || !editor.comment_draft().is_empty() {
            comments = comments.push(
                text_input("Write a comment", editor.comment_draft()).on_input(|body| {
                    ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                        EditorMessage::SetCommentDraft(body),
                    ))
                }),
            );
            comments = comments.push(
                row![
                    button(text("Add at selection")).on_press(ProjectSurfaceMessage::EditorCenter(
                        EditorCenterMessage::Workspace(EditorMessage::CreateComment {
                            document_level: false
                        })
                    )),
                    button(text("Add to document")).on_press(ProjectSurfaceMessage::EditorCenter(
                        EditorCenterMessage::Workspace(EditorMessage::CreateComment {
                            document_level: true
                        })
                    )),
                ]
                .spacing(6),
            );
        }
        if editor.pending_delete_comment().is_some() {
            comments = comments.push(
                row![
                    text("Delete this thread?").size(12),
                    button(text("Confirm delete")).on_press(ProjectSurfaceMessage::EditorCenter(
                        EditorCenterMessage::Workspace(EditorMessage::ConfirmDeleteCommentThread)
                    )),
                    button(text("Cancel")).on_press(ProjectSurfaceMessage::EditorCenter(
                        EditorCenterMessage::Workspace(EditorMessage::CancelDeleteCommentThread)
                    )),
                ]
                .spacing(6),
            );
        }
        if let Some(feedback) = editor.comment_feedback() {
            comments = comments.push(text(feedback).size(12));
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
            sections = sections.push(
                button(
                    row![
                        text(if comments_expanded { "⌄" } else { "›" }).size(12),
                        text("COMMENTS").size(12),
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
        let inspector_heading: Element<'a, ProjectSurfaceMessage> = if compact {
            Space::new().height(0).into()
        } else {
            text("Inspector").size(12).into()
        };
        column![
            inspector_heading,
            title,
            scrollable(sections).height(Length::Fill),
        ]
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
        SaveState::SavedThrough(revision) => format!("Saved · revision {revision}"),
        SaveState::Dirty { current_revision } => format!("Unsaved · revision {current_revision}"),
        SaveState::Saving { through_revision } => format!("Saving · revision {through_revision}"),
        SaveState::Error(error) => format!("Save failed · {error}"),
    };
    let editor_status = workspace.editor().status_bar();
    let active_count = match editor_status.current_count() {
        StatusCount::Selection(words) => format!("Selection · {words} words"),
        StatusCount::ActiveDocument(words) => format!("Document · {words} words"),
    };
    let content = row![
        button(text("Explorer").size(12))
            .on_press(ProjectSurfaceMessage::ToggleExplorer)
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                interaction(status, explorer_visible)
            )),
        text(active_count).size(12),
        text(format!(
            "Manuscript · {} words",
            editor_status.manuscript_total()
        ))
        .size(12),
        button(text("Document History").size(12))
            .on_press_maybe(
                workspace
                    .focused_history_document()
                    .is_some()
                    .then_some(ProjectSurfaceMessage::OpenContextualHistory)
            )
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                interaction(status, false)
            )),
        Space::new().width(Length::Fill),
        text(label).size(12),
        button(text("Inspector").size(12))
            .on_press(ProjectSurfaceMessage::ToggleInspector)
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                interaction(status, inspector_visible)
            )),
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
    let (title, detail) = match &modal {
        ProjectModal::HistoryRestore { .. } => (
            "Restore project history",
            "Replace the entire current project with the selected checkpoint.".to_owned(),
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

    use iced::{Settings, Size, Theme};
    use iced_test::Simulator;
    use parchmint_application::{DocumentSnapshot, DocumentVisibility, EditorRevision};
    use parchmint_domain::{
        DocumentId, MetadataApplicability, MetadataFieldDefinition, MetadataFieldId,
        MetadataTextKind, NodeId, Project, ProjectCommand, ProjectId, apply_project_command,
    };
    use parchmint_preferences::ResolvedAppearance;
    use parchmint_ui_api::ProjectSnapshot;

    use super::*;

    fn assert_fixture_hash(fixture: ProjectFixture, theme: &Theme, appearance: ResolvedAppearance) {
        let workspace = ProjectWorkspace::from_fixture(fixture);
        let stem = workspace.fixture_reference(appearance);
        let mut simulator = Simulator::<ProjectMessage>::with_size(
            Settings::default(),
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
        assert!(cards_surface.find("Inspector").is_ok());
        assert!(cards_surface.find("Document History").is_ok());
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
        assert!(search_surface.find("Inspector").is_ok());
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
    fn comment_inspector_exposes_accessible_empty_and_creation_controls() {
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
                .find("Right-click selected text to add a comment.")
                .is_ok()
        );
        assert!(simulator.find("Write a comment").is_err());
        assert!(simulator.find("Add at selection").is_err());
        assert!(simulator.find("Add to document").is_err());
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
                "All project checkpoints",
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
        assert!(
            workspace.accept_completion(crate::ProjectTaskCompletion::for_ticket(
                ticket,
                crate::ProjectTaskPayload::HistoryPreviewReady {
                    preview: crate::HistoryPreviewData {
                        checkpoint,
                        resource_paths: vec!["documents/chapter-one.html".to_owned()],
                        document: Some(crate::HistoryDocumentPreview {
                            document_id: "chapter-one".to_owned(),
                            canonical_path: "documents/chapter-one.html".to_owned(),
                            semantic: semantic(&["The blue house", "Keep", "Remove me"]),
                        }),
                    },
                },
            ))
        );
        workspace.set_history_current_document(Some(crate::HistoryCurrentDocument {
            document_id: "chapter-one".to_owned(),
            title: "Chapter One".to_owned(),
            body: "<p>The green house</p><p>Keep</p><p>Added one</p><p>Added two</p>".to_owned(),
            semantic: semantic(&["The green house", "Keep", "Added one", "Added two"]),
        }));
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
    fn hierarchy_context_overlay_exposes_only_applicable_actions() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::OpenHierarchyContextMenu(
            "part-one".to_owned(),
        ));
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
        assert!(simulator.find("Item actions").is_ok());
        assert!(simulator.find("Create document").is_ok());
        assert!(simulator.find("Create group").is_ok());
        assert!(simulator.find("Open in companion").is_err());
        assert!(simulator.find("Copy").is_ok());
        assert!(simulator.find("Cut").is_ok());
        assert!(simulator.find("Delete").is_ok());
    }

    #[test]
    fn explorer_row_double_click_publishes_open_and_keeps_single_click_selection() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
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

        simulator.click("Chapter One").expect("first row click");
        simulator.click("Chapter One").expect("second row click");

        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::SelectHierarchy {
                node_id,
                gesture: SelectionGesture::Replace,
            }) if node_id == "chapter-one"
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyNode(node_id))
                if node_id == "chapter-one"
        )));
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

    struct ProductionIds {
        group: String,
        live_node: String,
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
                deleted_node: id_string(deleted_group.as_bytes()),
            },
        )
    }

    fn id_string(bytes: &[u8; 16]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
