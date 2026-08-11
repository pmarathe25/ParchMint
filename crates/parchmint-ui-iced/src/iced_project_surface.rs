//! Private Iced composition for production and deterministic project workspaces.

use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Background, Element, Length, Theme, border};

use crate::{
    ContentState, DragDestination, FocusTarget, HierarchyItemKind, HierarchyRowKind,
    MetadataFieldApplicability, ProjectFixture, ProjectMessage, ProjectModal, ProjectWorkspace,
    RestoreLocation, RibbonDestination, SaveState, SelectionGesture, SidebarSurface, StatusCount,
    components::{self, ButtonKind, Interaction, StatusKind, Surface},
    design_tokens::{ParchMintTheme, RIBBON_HEIGHT, STATUS_HEIGHT},
    iced_editor_surface::EditorCenterMessage,
};

/// Typed output from the reusable project surface. The native integrator maps
/// these directly to its existing project, editor, and shell reducers.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectSurfaceMessage {
    Project(ProjectMessage),
    EditorCenter(EditorCenterMessage),
    Navigate(RibbonDestination),
    Focus(FocusTarget),
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
    project_surface_with_title(workspace, destination, theme, editor_child, "ParchMint")
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
    project_surface_with_title(
        workspace,
        destination,
        theme,
        editor_child,
        "The Glass Harbor",
    )
}

fn project_surface_with_title<'a>(
    workspace: &'a ProjectWorkspace,
    destination: RibbonDestination,
    theme: ParchMintTheme,
    editor_child: Element<'a, ProjectSurfaceMessage>,
    project_title: &'static str,
) -> Element<'a, ProjectSurfaceMessage> {
    let ribbon = ribbon(project_title, destination, theme);
    let left = left_rail(workspace, theme);
    let center = center_view(workspace, destination, theme, editor_child);
    let inspector = inspector(workspace, theme);
    let body = row![left, center, inspector].height(Length::Fill);
    let status = status_bar(workspace, theme);
    let mut content = column![ribbon, body, status]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill);
    if let Some(modal) = workspace.modal() {
        content = content.push(modal_view(modal, theme));
    }
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Application, Interaction::Rest))
        .into()
}

fn ribbon(
    project_title: &'static str,
    destination: RibbonDestination,
    theme: ParchMintTheme,
) -> Element<'static, ProjectSurfaceMessage> {
    let destinations = [
        ("✎", "Editor", RibbonDestination::Editor),
        ("▦", "Cards", RibbonDestination::Cards),
        ("↶", "History", RibbonDestination::History),
        ("▱", "Recently Deleted", RibbonDestination::RecentlyDeleted),
        ("↓", "Export", RibbonDestination::Export),
        ("⚙", "Settings", RibbonDestination::Settings),
    ];
    let buttons = destinations
        .into_iter()
        .fold(row![].spacing(4), |row, (glyph, _label, item)| {
            let selected = item == destination;
            row.push(
                button(text(glyph).size(20))
                    .padding([5, 9])
                    .height(40)
                    .on_press(ProjectSurfaceMessage::Navigate(item))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Tab,
                            interaction(status, selected),
                        )
                    }),
            )
        });
    let title = row![text("▯").size(20), text(project_title).size(16),]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center)
        .width(280);
    container(
        row![title, buttons]
            .spacing(16)
            .align_y(iced::alignment::Vertical::Center),
    )
    .padding([6, 18])
    .width(Length::Fill)
    .height(u32::from(RIBBON_HEIGHT))
    .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest))
    .into()
}

fn left_rail<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let content = match workspace.sidebar_surface() {
        SidebarSurface::Explorer => explorer_rail(workspace, theme),
        SidebarSurface::GlobalSearch => global_search_rail(workspace, theme),
    };
    container(content)
        .padding(12)
        .width(280)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest))
        .into()
}

