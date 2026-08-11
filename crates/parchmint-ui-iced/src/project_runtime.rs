//! Session-authorized execution boundary for project and editor workspace effects.
//!
//! UI IDs are resolved only through maps derived from an authoritative snapshot.
//! This module never interprets user-controlled strings as typed domain IDs.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use parchmint_application::{
    ApplicationError, CreateDocumentWorkflow, DeleteSubtreesWorkflow, DuplicateSubtreesWorkflow,
    MoveNodeWorkflow, MoveNodesWorkflow,
};
use parchmint_domain::{
    DocumentId, MetadataFieldId, NodeId, NodeKind, ProjectCommand, ProjectRevision, StyleId,
    apply_project_command,
};
use parchmint_editor_api::{
    CanonicalCommentAnchor, CanonicalDocumentLoad, CommentId, DocumentPosition, EditorSelection,
    SearchDecoration as AdapterSearchDecoration, SpellcheckDecoration as AdapterSpellDecoration,
    StyleCatalogProjection, ViewId,
};
use parchmint_preferences::{AppearanceMode, PreferenceCommand, ThemeSnapshot};
use parchmint_ui_api::{ProjectSaveKind, ProjectSnapshot, ProjectUiPorts};

use crate::{
    DragDestination, EditorCommand, EditorEffect, EditorPane, FindMatch, HierarchyItemKind,
    HistoryRestoreScope, ProjectEffect, RestoreLocation, SpellingDictionaryScope, SpellingMenu,
    TreeClipboardKind,
};

type RuntimeFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Executes effects against one exact project-session generation.
///
/// Clone this value into `Task::perform`. Every port call reacquires session
/// authorization, and every authored mutation is followed by a fresh query.
#[derive(Clone)]
pub(crate) struct NativeProjectEffectExecutor {
    ports: Arc<dyn RuntimeProjectPorts>,
    snapshot: Arc<ProjectSnapshot>,
    operation_sequence: Arc<AtomicU64>,
}

