#![allow(clippy::struct_excessive_bools, clippy::unused_self)]
//! QObject-facing projection of the Rust project workspace.
//!
//! CXX-Qt invokables deliberately retain `self` and a flat QObject property
//! projection even when a particular getter does not need Rust state.

use self::qobject::ParchMintBackend;
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use parchmint_app::{
    CancellationToken, CollisionPolicy, CommandSpec, CompileExportJob, CompileExportOutput,
    CompileExportWorker, CompileIr, DiagnosticsSnapshot, DropPlacement, ExportFormat,
    ExportOptions, HtmlAssetMode, OutlineSort, PaneView, PreparedExport, ProjectReplacePreview,
    ProjectWorkspace, SearchQuery, SearchResult, SplitOrientation, commit_prepared_export,
    export_diagnostics, matching_commands, prepare_export_bytes, render_html, text_statistics,
};
use parchmint_domain::{
    CompilePreset, DocumentMetadata, NodeId, ProjectGeneration, Revision, WorkStamp,
};
use std::{fs, path::Path};

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
    pane_one_id: QString,
    pane_two_id: QString,
    pane_one_view: QString,
    pane_two_view: QString,
    pane_one_pinned: bool,
    pane_two_pinned: bool,
    focused_pane: i32,
    split_enabled: bool,
    search_result_count: i32,
    search_status: QString,
    export_status: QString,
    export_in_progress: bool,
    command_count: i32,
    replace_count: i32,
    search_results: Vec<SearchResult>,
    command_results: Vec<CommandSpec>,
    command_query: String,
    replace_preview: Option<ProjectReplacePreview>,
    filter: String,
    sort: OutlineSort,
    project_generation: u64,
    export_worker: Option<CompileExportWorker>,
    export_cancellation: Option<CancellationToken>,
    workspace: Option<ProjectWorkspace>,
}