fn explorer_rail<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let explorer = workspace.explorer();
    let selected = explorer.selected_ids();
    let selected_parent = selected.first().and_then(|selected| {
        explorer.row(selected).map(|row| match row.kind {
            HierarchyRowKind::Root | HierarchyRowKind::Group => row.id,
            HierarchyRowKind::Document => row.parent_id.unwrap_or(row.section_id),
        })
    });
    let creation_parent = selected_parent
        .or_else(|| explorer.root_ids().first().copied())
        .map(str::to_owned);
    let paste_destination = creation_parent
        .as_ref()
        .map(|parent| DragDestination::IntoGroup(parent.clone()));
    let has_tree_clipboard = workspace.tree_clipboard_kind().is_some();
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
                    components::button_style(
                        theme,
                        ButtonKind::Quiet,
                        interaction(status, item.expanded),
                    )
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
            let select = button(text(title).size(13))
                .padding([5, 6])
                .width(Length::Fill)
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::SelectHierarchy {
                        node_id: item.id.to_owned(),
                        gesture: SelectionGesture::Replace,
                    },
                ))
                .style(move |_, status| {
                    components::button_style(
                        theme,
                        ButtonKind::Quiet,
                        interaction(status, item.selected),
                    )
                });
            let open: Element<'a, ProjectSurfaceMessage> =
                if item.kind == HierarchyRowKind::Document {
                    button(text("→").size(13))
                        .padding([4, 6])
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::OpenHierarchyNode(item.id.to_owned()),
                        ))
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                interaction(status, false),
                            )
                        })
                        .into()
                } else {
                    Space::new().width(24).into()
                };
            column.push(
                row![
                    Space::new().width((depth * 14) as f32),
                    disclosure,
                    select,
                    open
                ]
                .spacing(1)
                .align_y(iced::alignment::Vertical::Center),
            )
        });
    let mut actions = row![].spacing(2);
    if let Some(parent) = creation_parent {
        actions = actions
            .push(
                button(text("+ Document").size(11))
                    .padding([4, 6])
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::RequestCreateHierarchy {
                            parent_id: parent.clone(),
                            kind: HierarchyItemKind::Document,
                        },
                    ))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false),
                        )
                    }),
            )
            .push(
                button(text("+ Group").size(11))
                    .padding([4, 6])
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::RequestCreateHierarchy {
                            parent_id: parent,
                            kind: HierarchyItemKind::Group,
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
    if workspace.can_copy_or_cut_selection() {
        actions = actions
            .push(
                button(text("Copy").size(11))
                    .padding([4, 6])
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::CopySelection,
                    ))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false),
                        )
                    }),
            )
            .push(
                button(text("Cut").size(11))
                    .padding([4, 6])
                    .on_press(ProjectSurfaceMessage::Project(ProjectMessage::CutSelection))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            interaction(status, false),
                        )
                    }),
            )
            .push(
                button(text("Delete").size(11))
                    .padding([4, 6])
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::DeleteSelection,
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
    if has_tree_clipboard && let Some(destination) = paste_destination {
        actions = actions.push(
            button(text("Paste").size(11))
                .padding([4, 6])
                .on_press(ProjectSurfaceMessage::Project(
                    ProjectMessage::PasteSelection { destination },
                ))
                .style(move |_, status| {
                    components::button_style(theme, ButtonKind::Quiet, interaction(status, false))
                }),
        );
    }
    column![
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
        actions,
        scrollable(rows).height(Length::Fill),
    ]
    .spacing(8)
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
        button(text("Explorer").size(12))
            .on_press(ProjectSurfaceMessage::Project(ProjectMessage::ShowExplorer))
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                interaction(status, false)
            )),
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
        button(
            text(if search.whole_word() {
                "Whole ✓"
            } else {
                "Whole"
            })
            .size(12)
        )
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
    .spacing(4);
    let results = search
        .results()
        .iter()
        .fold(column![].spacing(4), |column, result| {
            let line = format!("{}{}{}", result.prefix, result.matching_text, result.suffix);
            column.push(
                button(column![text(&result.document_id).size(12), text(line).size(12)].spacing(2))
                    .padding(6)
                    .width(Length::Fill)
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::NavigateGlobalSearchResult(result.match_id.clone()),
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
        results
    };
    column![controls, query, results].spacing(10).into()
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
        ContentState::Recovery => recovery_center(theme),
        ContentState::Ready => match destination {
            RibbonDestination::Editor => editor_child,
            RibbonDestination::Cards => cards_center(workspace, theme),
            RibbonDestination::GlobalSearch => search_center(workspace, theme),
            RibbonDestination::History => history_center(workspace, theme),
            RibbonDestination::RecentlyDeleted => deleted_center(workspace, theme),
            RibbonDestination::Export => export_center(workspace, theme),
            RibbonDestination::Settings => settings_center(workspace, theme),
        },
    };
    let padding = if destination == RibbonDestination::Editor {
        0
    } else {
        24
    };
    container(content)
        .padding(padding)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Manuscript, Interaction::Rest))
        .into()
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
    let selected = cards.selected_ids();
    let items = cards
        .items()
        .into_iter()
        .fold(column![].spacing(10), |column, item| {
            let is_selected = selected.contains(&item.node_id);
            let kind = match item.kind {
                HierarchyRowKind::Root => "Section",
                HierarchyRowKind::Group => "Group",
                HierarchyRowKind::Document => "Document",
            };
            let metadata = item.metadata.into_iter().fold(
                column![].spacing(3),
                |metadata, (_, label, value)| {
                    metadata.push(
                        row![
                            text(label).size(12).width(120),
                            text(value.unwrap_or("—")).size(12),
                        ]
                        .spacing(8),
                    )
                },
            );
            let card_content = column![
                row![
                    text(item.title).size(16),
                    Space::new().width(Length::Fill),
                    text(kind).size(11),
                ]
                .align_y(iced::alignment::Vertical::Center),
                text(if item.synopsis.is_empty() {
                    "No synopsis"
                } else {
                    item.synopsis
                })
                .size(13),
                metadata,
            ]
            .spacing(8);
            column.push(
                button(card_content)
                    .padding(14)
                    .width(Length::Fill)
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::ActivateCard(item.node_id.to_owned()),
                    ))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Secondary,
                            interaction(status, is_selected),
                        )
                    }),
            )
        });
    let items = if cards.items().is_empty() {
        items.push(text("This section has no cards yet.").size(13))
    } else {
        items
    };
    column![
        text(format!("Cards · {section_title}")).size(22),
        text("Titles and synopses are editable from the shared project hierarchy. Metadata values shown here are read-only.").size(12),
        scrollable(items).height(Length::Fill),
    ]
    .spacing(14)
    .into()
}