impl NativeProjectEffectExecutor {
    pub(crate) fn new(ports: ProjectUiPorts, snapshot: Arc<ProjectSnapshot>) -> Self {
        Self {
            ports: Arc::new(ProjectUiPortAdapter { ports }),
            snapshot,
            operation_sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn refreshed(&self, snapshot: Arc<ProjectSnapshot>) -> Self {
        Self {
            ports: Arc::clone(&self.ports),
            snapshot,
            operation_sequence: Arc::clone(&self.operation_sequence),
        }
    }

    /// Executes one project effect and returns an owned Iced-task completion.
    pub(crate) async fn execute_project_effect(
        self,
        effect: ProjectEffect,
    ) -> Result<ProjectEffectCompletion, ProjectRuntimeError> {
        self.ports.authorize()?;

        if let Some(unsupported) = unsupported_project_effect(&effect) {
            return Err(ProjectRuntimeError::Unsupported(unsupported));
        }

        let current = self.current_snapshot().await?;
        let resolvers = StableIdResolvers::from_snapshot(&current);

        match effect {
            ProjectEffect::OpenDocumentInPrimary(document_id) => {
                self.open_documents(&current, &resolvers, EditorPane::Primary, [document_id])
                    .await
            }
            ProjectEffect::OpenDocumentInCompanion(document_id) => {
                self.open_documents(&current, &resolvers, EditorPane::Companion, [document_id])
                    .await
            }
            ProjectEffect::CreateHierarchy { parent_id, kind } => {
                let parent = resolvers.node(&parent_id)?;
                let index = current.project.nodes.children(parent).len();
                let operation = self.operation_sequence.fetch_add(1, Ordering::Relaxed);
                let id = generated_node_id(&current, operation);
                let command = match kind {
                    HierarchyItemKind::Group => {
                        ProjectCommand::create_group(id, parent, index, "New Group")
                    }
                    HierarchyItemKind::Document => {
                        let document = generated_document_id(&current, operation);
                        let snapshot = self
                            .ports
                            .create_document(CreateDocumentWorkflow {
                                node: id,
                                document,
                                parent,
                                index,
                                title: "Untitled".to_owned(),
                            })
                            .await?;
                        return Ok(ProjectEffectCompletion::WorkflowSnapshot(Box::new(
                            snapshot,
                        )));
                    }
                };
                self.execute_commands([command]).await
            }
            ProjectEffect::DeleteHierarchy(node_ids) => {
                let nodes = resolve_distinct_nodes(&resolvers, node_ids)?;
                if nodes.is_empty() {
                    return Err(ProjectRuntimeError::InvalidEffect(
                        "delete hierarchy requires at least one node",
                    ));
                }
                let snapshot = self
                    .ports
                    .delete_subtrees(DeleteSubtreesWorkflow {
                        nodes,
                        deleted_at_unix_millis: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_err(|_| {
                                ProjectRuntimeError::InvalidEffect(
                                    "system time predates the Unix epoch",
                                )
                            })?
                            .as_millis()
                            .try_into()
                            .map_err(|_| {
                                ProjectRuntimeError::InvalidEffect(
                                    "system time exceeds the deletion timestamp range",
                                )
                            })?,
                    })
                    .await?;
                Ok(ProjectEffectCompletion::WorkflowSnapshot(Box::new(
                    snapshot,
                )))
            }
            ProjectEffect::MoveHierarchy {
                node_ids,
                destination,
            } => {
                let nodes = resolve_distinct_nodes(&resolvers, node_ids)?;
                if nodes.is_empty() {
                    return Err(ProjectRuntimeError::InvalidEffect(
                        "move hierarchy requires at least one node",
                    ));
                }
                if let DragDestination::EditorPane(pane) = destination {
                    let documents = nodes
                        .into_iter()
                        .map(|node| resolvers.document_for_node(node))
                        .collect::<Result<Vec<_>, _>>()?;
                    return self.open_documents_by_id(&current, pane, documents).await;
                }
                let commands = plan_moves(&current, &resolvers, nodes, destination)?;
                self.execute_commands(commands).await
            }
            ProjectEffect::CommitNodeTitle { node_id, title } => {
                let node = resolvers.node(&node_id)?;
                self.execute_commands([ProjectCommand::rename_node(node, title)])
                    .await
            }
            ProjectEffect::CommitSynopsis { node_id, synopsis } => {
                let node = resolvers.node(&node_id)?;
                self.execute_commands([ProjectCommand::set_synopsis(node, synopsis)])
                    .await
            }
            ProjectEffect::CommitMetadataValue {
                node_id,
                field_id,
                value,
            } => {
                let node = resolvers.node(&node_id)?;
                let field = resolvers.metadata_field(&field_id)?;
                self.execute_commands([ProjectCommand::set_metadata_value(
                    node,
                    field,
                    Some(value),
                )])
                .await
            }
            ProjectEffect::UpsertMetadataField(definition) => {
                self.execute_commands([ProjectCommand::upsert_metadata_field(definition)])
                    .await
            }
            ProjectEffect::ReorderMetadataField {
                field_id,
                target_index,
            } => {
                let field = resolvers.metadata_field(&field_id)?;
                self.execute_commands([ProjectCommand::move_metadata_field(field, target_index)])
                    .await
            }
            ProjectEffect::DeleteMetadataField(field_id) => {
                let field = resolvers.metadata_field(&field_id)?;
                self.execute_commands([ProjectCommand::delete_metadata_field(field)])
                    .await
            }
            ProjectEffect::UpsertStyle(definition) => {
                self.execute_commands([ProjectCommand::upsert_style(definition)])
                    .await
            }
            ProjectEffect::DeleteStyle(style_id) => {
                let style = resolvers.style(&style_id)?;
                self.execute_commands([ProjectCommand::delete_style(style)])
                    .await
            }
            ProjectEffect::RestoreDeletedSubtree { node_id, location } => {
                let node = resolvers.deleted_node(&node_id)?;
                validate_restore_location(&current, &resolvers, node, &location)?;
                self.execute_commands([ProjectCommand::restore_deleted(node)])
                    .await
            }
            ProjectEffect::ApplyAppearanceToAllWindows(mode) => {
                let theme = self.ports.set_appearance(mode).await?;
                Ok(ProjectEffectCompletion::ApplyAppearance(theme))
            }
            ProjectEffect::SetProjectExportSettings(settings) => {
                self.execute_commands([ProjectCommand::set_project_export_settings(settings)])
                    .await
            }
            ProjectEffect::SaveThroughRevision(requested) => {
                let written = self.ports.save(ProjectSaveKind::Explicit).await?;
                if written < requested {
                    return Err(ProjectRuntimeError::SaveDidNotReach { requested, written });
                }
                Ok(ProjectEffectCompletion::SavedThrough(written))
            }
            ProjectEffect::FocusRecoveredEditor => {
                Ok(ProjectEffectCompletion::FocusRecoveredEditor)
            }
            ProjectEffect::NavigateSearchResult {
                match_id,
                revalidate_revision,
            } => {
                let (document_id, range, indexed_revision) = parse_search_match_id(&match_id)?;
                let document = resolvers.document(&document_id)?;
                let (load, snapshot) = self.load_document(&current, document).await?;
                if revalidate_revision && load.revision.value() != indexed_revision {
                    return Err(ProjectRuntimeError::InvalidEffect(
                        "search result no longer matches the current document revision",
                    ));
                }
                Ok(ProjectEffectCompletion::NavigateSearch {
                    snapshot: Box::new(snapshot),
                    document: ResolvedDocumentMount {
                        pane: EditorPane::Primary,
                        load,
                    },
                    range,
                })
            }
            ProjectEffect::CreateNamedSnapshot(name) => {
                let snapshot = self.ports.create_named_snapshot(name).await?;
                Ok(ProjectEffectCompletion::WorkflowSnapshot(Box::new(
                    snapshot,
                )))
            }
            ProjectEffect::RestoreHistory {
                checkpoint_id,
                scope: HistoryRestoreScope::EntireProject,
            } => {
                let checkpoint = parchmint_domain::CheckpointId::from_bytes(parse_stable_hex(
                    &checkpoint_id,
                    "History checkpoint",
                )?);
                let snapshot = self.ports.restore_checkpoint(checkpoint).await?;
                Ok(ProjectEffectCompletion::WorkflowSnapshot(Box::new(
                    snapshot,
                )))
            }
            ProjectEffect::PasteCopiedSubtrees {
                node_ids,
                destination,
            } => {
                if node_ids.is_empty() {
                    return Err(ProjectRuntimeError::InvalidEffect(
                        "copy paste requires at least one subtree root",
                    ));
                }
                let sources = resolve_distinct_nodes(&resolvers, node_ids)?;
                let (parent, index) = paste_location(&current, &resolvers, &destination)?;
                let snapshot = self
                    .ports
                    .duplicate_subtrees(DuplicateSubtreesWorkflow {
                        sources,
                        parent,
                        index,
                    })
                    .await?;
                Ok(ProjectEffectCompletion::TreePaste {
                    snapshot: Box::new(snapshot.snapshot),
                    kind: TreeClipboardKind::Copy,
                    created_roots: snapshot
                        .created_roots
                        .iter()
                        .map(|node| stable_id_string(node.as_bytes()))
                        .collect(),
                })
            }
            ProjectEffect::PasteCutSubtrees {
                node_ids,
                destination,
            } => {
                let nodes = resolve_distinct_nodes(&resolvers, node_ids)?;
                if nodes.is_empty() {
                    return Err(ProjectRuntimeError::InvalidEffect(
                        "cut paste requires at least one subtree root",
                    ));
                }
                let commands = plan_moves(&current, &resolvers, nodes, destination)?;
                let moves = commands
                    .into_iter()
                    .map(|command| match command {
                        ProjectCommand::MoveNode { id, parent, index } => MoveNodeWorkflow {
                            node: id,
                            parent,
                            index,
                        },
                        _ => unreachable!("move planner returns only MoveNode commands"),
                    })
                    .collect();
                let snapshot = self.ports.move_nodes(MoveNodesWorkflow { moves }).await?;
                Ok(ProjectEffectCompletion::TreePaste {
                    snapshot: Box::new(snapshot),
                    kind: TreeClipboardKind::Cut,
                    created_roots: Vec::new(),
                })
            }
            ProjectEffect::SearchProject { .. }
            | ProjectEffect::PreviewHistory(_)
            | ProjectEffect::PreviewDeleted { .. }
            | ProjectEffect::BuildReplacementPreview { .. }
            | ProjectEffect::ApplyGlobalReplacement { .. }
            | ProjectEffect::ExportEntireManuscript { .. }
            | ProjectEffect::ChooseExportDestination { .. }
            | ProjectEffect::CancelExport
            | ProjectEffect::OpenExportResult(_)
            | ProjectEffect::RevealExportResult(_)
            | ProjectEffect::ReconcileRecovery
            | ProjectEffect::DiscardRecovery
            | ProjectEffect::ReinitializeHistory => {
                unreachable!("unsupported project effects return before snapshot resolution")
            }
        }
    }

    /// Resolves or executes one editor effect without retaining framework handles.
    pub(crate) async fn execute_editor_effect(
        self,
        effect: EditorEffect,
    ) -> Result<EditorEffectCompletion, ProjectRuntimeError> {
        self.ports.authorize()?;
        let current = self.current_snapshot().await?;
        let resolvers = StableIdResolvers::from_snapshot(&current);

        match effect {
            EditorEffect::RequestSave => {
                let written = self.ports.save(ProjectSaveKind::Explicit).await?;
                Ok(EditorEffectCompletion::SavedThrough(written))
            }
            EditorEffect::MountDocument {
                pane,
                view,
                document_id,
            } => {
                let document = resolvers.document(&document_id)?;
                let (load, snapshot) = self.load_document(&current, document).await?;
                Ok(EditorEffectCompletion::Intent(EditorRuntimeIntent::Mount {
                    pane,
                    view,
                    load,
                    snapshot: Box::new(snapshot),
                }))
            }
            EditorEffect::UnmountView { pane, view } => Ok(EditorEffectCompletion::Intent(
                EditorRuntimeIntent::Unmount { pane, view },
            )),
            EditorEffect::SetSearchDecorations {
                view,
                matches,
                active,
            } => {
                let decorations = matches
                    .into_iter()
                    .map(|range| {
                        AdapterSearchDecoration::new(selection(range.start(), range.end()))
                    })
                    .collect();
                let active = active.map(|range| {
                    AdapterSearchDecoration::new(selection(range.start(), range.end()))
                });
                Ok(EditorEffectCompletion::Intent(
                    EditorRuntimeIntent::SetSearchDecorations {
                        view,
                        decorations,
                        active,
                    },
                ))
            }
            EditorEffect::SetSpellcheckDecorations { view, decorations } => {
                let decorations = decorations
                    .into_iter()
                    .map(|decoration| {
                        let range = decoration.range();
                        AdapterSpellDecoration::new(selection(range.start(), range.end()))
                    })
                    .collect();
                Ok(EditorEffectCompletion::Intent(
                    EditorRuntimeIntent::SetSpellcheckDecorations { view, decorations },
                ))
            }
            EditorEffect::ShowSpellingMenu(menu) => Ok(EditorEffectCompletion::Intent(
                EditorRuntimeIntent::ShowSpellingMenu(menu),
            )),
            EditorEffect::SpellingDictionaryAction {
                word, scope, add, ..
            } => match scope {
                SpellingDictionaryScope::Project => {
                    let command = if add {
                        ProjectCommand::add_dictionary_word(word)
                    } else {
                        ProjectCommand::remove_dictionary_word(word)
                    };
                    self.execute_commands([command])
                        .await
                        .map(EditorEffectCompletion::ProjectMutation)
                }
                SpellingDictionaryScope::Global => {
                    self.ports.update_global_dictionary(word, add).await?;
                    Ok(EditorEffectCompletion::GlobalDictionaryUpdated)
                }
            },
            EditorEffect::RestoreEditorFocus { view } => Ok(EditorEffectCompletion::Intent(
                EditorRuntimeIntent::RestoreFocus { view },
            )),
            EditorEffect::Command { view, command } => Ok(EditorEffectCompletion::Intent(
                EditorRuntimeIntent::Command { view, command },
            )),
            EditorEffect::NavigateCommentAnchor {
                view,
                comment_id,
                highlight,
            } => {
                let id = CommentId::from_bytes(parse_stable_hex(&comment_id, "comment")?);
                let thread = current
                    .documents
                    .iter()
                    .flat_map(|document| &document.comments)
                    .find(|thread| thread.id == id)
                    .ok_or_else(|| unknown_id(StableIdKind::Comment, comment_id))?;
                let CanonicalCommentAnchor::Text {
                    range,
                    orphaned: false,
                    ..
                } = &thread.anchor
                else {
                    return Err(ProjectRuntimeError::InvalidEffect(
                        "comment has no live text anchor",
                    ));
                };
                Ok(EditorEffectCompletion::Intent(
                    EditorRuntimeIntent::NavigateCommentAnchor {
                        view,
                        comment: highlight.then_some(id),
                        range: *range,
                    },
                ))
            }
            EditorEffect::ShowOrphanedComment { comment_id } => {
                let id = CommentId::from_bytes(parse_stable_hex(&comment_id, "comment")?);
                let orphaned = current
                    .documents
                    .iter()
                    .flat_map(|document| &document.comments)
                    .any(|thread| {
                        thread.id == id
                            && matches!(
                                thread.anchor,
                                CanonicalCommentAnchor::Text { orphaned: true, .. }
                            )
                    });
                if !orphaned {
                    return Err(ProjectRuntimeError::InvalidEffect(
                        "comment is not orphaned",
                    ));
                }
                Ok(EditorEffectCompletion::GlobalDictionaryUpdated)
            }
        }
    }

    async fn current_snapshot(&self) -> Result<ProjectSnapshot, ProjectRuntimeError> {
        let current = self.ports.snapshot().await?;
        if self.snapshot.project.revision != current.project.revision
            || document_revision_frontier(&self.snapshot) != document_revision_frontier(&current)
        {
            return Err(ProjectRuntimeError::StaleSnapshot {
                expected: self.snapshot.project.revision,
                actual: current.project.revision,
            });
        }
        Ok(current)
    }

    async fn load_document(
        &self,
        snapshot: &ProjectSnapshot,
        document: DocumentId,
    ) -> Result<(CanonicalDocumentLoad, ProjectSnapshot), ProjectRuntimeError> {
        if snapshot
            .documents
            .iter()
            .any(|candidate| candidate.document_id == document)
        {
            return Ok((canonical_load(snapshot, document)?, snapshot.clone()));
        }
        self.ports.load_document(document).await?;
        let refreshed = self.ports.snapshot().await?;
        let load = canonical_load(&refreshed, document)?;
        Ok((load, refreshed))
    }

    async fn execute_commands(
        &self,
        commands: impl IntoIterator<Item = ProjectCommand>,
    ) -> Result<ProjectEffectCompletion, ProjectRuntimeError> {
        let commands = commands.into_iter().collect::<Vec<_>>();
        let mut simulated = self.snapshot.project.clone();
        for command in &commands {
            simulated = apply_project_command(&simulated, simulated.revision, command.clone())
                .map_err(|error| ProjectRuntimeError::Port {
                    service: "apply_project_command preflight",
                    message: error.to_string(),
                })?
                .project;
        }
        for command in commands {
            self.ports.execute(command).await?;
        }
        let snapshot = self.ports.snapshot().await?;
        Ok(ProjectEffectCompletion::RefreshedSnapshot(Box::new(
            snapshot,
        )))
    }

    async fn open_documents(
        &self,
        snapshot: &ProjectSnapshot,
        resolvers: &StableIdResolvers,
        pane: EditorPane,
        document_ids: impl IntoIterator<Item = String>,
    ) -> Result<ProjectEffectCompletion, ProjectRuntimeError> {
        let documents = document_ids
            .into_iter()
            .map(|id| resolvers.document(&id))
            .collect::<Result<Vec<_>, _>>()?;
        self.open_documents_by_id(snapshot, pane, documents).await
    }

    async fn open_documents_by_id(
        &self,
        snapshot: &ProjectSnapshot,
        pane: EditorPane,
        documents: impl IntoIterator<Item = DocumentId>,
    ) -> Result<ProjectEffectCompletion, ProjectRuntimeError> {
        let mut snapshot = snapshot.clone();
        let mut mounts = Vec::new();
        for document in documents {
            let (load, refreshed) = self.load_document(&snapshot, document).await?;
            snapshot = refreshed;
            mounts.push(ResolvedDocumentMount { pane, load });
        }
        Ok(ProjectEffectCompletion::OpenDocuments {
            snapshot: Box::new(snapshot),
            documents: mounts,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectEffectCompletion {
    RefreshedSnapshot(Box<ProjectSnapshot>),
    WorkflowSnapshot(Box<ProjectSnapshot>),
    TreePaste {
        snapshot: Box<ProjectSnapshot>,
        kind: TreeClipboardKind,
        created_roots: Vec<String>,
    },
    OpenDocuments {
        snapshot: Box<ProjectSnapshot>,
        documents: Vec<ResolvedDocumentMount>,
    },
    ApplyAppearance(ThemeSnapshot),
    SavedThrough(u64),
    FocusRecoveredEditor,
    NavigateSearch {
        snapshot: Box<ProjectSnapshot>,
        document: ResolvedDocumentMount,
        range: FindMatch,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedDocumentMount {
    pub(crate) pane: EditorPane,
    pub(crate) load: CanonicalDocumentLoad,
}

#[derive(Debug, Clone, PartialEq)]
struct DuplicateWorkflowResult {
    snapshot: ProjectSnapshot,
    created_roots: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EditorEffectCompletion {
    Intent(EditorRuntimeIntent),
    ProjectMutation(ProjectEffectCompletion),
    GlobalDictionaryUpdated,
    SavedThrough(u64),
}

/// Typed intent requiring the native mounted-editor registry or native UI.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EditorRuntimeIntent {
    Command {
        view: ViewId,
        command: EditorCommand,
    },
    Mount {
        pane: EditorPane,
        view: ViewId,
        load: CanonicalDocumentLoad,
        snapshot: Box<ProjectSnapshot>,
    },
    Unmount {
        pane: EditorPane,
        view: ViewId,
    },
    SetSearchDecorations {
        view: ViewId,
        decorations: Vec<AdapterSearchDecoration>,
        active: Option<AdapterSearchDecoration>,
    },
    SetSpellcheckDecorations {
        view: ViewId,
        decorations: Vec<AdapterSpellDecoration>,
    },
    NavigateCommentAnchor {
        view: ViewId,
        comment: Option<CommentId>,
        range: EditorSelection,
    },
    ShowSpellingMenu(SpellingMenu),
    RestoreFocus {
        view: ViewId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StableIdKind {
    Node,
    DeletedNode,
    Document,
    MetadataField,
    Style,
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnsupportedCategory {
    Search,
    Replacement,
    Export,
    ExternalOpen,
    History,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnsupportedEffect {
    pub(crate) category: UnsupportedCategory,
    pub(crate) missing_boundary: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectRuntimeError {
    StaleSession {
        session_id: u64,
        generation: u64,
    },
    StaleSnapshot {
        expected: ProjectRevision,
        actual: ProjectRevision,
    },
    UnknownStableId {
        kind: StableIdKind,
        ui_id: String,
    },
    InvalidEffect(&'static str),
    Port {
        service: &'static str,
        message: String,
    },
    SaveDidNotReach {
        requested: u64,
        written: u64,
    },
    Unsupported(UnsupportedEffect),
}

impl fmt::Display for ProjectRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleSession {
                session_id,
                generation,
            } => write!(
                formatter,
                "project session {session_id} generation {generation} is no longer current"
            ),
            Self::StaleSnapshot { expected, actual } => write!(
                formatter,
                "project snapshot changed before the effect ran: expected revision {}, actual {}",
                expected.value(),
                actual.value()
            ),
            Self::UnknownStableId { kind, ui_id } => {
                write!(formatter, "unknown {kind:?} UI ID {ui_id}")
            }
            Self::InvalidEffect(reason) => write!(formatter, "invalid project effect: {reason}"),
            Self::Port { service, message } => write!(formatter, "{service} failed: {message}"),
            Self::SaveDidNotReach { requested, written } => write!(
                formatter,
                "save wrote through revision {written}, below requested revision {requested}"
            ),
            Self::Unsupported(unsupported) => write!(
                formatter,
                "unsupported {:?} effect: missing {}",
                unsupported.category, unsupported.missing_boundary
            ),
        }
    }
}

impl Error for ProjectRuntimeError {}

#[derive(Debug)]
enum PortError {
    Stale {
        session_id: u64,
        generation: u64,
    },
    Failed {
        service: &'static str,
        message: String,
    },
}

impl From<PortError> for ProjectRuntimeError {
    fn from(error: PortError) -> Self {
        match error {
            PortError::Stale {
                session_id,
                generation,
            } => Self::StaleSession {
                session_id,
                generation,
            },
            PortError::Failed { service, message } => Self::Port { service, message },
        }
    }
}

trait RuntimeProjectPorts: Send + Sync {
    fn authorize(&self) -> Result<(), PortError>;
    fn snapshot(&self) -> RuntimeFuture<Result<ProjectSnapshot, PortError>>;
    fn load_document(
        &self,
        document: DocumentId,
    ) -> RuntimeFuture<Result<parchmint_application::DocumentSnapshot, PortError>>;
    fn execute(&self, command: ProjectCommand) -> RuntimeFuture<Result<(), PortError>>;
    fn save(&self, kind: ProjectSaveKind) -> RuntimeFuture<Result<u64, PortError>>;
    fn set_appearance(
        &self,
        mode: AppearanceMode,
    ) -> RuntimeFuture<Result<ThemeSnapshot, PortError>>;
    fn update_global_dictionary(
        &self,
        word: String,
        add: bool,
    ) -> RuntimeFuture<Result<(), PortError>>;
    fn create_document(
        &self,
        _request: CreateDocumentWorkflow,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        Box::pin(async {
            Err(PortError::Failed {
                service: "ProjectWorkflowPort::create_document",
                message: "workflow port is unavailable".to_owned(),
            })
        })
    }
    fn move_nodes(
        &self,
        _request: MoveNodesWorkflow,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        Box::pin(async {
            Err(PortError::Failed {
                service: "ProjectWorkflowPort::move_nodes",
                message: "workflow port is unavailable".to_owned(),
            })
        })
    }
    fn delete_subtrees(
        &self,
        _request: DeleteSubtreesWorkflow,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        Box::pin(async {
            Err(PortError::Failed {
                service: "ProjectWorkflowPort::delete_subtrees",
                message: "workflow port is unavailable".to_owned(),
            })
        })
    }
    fn duplicate_subtrees(
        &self,
        _request: DuplicateSubtreesWorkflow,
    ) -> RuntimeFuture<Result<DuplicateWorkflowResult, PortError>> {
        Box::pin(async {
            Err(PortError::Failed {
                service: "ProjectWorkflowPort::duplicate_subtrees",
                message: "workflow port is unavailable".to_owned(),
            })
        })
    }
    fn create_named_snapshot(
        &self,
        _name: String,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        Box::pin(async {
            Err(PortError::Failed {
                service: "ProjectWorkflowPort::create_named_snapshot",
                message: "workflow port is unavailable".to_owned(),
            })
        })
    }
    fn restore_checkpoint(
        &self,
        _checkpoint: parchmint_domain::CheckpointId,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        Box::pin(async {
            Err(PortError::Failed {
                service: "ProjectWorkflowPort::restore_checkpoint",
                message: "workflow port is unavailable".to_owned(),
            })
        })
    }
}

struct ProjectUiPortAdapter {
    ports: ProjectUiPorts,
}

impl ProjectUiPortAdapter {
    fn stale(&self) -> PortError {
        let session = self.ports.session();
        PortError::Stale {
            session_id: session.session_id(),
            generation: session.generation(),
        }
    }
}

impl RuntimeProjectPorts for ProjectUiPortAdapter {
    fn authorize(&self) -> Result<(), PortError> {
        self.ports.access().map(|_| ()).map_err(|_| self.stale())
    }

    fn snapshot(&self) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .snapshot(|query| query.snapshot())
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .map_err(|error| PortError::Failed {
                    service: "ProjectSnapshotQuery::snapshot",
                    message: error.to_string(),
                })
        })
    }

    fn load_document(
        &self,
        document: DocumentId,
    ) -> RuntimeFuture<Result<parchmint_application::DocumentSnapshot, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .snapshot(|query| query.load_document(document))
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .map_err(|error| PortError::Failed {
                    service: "ProjectSnapshotQuery::load_document",
                    message: error.to_string(),
                })
        })
    }

    fn execute(&self, command: ProjectCommand) -> RuntimeFuture<Result<(), PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .commands_service()
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .execute(command)
                .await
                .map(|_| ())
                .map_err(application_error)
        })
    }

    fn save(&self, kind: ProjectSaveKind) -> RuntimeFuture<Result<u64, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let handle = {
                let access = ports.access().map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?;
                let (handle, _) = access
                    .persistence(|persistence| persistence.request_save(kind))
                    .map_err(|error| PortError::Stale {
                        session_id: error.session().session_id(),
                        generation: error.session().generation(),
                    })?
                    .map_err(|error| PortError::Failed {
                        service: "ProjectPersistencePort::request_save",
                        message: error.to_string(),
                    })?;
                handle
            };
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            let saved = access
                .persistence(|persistence| persistence.await_save(handle))
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .map_err(|error| PortError::Failed {
                    service: "ProjectPersistencePort::await_save",
                    message: error.to_string(),
                })?;
            Ok(saved.written.project_revision.value())
        })
    }

    fn set_appearance(
        &self,
        mode: AppearanceMode,
    ) -> RuntimeFuture<Result<ThemeSnapshot, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let preferences = {
                let access = ports.access().map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?;
                access
                    .preferences_service()
                    .map_err(|error| PortError::Stale {
                        session_id: error.session().session_id(),
                        generation: error.session().generation(),
                    })?
                    .load()
                    .await
                    .map_err(|error| PortError::Failed {
                        service: "PreferenceService::load",
                        message: error.to_string(),
                    })?
            };
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .appearance_service()
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .set_mode(preferences.revision, mode)
                .await
                .map_err(|error| PortError::Failed {
                    service: "AppearanceService::set_mode",
                    message: error.to_string(),
                })
        })
    }

    fn update_global_dictionary(
        &self,
        word: String,
        add: bool,
    ) -> RuntimeFuture<Result<(), PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let preferences = {
                let access = ports.access().map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?;
                access
                    .preferences_service()
                    .map_err(|error| PortError::Stale {
                        session_id: error.session().session_id(),
                        generation: error.session().generation(),
                    })?
                    .load()
                    .await
                    .map_err(|error| PortError::Failed {
                        service: "PreferenceService::load",
                        message: error.to_string(),
                    })?
            };
            let command = if add {
                PreferenceCommand::AddGlobalDictionaryWord(word)
            } else {
                PreferenceCommand::RemoveGlobalDictionaryWord(word)
            };
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .preferences_service()
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .update(preferences.revision, command)
                .await
                .map(|_| ())
                .map_err(|error| PortError::Failed {
                    service: "PreferenceService::update",
                    message: error.to_string(),
                })
        })
    }

    fn create_document(
        &self,
        request: CreateDocumentWorkflow,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .workflows(|workflows| workflows.create_document(request))
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .map(|result| result.snapshot)
                .map_err(|error| PortError::Failed {
                    service: "ProjectWorkflowPort::create_document",
                    message: error.to_string(),
                })
        })
    }

    fn move_nodes(
        &self,
        request: MoveNodesWorkflow,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .workflows(|workflows| workflows.move_nodes(request))
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .map(|result| result.snapshot)
                .map_err(|error| PortError::Failed {
                    service: "ProjectWorkflowPort::move_nodes",
                    message: error.to_string(),
                })
        })
    }

    fn delete_subtrees(
        &self,
        request: DeleteSubtreesWorkflow,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .workflows(|workflows| workflows.delete_subtrees(request))
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .map(|result| result.snapshot)
                .map_err(|error| PortError::Failed {
                    service: "ProjectWorkflowPort::delete_subtrees",
                    message: error.to_string(),
                })
        })
    }

    fn duplicate_subtrees(
        &self,
        request: DuplicateSubtreesWorkflow,
    ) -> RuntimeFuture<Result<DuplicateWorkflowResult, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .workflows(|workflows| workflows.duplicate_subtrees(request))
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .map(|result| DuplicateWorkflowResult {
                    snapshot: result.workflow.snapshot,
                    created_roots: result.created_roots,
                })
                .map_err(|error| PortError::Failed {
                    service: "ProjectWorkflowPort::duplicate_subtrees",
                    message: error.to_string(),
                })
        })
    }

    fn create_named_snapshot(
        &self,
        name: String,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .workflows(|workflows| workflows.create_named_snapshot(name))
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .map(|result| result.snapshot)
                .map_err(|error| PortError::Failed {
                    service: "ProjectWorkflowPort::create_named_snapshot",
                    message: error.to_string(),
                })
        })
    }

    fn restore_checkpoint(
        &self,
        checkpoint: parchmint_domain::CheckpointId,
    ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
        let ports = self.ports.clone();
        Box::pin(async move {
            let access = ports.access().map_err(|error| PortError::Stale {
                session_id: error.session().session_id(),
                generation: error.session().generation(),
            })?;
            access
                .workflows(|workflows| workflows.restore_checkpoint(checkpoint))
                .map_err(|error| PortError::Stale {
                    session_id: error.session().session_id(),
                    generation: error.session().generation(),
                })?
                .map(|result| result.snapshot)
                .map_err(|error| PortError::Failed {
                    service: "ProjectWorkflowPort::restore_checkpoint",
                    message: error.to_string(),
                })
        })
    }
}