impl Default for ParchMintBackendRust {
    fn default() -> Self {
        let command_results = matching_commands("", false, false);
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
            pane_one_id: QString::default(),
            pane_two_id: QString::default(),
            pane_one_view: QString::from("editor"),
            pane_two_view: QString::from("outline"),
            pane_one_pinned: false,
            pane_two_pinned: false,
            focused_pane: 0,
            split_enabled: false,
            search_result_count: 0,
            search_status: QString::from("Index ready when you search"),
            export_status: QString::from("No export in progress"),
            export_in_progress: false,
            command_count: i32::try_from(command_results.len()).unwrap_or(i32::MAX),
            replace_count: 0,
            search_results: Vec::new(),
            command_results,
            command_query: String::new(),
            replace_preview: None,
            filter: String::new(),
            sort: OutlineSort::Binder,
            project_generation: 1,
            export_worker: None,
            export_cancellation: None,
            workspace: None,
        }
    }
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("pdf_renderer.h");
        type QString = cxx_qt_lib::QString;

        fn parchmint_render_pdf_qt(
            destination: &QString,
            html: &QString,
            width_micrometres: i32,
            height_micrometres: i32,
            margin_left_micrometres: i32,
            margin_top_micrometres: i32,
            margin_right_micrometres: i32,
            margin_bottom_micrometres: i32,
        ) -> bool;
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
        #[qproperty(QString, pane_one_id)]
        #[qproperty(QString, pane_two_id)]
        #[qproperty(QString, pane_one_view)]
        #[qproperty(QString, pane_two_view)]
        #[qproperty(bool, pane_one_pinned)]
        #[qproperty(bool, pane_two_pinned)]
        #[qproperty(i32, focused_pane)]
        #[qproperty(bool, split_enabled)]
        #[qproperty(i32, search_result_count, READ, NOTIFY)]
        #[qproperty(QString, search_status)]
        #[qproperty(QString, export_status)]
        #[qproperty(bool, export_in_progress)]
        #[qproperty(i32, command_count, READ, NOTIFY)]
        #[qproperty(i32, replace_count, READ, NOTIFY)]
        type ParchMintBackend = super::ParchMintBackendRust;

        #[qinvokable]
        #[cxx_name = "filterCommands"]
        fn filter_commands(self: Pin<&mut ParchMintBackend>, query: &QString);
        #[qinvokable]
        #[cxx_name = "commandId"]
        fn command_id(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "commandLabel"]
        fn command_label(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "commandShortcut"]
        fn command_shortcut(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "requestCommand"]
        fn request_command(self: Pin<&mut ParchMintBackend>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "previewProjectReplace"]
        fn preview_project_replace(
            self: Pin<&mut ParchMintBackend>,
            query: &QString,
            replacement: &QString,
            case_sensitive: bool,
        ) -> bool;
        #[qinvokable]
        #[cxx_name = "replaceTitle"]
        fn replace_title(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "replaceContext"]
        fn replace_context(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "replaceSelected"]
        fn replace_selected(self: &ParchMintBackend, row: i32) -> bool;
        #[qinvokable]
        #[cxx_name = "setReplaceSelected"]
        fn set_replace_selected(self: Pin<&mut ParchMintBackend>, row: i32, selected: bool)
        -> bool;
        #[qinvokable]
        #[cxx_name = "applyProjectReplace"]
        fn apply_project_replace(self: Pin<&mut ParchMintBackend>) -> bool;
        #[qinvokable]
        #[cxx_name = "undoProjectReplace"]
        fn undo_project_replace(self: Pin<&mut ParchMintBackend>) -> bool;
        #[qinvokable]
        #[cxx_name = "exportDiagnostics"]
        fn export_diagnostics(self: Pin<&mut ParchMintBackend>, destination: &QString) -> bool;

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
        #[cxx_name = "projectSearch"]
        fn project_search(self: Pin<&mut ParchMintBackend>, query: &QString);
        #[qinvokable]
        #[cxx_name = "searchResultId"]
        fn search_result_id(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "searchResultTitle"]
        fn search_result_title(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "searchResultContext"]
        fn search_result_context(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "searchResultSnippet"]
        fn search_result_snippet(self: &ParchMintBackend, row: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "openSearchResult"]
        fn open_search_result(self: Pin<&mut ParchMintBackend>, row: i32, other_pane: bool)
        -> bool;
        #[qinvokable]
        #[cxx_name = "textStatistics"]
        fn text_statistics(self: &ParchMintBackend, text: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "exportProject"]
        fn export_project(
            self: Pin<&mut ParchMintBackend>,
            format: &QString,
            destination: &QString,
        ) -> bool;
        #[qinvokable]
        #[cxx_name = "exportProjectWithOverwrite"]
        fn export_project_with_overwrite(
            self: Pin<&mut ParchMintBackend>,
            format: &QString,
            destination: &QString,
            overwrite_confirmed: bool,
        ) -> bool;
        #[qinvokable]
        #[cxx_name = "exportDestinationExists"]
        fn export_destination_exists(self: &ParchMintBackend, destination: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "pollExport"]
        fn poll_export(self: Pin<&mut ParchMintBackend>);
        #[qinvokable]
        #[cxx_name = "cancelExport"]
        fn cancel_export(self: Pin<&mut ParchMintBackend>);

        #[qinvokable]
        #[cxx_name = "createProject"]
        fn create_project(self: Pin<&mut ParchMintBackend>, path: &QString, name: &QString)
        -> bool;
        #[qinvokable]
        #[cxx_name = "createSampleProject"]
        fn create_sample_project(self: Pin<&mut ParchMintBackend>, path: &QString) -> bool;
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
        #[cxx_name = "importAttachment"]
        fn import_attachment(
            self: Pin<&mut ParchMintBackend>,
            parent: &QString,
            path: &QString,
        ) -> bool;
        #[qinvokable]
        #[cxx_name = "createResearchChild"]
        fn create_research_child(
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
        #[cxx_name = "openInOtherPane"]
        fn open_in_other_pane(self: Pin<&mut ParchMintBackend>, id: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "swapPanes"]
        fn swap_panes(self: Pin<&mut ParchMintBackend>) -> bool;
        #[qinvokable]
        #[cxx_name = "closePane"]
        fn close_pane(self: Pin<&mut ParchMintBackend>, pane: i32) -> bool;
        #[qinvokable]
        #[cxx_name = "focusNextPane"]
        fn focus_next_pane(self: Pin<&mut ParchMintBackend>);
        #[qinvokable]
        #[cxx_name = "setPanePinned"]
        fn set_pane_pinned(self: Pin<&mut ParchMintBackend>, pane: i32, pinned: bool) -> bool;
        #[qinvokable]
        #[cxx_name = "setSplit"]
        fn set_split(
            self: Pin<&mut ParchMintBackend>,
            enabled: bool,
            orientation: &QString,
            ratio_milli: i32,
        ) -> bool;
        #[qinvokable]
        #[cxx_name = "focusPane"]
        fn focus_pane(self: Pin<&mut ParchMintBackend>, pane: i32);
        #[qinvokable]
        #[cxx_name = "paneDocumentBody"]
        fn pane_document_body(self: &ParchMintBackend, pane: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "savePaneBody"]
        fn save_pane_body(self: Pin<&mut ParchMintBackend>, pane: i32, body: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "paneAttachmentDescription"]
        fn pane_attachment_description(self: &ParchMintBackend, pane: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "paneAttachmentUrl"]
        fn pane_attachment_url(self: &ParchMintBackend, pane: i32) -> QString;

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
        #[qsignal]
        #[cxx_name = "commandRequested"]
        fn command_requested(self: Pin<&mut ParchMintBackend>, id: QString);
    }
}