fn search_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let preview = workspace.replacement_preview();
    let header = if preview.uses_middle_pane() {
        "Replace Preview"
    } else {
        "Global Search"
    };
    let action = if preview.uses_middle_pane() {
        button(text("Apply replacement")).on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::ApplyReplacement,
        ))
    } else {
        button(text("Review replacement")).on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::OpenReplacementPreview,
        ))
    };
    column![
        text(header).size(22),
        text_input(
            "Replace matches with",
            workspace.global_search().replacement()
        )
        .on_input(|replacement| ProjectSurfaceMessage::Project(
            ProjectMessage::SetGlobalReplacement(replacement)
        ))
        .padding([7, 8])
        .style(move |_, status| components::field_style(theme, field_interaction(status))),
        text(format!(
            "{} included matches",
            preview.included_match_ids().len()
        ))
        .size(14),
        action.style(move |_, status| components::button_style(
            theme,
            ButtonKind::Primary,
            interaction(status, false)
        ))
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
            .checkpoints()
            .iter()
            .fold(column![].spacing(4), |column, checkpoint| {
                column.push(
                    button(text(checkpoint).size(13))
                        .padding(8)
                        .width(Length::Fill)
                        .on_press(ProjectSurfaceMessage::Project(
                            ProjectMessage::RequestHistoryRestore {
                                checkpoint_id: checkpoint.clone(),
                            },
                        ))
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Secondary,
                                interaction(status, false),
                            )
                        }),
                )
            });
    column![
        text("Project History").size(22),
        text("Restore always applies to the entire project.").size(13),
        checkpoints
    ]
    .spacing(14)
    .into()
}