fn application_error(error: ApplicationError) -> PortError {
    PortError::Failed {
        service: "ProjectCommandDispatcher::execute",
        message: error.to_string(),
    }
}

struct StableIdResolvers {
    nodes: BTreeMap<String, NodeId>,
    deleted_nodes: BTreeMap<String, NodeId>,
    documents: BTreeMap<String, DocumentId>,
    node_documents: BTreeMap<NodeId, DocumentId>,
    metadata_fields: BTreeMap<String, MetadataFieldId>,
    styles: BTreeMap<String, StyleId>,
}

impl StableIdResolvers {
    fn from_snapshot(snapshot: &ProjectSnapshot) -> Self {
        let mut nodes = BTreeMap::new();
        let mut node_documents = BTreeMap::new();
        for (id, node) in snapshot.project.nodes.iter() {
            nodes.insert(stable_id_string(id.as_bytes()), *id);
            if let NodeKind::Document(document) = node.kind {
                node_documents.insert(*id, document);
            }
        }
        Self {
            nodes,
            deleted_nodes: snapshot
                .project
                .deleted
                .keys()
                .map(|id| (stable_id_string(id.as_bytes()), *id))
                .collect(),
            documents: snapshot
                .document_summaries
                .iter()
                .map(|document| {
                    (
                        stable_id_string(document.document_id.as_bytes()),
                        document.document_id,
                    )
                })
                .chain(snapshot.documents.iter().map(|document| {
                    (
                        stable_id_string(document.document_id.as_bytes()),
                        document.document_id,
                    )
                }))
                .collect(),
            node_documents,
            metadata_fields: snapshot
                .project
                .metadata
                .iter()
                .map(|field| (stable_id_string(field.id.as_bytes()), field.id))
                .collect(),
            styles: snapshot
                .project
                .styles
                .iter()
                .map(|style| (stable_id_string(style.id.as_bytes()), style.id))
                .collect(),
        }
    }