impl ParchMintBackend {
    pub fn filter_commands(mut self: Pin<&mut Self>, query: &QString) {
        self.as_mut().rust_mut().command_query = query.to_string();
        self.as_mut().refresh_commands();
    }

    fn command_row(&self, row: i32) -> Option<CommandSpec> {
        usize::try_from(row)
            .ok()
            .and_then(|index| self.rust().command_results.get(index).copied())
    }

    pub fn command_id(&self, row: i32) -> QString {
        self.command_row(row)
            .map_or_else(QString::default, |item| QString::from(item.id))
    }

    pub fn command_label(&self, row: i32) -> QString {
        self.command_row(row)
            .map_or_else(QString::default, |item| QString::from(item.label))
    }

    pub fn command_shortcut(&self, row: i32) -> QString {
        self.command_row(row)
            .map_or_else(QString::default, |item| QString::from(item.shortcut))
    }

    pub fn request_command(mut self: Pin<&mut Self>, id: &QString) -> bool {
        let id = id.to_string();
        let valid = matching_commands("", *self.project_open(), !self.selected_id().is_empty())
            .iter()
            .any(|item| item.id == id);
        if !valid {
            return self
                .as_mut()
                .fail("That command is unavailable in the current context");
        }
        self.as_mut().command_requested(QString::from(id));
        true
    }

