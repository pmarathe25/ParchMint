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
};

use parchmint_application::{ApplicationError, CreateDocumentWorkflow};
use parchmint_domain::{
    DocumentId, MetadataApplicability, MetadataFieldId, NodeId, NodeKind, ProjectCommand,
    ProjectRevision, apply_project_command,
};
use parchmint_editor_api::{
    CanonicalDocumentLoad, DocumentPosition, EditorSelection,
    SearchDecoration as AdapterSearchDecoration, SpellcheckDecoration as AdapterSpellDecoration,
    StyleCatalogProjection, ViewId,
};
use parchmint_preferences::{AppearanceMode, PreferenceCommand, ThemeSnapshot};
use parchmint_ui_api::{ProjectSaveKind, ProjectSnapshot, ProjectUiPorts};

use crate::{
    DragDestination, EditorCommand, EditorEffect, EditorPane, FindMatch, HierarchyItemKind,
    HistoryRestoreScope, ProjectEffect, RestoreLocation, SpellingDictionaryScope, SpellingMenu,
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
            ProjectEffect::OpenDocumentInPrimary(document_id) => self
                .open_documents(&resolvers, EditorPane::Primary, [document_id])
                .map(ProjectEffectCompletion::OpenDocuments),
            ProjectEffect::OpenDocumentInCompanion(document_id) => self
                .open_documents(&resolvers, EditorPane::Companion, [document_id])
                .map(ProjectEffectCompletion::OpenDocuments),
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
                self.execute_commands(nodes.into_iter().map(ProjectCommand::delete_node))
                    .await
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
                    return self
                        .open_documents_by_id(pane, documents)
                        .map(ProjectEffectCompletion::OpenDocuments);
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
            ProjectEffect::SetMetadataApplicability {
                field_id,
                applies_to_documents,
            } => {
                let field = resolvers.metadata_field(&field_id)?;
                let mut definition = current
                    .project
                    .metadata
                    .get(field)
                    .cloned()
                    .ok_or_else(|| unknown_id(StableIdKind::MetadataField, field_id))?;
                definition.applicability = match (definition.applicability, applies_to_documents) {
                    (MetadataApplicability::GroupsAndDocuments, false) => {
                        MetadataApplicability::Groups
                    }
                    (MetadataApplicability::Groups, true)
                    | (MetadataApplicability::Documents, true) => {
                        MetadataApplicability::GroupsAndDocuments
                    }
                    (MetadataApplicability::Documents, false) => {
                        return Err(ProjectRuntimeError::Unsupported(UnsupportedEffect {
                            category: UnsupportedCategory::MetadataApplicability,
                            missing_boundary: "MetadataApplicability::None",
                        }));
                    }
                    (current, _) => current,
                };
                self.execute_commands([ProjectCommand::upsert_metadata_field(definition)])
                    .await
            }
            ProjectEffect::RenameMetadataField { field_id, label } => {
                let field = resolvers.metadata_field(&field_id)?;
                let mut definition = current
                    .project
                    .metadata
                    .get(field)
                    .cloned()
                    .ok_or_else(|| unknown_id(StableIdKind::MetadataField, field_id))?;
                definition.label = label;
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
                let source = current
                    .documents
                    .iter()
                    .find(|source| source.document_id == document)
                    .ok_or_else(|| unknown_id(StableIdKind::Document, document_id.clone()))?;
                if revalidate_revision && source.revision.value() != indexed_revision {
                    return Err(ProjectRuntimeError::InvalidEffect(
                        "search result no longer matches the current document revision",
                    ));
                }
                Ok(ProjectEffectCompletion::NavigateSearch {
                    document: ResolvedDocumentMount {
                        pane: EditorPane::Primary,
                        load: canonical_load(&current, document)?,
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
            ProjectEffect::CopyDocuments(_)
            | ProjectEffect::PasteCopiedDocuments { .. }
            | ProjectEffect::PasteCutDocuments { .. }
            | ProjectEffect::SearchProject { .. }
            | ProjectEffect::BuildReplacementPreview { .. }
            | ProjectEffect::ApplyGlobalReplacement { .. }
            | ProjectEffect::ExportEntireManuscript { .. }
            | ProjectEffect::OpenExportResult(_)
            | ProjectEffect::RevealExportResult(_) => {
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
            EditorEffect::MountDocument {
                pane,
                view,
                document_id,
            } => {
                let document = resolvers.document(&document_id)?;
                let load = canonical_load(&current, document)?;
                Ok(EditorEffectCompletion::Intent(EditorRuntimeIntent::Mount {
                    pane,
                    view,
                    load,
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
            EditorEffect::NavigateCommentAnchor { .. }
            | EditorEffect::ShowOrphanedComment { .. } => {
                Err(ProjectRuntimeError::Unsupported(UnsupportedEffect {
                    category: UnsupportedCategory::CommentNavigation,
                    missing_boundary: "typed CommentId resolver and mounted comment-anchor feed",
                }))
            }
        }
    }

    async fn current_snapshot(&self) -> Result<ProjectSnapshot, ProjectRuntimeError> {
        let current = self.ports.snapshot().await?;
        if self.snapshot.as_ref() != &current {
            return Err(ProjectRuntimeError::StaleSnapshot {
                expected: self.snapshot.project.revision,
                actual: current.project.revision,
            });
        }
        Ok(current)
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

    fn open_documents(
        &self,
        resolvers: &StableIdResolvers,
        pane: EditorPane,
        document_ids: impl IntoIterator<Item = String>,
    ) -> Result<Vec<ResolvedDocumentMount>, ProjectRuntimeError> {
        let documents = document_ids
            .into_iter()
            .map(|id| resolvers.document(&id))
            .collect::<Result<Vec<_>, _>>()?;
        self.open_documents_by_id(pane, documents)
    }

    fn open_documents_by_id(
        &self,
        pane: EditorPane,
        documents: impl IntoIterator<Item = DocumentId>,
    ) -> Result<Vec<ResolvedDocumentMount>, ProjectRuntimeError> {
        documents
            .into_iter()
            .map(|document| {
                Ok(ResolvedDocumentMount {
                    pane,
                    load: canonical_load(&self.snapshot, document)?,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectEffectCompletion {
    RefreshedSnapshot(Box<ProjectSnapshot>),
    WorkflowSnapshot(Box<ProjectSnapshot>),
    OpenDocuments(Vec<ResolvedDocumentMount>),
    ApplyAppearance(ThemeSnapshot),
    SavedThrough(u64),
    FocusRecoveredEditor,
    NavigateSearch {
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
pub(crate) enum EditorEffectCompletion {
    Intent(EditorRuntimeIntent),
    ProjectMutation(ProjectEffectCompletion),
    GlobalDictionaryUpdated,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnsupportedCategory {
    CopyPaste,
    Search,
    Replacement,
    Export,
    ExternalOpen,
    CommentNavigation,
    MetadataApplicability,
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
                .documents
                .iter()
                .map(|document| {
                    (
                        stable_id_string(document.document_id.as_bytes()),
                        document.document_id,
                    )
                })
                .collect(),
            node_documents,
            metadata_fields: snapshot
                .project
                .metadata
                .iter()
                .map(|field| (stable_id_string(field.id.as_bytes()), field.id))
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
                    (parent, index)
                }
                DragDestination::EditorPane(_) => {
                    unreachable!("editor drops resolve before planning")
                }
            };
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

fn canonical_load(
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
    load.styles = StyleCatalogProjection::new(snapshot.project.styles.clone());
    Ok(load)
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
        ProjectEffect::CopyDocuments(_)
        | ProjectEffect::PasteCopiedDocuments { .. }
        | ProjectEffect::PasteCutDocuments { .. } => (
            UnsupportedCategory::CopyPaste,
            "session-scoped clipboard payload feed",
        ),
        ProjectEffect::SearchProject { .. } => (
            UnsupportedCategory::Search,
            "generation-scoped SearchIndex completion feed",
        ),
        ProjectEffect::BuildReplacementPreview { .. }
        | ProjectEffect::ApplyGlobalReplacement { .. } => (
            UnsupportedCategory::Replacement,
            "revision-scoped GlobalReplacement completion feed",
        ),
        ProjectEffect::ExportEntireManuscript { .. } => (
            UnsupportedCategory::Export,
            "progress-bearing Exporter completion feed",
        ),
        ProjectEffect::OpenExportResult(_) | ProjectEffect::RevealExportResult(_) => (
            UnsupportedCategory::ExternalOpen,
            "validated export artifact path resolver",
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
    use parchmint_editor_api::EditorRevision;
    use parchmint_preferences::ResolvedAppearance;

    use super::*;

    struct FakePorts {
        current: Mutex<bool>,
        snapshot: Mutex<ProjectSnapshot>,
    }

    impl FakePorts {
        fn new(snapshot: ProjectSnapshot) -> Self {
            Self {
                current: Mutex::new(true),
                snapshot: Mutex::new(snapshot),
            }
        }

        fn retire(&self) {
            *self.current.lock().expect("current mutex poisoned") = false;
        }

        fn replace_snapshot(&self, snapshot: ProjectSnapshot) {
            *self.snapshot.lock().expect("snapshot mutex poisoned") = snapshot;
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
                    document_id: request.document,
                    body: "<p></p>".to_owned(),
                    revision: EditorRevision::from(0),
                    visibility: DocumentVisibility::Open,
                });
                Ok(snapshot.clone())
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
                applicability: MetadataApplicability::Documents,
                text_kind: MetadataTextKind::SingleLine,
                default_value: None,
                visible_on_cards: true,
            })
            .expect("fixture metadata inserts");
        let snapshot = ProjectSnapshot {
            project,
            documents: vec![DocumentSnapshot {
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
    fn unsupported_effect_has_typed_category_and_is_not_success() {
        let (snapshot, _, _, _) = fixture();
        let (executor, _) = executor(snapshot);

        let result = block_on(
            executor
                .execute_project_effect(ProjectEffect::CopyDocuments(vec!["anything".to_owned()])),
        );

        assert_eq!(
            result,
            Err(ProjectRuntimeError::Unsupported(UnsupportedEffect {
                category: UnsupportedCategory::CopyPaste,
                missing_boundary: "session-scoped clipboard payload feed",
            }))
        );
    }
}