    fn node(&self, ui_id: &str) -> Result<NodeId, ProjectRuntimeError> {
        self.nodes
            .get(ui_id)
            .copied()
            .ok_or_else(|| unknown_id(StableIdKind::Node, ui_id))
    }

    fn deleted_node(&self, ui_id: &str) -> Result<NodeId, ProjectRuntimeError> {
        self.deleted_nodes
            .get(ui_id)
            .copied()
            .ok_or_else(|| unknown_id(StableIdKind::DeletedNode, ui_id))
    }

    fn document(&self, ui_id: &str) -> Result<DocumentId, ProjectRuntimeError> {
        self.documents
            .get(ui_id)
            .copied()
            .ok_or_else(|| unknown_id(StableIdKind::Document, ui_id))
    }

    fn document_for_node(&self, node: NodeId) -> Result<DocumentId, ProjectRuntimeError> {
        let document =
            self.node_documents
                .get(&node)
                .copied()
                .ok_or(ProjectRuntimeError::InvalidEffect(
                    "only document nodes can be opened in an editor pane",
                ))?;
        if self
            .documents
            .values()
            .any(|candidate| *candidate == document)
        {
            Ok(document)
        } else {
            Err(unknown_id(
                StableIdKind::Document,
                stable_id_string(document.as_bytes()),
            ))
        }
    }

