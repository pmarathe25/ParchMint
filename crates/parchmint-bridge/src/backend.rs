//! QObject-facing projection of the Rust project workspace.

use self::qobject::ParchMintBackend;
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use parchmint_app::{DropPlacement, OutlineSort, ProjectWorkspace};
use parchmint_domain::{DocumentMetadata, NodeId};

/// Rust state hidden behind the generated QObject.
pub struct ParchMintBackendRust {
    status: QString,
    node_count: i32,
    revision: u64,
    document_revision: u64,
    save_status: QString,
    source_mode: bool,
    project_name: QString,
    project_open: bool,
    selected_count: i32,
    selected_id: QString,
    selected_title: QString,
    selected_synopsis: QString,
    selected_status: QString,
    selected_label: QString,
    filter: String,
    sort: OutlineSort,
    workspace: Option<ProjectWorkspace>,
}

impl Default for ParchMintBackendRust {
    fn default() -> Self {
        Self {
            status: QString::from("Create or open a project"),
            node_count: 0,
            revision: 0,
            document_revision: 0,
            save_status: QString::from("No project"),
            source_mode: false,
            project_name: QString::default(),
            project_open: false,
            selected_count: 0,
            selected_id: QString::default(),
            selected_title: QString::default(),
            selected_synopsis: QString::default(),
            selected_status: QString::default(),
            selected_label: QString::default(),
            filter: String::new(),
            sort: OutlineSort::Binder,
            workspace: None,
        }
    }
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status)]
        #[qproperty(i32, node_count, READ, NOTIFY)]
        #[qproperty(u64, revision, READ, NOTIFY)]
        #[qproperty(u64, document_revision)]
        #[qproperty(QString, save_status)]
        #[qproperty(bool, source_mode)]
        #[qproperty(QString, project_name)]
        #[qproperty(bool, project_open)]
        #[qproperty(i32, selected_count, READ, NOTIFY)]
        #[qproperty(QString, selected_id)]
        #[qproperty(QString, selected_title)]
        #[qproperty(QString, selected_synopsis)]
        #[qproperty(QString, selected_status)]
        #[qproperty(QString, selected_label)]
        type ParchMintBackend = super::ParchMintBackendRust;

        #[qinvokable]
        #[cxx_name = "nodeTitle"]
        fn node_title(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "nodeSynopsis"]
        fn node_synopsis(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "nodeStatus"]
        fn node_status(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "nodeLabel"]
        fn node_label(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "nodeId"]
        fn node_id(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "nodeDepth"]
        fn node_depth(self: &ParchMintBackend, row: i32) -> i32;
        #[qinvokable]
        #[cxx_name = "nodeParent"]
        fn node_parent(self: &ParchMintBackend, row: i32) -> i32;
        #[qinvokable]
        #[cxx_name = "nodeIsGroup"]
        fn node_is_group(self: &ParchMintBackend, row: i32) -> bool;
        #[qinvokable]
        #[cxx_name = "nodeIsRoot"]
        fn node_is_root(self: &ParchMintBackend, row: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "createProject"]
        fn create_project(self: Pin<&mut ParchMintBackend>, path: &QString, name: &QString)
        -> bool;
        #[qinvokable]
        #[cxx_name = "openProject"]
        fn open_project(self: Pin<&mut ParchMintBackend>, path: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "closeProject"]
        fn close_project(self: Pin<&mut ParchMintBackend>);
        #[qinvokable]
        #[cxx_name = "selectNode"]
        fn select_node(self: Pin<&mut ParchMintBackend>, id: &QString, additive: bool);
        #[qinvokable]
        #[cxx_name = "setFilter"]
        fn set_filter(self: Pin<&mut ParchMintBackend>, filter: &QString);
        #[qinvokable]
        #[cxx_name = "setOutlineSort"]
        fn set_outline_sort(self: Pin<&mut ParchMintBackend>, sort: &QString);
        #[qinvokable]
        #[cxx_name = "createChild"]
        fn create_child(
            self: Pin<&mut ParchMintBackend>,
            parent: &QString,
            title: &QString,
            group: bool,
        ) -> bool;
        #[qinvokable]
        #[cxx_name = "renameNode"]
        fn rename_node(self: Pin<&mut ParchMintBackend>, id: &QString, title: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "editSummary"]
        fn edit_summary(self: Pin<&mut ParchMintBackend>, id: &QString, summary: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "editStatus"]
        fn edit_status(self: Pin<&mut ParchMintBackend>, id: &QString, status: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "editLabel"]
        fn edit_label(self: Pin<&mut ParchMintBackend>, id: &QString, label: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "duplicateNode"]
        fn duplicate_node(self: Pin<&mut ParchMintBackend>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "moveNode"]
        fn move_node(
            self: Pin<&mut ParchMintBackend>,
            id: &QString,
            target: &QString,
            placement: &QString,
        ) -> bool;
        #[qinvokable]
        #[cxx_name = "moveUp"]
        fn move_up(self: Pin<&mut ParchMintBackend>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "moveDown"]
        fn move_down(self: Pin<&mut ParchMintBackend>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "indentNode"]
        fn indent_node(self: Pin<&mut ParchMintBackend>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "outdentNode"]
        fn outdent_node(self: Pin<&mut ParchMintBackend>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "trashNode"]
        fn trash_node(self: Pin<&mut ParchMintBackend>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "restoreNode"]
        fn restore_node(self: Pin<&mut ParchMintBackend>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "undoStructural"]
        fn undo_structural(self: Pin<&mut ParchMintBackend>) -> bool;
        #[qinvokable]
        #[cxx_name = "redoStructural"]
        fn redo_structural(self: Pin<&mut ParchMintBackend>) -> bool;

        #[qinvokable]
        #[cxx_name = "validateMarkdown"]
        fn validate_markdown(self: &ParchMintBackend, source: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "beginSourceMode"]
        fn begin_source_mode(self: Pin<&mut ParchMintBackend>, source: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "commitSourceMode"]
        fn commit_source_mode(self: Pin<&mut ParchMintBackend>, source: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "noteEditorDelta"]
        fn note_editor_delta(
            self: Pin<&mut ParchMintBackend>,
            revision: u64,
            first_block: i32,
            last_block: i32,
        );

        #[qsignal]
        #[cxx_name = "commandCompleted"]
        fn command_completed(self: Pin<&mut ParchMintBackend>, command: QString, revision: u64);
        #[qsignal]
        #[cxx_name = "operationFailed"]
        fn operation_failed(self: Pin<&mut ParchMintBackend>, message: QString);
    }
}