fn deleted_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let deleted = workspace.recently_deleted();
    let items = deleted.items();
    let rows = items.iter().fold(column![].spacing(12), |column, item| {
        let former = restore_location_label(workspace, item.former_location);
        let destination = restore_location_label(workspace, item.restore_location);
        let kind = match item.kind {
            HierarchyRowKind::Root => "Section",
            HierarchyRowKind::Group => "Group",
            HierarchyRowKind::Document => "Document",
        };
        let preview = if item.formatted_preview_available {
            "Formatted read-only preview available"
        } else {
            "Formatted preview is unavailable"
        };
        let using_fallback = matches!(item.restore_location, RestoreLocation::SectionRoot(_));
        let node_id = item.node_id.to_owned();
        let fallback_id = item.node_id.to_owned();
        column.push(
            container(
                column![
                    row![
                        text(item.title).size(16),
                        Space::new().width(Length::Fill),
                        text(kind).size(11),
                    ]
                    .align_y(iced::alignment::Vertical::Center),
                    text(format!(
                        "Former location: {former} · position {}",
                        item.former_index + 1
                    ))
                    .size(12),
                    text(format!("Restore destination: {destination}")).size(12),
                    text(preview).size(12),
                    row![
                        button(text("Restore").size(12))
                            .on_press(ProjectSurfaceMessage::Project(
                                ProjectMessage::RestoreDeleted(node_id),
                            ))
                            .style(move |_, status| components::button_style(
                                theme,
                                ButtonKind::Primary,
                                interaction(status, false),
                            )),
                        button(text("Use section root").size(12))
                            .on_press(ProjectSurfaceMessage::Project(
                                ProjectMessage::UseRestoreFallback(fallback_id),
                            ))
                            .style(move |_, status| components::button_style(
                                theme,
                                ButtonKind::Secondary,
                                interaction(status, using_fallback),
                            )),
                    ]
                    .spacing(8),
                ]
                .spacing(8),
            )
            .padding(14)
            .width(Length::Fill)
            .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
        )
    });
    let rows = if items.is_empty() {
        rows.push(text("No deleted content is available.").size(13))
    } else {
        rows
    };
    column![
        text("Recently Deleted").size(22),
        text("Deleted groups and documents remain recoverable through History. ParchMint does not permanently purge them in v1.").size(12),
        scrollable(rows).height(Length::Fill),
    ]
    .spacing(14)
    .into()
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
        crate::ExportState::Exporting { completed, total } => {
            format!("Exporting {completed}/{total}")
        }
        crate::ExportState::Succeeded { output_name } => format!("Exported {output_name}"),
        crate::ExportState::Failed(error) => error,
    };
    column![
        text("Export").size(22),
        text("Entire Manuscript").size(13),
        text_input("Output file", export.output_name())
            .on_input(
                |value| ProjectSurfaceMessage::Project(ProjectMessage::SetExportOutputName(value))
            )
            .padding([7, 8])
            .style(move |_, status| components::field_style(theme, field_interaction(status))),
        button(text(if export.numbers_documents() {
            "Number documents: on"
        } else {
            "Number documents: off"
        }))
        .on_press(ProjectSurfaceMessage::Project(
            ProjectMessage::SetExportNumbering(!export.numbers_documents())
        ))
        .style(move |_, status| components::button_style(
            theme,
            ButtonKind::Secondary,
            interaction(status, export.numbers_documents())
        )),
        text(state).size(12),
        button(text("Export"))
            .on_press(ProjectSurfaceMessage::Project(ProjectMessage::StartExport))
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Primary,
                interaction(status, false)
            ))
    ]
    .spacing(12)
    .into()
}

fn settings_center<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let settings = workspace.settings();
    let choices = settings
        .appearance_choices()
        .into_iter()
        .fold(row![].spacing(6), |row, mode| {
            row.push(
                button(text(format!("{mode:?}")).size(13))
                    .on_press(ProjectSurfaceMessage::Project(
                        ProjectMessage::SetAppearance(mode),
                    ))
                    .style(move |_, status| {
                        components::button_style(
                            theme,
                            ButtonKind::Secondary,
                            interaction(status, settings.appearance() == mode),
                        )
                    }),
            )
        });
    let metadata =
        settings
            .metadata_fields()
            .into_iter()
            .fold(column![].spacing(6), |column, field| {
                let applies_to = match field.applicability {
                    MetadataFieldApplicability::Groups => "Groups",
                    MetadataFieldApplicability::Documents => "Documents",
                    MetadataFieldApplicability::GroupsAndDocuments => "Groups and documents",
                    MetadataFieldApplicability::None => "None",
                };
                column.push(
                    container(
                        row![
                            column![
                                text(field.label).size(13),
                                text(field.description.unwrap_or("No description")).size(11),
                            ]
                            .spacing(2),
                            Space::new().width(Length::Fill),
                            text(applies_to).size(11),
                            text(field.default_value.unwrap_or("No default")).size(11),
                            text(if field.visible_on_cards {
                                "Shown on cards"
                            } else {
                                "Hidden on cards"
                            })
                            .size(11),
                        ]
                        .spacing(14)
                        .align_y(iced::alignment::Vertical::Center),
                    )
                    .padding(10)
                    .width(Length::Fill)
                    .style(move |_| components::surface(theme, Surface::Panel, Interaction::Rest)),
                )
            });
    column![
        text("Settings · Appearance").size(22),
        text("Appearance changes every open window and does not enter project history.").size(13),
        choices,
        text("Metadata fields").size(16),
        scrollable(metadata).height(Length::Fill),
    ]
    .spacing(14)
    .into()
}