    fn metadata_field(&self, ui_id: &str) -> Result<MetadataFieldId, ProjectRuntimeError> {
        self.metadata_fields
            .get(ui_id)
            .copied()
            .ok_or_else(|| unknown_id(StableIdKind::MetadataField, ui_id))
    }

    fn style(&self, ui_id: &str) -> Result<StyleId, ProjectRuntimeError> {
        self.styles
            .get(ui_id)
            .copied()
            .ok_or_else(|| unknown_id(StableIdKind::Style, ui_id))
    }
}

fn unknown_id(kind: StableIdKind, ui_id: impl Into<String>) -> ProjectRuntimeError {
    ProjectRuntimeError::UnknownStableId {
        kind,
        ui_id: ui_id.into(),
    }
}

fn resolve_distinct_nodes(
    resolvers: &StableIdResolvers,
    ids: Vec<String>,
) -> Result<Vec<NodeId>, ProjectRuntimeError> {
    let mut seen = BTreeSet::new();
    ids.into_iter()
        .map(|id| resolvers.node(&id))
        .filter(|resolved| resolved.as_ref().is_err() || seen.insert(*resolved.as_ref().unwrap()))
        .collect()
}

fn plan_moves(
    snapshot: &ProjectSnapshot,
    resolvers: &StableIdResolvers,
    nodes: Vec<NodeId>,
    destination: DragDestination,
) -> Result<Vec<ProjectCommand>, ProjectRuntimeError> {
    let mut simulated = snapshot.project.clone();
    let mut commands = Vec::with_capacity(nodes.len());
    for node in nodes {
        let (parent, index) =
            match &destination {
                DragDestination::IntoGroup(target) => {
                    let parent = resolvers.node(target)?;
                    (parent, simulated.nodes.children(parent).len())
                }
                DragDestination::BeforeSibling(target) => {
                    let sibling = resolvers.node(target)?;
                    let parent = simulated.nodes.parent(sibling).ok_or(
                        ProjectRuntimeError::InvalidEffect("a fixed root cannot be a move sibling"),
                    )?;
                    let index = simulated
                        .nodes
                        .children(parent)
                        .iter()
                        .position(|candidate| *candidate == sibling)
                        .ok_or(ProjectRuntimeError::InvalidEffect(
                            "move sibling is absent from its parent",
                        ))?;
                    let old_index = simulated
                        .nodes
                        .parent(node)
                        .filter(|old_parent| *old_parent == parent)
                        .and_then(|_| {
                            simulated
                                .nodes
                                .children(parent)
                                .iter()
                                .position(|candidate| *candidate == node)
                        });
                    let index = if old_index.is_some_and(|old| old < index) {
                        index.saturating_sub(1)
                    } else {
                        index
                    };
                    (parent, index)
                }
                DragDestination::AfterSibling(target) => {
                    let sibling = resolvers.node(target)?;
                    let parent = simulated.nodes.parent(sibling).ok_or(
                        ProjectRuntimeError::InvalidEffect("a fixed root cannot be a move sibling"),
                    )?;
                    let mut index = simulated
                        .nodes
                        .children(parent)
                        .iter()
                        .position(|candidate| *candidate == sibling)
                        .ok_or(ProjectRuntimeError::InvalidEffect(
                            "move sibling is absent from its parent",
                        ))?
                        .saturating_add(1);
                    let old_index = simulated
                        .nodes
                        .parent(node)
                        .filter(|old_parent| *old_parent == parent)
                        .and_then(|_| {
                            simulated
                                .nodes
                                .children(parent)
                                .iter()
                                .position(|candidate| *candidate == node)
                        });
                    if old_index.is_some_and(|old| old < index) {
                        index = index.saturating_sub(1);
                    }
                    (parent, index)
                }
                DragDestination::EditorPane(_) => {
                    unreachable!("editor drops resolve before planning")
                }
            };
        let old_parent = simulated
            .nodes
            .parent(node)
            .ok_or(ProjectRuntimeError::InvalidEffect(
                "a fixed root cannot be moved",
            ))?;
        let old_index = simulated
            .nodes
            .children(old_parent)
            .iter()
            .position(|candidate| *candidate == node)
            .ok_or(ProjectRuntimeError::InvalidEffect(
                "move source is absent from its parent",
            ))?;
        let available_slots = simulated
            .nodes
            .children(parent)
            .len()
            .saturating_sub(usize::from(old_parent == parent));
        let index = index.min(available_slots);
        if old_parent == parent && old_index == index {
            return Err(ProjectRuntimeError::InvalidEffect(
                "move destination is the current location",
            ));
        }
        let command = ProjectCommand::move_node(node, parent, index);
        simulated = apply_project_command(&simulated, simulated.revision, command.clone())
            .map_err(|error| ProjectRuntimeError::Port {
                service: "apply_project_command move validation",
                message: error.to_string(),
            })?
            .project;
        commands.push(command);
    }
    Ok(commands)
}

fn paste_location(
    snapshot: &ProjectSnapshot,
    resolvers: &StableIdResolvers,
    destination: &DragDestination,
) -> Result<(NodeId, usize), ProjectRuntimeError> {
    match destination {
        DragDestination::IntoGroup(target) => {
            let parent = resolvers.node(target)?;
            if !snapshot
                .project
                .nodes
                .get(parent)
                .is_some_and(|node| node.kind.can_have_children())
            {
                return Err(ProjectRuntimeError::InvalidEffect(
                    "paste destination is not a container",
                ));
            }
            Ok((parent, snapshot.project.nodes.children(parent).len()))
        }
        DragDestination::BeforeSibling(target) | DragDestination::AfterSibling(target) => {
            let sibling = resolvers.node(target)?;
            let parent = snapshot.project.nodes.parent(sibling).ok_or(
                ProjectRuntimeError::InvalidEffect("a fixed root cannot be a paste sibling"),
            )?;
            let index = snapshot
                .project
                .nodes
                .children(parent)
                .iter()
                .position(|candidate| *candidate == sibling)
                .ok_or(ProjectRuntimeError::InvalidEffect(
                    "paste sibling is absent from its parent",
                ))?;
            Ok((
                parent,
                index + usize::from(matches!(destination, DragDestination::AfterSibling(_))),
            ))
        }
        DragDestination::EditorPane(_) => Err(ProjectRuntimeError::InvalidEffect(
            "tree clipboard cannot paste into an editor pane",
        )),
    }
}