impl ParchMintBackend {
    fn row(&self, row: i32) -> Option<&parchmint_app::BinderRow> {
        usize::try_from(row)
            .ok()
            .and_then(|index| self.rust().workspace.as_ref()?.snapshot().rows().get(index))
    }

    pub fn node_title(&self, row: i32) -> QString {
        self.row(row)
            .map_or_else(QString::default, |value| QString::from(&value.title))
    }
    pub fn node_synopsis(&self, row: i32) -> QString {
        self.row(row)
            .map_or_else(QString::default, |value| QString::from(&value.synopsis))
    }
    pub fn node_status(&self, row: i32) -> QString {
        self.row(row)
            .map_or_else(QString::default, |value| QString::from(&value.status))
    }
    pub fn node_label(&self, row: i32) -> QString {
        self.row(row)
            .map_or_else(QString::default, |value| QString::from(&value.label))
    }
    pub fn node_id(&self, row: i32) -> QString {
        self.row(row).map_or_else(QString::default, |value| {
            QString::from(value.id.to_string())
        })
    }
    pub fn node_depth(&self, row: i32) -> i32 {
        self.row(row).map_or(0, |value| i32::from(value.depth))
    }
    pub fn node_parent(&self, row: i32) -> i32 {
        let Some(parent) = self.row(row).and_then(|value| value.parent) else {
            return -1;
        };
        self.rust()
            .workspace
            .as_ref()
            .and_then(|workspace| {
                workspace
                    .snapshot()
                    .rows()
                    .iter()
                    .position(|value| value.id == parent)
            })
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
    }
    pub fn node_is_group(&self, row: i32) -> bool {
        self.row(row).is_some_and(|value| value.is_group)
    }
    pub fn node_is_root(&self, row: i32) -> bool {
        self.row(row).is_some_and(|value| value.is_root)
    }