    pub fn preview_project_replace(
        mut self: Pin<&mut Self>,
        query: &QString,
        replacement: &QString,
        case_sensitive: bool,
    ) -> bool {
        let result = self
            .as_ref()
            .rust()
            .workspace
            .as_ref()
            .ok_or_else(|| "Create or open a project first".to_owned())
            .and_then(|workspace| {
                workspace
                    .preview_project_replace(
                        &query.to_string(),
                        &replacement.to_string(),
                        case_sensitive,
                    )
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(preview) => {
                let count = i32::try_from(preview.matches().len()).unwrap_or(i32::MAX);
                self.as_mut().rust_mut().replace_preview = Some(preview);
                self.as_mut().rust_mut().replace_count = count;
                self.as_mut().replace_count_changed();
                true
            }
            Err(error) => self.as_mut().fail(error),
        }
    }

    fn replace_row(&self, row: i32) -> Option<&parchmint_app::ProjectReplaceMatch> {
        usize::try_from(row)
            .ok()
            .and_then(|index| self.rust().replace_preview.as_ref()?.matches().get(index))
    }

    pub fn replace_title(&self, row: i32) -> QString {
        self.replace_row(row)
            .map_or_else(QString::default, |item| QString::from(&item.title))
    }

    pub fn replace_context(&self, row: i32) -> QString {
        self.replace_row(row)
            .map_or_else(QString::default, |item| QString::from(&item.context))
    }

    pub fn replace_selected(&self, row: i32) -> bool {
        self.replace_row(row).is_some_and(|item| item.selected)
    }

    pub fn set_replace_selected(mut self: Pin<&mut Self>, row: i32, selected: bool) -> bool {
        let Some(index) = usize::try_from(row).ok() else {
            return false;
        };
        self.as_mut()
            .rust_mut()
            .replace_preview
            .as_mut()
            .is_some_and(|preview| preview.set_selected(index, selected))
    }

    pub fn apply_project_replace(mut self: Pin<&mut Self>) -> bool {
        let Some(preview) = self.as_mut().rust_mut().replace_preview.take() else {
            return self
                .as_mut()
                .fail("Preview project changes before applying them");
        };
        let result = self
            .as_mut()
            .rust_mut()
            .workspace
            .as_mut()
            .ok_or_else(|| "Create or open a project first".to_owned())
            .and_then(|workspace| {
                workspace
                    .apply_project_replace(&preview)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(count) => {
                self.as_mut().rust_mut().replace_count = 0;
                self.as_mut().replace_count_changed();
                self.as_mut()
                    .refresh_projection(&format!("Replace {count} project matches"));
                true
            }
            Err(error) => {
                self.as_mut().rust_mut().replace_preview = Some(preview);
                self.as_mut().fail(error)
            }
        }
    }

    pub fn undo_project_replace(mut self: Pin<&mut Self>) -> bool {
        self.as_mut()
            .perform("Undo project replacement", |workspace| {
                workspace
                    .undo_project_replace()
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
    }

    pub fn export_diagnostics(mut self: Pin<&mut Self>, destination: &QString) -> bool {
        let snapshot = self.as_ref().rust().workspace.as_ref().map_or(
            DiagnosticsSnapshot {
                project_open: false,
                node_count: 0,
                workspace_warning: None,
                index_warning: None,
            },
            |workspace| DiagnosticsSnapshot {
                project_open: true,
                node_count: workspace.project().nodes.len(),
                workspace_warning: workspace.workspace_diagnostic().map(str::to_owned),
                index_warning: workspace.index_diagnostic().map(str::to_owned),
            },
        );
        match export_diagnostics(Path::new(&destination.to_string()), &snapshot) {
            Ok(()) => {
                self.as_mut().bump("Export diagnostics");
                true
            }
            Err(error) => self.as_mut().fail(error),
        }
    }

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
    pub fn project_search(mut self: Pin<&mut Self>, query: &QString) {
        let query_text = query.to_string();
        let result = self
            .as_mut()
            .rust_mut()
            .workspace
            .as_mut()
            .ok_or_else(|| "Create or open a project first".to_owned())
            .and_then(|workspace| {
                workspace
                    .search_project(
                        &SearchQuery {
                            text: &query_text,
                            ..SearchQuery::default()
                        },
                        100,
                    )
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(results) => {
                let count = i32::try_from(results.len()).unwrap_or(i32::MAX);
                self.as_mut().rust_mut().search_results = results;
                self.as_mut().rust_mut().search_result_count = count;
                self.as_mut().search_result_count_changed();
                self.as_mut().set_search_status(QString::from(format!(
                    "{count} results — unquoted words match prefixes; quoted text matches a phrase"
                )));
            }
            Err(error) => {
                self.as_mut().rust_mut().search_results.clear();
                self.as_mut().rust_mut().search_result_count = 0;
                self.as_mut().search_result_count_changed();
                self.as_mut().set_search_status(QString::from(error));
            }
        }
    }
    fn search_row(&self, row: i32) -> Option<&SearchResult> {
        usize::try_from(row)
            .ok()
            .and_then(|index| self.rust().search_results.get(index))
    }
    pub fn search_result_id(&self, row: i32) -> QString {
        self.search_row(row)
            .map_or_else(QString::default, |result| QString::from(&result.node_id))
    }
    pub fn search_result_title(&self, row: i32) -> QString {
        self.search_row(row)
            .map_or_else(QString::default, |result| QString::from(&result.title))
    }
    pub fn search_result_context(&self, row: i32) -> QString {
        self.search_row(row)
            .map_or_else(QString::default, |result| {
                QString::from(format!("{} · {}", result.scope, result.path))
            })
    }
    pub fn search_result_snippet(&self, row: i32) -> QString {
        self.search_row(row)
            .map_or_else(QString::default, |result| {
                // Markers originate from SQLite's `snippet`, but QML renders plain
                // text here so source content can never become markup.
                QString::from(result.snippet.replace(['\u{1}', '\u{2}'], ""))
            })
    }
    pub fn open_search_result(mut self: Pin<&mut Self>, row: i32, other_pane: bool) -> bool {
        let node = self
            .search_row(row)
            .and_then(|result| NodeId::parse(&result.node_id).ok());
        self.as_mut().perform("Open search result", |workspace| {
            let node = node.ok_or("Choose a valid search result")?;
            workspace.select([node]);
            if other_pane {
                let other = 1 - usize::from(workspace.preferences().focused_pane.min(1));
                workspace
                    .set_split(
                        true,
                        workspace.preferences().split_orientation,
                        workspace.preferences().split_ratio_milli,
                    )
                    .and_then(|()| workspace.open_node_in_pane(other, node))
            } else {
                workspace.navigate_focused_pane(node).map(|_| ())
            }
            .map_err(|error| error.to_string())
        })
    }
    pub fn text_statistics(&self, text: &QString) -> QString {
        let counts = text_statistics(&text.to_string());
        QString::from(format!(
            "{} words · {} characters",
            counts.words, counts.characters
        ))
    }

    pub fn export_project(
        mut self: Pin<&mut Self>,
        format: &QString,
        destination: &QString,
    ) -> bool {
        self.as_mut()
            .export_project_with_overwrite(format, destination, false)
    }

    pub fn export_destination_exists(&self, destination: &QString) -> bool {
        let destination = destination.to_string();
        !destination.trim().is_empty() && Path::new(destination.trim()).is_file()
    }

    pub fn export_project_with_overwrite(
        mut self: Pin<&mut Self>,
        format: &QString,
        destination: &QString,
        overwrite_confirmed: bool,
    ) -> bool {
        let format = match format.to_string().as_str() {
            "markdown" => ExportFormat::Markdown,
            "plain_text" => ExportFormat::PlainText,
            "html" => ExportFormat::Html,
            "pdf" => ExportFormat::Pdf,
            "epub" => ExportFormat::Epub,
            "docx" => ExportFormat::Docx,
            _ => return self.as_mut().fail("Choose a supported export format"),
        };
        let destination = destination.to_string();
        if destination.trim().is_empty() {
            return self.as_mut().fail("Choose an export destination");
        }
        self.as_mut().cancel_export();
        let stamp = self.as_ref().export_stamp();
        let (input, preset) = match self
            .as_ref()
            .rust()
            .workspace
            .as_ref()
            .ok_or_else(|| "Create or open a project first".to_owned())
            .and_then(|workspace| {
                let preset = workspace.compile_presets().first().map_or_else(
                    || CompilePreset::manuscript("Manuscript"),
                    |preset| (*preset).clone(),
                );
                workspace
                    .compile_input(stamp)
                    .map(|input| (input, preset))
                    .map_err(|error| error.to_string())
            }) {
            Ok(values) => values,
            Err(error) => return self.as_mut().fail(error),
        };
        let cancellation = CancellationToken::default();
        let mut options = ExportOptions::file(format, &destination);
        if Path::new(&destination).exists() {
            if !overwrite_confirmed {
                return self.as_mut().fail("The export destination already exists; explicit overwrite confirmation is required");
            }
            options.collision = CollisionPolicy::ReplaceFile;
        }
        let new_worker = if self.as_ref().rust().export_worker.is_none() {
            match CompileExportWorker::start("parchmint-export") {
                Ok(worker) => Some(worker),
                Err(error) => return self.as_mut().fail(error),
            }
        } else {
            None
        };
        let started: Result<(), String> = {
            let mut rust = self.as_mut().rust_mut();
            if let Some(worker) = new_worker {
                rust.export_worker = Some(worker);
            }
            rust.export_worker
                .as_ref()
                .expect("worker was just initialized")
                .submit(CompileExportJob {
                    stamp,
                    input,
                    preset,
                    options,
                    cancellation: cancellation.clone(),
                    defer_pdf_render_to_ui: format == ExportFormat::Pdf,
                })
                .map_err(|error| error.to_string())
        };
        match started {
            Ok(()) => {
                self.as_mut().rust_mut().export_cancellation = Some(cancellation);
                self.as_mut().set_export_in_progress(true);
                self.as_mut()
                    .set_export_status(QString::from("Compiling on background worker…"));
                true
            }
            Err(error) => self.as_mut().fail(error),
        }
    }

    pub fn poll_export(mut self: Pin<&mut Self>) {
        let completion = self
            .as_mut()
            .rust_mut()
            .export_worker
            .as_ref()
            .map(CompileExportWorker::try_result);
        let Some(completion) = completion else {
            return;
        };
        match completion {
            Ok(Some(completion)) => {
                let cancellation = self
                    .as_mut()
                    .rust_mut()
                    .export_cancellation
                    .take()
                    .unwrap_or_default();
                self.as_mut().set_export_in_progress(false);
                if cancellation.is_cancelled() || completion.stamp != self.as_ref().export_stamp() {
                    self.as_mut()
                        .set_export_status(QString::from("Discarded stale export completion"));
                    return;
                }
                let prepared = match completion.outcome {
                    Ok(CompileExportOutput::Prepared(prepared)) => Ok(prepared),
                    Ok(CompileExportOutput::Compiled { ir, options }) => {
                        self.as_ref().render_qt_pdf(&ir, &options, &cancellation)
                    }
                    Err(error) => Err(error.to_string()),
                };
                match prepared {
                    Ok(prepared) => match commit_prepared_export(prepared, &cancellation) {
                        Ok(report) => self.as_mut().set_export_status(QString::from(format!(
                            "Exported {} bytes to {}{}",
                            report.bytes,
                            report.destination.display(),
                            if report.warnings.is_empty() {
                                ""
                            } else {
                                " (with warnings)"
                            }
                        ))),
                        Err(error) => {
                            self.as_mut()
                                .set_export_status(QString::from(error.to_string()));
                            self.as_mut()
                                .operation_failed(QString::from(error.to_string()));
                        }
                    },
                    Err(error) => {
                        self.as_mut()
                            .set_export_status(QString::from(error.clone()));
                        self.as_mut().operation_failed(QString::from(error));
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.as_mut().set_export_in_progress(false);
                self.as_mut()
                    .set_export_status(QString::from(error.to_string()));
            }
        }
    }

    pub fn cancel_export(mut self: Pin<&mut Self>) {
        if let Some(token) = self.as_mut().rust_mut().export_cancellation.take() {
            token.cancel();
            self.as_mut()
                .set_export_status(QString::from("Export cancellation requested"));
        }
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
    pub fn create_sample_project(mut self: Pin<&mut Self>, path: &QString) -> bool {
        let result = ProjectWorkspace::create(path.to_string(), "ParchMint Tour").and_then(
            |mut workspace| {
                let manuscript = workspace.project().manuscript_root();
                let research = workspace.project().research_root();
                let chapter = workspace.create_node(manuscript, "Chapter One", true)?;
                let scene = workspace.create_node(chapter, "A Place to Begin", false)?;
                workspace.save_document_body(
                    scene,
                    "Welcome to **ParchMint**. Use the binder to plan, then write here.\n\n<!-- parchmint:page-break -->\n".into(),
                )?;
                let note = workspace.create_research_node(research, "Tour Notes", false)?;
                workspace.save_document_body(
                    note,
                    "Keep research visible in the second pane while you write.\n".into(),
                )?;
                workspace.open_in_pane(0, Some(scene), PaneView::Editor)?;
                workspace.open_in_pane(1, Some(note), PaneView::Editor)?;
                workspace.set_split(true, SplitOrientation::Horizontal, 600)?;
                Ok(workspace)
            },
        );
        match result {
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
        self.as_mut().cancel_export();
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
        self.as_mut().set_pane_one_id(QString::default());
        self.as_mut().set_pane_two_id(QString::default());
        self.as_mut().set_split_enabled(false);
        self.as_mut().set_save_status(QString::from("No project"));
        self.as_mut().set_status(QString::from("Project closed"));
        self.as_mut().rust_mut().replace_preview = None;
        self.as_mut().rust_mut().replace_count = 0;
        self.as_mut().replace_count_changed();
        self.as_mut().refresh_commands();
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
            if !additive {
                let _ = workspace.navigate_focused_pane(id);
            }
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
    pub fn import_attachment(mut self: Pin<&mut Self>, parent: &QString, path: &QString) -> bool {
        let parent = parse_node(parent);
        let path = path.to_string();
        self.as_mut().perform("Import attachment", |workspace| {
            workspace
                .import_attachment(parent.ok_or("Select a research group")?, path)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
    pub fn create_research_child(
        mut self: Pin<&mut Self>,
        parent: &QString,
        title: &QString,
        group: bool,
    ) -> bool {
        self.as_mut().perform("Create research node", |workspace| {
            workspace
                .create_research_node(
                    parse_node(parent).ok_or("Select a research group")?,
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
                let status = status.to_string();
                metadata.status = (!status.trim().is_empty()).then_some(status);
            },
            "Edit status",
        )
    }
    pub fn edit_label(mut self: Pin<&mut Self>, id: &QString, label: &QString) -> bool {
        self.as_mut().edit_metadata_field(
            id,
            |metadata| {
                let label = label.to_string();
                metadata.labels = if label.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![label]
                };
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
    pub fn open_in_other_pane(mut self: Pin<&mut Self>, id: &QString) -> bool {
        let id = parse_node(id);
        self.as_mut().perform("Open in other pane", |workspace| {
            let node = id.ok_or("Select a valid node")?;
            let other = 1 - usize::from(workspace.preferences().focused_pane.min(1));
            workspace
                .set_split(
                    true,
                    workspace.preferences().split_orientation,
                    workspace.preferences().split_ratio_milli,
                )
                .and_then(|()| workspace.open_node_in_pane(other, node))
                .map_err(|error| error.to_string())
        })
    }
    pub fn swap_panes(mut self: Pin<&mut Self>) -> bool {
        self.as_mut().perform("Swap panes", |workspace| {
            workspace.swap_panes().map_err(|error| error.to_string())
        })
    }
    pub fn close_pane(mut self: Pin<&mut Self>, pane: i32) -> bool {
        self.as_mut().perform("Close pane", |workspace| {
            usize::try_from(pane)
                .map_err(|_| "Choose a valid pane".to_owned())
                .and_then(|pane| {
                    workspace
                        .close_pane(pane)
                        .map_err(|error| error.to_string())
                })
        })
    }
    pub fn focus_next_pane(mut self: Pin<&mut Self>) {
        if let Some(workspace) = self.as_mut().rust_mut().workspace.as_mut() {
            workspace.focus_next_pane();
        }
        self.as_mut().refresh_projection("Focus next pane");
    }
    pub fn set_pane_pinned(mut self: Pin<&mut Self>, pane: i32, pinned: bool) -> bool {
        self.as_mut().perform("Pin pane", |workspace| {
            usize::try_from(pane)
                .map_err(|_| "Choose a valid pane".to_owned())
                .and_then(|pane| {
                    workspace
                        .set_pane_pin(pane, pinned)
                        .map_err(|error| error.to_string())
                })
        })
    }
    pub fn set_split(
        mut self: Pin<&mut Self>,
        enabled: bool,
        orientation: &QString,
        ratio_milli: i32,
    ) -> bool {
        let orientation = if orientation.to_string() == "vertical" {
            SplitOrientation::Vertical
        } else {
            SplitOrientation::Horizontal
        };
        self.as_mut().perform("Set split", |workspace| {
            workspace
                .set_split(
                    enabled,
                    orientation,
                    u16::try_from(ratio_milli).unwrap_or(500),
                )
                .map_err(|error| error.to_string())
        })
    }
    pub fn focus_pane(mut self: Pin<&mut Self>, pane: i32) {
        if let (Some(workspace), Ok(pane)) = (
            self.as_mut().rust_mut().workspace.as_mut(),
            usize::try_from(pane),
        ) {
            let _ = workspace.focus_pane(pane);
        }
        self.as_mut().refresh_projection("Focus pane");
    }
    pub fn pane_document_body(&self, pane: i32) -> QString {
        usize::try_from(pane)
            .ok()
            .and_then(|pane| {
                self.rust()
                    .workspace
                    .as_ref()?
                    .pane_document_body(pane)
                    .ok()
            })
            .map_or_else(QString::default, QString::from)
    }
    pub fn save_pane_body(mut self: Pin<&mut Self>, pane: i32, body: &QString) -> bool {
        let body = body.to_string();
        self.as_mut().perform("Save document", |workspace| {
            usize::try_from(pane)
                .map_err(|_| "Choose a valid pane".to_owned())
                .and_then(|pane| {
                    workspace
                        .save_pane_document_body(pane, body)
                        .map_err(|error| error.to_string())
                })
        })
    }
    pub fn pane_attachment_description(&self, pane: i32) -> QString {
        let description = usize::try_from(pane)
            .ok()
            .and_then(|pane| self.rust().workspace.as_ref()?.pane_attachment(pane).ok())
            .map(|attachment| {
                format!(
                    "{} · {} · {} bytes",
                    attachment.display_name, attachment.media_type, attachment.bytes
                )
            });
        description.map_or_else(QString::default, QString::from)
    }
    pub fn pane_attachment_url(&self, pane: i32) -> QString {
        let path = usize::try_from(pane).ok().and_then(|pane| {
            let workspace = self.rust().workspace.as_ref()?;
            let attachment = workspace.pane_attachment(pane).ok()?;
            workspace
                .attachment_preview(attachment.id)
                .ok()
                .map(|(path, _)| path)
        });
        path.map(|path| local_file_url(&path))
            .map_or_else(QString::default, QString::from)
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
        self.as_mut().cancel_export();
        self.as_mut().set_document_revision(revision);
        self.as_mut().set_save_status(QString::from("Unsaved"));
    }

    fn install_workspace(mut self: Pin<&mut Self>, workspace: ProjectWorkspace) {
        let name = QString::from(workspace.project().name.clone());
        let generation = self
            .as_ref()
            .rust()
            .project_generation
            .saturating_add(1)
            .max(1);
        self.as_mut().rust_mut().project_generation = generation;
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
        self.as_mut().sync_panes();
        self.as_mut().refresh_commands();
        self.as_mut().bump(command);
    }
    fn refresh_commands(mut self: Pin<&mut Self>) {
        let project_open = *self.project_open();
        let has_selection = !self.selected_id().is_empty();
        let query = self.as_ref().rust().command_query.clone();
        let results = matching_commands(&query, project_open, has_selection);
        let count = i32::try_from(results.len()).unwrap_or(i32::MAX);
        self.as_mut().rust_mut().command_results = results;
        self.as_mut().rust_mut().command_count = count;
        self.as_mut().command_count_changed();
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
    fn sync_panes(mut self: Pin<&mut Self>) {
        let values = self.as_ref().rust().workspace.as_ref().map(|workspace| {
            let value = |index| workspace.pane(index).cloned().unwrap_or_default();
            (value(0), value(1), workspace.preferences().focused_pane)
        });
        let (first, second, focused) = values.unwrap_or_default();
        self.as_mut().set_pane_one_id(QString::from(
            first.node.map_or_else(String::new, |id| id.to_string()),
        ));
        self.as_mut().set_pane_two_id(QString::from(
            second.node.map_or_else(String::new, |id| id.to_string()),
        ));
        self.as_mut()
            .set_pane_one_view(QString::from(pane_view_name(first.view)));
        self.as_mut()
            .set_pane_two_view(QString::from(pane_view_name(second.view)));
        self.as_mut().set_pane_one_pinned(first.pinned);
        self.as_mut().set_pane_two_pinned(second.pinned);
        self.as_mut().set_focused_pane(i32::from(focused));
        let split = self
            .as_ref()
            .rust()
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.preferences().split_enabled);
        self.as_mut().set_split_enabled(split);
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
    #[allow(clippy::needless_pass_by_value)]
    fn fail(mut self: Pin<&mut Self>, error: impl ToString) -> bool {
        self.as_mut()
            .operation_failed(QString::from(error.to_string()));
        false
    }
    fn export_stamp(&self) -> WorkStamp {
        WorkStamp {
            generation: ProjectGeneration::new(self.rust().project_generation.max(1))
                .expect("non-zero backend generation"),
            revision: Revision::new(*self.revision().max(self.document_revision())),
        }
    }

    /// Qt owns the PDF painter/document boundary. Rust has already produced a
    /// frozen semantic IR on the worker; this method renders it into a
    /// destination-adjacent temporary PDF and returns to the common commit
    /// transaction without touching the requested path.
    fn render_qt_pdf(
        &self,
        ir: &CompileIr,
        options: &ExportOptions,
        cancellation: &CancellationToken,
    ) -> Result<PreparedExport, String> {
        if cancellation.is_cancelled() {
            return Err("export cancelled before Qt PDF render".into());
        }
        let parent = options
            .destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| "PDF destination has no parent directory".to_owned())?;
        let artifact = tempfile::Builder::new()
            .prefix(".parchmint-qt-pdf-")
            .suffix(".pdf")
            .tempfile_in(parent)
            .map_err(|error| error.to_string())?;
        let (html, warnings) = render_html(ir, HtmlAssetMode::SelfContained);
        let destination = QString::from(artifact.path().to_string_lossy().into_owned());
        let html = QString::from(html);
        let page = &ir.page;
        let measure = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
        // The QPdfWriter/QTextDocument pair must be created by the Qt owner.
        let rendered = qobject::parchmint_render_pdf_qt(
            &destination,
            &html,
            measure(page.width_micrometres),
            measure(page.height_micrometres),
            measure(page.margin_left_micrometres),
            measure(page.margin_top_micrometres),
            measure(page.margin_right_micrometres),
            measure(page.margin_bottom_micrometres),
        );
        if !rendered {
            return Err("Qt PDF renderer could not create the temporary artifact".into());
        }
        if cancellation.is_cancelled() {
            return Err("export cancelled before Qt PDF validation".into());
        }
        let bytes = fs::read(artifact.path()).map_err(|error| error.to_string())?;
        prepare_export_bytes(options, &bytes, warnings, cancellation)
            .map_err(|error| error.to_string())
    }
}

fn parse_node(value: &QString) -> Option<NodeId> {
    NodeId::parse(&value.to_string()).ok()
}

fn pane_view_name(view: PaneView) -> &'static str {
    match view {
        PaneView::Editor => "editor",
        PaneView::Attachment => "attachment",
        PaneView::Outline => "outline",
        PaneView::Cards => "cards",
    }
}

fn local_file_url(path: &Path) -> String {
    let portable = path.to_string_lossy().replace('\\', "/");
    let encoded = portable
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':')
            {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect::<String>();
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}