fn validate_restore_location(
    snapshot: &ProjectSnapshot,
    resolvers: &StableIdResolvers,
    node: NodeId,
    location: &RestoreLocation,
) -> Result<(), ProjectRuntimeError> {
    let tombstone =
        snapshot.project.deleted.get(&node).ok_or_else(|| {
            unknown_id(StableIdKind::DeletedNode, stable_id_string(node.as_bytes()))
        })?;
    match location {
        RestoreLocation::FormerParent(parent) => {
            let requested = resolvers.node(parent)?;
            let live_container = snapshot
                .project
                .nodes
                .get(requested)
                .is_some_and(|node| node.kind.can_have_children());
            if requested != tombstone.former_parent || !live_container {
                return Err(ProjectRuntimeError::InvalidEffect(
                    "restore former parent no longer matches the tombstone",
                ));
            }
        }
        RestoreLocation::SectionRoot(root) => {
            let requested = resolvers.node(root)?;
            if requested != tombstone.section.root_id() {
                return Err(ProjectRuntimeError::InvalidEffect(
                    "restore fallback does not match the tombstone section",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn canonical_load(
    snapshot: &ProjectSnapshot,
    document: DocumentId,
) -> Result<CanonicalDocumentLoad, ProjectRuntimeError> {
    let source = snapshot
        .documents
        .iter()
        .find(|candidate| candidate.document_id == document)
        .ok_or_else(|| {
            unknown_id(
                StableIdKind::Document,
                stable_id_string(document.as_bytes()),
            )
        })?;
    let mut load = CanonicalDocumentLoad::new(document, source.body.clone());
    load.revision = source.revision;
    load.comments = source.comments.clone();
    load.styles = StyleCatalogProjection::new(snapshot.project.styles.clone());
    Ok(load)
}

fn document_revision_frontier(
    snapshot: &ProjectSnapshot,
) -> BTreeMap<DocumentId, parchmint_editor_api::EditorRevision> {
    snapshot
        .document_summaries
        .iter()
        .map(|summary| (summary.document_id, summary.revision))
        .chain(
            snapshot
                .documents
                .iter()
                .map(|document| (document.document_id, document.revision)),
        )
        .collect()
}

fn selection(start: u64, end: u64) -> EditorSelection {
    EditorSelection::new(DocumentPosition::from(start), DocumentPosition::from(end))
}

fn generated_node_id(snapshot: &ProjectSnapshot, operation: u64) -> NodeId {
    let mut ordinal = 0_u64;
    loop {
        let candidate = NodeId::from_bytes(derived_id_bytes(snapshot, operation, ordinal, 1));
        if node_id_is_available(snapshot, candidate) {
            return candidate;
        }
        ordinal = ordinal.saturating_add(1);
    }
}

fn generated_document_id(snapshot: &ProjectSnapshot, operation: u64) -> DocumentId {
    let mut ordinal = 0_u64;
    loop {
        let candidate = DocumentId::from_bytes(derived_id_bytes(snapshot, operation, ordinal, 2));
        if snapshot
            .documents
            .iter()
            .all(|document| document.document_id != candidate)
        {
            return candidate;
        }
        ordinal = ordinal.saturating_add(1);
    }
}

fn derived_id_bytes(snapshot: &ProjectSnapshot, operation: u64, ordinal: u64, tag: u8) -> [u8; 16] {
    let mut left = 0xcbf2_9ce4_8422_2325_u64;
    let mut right = 0x8422_2325_cbf2_9ce4_u64;
    for byte in snapshot
        .project
        .id
        .as_bytes()
        .iter()
        .copied()
        .chain(snapshot.project.revision.value().to_be_bytes())
        .chain(operation.to_be_bytes())
        .chain(ordinal.to_be_bytes())
        .chain([tag])
    {
        left ^= u64::from(byte);
        left = left.wrapping_mul(0x100_0000_01b3);
        right ^= u64::from(byte.rotate_left(1));
        right = right.wrapping_mul(0x100_0000_01b3);
    }
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&left.to_be_bytes());
    bytes[8..].copy_from_slice(&right.to_be_bytes());
    bytes
}

fn node_id_is_available(snapshot: &ProjectSnapshot, candidate: NodeId) -> bool {
    !candidate.is_fixed_root()
        && !snapshot.project.nodes.contains(candidate)
        && !snapshot.project.deleted.values().any(|tombstone| {
            tombstone
                .subtree
                .iter()
                .any(|deleted| deleted.node.id == candidate)
        })
}

fn unsupported_project_effect(effect: &ProjectEffect) -> Option<UnsupportedEffect> {
    let (category, missing_boundary) = match effect {
        ProjectEffect::SearchProject { .. } => (
            UnsupportedCategory::Search,
            "generation-scoped SearchIndex completion feed",
        ),
        ProjectEffect::BuildReplacementPreview { .. }
        | ProjectEffect::ApplyGlobalReplacement { .. } => (
            UnsupportedCategory::Replacement,
            "revision-scoped GlobalReplacement completion feed",
        ),
        ProjectEffect::ExportEntireManuscript { .. }
        | ProjectEffect::ChooseExportDestination { .. }
        | ProjectEffect::CancelExport => (
            UnsupportedCategory::Export,
            "progress-bearing Exporter completion feed",
        ),
        ProjectEffect::OpenExportResult(_) | ProjectEffect::RevealExportResult(_) => (
            UnsupportedCategory::ExternalOpen,
            "validated export artifact path resolver",
        ),
        ProjectEffect::ReinitializeHistory => (
            UnsupportedCategory::History,
            "session-scoped History maintenance completion",
        ),
        _ => return None,
    };
    Some(UnsupportedEffect {
        category,
        missing_boundary,
    })
}

fn parse_search_match_id(match_id: &str) -> Result<(String, FindMatch, u64), ProjectRuntimeError> {
    let parts = match_id.split(':').collect::<Vec<_>>();
    if parts.len() != 6 || parts[0].len() != 32 || parts[1].len() != 32 {
        return Err(ProjectRuntimeError::InvalidEffect(
            "search result identity is malformed",
        ));
    }
    let start = parts[3]
        .parse::<u64>()
        .map_err(|_| ProjectRuntimeError::InvalidEffect("search result start is malformed"))?;
    let end = parts[4]
        .parse::<u64>()
        .map_err(|_| ProjectRuntimeError::InvalidEffect("search result end is malformed"))?;
    let revision = parts[5]
        .parse::<u64>()
        .map_err(|_| ProjectRuntimeError::InvalidEffect("search result revision is malformed"))?;
    if start > end {
        return Err(ProjectRuntimeError::InvalidEffect(
            "search result range is reversed",
        ));
    }
    Ok((parts[0].to_owned(), FindMatch::new(start, end), revision))
}

fn parse_stable_hex(value: &str, field: &'static str) -> Result<[u8; 16], ProjectRuntimeError> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProjectRuntimeError::InvalidEffect(field));
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let pair =
            std::str::from_utf8(pair).map_err(|_| ProjectRuntimeError::InvalidEffect(field))?;
        bytes[index] =
            u8::from_str_radix(pair, 16).map_err(|_| ProjectRuntimeError::InvalidEffect(field))?;
    }
    Ok(bytes)
}

fn stable_id_string(bytes: &[u8; 16]) -> String {
    use std::fmt::Write as _;

    let mut serialized = String::with_capacity(32);
    for byte in bytes {
        write!(&mut serialized, "{byte:02x}").expect("writing to a String cannot fail");
    }
    serialized
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use iced::futures::executor::block_on;
    use parchmint_application::{DocumentSnapshot, DocumentVisibility};
    use parchmint_domain::{
        DocumentId, MetadataFieldDefinition, MetadataTextKind, NodeId, Project, ProjectId,
    };
    use parchmint_editor_api::{
        BlockId, CanonicalComment, CommentId, EditorRevision, EditorSelection,
    };
    use parchmint_preferences::ResolvedAppearance;

    use super::*;

    struct FakePorts {
        current: Mutex<bool>,
        snapshot: Mutex<ProjectSnapshot>,
        lazy_documents: Mutex<BTreeMap<DocumentId, DocumentSnapshot>>,
        duplicate_requests: Mutex<Vec<DuplicateSubtreesWorkflow>>,
    }

    impl FakePorts {
        fn new(snapshot: ProjectSnapshot) -> Self {
            Self {
                current: Mutex::new(true),
                snapshot: Mutex::new(snapshot),
                lazy_documents: Mutex::new(BTreeMap::new()),
                duplicate_requests: Mutex::new(Vec::new()),
            }
        }

        fn retire(&self) {
            *self.current.lock().expect("current mutex poisoned") = false;
        }

        fn replace_snapshot(&self, snapshot: ProjectSnapshot) {
            *self.snapshot.lock().expect("snapshot mutex poisoned") = snapshot;
        }

        fn add_lazy_document(&self, document: DocumentSnapshot) {
            self.lazy_documents
                .lock()
                .expect("lazy documents mutex poisoned")
                .insert(document.document_id, document);
        }
    }

    impl RuntimeProjectPorts for FakePorts {
        fn authorize(&self) -> Result<(), PortError> {
            if *self.current.lock().expect("current mutex poisoned") {
                Ok(())
            } else {
                Err(PortError::Stale {
                    session_id: 7,
                    generation: 3,
                })
            }
        }

        fn snapshot(&self) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
            let result = self.authorize().map(|_| {
                self.snapshot
                    .lock()
                    .expect("snapshot mutex poisoned")
                    .clone()
            });
            Box::pin(async move { result })
        }

        fn load_document(
            &self,
            document: DocumentId,
        ) -> RuntimeFuture<Result<DocumentSnapshot, PortError>> {
            let result = self.authorize().and_then(|_| {
                let mut snapshot = self.snapshot.lock().expect("snapshot mutex poisoned");
                if let Some(loaded) = snapshot
                    .documents
                    .iter()
                    .find(|candidate| candidate.document_id == document)
                    .cloned()
                {
                    return Ok(loaded);
                }
                let loaded = self
                    .lazy_documents
                    .lock()
                    .expect("lazy documents mutex poisoned")
                    .get(&document)
                    .cloned()
                    .ok_or_else(|| PortError::Failed {
                        service: "fake document loader",
                        message: "document body is not loaded".into(),
                    })?;
                snapshot.documents.push(loaded.clone());
                Ok(loaded)
            });
            Box::pin(async move { result })
        }

        fn execute(&self, command: ProjectCommand) -> RuntimeFuture<Result<(), PortError>> {
            let result = self.authorize().and_then(|_| {
                let mut snapshot = self.snapshot.lock().expect("snapshot mutex poisoned");
                let applied =
                    apply_project_command(&snapshot.project, snapshot.project.revision, command)
                        .map_err(|error| PortError::Failed {
                            service: "fake command dispatcher",
                            message: error.to_string(),
                        })?;
                snapshot.project = applied.project;
                Ok(())
            });
            Box::pin(async move { result })
        }

        fn save(&self, _kind: ProjectSaveKind) -> RuntimeFuture<Result<u64, PortError>> {
            let result = self.authorize().map(|_| {
                self.snapshot
                    .lock()
                    .expect("snapshot mutex poisoned")
                    .project
                    .revision
                    .value()
            });
            Box::pin(async move { result })
        }

        fn set_appearance(
            &self,
            mode: AppearanceMode,
        ) -> RuntimeFuture<Result<ThemeSnapshot, PortError>> {
            let result = self.authorize().map(|_| {
                ThemeSnapshot::new(
                    match mode {
                        AppearanceMode::Dark => ResolvedAppearance::Dark,
                        AppearanceMode::System | AppearanceMode::Light => ResolvedAppearance::Light,
                    },
                    1,
                )
            });
            Box::pin(async move { result })
        }

        fn update_global_dictionary(
            &self,
            _word: String,
            _add: bool,
        ) -> RuntimeFuture<Result<(), PortError>> {
            let result = self.authorize();
            Box::pin(async move { result })
        }

        fn create_document(
            &self,
            request: CreateDocumentWorkflow,
        ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
            let result = self.authorize().and_then(|_| {
                let mut snapshot = self.snapshot.lock().expect("snapshot mutex poisoned");
                let applied = apply_project_command(
                    &snapshot.project,
                    snapshot.project.revision,
                    ProjectCommand::create_document(
                        request.node,
                        request.document,
                        request.parent,
                        request.index,
                        request.title,
                    ),
                )
                .map_err(|error| PortError::Failed {
                    service: "fake document workflow",
                    message: error.to_string(),
                })?;
                snapshot.project = applied.project;
                snapshot.documents.push(DocumentSnapshot {
                    comments: Vec::new(),
                    document_id: request.document,
                    body: "<p></p>".to_owned(),
                    revision: EditorRevision::from(0),
                    visibility: DocumentVisibility::Open,
                });
                Ok(snapshot.clone())
            });
            Box::pin(async move { result })
        }

        fn move_nodes(
            &self,
            request: MoveNodesWorkflow,
        ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
            let result = self.authorize().and_then(|_| {
                let mut snapshot = self.snapshot.lock().expect("snapshot mutex poisoned");
                let mut project = snapshot.project.clone();
                for movement in request.moves {
                    project = apply_project_command(
                        &project,
                        project.revision,
                        ProjectCommand::move_node(movement.node, movement.parent, movement.index),
                    )
                    .map_err(|error| PortError::Failed {
                        service: "fake move workflow",
                        message: error.to_string(),
                    })?
                    .project;
                }
                snapshot.project = project;
                Ok(snapshot.clone())
            });
            Box::pin(async move { result })
        }

        fn delete_subtrees(
            &self,
            request: DeleteSubtreesWorkflow,
        ) -> RuntimeFuture<Result<ProjectSnapshot, PortError>> {
            let result = self.authorize().and_then(|_| {
                let mut snapshot = self.snapshot.lock().expect("snapshot mutex poisoned");
                let checkpoint = parchmint_domain::CheckpointId::from_bytes([0x77; 16]);
                let mut project = snapshot.project.clone();
                for node in request.nodes {
                    project = apply_project_command(
                        &project,
                        project.revision,
                        ProjectCommand::delete_node_from_checkpoint(
                            node,
                            request.deleted_at_unix_millis,
                            checkpoint,
                        ),
                    )
                    .map_err(|error| PortError::Failed {
                        service: "fake delete workflow",
                        message: error.to_string(),
                    })?
                    .project;
                }
                snapshot.project = project;
                Ok(snapshot.clone())
            });
            Box::pin(async move { result })
        }

        fn duplicate_subtrees(
            &self,
            request: DuplicateSubtreesWorkflow,
        ) -> RuntimeFuture<Result<DuplicateWorkflowResult, PortError>> {
            let result = self.authorize().map(|_| {
                let created_roots = request.sources.clone();
                self.duplicate_requests
                    .lock()
                    .expect("duplicate request mutex poisoned")
                    .push(request);
                DuplicateWorkflowResult {
                    snapshot: self
                        .snapshot
                        .lock()
                        .expect("snapshot mutex poisoned")
                        .clone(),
                    created_roots,
                }
            });
            Box::pin(async move { result })
        }
    }

    fn executor(snapshot: ProjectSnapshot) -> (NativeProjectEffectExecutor, Arc<FakePorts>) {
        let ports = Arc::new(FakePorts::new(snapshot.clone()));
        (
            NativeProjectEffectExecutor {
                ports: ports.clone(),
                snapshot: Arc::new(snapshot),
                operation_sequence: Arc::new(AtomicU64::new(0)),
            },
            ports,
        )
    }

    fn fixture() -> (ProjectSnapshot, NodeId, DocumentId, MetadataFieldId) {
        let mut project = Project::new(ProjectId::from_bytes([1; 16]));
        let node = NodeId::from_bytes([3; 16]);
        let document = DocumentId::from_bytes([4; 16]);
        let field = MetadataFieldId::from_bytes([5; 16]);
        project
            .nodes
            .try_insert_document(node, document, NodeId::manuscript_root(), 0, "Chapter One")
            .expect("fixture document inserts");
        project
            .metadata
            .upsert(MetadataFieldDefinition {
                id: field,
                label: "Status".to_owned(),
                description: None,
                applicability: parchmint_domain::MetadataApplicability::Documents,
                text_kind: MetadataTextKind::SingleLine,
                default_value: None,
                visible_on_cards: true,
            })
            .expect("fixture metadata inserts");
        let snapshot = ProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents: vec![DocumentSnapshot {
                comments: Vec::new(),
                document_id: document,
                body: "hello".to_owned(),
                revision: EditorRevision::from(1),
                visibility: DocumentVisibility::Open,
            }],
            styles_css: String::new(),
        };
        (snapshot, node, document, field)
    }

    #[test]
    fn canonical_load_hydrates_revision_comments_and_styles_for_initial_mounts() {
        let (mut snapshot, _, document, _) = fixture();
        snapshot.documents[0].revision = EditorRevision::from(7);
        snapshot.documents[0].comments.push(CanonicalComment::new(
            CommentId::from_bytes([8; 16]),
            EditorSelection::new(0.into(), 5.into()),
            "Hydrated",
            BlockId::from_bytes(*document.as_bytes()),
        ));

        let load = canonical_load(&snapshot, document).unwrap();

        assert_eq!(load.revision, EditorRevision::from(7));
        assert_eq!(load.comments, snapshot.documents[0].comments);
        assert_eq!(
            load.styles,
            StyleCatalogProjection::new(snapshot.project.styles.clone())
        );
    }

    #[test]
    fn mount_document_loads_a_summary_only_document_on_demand() {
        let (mut snapshot, _, document, _) = fixture();
        let lazy = snapshot.documents.pop().expect("fixture body");
        snapshot
            .document_summaries
            .push(parchmint_ui_api::DocumentSummary {
                document_id: document,
                revision: lazy.revision,
                visibility: lazy.visibility,
                content_hash: None,
                word_count: parchmint_ui_api::DocumentWordCount::Known(1),
            });
        let (executor, ports) = executor(snapshot);
        ports.add_lazy_document(lazy);

        let completion = block_on(executor.execute_editor_effect(EditorEffect::MountDocument {
            pane: EditorPane::Primary,
            view: ViewId::from_bytes([17; 16]),
            document_id: stable_id_string(document.as_bytes()),
        }))
        .expect("summary-only document should load through the session port");
        let EditorEffectCompletion::Intent(EditorRuntimeIntent::Mount { load, .. }) = completion
        else {
            panic!("expected mount completion")
        };
        assert_eq!(load.document_id, document);
        assert_eq!(load.body, "hello");
    }

    #[test]
    fn project_open_document_effect_hydrates_and_returns_a_refreshed_snapshot() {
        let (mut snapshot, _, _, _) = fixture();
        let node = NodeId::from_bytes([18; 16]);
        let document = DocumentId::from_bytes([19; 16]);
        snapshot
            .project
            .nodes
            .try_insert_document(node, document, NodeId::manuscript_root(), 1, "Chapter Two")
            .unwrap();
        snapshot
            .document_summaries
            .push(parchmint_ui_api::DocumentSummary {
                document_id: document,
                revision: EditorRevision::from(3),
                visibility: DocumentVisibility::Closed,
                content_hash: None,
                word_count: parchmint_ui_api::DocumentWordCount::Known(2),
            });
        let (executor, ports) = executor(snapshot);
        ports.add_lazy_document(DocumentSnapshot {
            document_id: document,
            body: "second chapter".into(),
            comments: Vec::new(),
            revision: EditorRevision::from(3),
            visibility: DocumentVisibility::Closed,
        });

        let completion = block_on(executor.execute_project_effect(
            ProjectEffect::OpenDocumentInPrimary(stable_id_string(document.as_bytes())),
        ))
        .unwrap();
        let ProjectEffectCompletion::OpenDocuments {
            snapshot,
            documents,
        } = completion
        else {
            panic!("expected hydrated document completion")
        };
        assert!(
            snapshot
                .documents
                .iter()
                .any(|loaded| loaded.document_id == document)
        );
        assert_eq!(documents[0].load.body, "second chapter");
    }

    #[test]
    fn summary_hydration_does_not_make_search_navigation_snapshot_stale() {
        let (mut snapshot, _, _, _) = fixture();
        let node = NodeId::from_bytes([20; 16]);
        let document = DocumentId::from_bytes([21; 16]);
        snapshot
            .project
            .nodes
            .try_insert_document(
                node,
                document,
                NodeId::manuscript_root(),
                1,
                "Search Target",
            )
            .unwrap();
        snapshot
            .document_summaries
            .push(parchmint_ui_api::DocumentSummary {
                document_id: document,
                revision: EditorRevision::from(1),
                visibility: DocumentVisibility::Closed,
                content_hash: None,
                word_count: parchmint_ui_api::DocumentWordCount::Pending,
            });
        let (executor, ports) = executor(snapshot.clone());
        let mut hydrated = snapshot;
        hydrated.document_summaries.last_mut().unwrap().word_count =
            parchmint_ui_api::DocumentWordCount::Known(1);
        hydrated.document_summaries.last_mut().unwrap().content_hash = Some([8; 32]);
        ports.replace_snapshot(hydrated);
        ports.add_lazy_document(DocumentSnapshot {
            document_id: document,
            body: "world".into(),
            comments: Vec::new(),
            revision: EditorRevision::from(1),
            visibility: DocumentVisibility::Closed,
        });
        let match_id = format!(
            "{}:{}:Body:0:5:1",
            stable_id_string(document.as_bytes()),
            stable_id_string(&[22; 16])
        );

        let completion = block_on(executor.execute_project_effect(
            ProjectEffect::NavigateSearchResult {
                match_id,
                revalidate_revision: true,
            },
        ))
        .unwrap();
        assert!(matches!(
            completion,
            ProjectEffectCompletion::NavigateSearch { document: mount, .. }
                if mount.load.body == "world"
        ));
    }

    #[test]
    fn refreshed_executor_uses_the_post_persist_snapshot_without_resetting_sequence() {
        let (snapshot, _, document, _) = fixture();
        let (executor, _) = executor(snapshot.clone());
        executor.operation_sequence.store(9, Ordering::SeqCst);
        let mut refreshed_snapshot = snapshot;
        refreshed_snapshot.documents[0].revision = EditorRevision::from(2);
        refreshed_snapshot.documents[0]
            .comments
            .push(CanonicalComment::new(
                CommentId::from_bytes([9; 16]),
                EditorSelection::new(0.into(), 5.into()),
                "Persisted",
                BlockId::from_bytes(*document.as_bytes()),
            ));

        let refreshed = executor.refreshed(Arc::new(refreshed_snapshot.clone()));

        assert_eq!(refreshed.snapshot, Arc::new(refreshed_snapshot));
        assert_eq!(refreshed.operation_sequence.load(Ordering::SeqCst), 9);
    }

    #[test]
    fn resolver_accepts_only_snapshot_derived_ui_ids() {
        let (snapshot, node, document, field) = fixture();
        let resolver = StableIdResolvers::from_snapshot(&snapshot);

        assert_eq!(resolver.node(&stable_id_string(node.as_bytes())), Ok(node));
        assert_eq!(
            resolver.document(&stable_id_string(document.as_bytes())),
            Ok(document)
        );
        assert_eq!(
            resolver.metadata_field(&stable_id_string(field.as_bytes())),
            Ok(field)
        );
        assert!(matches!(
            resolver.node("03030303030303030303030303030304"),
            Err(ProjectRuntimeError::UnknownStableId {
                kind: StableIdKind::Node,
                ..
            })
        ));
    }

    #[test]
    fn stale_snapshot_is_rejected_before_id_resolution() {
        let (snapshot, node, _, _) = fixture();
        let (executor, ports) = executor(snapshot.clone());
        let mut newer = snapshot;
        newer.project.revision = ProjectRevision::from(9);
        ports.replace_snapshot(newer);

        let result = block_on(
            executor.execute_project_effect(ProjectEffect::CommitNodeTitle {
                node_id: stable_id_string(node.as_bytes()),
                title: "Changed".to_owned(),
            }),
        );

        assert!(matches!(
            result,
            Err(ProjectRuntimeError::StaleSnapshot { .. })
        ));
    }

    #[test]
    fn stale_session_is_rejected_before_any_action() {
        let (snapshot, node, _, _) = fixture();
        let (executor, ports) = executor(snapshot);
        ports.retire();

        let result = block_on(
            executor.execute_project_effect(ProjectEffect::CommitNodeTitle {
                node_id: stable_id_string(node.as_bytes()),
                title: "Changed".to_owned(),
            }),
        );

        assert_eq!(
            result,
            Err(ProjectRuntimeError::StaleSession {
                session_id: 7,
                generation: 3,
            })
        );
    }

    #[test]
    fn mutation_returns_requeried_authoritative_snapshot() {
        let (snapshot, node, _, _) = fixture();
        let (executor, _) = executor(snapshot);

        let result = block_on(
            executor.execute_project_effect(ProjectEffect::CommitNodeTitle {
                node_id: stable_id_string(node.as_bytes()),
                title: "Authoritative Title".to_owned(),
            }),
        )
        .expect("rename succeeds");
        let ProjectEffectCompletion::RefreshedSnapshot(snapshot) = result else {
            panic!("mutation must return a refreshed snapshot");
        };

        assert_eq!(
            snapshot
                .project
                .nodes
                .get(node)
                .map(|node| node.title.as_str()),
            Some("Authoritative Title")
        );
        assert_eq!(snapshot.project.revision, ProjectRevision::from(1));
    }

    #[test]
    fn generated_node_ids_are_deterministic_and_skip_snapshot_collisions() {
        let (snapshot, _, _, _) = fixture();
        let first = generated_node_id(&snapshot, 4);
        assert_eq!(first, generated_node_id(&snapshot, 4));

        let mut collided = snapshot;
        collided
            .project
            .nodes
            .try_insert_group(first, NodeId::manuscript_root(), usize::MAX, "Collision")
            .expect("candidate can be reserved in the collision fixture");

        assert_ne!(first, generated_node_id(&collided, 4));
    }

    #[test]
    fn document_creation_uses_the_atomic_project_workflow() {
        let (snapshot, _, _, _) = fixture();
        let (executor, ports) = executor(snapshot);

        let result = block_on(
            executor.execute_project_effect(ProjectEffect::CreateHierarchy {
                parent_id: stable_id_string(NodeId::manuscript_root().as_bytes()),
                kind: HierarchyItemKind::Document,
            }),
        );

        let Ok(ProjectEffectCompletion::WorkflowSnapshot(created)) = result else {
            panic!("document creation must return the durable workflow snapshot");
        };
        assert_eq!(created.documents.len(), 2);
        assert!(
            created
                .documents
                .iter()
                .any(|document| document.body == "<p></p>")
        );
        assert_eq!(
            ports
                .snapshot
                .lock()
                .expect("snapshot mutex poisoned")
                .project
                .nodes
                .children(NodeId::manuscript_root())
                .len(),
            2
        );
    }

    #[test]
    fn unsupported_search_effect_has_typed_category_and_is_not_success() {
        let (snapshot, _, _, _) = fixture();
        let (executor, _) = executor(snapshot);

        let result = block_on(
            executor.execute_project_effect(ProjectEffect::SearchProject {
                query: "anything".to_owned(),
                case_sensitive: false,
                whole_word: false,
                generation: 1,
            }),
        );

        assert_eq!(
            result,
            Err(ProjectRuntimeError::Unsupported(UnsupportedEffect {
                category: UnsupportedCategory::Search,
                missing_boundary: "generation-scoped SearchIndex completion feed",
            }))
        );
    }

    #[test]
    fn cut_paste_moves_through_the_durable_workflow_and_returns_refresh() {
        let (snapshot, node, _, _) = fixture();
        let (executor, ports) = executor(snapshot);
        let node_id = stable_id_string(node.as_bytes());
        let research = stable_id_string(NodeId::research_root().as_bytes());

        let result = block_on(
            executor.execute_project_effect(ProjectEffect::PasteCutSubtrees {
                node_ids: vec![node_id],
                destination: DragDestination::IntoGroup(research),
            }),
        )
        .expect("valid cut paste succeeds");

        assert!(matches!(
            result,
            ProjectEffectCompletion::TreePaste {
                kind: TreeClipboardKind::Cut,
                ..
            }
        ));
        assert_eq!(
            ports.snapshot.lock().unwrap().project.nodes.parent(node),
            Some(NodeId::research_root())
        );
    }

    #[test]
    fn copy_paste_forwards_a_mixed_multi_root_forest_to_one_workflow() {
        let (mut snapshot, document_node, _, _) = fixture();
        let group = NodeId::from_bytes([8; 16]);
        snapshot
            .project
            .nodes
            .try_insert_group(group, NodeId::manuscript_root(), 1, "Notes")
            .unwrap();
        let (executor, ports) = executor(snapshot);

        let result = block_on(executor.execute_project_effect(
            ProjectEffect::PasteCopiedSubtrees {
                node_ids: vec![
                    stable_id_string(document_node.as_bytes()),
                    stable_id_string(group.as_bytes()),
                ],
                destination: DragDestination::IntoGroup(stable_id_string(
                    NodeId::research_root().as_bytes(),
                )),
            },
        ))
        .expect("mixed copy paste reaches the workflow");

        let ProjectEffectCompletion::TreePaste {
            kind: TreeClipboardKind::Copy,
            created_roots,
            ..
        } = result
        else {
            panic!("copy paste must return created roots");
        };
        assert_eq!(
            created_roots,
            [
                stable_id_string(document_node.as_bytes()),
                stable_id_string(group.as_bytes()),
            ]
        );
        let requests = ports.duplicate_requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].sources, [document_node, group]);
        assert_eq!(requests[0].parent, NodeId::research_root());
    }

    #[test]
    fn invalid_and_stale_cut_pastes_fail_before_the_move_workflow() {
        let (snapshot, node, _, _) = fixture();
        let node_id = stable_id_string(node.as_bytes());
        let (invalid_executor, ports) = executor(snapshot.clone());
        let invalid = block_on(invalid_executor.execute_project_effect(
            ProjectEffect::PasteCutSubtrees {
                node_ids: vec![node_id.clone()],
                destination: DragDestination::IntoGroup(node_id.clone()),
            },
        ));
        assert!(invalid.is_err());
        assert_eq!(ports.snapshot.lock().unwrap().project, snapshot.project);

        let (stale_executor, stale_ports) = executor(snapshot);
        stale_ports.retire();
        assert!(matches!(
            block_on(
                stale_executor.execute_project_effect(ProjectEffect::PasteCutSubtrees {
                    node_ids: vec![node_id],
                    destination: DragDestination::IntoGroup(stable_id_string(
                        NodeId::research_root().as_bytes()
                    )),
                })
            ),
            Err(ProjectRuntimeError::StaleSession { .. })
        ));
    }
}