    pub fn create_project(mut self: Pin<&mut Self>, path: &QString, name: &QString) -> bool {
        match ProjectWorkspace::create(path.to_string(), name.to_string()) {
            Ok(workspace) => {
                self.as_mut().install_workspace(workspace);
                true
            }
            Err(error) => self.as_mut().fail(error),
        }
    }
    pub fn open_project(mut self: Pin<&mut Self>, path: &QString) -> bool {
        match ProjectWorkspace::open(path.to_string()) {
            Ok(workspace) => {
                self.as_mut().install_workspace(workspace);
                true
            }
            Err(error) => self.as_mut().fail(error),
        }
    }
    pub fn close_project(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().workspace = None;
        self.as_mut().set_project_open(false);
        self.as_mut().set_project_name(QString::default());
        self.as_mut().rust_mut().node_count = 0;
        self.as_mut().node_count_changed();
        self.as_mut().rust_mut().selected_count = 0;
        self.as_mut().selected_count_changed();
        self.as_mut().set_selected_id(QString::default());
        self.as_mut().set_selected_title(QString::default());
        self.as_mut().set_selected_synopsis(QString::default());
        self.as_mut().set_selected_status(QString::default());
        self.as_mut().set_selected_label(QString::default());
        self.as_mut().set_save_status(QString::from("No project"));
        self.as_mut().set_status(QString::from("Project closed"));
        self.as_mut().bump("Close project");
    }
    pub fn select_node(mut self: Pin<&mut Self>, id: &QString, additive: bool) {
        let parsed = parse_node(id);
        if let (Some(workspace), Some(id)) = (self.as_mut().rust_mut().workspace.as_mut(), parsed) {
            let mut selection = if additive {
                workspace.selected().to_vec()
            } else {
                Vec::new()
            };
            if additive && selection.contains(&id) {
                selection.retain(|value| *value != id);
            } else {
                selection.push(id);
            }
            workspace.select(selection);
        }
        self.as_mut().refresh_projection("Select");
    }
    pub fn set_filter(mut self: Pin<&mut Self>, filter: &QString) {
        self.as_mut().rust_mut().filter = filter.to_string();
        self.as_mut().refresh_projection("Filter outline");
    }
    pub fn set_outline_sort(mut self: Pin<&mut Self>, sort: &QString) {
        self.as_mut().rust_mut().sort = match sort.to_string().as_str() {
            "title" => OutlineSort::Title,
            "status" => OutlineSort::Status,
            _ => OutlineSort::Binder,
        };
        self.as_mut().refresh_projection("Sort outline");
    }
    pub fn create_child(
        mut self: Pin<&mut Self>,
        parent: &QString,
        title: &QString,
        group: bool,
    ) -> bool {
        self.as_mut().perform("Create node", |workspace| {
            workspace
                .create_node(
                    parse_node(parent).ok_or("Select a valid parent")?,
                    title.to_string(),
                    group,
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
    pub fn rename_node(mut self: Pin<&mut Self>, id: &QString, title: &QString) -> bool {
        self.as_mut().perform("Rename node", |workspace| {
            workspace
                .rename(
                    parse_node(id).ok_or("Select a valid node")?,
                    title.to_string(),
                )
                .map_err(|error| error.to_string())
        })
    }
    pub fn edit_summary(mut self: Pin<&mut Self>, id: &QString, summary: &QString) -> bool {
        self.as_mut().edit_metadata_field(
            id,
            |metadata| metadata.summary = summary.to_string(),
            "Edit synopsis",
        )
    }
    pub fn edit_status(mut self: Pin<&mut Self>, id: &QString, status: &QString) -> bool {
        self.as_mut().edit_metadata_field(
            id,
            |metadata| {
                metadata.status =
                    (!status.to_string().trim().is_empty()).then(|| status.to_string())
            },
            "Edit status",
        )
    }
    pub fn edit_label(mut self: Pin<&mut Self>, id: &QString, label: &QString) -> bool {
        self.as_mut().edit_metadata_field(
            id,
            |metadata| {
                metadata.labels = (!label.to_string().trim().is_empty())
                    .then(|| vec![label.to_string()])
                    .unwrap_or_default()
            },
            "Edit label",
        )
    }
    pub fn duplicate_node(mut self: Pin<&mut Self>, id: &QString) -> bool {
        self.as_mut().perform("Duplicate node", |workspace| {
            workspace
                .duplicate(parse_node(id).ok_or("Select a valid node")?)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
    pub fn move_node(
        mut self: Pin<&mut Self>,
        id: &QString,
        target: &QString,
        placement: &QString,
    ) -> bool {
        let target = parse_node(target);
        let placement = match placement.to_string().as_str() {
            "before" => target.map(DropPlacement::Before),
            "after" => target.map(DropPlacement::After),
            "inside" => target.map(DropPlacement::Inside),
            _ => None,
        };
        self.as_mut().perform("Move node", |workspace| {
            workspace
                .drop_node(
                    parse_node(id).ok_or("Select a valid node")?,
                    placement.ok_or("Choose a valid drop target")?,
                )
                .map_err(|error| error.to_string())
        })
    }
    pub fn move_up(mut self: Pin<&mut Self>, id: &QString) -> bool {
        self.as_mut().perform("Move up", |workspace| {
            workspace
                .move_up(parse_node(id).ok_or("Select a valid node")?)
                .map_err(|error| error.to_string())
        })
    }
    pub fn move_down(mut self: Pin<&mut Self>, id: &QString) -> bool {
        self.as_mut().perform("Move down", |workspace| {
            workspace
                .move_down(parse_node(id).ok_or("Select a valid node")?)
                .map_err(|error| error.to_string())
        })
    }
    pub fn indent_node(mut self: Pin<&mut Self>, id: &QString) -> bool {
        self.as_mut().perform("Indent", |workspace| {
            workspace
                .indent(parse_node(id).ok_or("Select a valid node")?)
                .map_err(|error| error.to_string())
        })
    }
    pub fn outdent_node(mut self: Pin<&mut Self>, id: &QString) -> bool {
        self.as_mut().perform("Outdent", |workspace| {
            workspace
                .outdent(parse_node(id).ok_or("Select a valid node")?)
                .map_err(|error| error.to_string())
        })
    }
    pub fn trash_node(mut self: Pin<&mut Self>, id: &QString) -> bool {
        self.as_mut().perform("Move to trash", |workspace| {
            workspace
                .trash(parse_node(id).ok_or("Select a valid node")?)
                .map_err(|error| error.to_string())
        })
    }
    pub fn restore_node(mut self: Pin<&mut Self>, id: &QString) -> bool {
        self.as_mut().perform("Restore", |workspace| {
            workspace
                .restore(parse_node(id).ok_or("Select a valid node")?)
                .map_err(|error| error.to_string())
        })
    }
    pub fn undo_structural(mut self: Pin<&mut Self>) -> bool {
        self.as_mut().perform("Undo", |workspace| {
            workspace
                .undo()
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
    pub fn redo_structural(mut self: Pin<&mut Self>) -> bool {
        self.as_mut().perform("Redo", |workspace| {
            workspace
                .redo()
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }

    pub fn validate_markdown(&self, source: &QString) -> QString {
        parchmint_markdown::Document::parse_body(
            &source.to_string(),
            &parchmint_markdown::ParseOptions::default(),
        )
        .map_or_else(
            |error| QString::from(error.to_string()),
            |document| {
                document
                    .diagnostics()
                    .iter()
                    .find(|item| item.severity == parchmint_markdown::DiagnosticSeverity::Error)
                    .map_or_else(QString::default, |item| QString::from(&item.message))
            },
        )
    }
    pub fn begin_source_mode(mut self: Pin<&mut Self>, source: &QString) -> bool {
        let error = self.validate_markdown(source);
        if !error.is_empty() {
            self.as_mut().operation_failed(error);
            return false;
        }
        self.as_mut().set_source_mode(true);
        true
    }
    pub fn commit_source_mode(mut self: Pin<&mut Self>, source: &QString) -> bool {
        let error = self.validate_markdown(source);
        if !error.is_empty() {
            self.as_mut().operation_failed(error);
            return false;
        }
        let revision = self.document_revision().saturating_add(1);
        self.as_mut().set_document_revision(revision);
        self.as_mut().set_save_status(QString::from("Unsaved"));
        self.as_mut().set_source_mode(false);
        true
    }
    pub fn note_editor_delta(
        mut self: Pin<&mut Self>,
        revision: u64,
        first_block: i32,
        last_block: i32,
    ) {
        if revision <= *self.document_revision() || first_block < 0 || last_block <= first_block {
            return;
        }
        self.as_mut().set_document_revision(revision);
        self.as_mut().set_save_status(QString::from("Unsaved"));
    }

    fn install_workspace(mut self: Pin<&mut Self>, workspace: ProjectWorkspace) {
        let name = QString::from(workspace.project().name.clone());
        self.as_mut().rust_mut().workspace = Some(workspace);
        self.as_mut().set_project_name(name);
        self.as_mut().set_project_open(true);
        self.as_mut().set_save_status(QString::from("Saved"));
        self.as_mut().refresh_projection("Open project");
    }
    fn refresh_projection(mut self: Pin<&mut Self>, command: &str) {
        let (count, selected) = {
            let mut rust = self.as_mut().rust_mut();
            let filter = rust.filter.clone();
            let sort = rust.sort;
            let Some(workspace) = rust.workspace.as_mut() else {
                return;
            };
            workspace.project_snapshot(None, &filter, sort);
            (workspace.snapshot().len(), workspace.selected().len())
        };
        self.as_mut().rust_mut().node_count = i32::try_from(count).unwrap_or(i32::MAX);
        self.as_mut().node_count_changed();
        self.as_mut().rust_mut().selected_count = i32::try_from(selected).unwrap_or(i32::MAX);
        self.as_mut().selected_count_changed();
        self.as_mut().sync_selected();
        self.as_mut().bump(command);
    }
    fn bump(mut self: Pin<&mut Self>, command: &str) {
        let revision = self.revision().saturating_add(1);
        self.as_mut().rust_mut().revision = revision;
        self.as_mut().revision_changed();
        self.as_mut()
            .set_status(QString::from(format!("{command} completed")));
        self.as_mut()
            .command_completed(QString::from(command), revision);
    }
    fn sync_selected(mut self: Pin<&mut Self>) {
        let values = self
            .as_ref()
            .rust()
            .workspace
            .as_ref()
            .and_then(|workspace| {
                (workspace.selected().len() == 1)
                    .then(|| workspace.selected()[0])
                    .and_then(|id| {
                        workspace
                            .snapshot()
                            .rows()
                            .iter()
                            .find(|row| row.id == id)
                            .map(|row| {
                                (
                                    row.id.to_string(),
                                    row.title.clone(),
                                    row.synopsis.clone(),
                                    row.status.clone(),
                                    row.label.clone(),
                                )
                            })
                    })
            });
        let (id, title, synopsis, status, label) = values.unwrap_or_default();
        self.as_mut().set_selected_id(QString::from(id));
        self.as_mut().set_selected_title(QString::from(title));
        self.as_mut().set_selected_synopsis(QString::from(synopsis));
        self.as_mut().set_selected_status(QString::from(status));
        self.as_mut().set_selected_label(QString::from(label));
    }
    fn perform<F>(mut self: Pin<&mut Self>, label: &str, operation: F) -> bool
    where
        F: FnOnce(&mut ProjectWorkspace) -> Result<(), String>,
    {
        let result = self
            .as_mut()
            .rust_mut()
            .workspace
            .as_mut()
            .ok_or_else(|| "Create or open a project first".to_owned())
            .and_then(operation);
        match result {
            Ok(()) => {
                self.as_mut().refresh_projection(label);
                true
            }
            Err(error) => self.as_mut().fail(error),
        }
    }
    fn edit_metadata_field<F>(mut self: Pin<&mut Self>, id: &QString, edit: F, label: &str) -> bool
    where
        F: FnOnce(&mut DocumentMetadata),
    {
        let id = parse_node(id);
        self.as_mut().perform(label, |workspace| {
            let id = id.ok_or("Select a valid node")?;
            let document = workspace
                .project()
                .nodes
                .get(&id)
                .and_then(|node| node.kind.document_id())
                .ok_or("Built-in roots have no metadata")?;
            let mut metadata = workspace
                .project()
                .documents
                .get(&document)
                .ok_or("Document metadata is unavailable")?
                .metadata
                .clone();
            edit(&mut metadata);
            workspace
                .edit_metadata(id, metadata)
                .map_err(|error| error.to_string())
        })
    }
    fn fail(mut self: Pin<&mut Self>, error: impl ToString) -> bool {
        self.as_mut()
            .operation_failed(QString::from(error.to_string()));
        false
    }
}

fn parse_node(value: &QString) -> Option<NodeId> {
    NodeId::parse(&value.to_string()).ok()
}