fn recovery_center<'a>(theme: ParchMintTheme) -> Element<'a, ProjectSurfaceMessage> {
    column![
        text("Recover unsaved changes").size(22),
        text("Replay valid unsaved edits on top of the last completed autosave.").size(14),
        button(text("Recover edits"))
            .on_press(ProjectSurfaceMessage::Project(
                ProjectMessage::AcceptRecovery
            ))
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Primary,
                interaction(status, false)
            ))
    ]
    .spacing(14)
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
) -> Element<'a, ProjectSurfaceMessage> {
    let selected = workspace.explorer().selected_ids().into_iter().next();
    let content = if let Some(selected) = selected {
        let title = workspace.explorer().title(selected).unwrap_or("Untitled");
        let synopsis = workspace.explorer().synopsis(selected).unwrap_or_default();
        let selected_id = selected.to_owned();
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
        let synopsis = text_input("No synopsis", synopsis)
            .on_input(move |synopsis| {
                ProjectSurfaceMessage::Project(ProjectMessage::SetSynopsis {
                    node_id: synopsis_id.clone(),
                    synopsis,
                })
            })
            .padding(12)
            .width(Length::Fill)
            .style(move |_, status| components::field_style(theme, field_interaction(status)));
        let title_id = selected_id;
        let title = text_input("Untitled", title)
            .on_input(move |title| {
                ProjectSurfaceMessage::Project(ProjectMessage::RenameNode {
                    node_id: title_id.clone(),
                    title,
                })
            })
            .padding([6, 8])
            .style(move |_, status| components::field_style(theme, field_interaction(status)));
        column![
            text("Inspector").size(12),
            title,
            text("⌄  SYNOPSIS").size(12),
            synopsis,
            text("⌄  METADATA").size(12),
            scrollable(metadata).height(Length::Fill),
        ]
        .spacing(12)
    } else {
        column![
            text("Inspector").size(12),
            text("No selection").size(13),
            text("Select a group or document in Explorer or Cards to inspect its synopsis and metadata.")
                .size(12),
        ]
        .spacing(10)
    };
    container(content)
        .padding(12)
        .width(320)
        .height(Length::Fill)
        .style(move |_| components::surface(theme, Surface::Sidebar, Interaction::Rest))
        .into()
}

fn status_bar<'a>(
    workspace: &'a ProjectWorkspace,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let (kind, label) = match workspace.save().state() {
        SaveState::SavedThrough(revision) => {
            (StatusKind::Success, format!("Saved · revision {revision}"))
        }
        SaveState::Dirty { current_revision } => (
            StatusKind::Saving,
            format!("Unsaved · revision {current_revision}"),
        ),
        SaveState::Saving { through_revision } => (
            StatusKind::Saving,
            format!("Saving · revision {through_revision}"),
        ),
        SaveState::Error(error) => (StatusKind::Error, format!("Save failed · {error}")),
    };
    let editor_status = workspace.editor().status_bar();
    let active_count = match editor_status.current_count() {
        StatusCount::Selection(words) => format!("Selection · {words} words"),
        StatusCount::ActiveDocument(words) => format!("Document · {words} words"),
    };
    let content = row![
        button(text("Explorer").size(12))
            .on_press(ProjectSurfaceMessage::Focus(FocusTarget::Explorer))
            .style(move |_, status| components::button_style(
                theme,
                ButtonKind::Quiet,
                interaction(status, false)
            )),
        text(active_count).size(12),
        text(format!(
            "Manuscript · {} words",
            editor_status.manuscript_total()
        ))
        .size(12),
        Space::new().width(Length::Fill),
        text(label).size(12),
    ]
    .align_y(iced::alignment::Vertical::Center)
    .spacing(14);
    container(content)
        .padding([6, 12])
        .width(Length::Fill)
        .height(u32::from(STATUS_HEIGHT))
        .style(move |_| components::status_style(theme, kind))
        .into()
}

fn modal_view<'a>(
    modal: ProjectModal,
    theme: ParchMintTheme,
) -> Element<'a, ProjectSurfaceMessage> {
    let detail = match &modal {
        ProjectModal::HistoryRestore { checkpoint_id, .. } => {
            format!("Restore the entire project to {checkpoint_id}?")
        }
        ProjectModal::DeleteMetadataField { field_id } => {
            format!("Delete metadata field {field_id}?")
        }
    };
    container(
        row![
            text(detail).size(13),
            button(text("Cancel"))
                .on_press(ProjectSurfaceMessage::Project(ProjectMessage::DismissModal))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Secondary,
                    interaction(status, false)
                )),
            button(text("Confirm"))
                .on_press(ProjectSurfaceMessage::Project(match modal {
                    ProjectModal::HistoryRestore { .. } => ProjectMessage::ConfirmHistoryRestore,
                    ProjectModal::DeleteMetadataField { .. } =>
                        ProjectMessage::ConfirmDeleteMetadataField,
                }))
                .style(move |_, status| components::button_style(
                    theme,
                    ButtonKind::Destructive,
                    interaction(status, false)
                ))
        ]
        .spacing(10),
    )
    .padding(16)
    .width(Length::Fill)
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
fn field_interaction(status: iced::widget::text_input::Status) -> Interaction {
    match status {
        iced::widget::text_input::Status::Active => Interaction::Rest,
        iced::widget::text_input::Status::Hovered => Interaction::Hovered,
        iced::widget::text_input::Status::Focused { .. } => Interaction::Focused,
        iced::widget::text_input::Status::Disabled => Interaction::Disabled,
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
                assert!(simulator.find("Manuscript").is_ok());
                assert!(simulator.find("Part One").is_ok());
                assert!(simulator.find("Opening Scene").is_ok());
                assert!(simulator.find("The opening synopsis.").is_ok());
                assert!(simulator.find("Final").is_ok());
                assert!(simulator.find("Inspector").is_ok());
                if destination == RibbonDestination::RecentlyDeleted {
                    assert!(simulator.find("Discarded Part").is_ok());
                    assert!(
                        simulator
                            .find("Formatted read-only preview available")
                            .is_ok()
                    );
                }
                let snapshot = simulator
                    .snapshot(&theme.iced_theme())
                    .expect("production project surface snapshot");
                assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
            }
        }
    }

    #[test]
    fn recently_deleted_surface_emits_exact_restore_and_fallback_messages() {
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
        simulator.click("Restore").expect("visible restore action");
        simulator
            .click("Use section root")
            .expect("visible fallback action");
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert_eq!(
            messages,
            [
                ProjectSurfaceMessage::Project(ProjectMessage::RestoreDeleted(
                    ids.deleted_node.clone(),
                )),
                ProjectSurfaceMessage::Project(ProjectMessage::UseRestoreFallback(
                    ids.deleted_node,
                )),
            ]
        );
    }

    #[test]
    fn explorer_exposes_copy_cut_and_session_clipboard_paste_messages() {
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
        assert!(selection_surface.find("Copy").is_ok());
        assert!(selection_surface.find("Cut").is_ok());
        selection_surface.click("Copy").unwrap();
        assert_eq!(
            selection_surface.into_messages().collect::<Vec<_>>(),
            [ProjectSurfaceMessage::Project(
                ProjectMessage::CopySelection
            )]
        );

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
        assert!(clipboard_surface.find("Paste").is_ok());
        clipboard_surface.click("Paste").unwrap();
        assert!(matches!(
            clipboard_surface.into_messages().next(),
            Some(ProjectSurfaceMessage::Project(
                ProjectMessage::PasteSelection { .. }
            ))
        ));
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
            documents: vec![
                DocumentSnapshot {
                    document_id: live_document,
                    body: "one two three".into(),
                    revision: EditorRevision::from(3),
                    visibility: DocumentVisibility::Open,
                },
                DocumentSnapshot {
                    document_id: deleted_document,
                    body: "formatted deleted preview".into(),
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
