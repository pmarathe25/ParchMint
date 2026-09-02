//! Complete-application interaction harness built from the production graph.

use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use parchmint_diagnostics::DiagnosticEvent;
use parchmint_platform_api::ApplicationPaths;
use parchmint_platform_api::UntrustedClipboardContent;
use parchmint_platform_native::testing::NativeFixture;
use parchmint_ui_iced::{
    EditorPane, FocusTarget, HarnessDropPosition, HarnessHierarchyEntry, HarnessHierarchySurface,
    HarnessHistoryCheckpoint, HarnessKey, HarnessNode, HarnessSelectionGesture, HarnessTarget,
    HarnessTraceEntry, HarnessWindow, NativeDesktopError, NativeDesktopHarness,
    NativeDesktopStartup,
};

use super::{
    ProductionControls, ProductionFaultKind, ProductionFaultPoint, ProductionObservation,
    composition::assemble_interaction_harness, native_callbacks::NativeDesktopDriver,
};
use crate::{LaunchRequest, StartupError};

const START_TIMEOUT: Duration = Duration::from_secs(10);

/// A failure while assembling or driving the complete-application harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionHarnessError {
    message: String,
}

impl InteractionHarnessError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for InteractionHarnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for InteractionHarnessError {}

impl From<StartupError> for InteractionHarnessError {
    fn from(error: StartupError) -> Self {
        Self::new(error.to_string())
    }
}

enum HarnessAction {
    HasWindow(HarnessWindow),
    ClickText(HarnessWindow, String),
    ClickTarget(HarnessWindow, HarnessTarget),
    CloseEditorTab(HarnessWindow, EditorPane, String),
    RightClickText(HarnessWindow, String),
    RightClickTarget(HarnessWindow, HarnessTarget),
    RightClickTargetAt(HarnessWindow, HarnessTarget, (f32, f32)),
    TypeInto(HarnessWindow, String, String),
    TypeAt(HarnessWindow, (f32, f32), String),
    TypeIntoTarget(HarnessWindow, HarnessTarget, String),
    TargetIsVisible(HarnessWindow, HarnessTarget),
    EditorTabIsVisible(HarnessWindow, EditorPane, String),
    TargetIsFocused(HarnessWindow, HarnessTarget),
    FocusTarget(HarnessWindow),
    ScrollTargetBy(HarnessWindow, HarnessTarget, f32),
    TypeFocused(HarnessWindow, String),
    PressKey(HarnessWindow, HarnessKey),
    PressShiftKey(HarnessWindow, HarnessKey),
    PressCommandKey(HarnessWindow, char),
    PressCommandShiftKey(HarnessWindow, char),
    ReplaceText(HarnessWindow, String, String),
    ReplaceTarget(HarnessWindow, HarnessTarget, String),
    ReplaceTextAndSubmit(HarnessWindow, String, String),
    DragTextToText(HarnessWindow, String, String),
    DragTextToTextAt(HarnessWindow, String, String, HarnessDropPosition),
    DragWithinTarget(HarnessWindow, HarnessTarget, (f32, f32), (f32, f32)),
    MovePointerToTarget(HarnessWindow, HarnessTarget, (f32, f32)),
    MovePointerOutside(HarnessWindow),
    MovePointerToEditorText(HarnessWindow, EditorPane, String),
    MovePointerToCommentAnchor(HarnessWindow, EditorPane),
    MultiClickEditorText(HarnessWindow, EditorPane, String, u8),
    SelectEditorText(HarnessWindow, EditorPane, String),
    HierarchyNode(String),
    ClickHierarchyNode(HarnessWindow, HarnessNode),
    ClickCardsNode(HarnessWindow, HarnessNode),
    DoubleClickCardsNode(HarnessWindow, HarnessNode),
    SelectHierarchyNode(HarnessWindow, HarnessNode, HarnessSelectionGesture),
    RightClickHierarchyNode(HarnessWindow, HarnessNode),
    HierarchyNodeIsVisible(HarnessWindow, HarnessNode),
    CardsNodeIsVisible(HarnessWindow, HarnessNode),
    ClickHistoryCheckpoint(HarnessWindow, usize),
    ClickHistoryCheckpointById(HarnessWindow, String),
    DragHierarchyNode(
        HarnessWindow,
        HarnessHierarchySurface,
        HarnessNode,
        HarnessNode,
        HarnessDropPosition,
    ),
    DragHierarchyNodeToPane(HarnessWindow, HarnessNode, EditorPane),
    ContainsText(HarnessWindow, String),
    Resize(HarnessWindow, f32, f32),
    Redraw(HarnessWindow),
    ElapseAutosaveIdle,
    ElapseRecoveryCapture,
    AdvanceAutosaveClock(Duration, Duration),
    Close(HarnessWindow),
    ActiveEditorBody,
    ActiveEditorTabTitle,
    ActiveEditorDocumentId(EditorPane),
    EditorPanesShareSession,
    ReplacementStatus,
    GlobalSearchStatus,
    ExportStatus,
    HierarchyTitles,
    TabTitles,
    CommentFeedback,
    CommentHoverStatus,
    HistoryStatus,
    HistoryCheckpoints,
    Hierarchy,
    Snapshot(HarnessWindow, PathBuf),
    Trace,
    Shutdown,
}

enum HarnessValue {
    Unit,
    Bool(bool),
    FocusTarget(FocusTarget),
    Text(String),
    Texts(Vec<String>),
    HistoryCheckpoints(Vec<HarnessHistoryCheckpoint>),
    Hierarchy(Vec<HarnessHierarchyEntry>),
    Node(HarnessNode),
    Trace(Vec<HarnessTraceEntry>),
}

struct HarnessCommand {
    action: HarnessAction,
    reply: mpsc::SyncSender<Result<HarnessValue, String>>,
}

enum HarnessReady {
    Started,
    Failed(String),
}

struct HeadlessDesktopDriver {
    commands: Mutex<Option<mpsc::Receiver<HarnessCommand>>>,
    ready: mpsc::Sender<HarnessReady>,
}

impl NativeDesktopDriver for HeadlessDesktopDriver {
    fn run(&self, startup: NativeDesktopStartup) -> Result<(), NativeDesktopError> {
        let mut harness = match NativeDesktopHarness::boot(startup) {
            Ok(harness) => harness,
            Err(error) => {
                let message = error.to_string();
                let _ = self.ready.send(HarnessReady::Failed(message.clone()));
                return Err(NativeDesktopError::new(message));
            }
        };
        let commands = self
            .commands
            .lock()
            .map_err(|_| NativeDesktopError::new("interaction command receiver is unavailable"))?
            .take()
            .ok_or_else(|| NativeDesktopError::new("interaction driver was already started"))?;
        self.ready
            .send(HarnessReady::Started)
            .map_err(|_| NativeDesktopError::new("interaction harness start was abandoned"))?;

        while let Ok(command) = commands.recv() {
            let shutdown = matches!(command.action, HarnessAction::Shutdown);
            let result = execute_action(&mut harness, command.action);
            let _ = command.reply.send(result);
            if shutdown {
                break;
            }
        }
        Ok(())
    }
}

fn execute_action(
    harness: &mut NativeDesktopHarness,
    action: HarnessAction,
) -> Result<HarnessValue, String> {
    let result = match action {
        HarnessAction::HasWindow(window) => {
            return Ok(HarnessValue::Bool(harness.has_window(window)));
        }
        HarnessAction::ClickText(window, label) => harness.click_text(window, &label),
        HarnessAction::ClickTarget(window, target) => harness.click_target(window, target),
        HarnessAction::CloseEditorTab(window, pane, document_id) => {
            harness.close_editor_tab(window, pane, &document_id)
        }
        HarnessAction::RightClickText(window, label) => harness.right_click_text(window, &label),
        HarnessAction::RightClickTarget(window, target) => {
            harness.right_click_target(window, target)
        }
        HarnessAction::RightClickTargetAt(window, target, position) => {
            harness.right_click_target_at(window, target, position)
        }
        HarnessAction::TypeInto(window, placeholder, value) => {
            harness.type_into(window, &placeholder, &value)
        }
        HarnessAction::TypeAt(window, point, value) => harness.type_at(window, point, &value),
        HarnessAction::TypeIntoTarget(window, target, value) => {
            harness.type_into_target(window, target, &value)
        }
        HarnessAction::TargetIsVisible(window, target) => {
            return harness
                .target_is_visible(window, target)
                .map(HarnessValue::Bool)
                .map_err(|error| error.to_string());
        }
        HarnessAction::TargetIsFocused(window, target) => {
            return harness
                .target_is_focused(window, target)
                .map(HarnessValue::Bool)
                .map_err(|error| error.to_string());
        }
        HarnessAction::FocusTarget(window) => {
            return harness
                .focus_target(window)
                .map(HarnessValue::FocusTarget)
                .map_err(|error| error.to_string());
        }
        HarnessAction::ScrollTargetBy(window, target, delta_y) => {
            harness.scroll_target_by(window, target, delta_y)
        }
        HarnessAction::TypeFocused(window, value) => harness.type_focused(window, &value),
        HarnessAction::PressKey(window, key) => harness.press_key(window, key),
        HarnessAction::PressShiftKey(window, key) => harness.press_shift_key(window, key),
        HarnessAction::PressCommandKey(window, key) => harness.press_command_key(window, key),
        HarnessAction::PressCommandShiftKey(window, key) => {
            harness.press_command_shift_key(window, key)
        }
        HarnessAction::ReplaceText(window, current_value, replacement) => {
            harness.replace_text(window, &current_value, &replacement)
        }
        HarnessAction::ReplaceTarget(window, target, replacement) => {
            harness.replace_target(window, target, &replacement)
        }
        HarnessAction::ReplaceTextAndSubmit(window, current_value, replacement) => {
            harness.replace_text_and_submit(window, &current_value, &replacement)
        }
        HarnessAction::DragTextToText(window, source, destination) => {
            harness.drag_text_to_text(window, &source, &destination)
        }
        HarnessAction::DragTextToTextAt(window, source, destination, position) => {
            harness.drag_text_to_text_at(window, &source, &destination, position)
        }
        HarnessAction::DragWithinTarget(window, target, from, to) => {
            harness.drag_within_target(window, target, from, to)
        }
        HarnessAction::MovePointerToTarget(window, target, position) => {
            harness.move_pointer_to_target(window, target, position)
        }
        HarnessAction::MovePointerOutside(window) => harness.move_pointer_outside(window),
        HarnessAction::MovePointerToEditorText(window, pane, text) => {
            harness.move_pointer_to_editor_text(window, pane, &text)
        }
        HarnessAction::MovePointerToCommentAnchor(window, pane) => {
            harness.move_pointer_to_comment_anchor(window, pane)
        }
        HarnessAction::MultiClickEditorText(window, pane, text, clicks) => {
            harness.multi_click_editor_text(window, pane, &text, clicks)
        }
        HarnessAction::SelectEditorText(window, pane, text) => {
            harness.select_editor_text(window, pane, &text)
        }
        HarnessAction::HierarchyNode(title) => {
            return harness
                .hierarchy_node(&title)
                .map(HarnessValue::Node)
                .map_err(|error| error.to_string());
        }
        HarnessAction::ClickHierarchyNode(window, node) => {
            harness.click_hierarchy_node(window, &node)
        }
        HarnessAction::ClickCardsNode(window, node) => harness.click_cards_node(window, &node),
        HarnessAction::DoubleClickCardsNode(window, node) => {
            harness.double_click_cards_node(window, &node)
        }
        HarnessAction::SelectHierarchyNode(window, node, gesture) => {
            harness.select_hierarchy_node(window, &node, gesture)
        }
        HarnessAction::RightClickHierarchyNode(window, node) => {
            harness.right_click_hierarchy_node(window, &node)
        }
        HarnessAction::HierarchyNodeIsVisible(window, node) => {
            return harness
                .hierarchy_node_is_visible(window, &node)
                .map(HarnessValue::Bool)
                .map_err(|error| error.to_string());
        }
        HarnessAction::CardsNodeIsVisible(window, node) => {
            return harness
                .cards_node_is_visible(window, &node)
                .map(HarnessValue::Bool)
                .map_err(|error| error.to_string());
        }
        HarnessAction::ClickHistoryCheckpoint(window, position) => {
            harness.click_history_checkpoint(window, position)
        }
        HarnessAction::ClickHistoryCheckpointById(window, checkpoint_id) => {
            harness.click_history_checkpoint_by_id(window, &checkpoint_id)
        }
        HarnessAction::DragHierarchyNode(window, surface, source, destination, position) => {
            harness.drag_hierarchy_node(window, surface, &source, &destination, position)
        }
        HarnessAction::DragHierarchyNodeToPane(window, source, pane) => {
            harness.drag_hierarchy_node_to_pane(window, &source, pane)
        }
        HarnessAction::ContainsText(window, text) => {
            return harness
                .contains_text(window, &text)
                .map(HarnessValue::Bool)
                .map_err(|error| error.to_string());
        }
        HarnessAction::EditorTabIsVisible(window, pane, document_id) => {
            return harness
                .editor_tab_is_visible(window, pane, &document_id)
                .map(HarnessValue::Bool)
                .map_err(|error| error.to_string());
        }
        HarnessAction::Resize(window, width, height) => harness.resize(window, width, height),
        HarnessAction::Redraw(window) => harness.redraw(window),
        HarnessAction::ElapseAutosaveIdle => harness.elapse_autosave_idle(),
        HarnessAction::ElapseRecoveryCapture => harness.elapse_recovery_capture(),
        HarnessAction::AdvanceAutosaveClock(first_dirty_age, last_edit_age) => {
            harness.advance_autosave_clock(first_dirty_age, last_edit_age)
        }
        HarnessAction::Close(window) => harness.close(window),
        HarnessAction::ActiveEditorBody => {
            return harness
                .active_editor_body()
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::ActiveEditorTabTitle => {
            return harness
                .active_editor_tab_title()
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::ActiveEditorDocumentId(pane) => {
            return harness
                .active_editor_document_id(pane)
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::EditorPanesShareSession => {
            return harness
                .editor_panes_share_session()
                .map(HarnessValue::Bool)
                .map_err(|error| error.to_string());
        }
        HarnessAction::ReplacementStatus => {
            return harness
                .replacement_status()
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::GlobalSearchStatus => {
            return harness
                .global_search_status()
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::ExportStatus => {
            return harness
                .export_status()
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::HierarchyTitles => {
            return harness
                .hierarchy_titles()
                .map(HarnessValue::Texts)
                .map_err(|error| error.to_string());
        }
        HarnessAction::TabTitles => {
            return harness
                .tab_titles()
                .map(HarnessValue::Texts)
                .map_err(|error| error.to_string());
        }
        HarnessAction::CommentFeedback => {
            return harness
                .comment_feedback()
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::CommentHoverStatus => {
            return harness
                .comment_hover_status()
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::HistoryStatus => {
            return harness
                .history_status()
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::HistoryCheckpoints => {
            return harness
                .history_checkpoints()
                .map(HarnessValue::HistoryCheckpoints)
                .map_err(|error| error.to_string());
        }
        HarnessAction::Hierarchy => {
            return harness
                .hierarchy()
                .map(HarnessValue::Hierarchy)
                .map_err(|error| error.to_string());
        }
        HarnessAction::Snapshot(window, path) => harness.snapshot(window, path),
        HarnessAction::Trace => return Ok(HarnessValue::Trace(harness.trace().to_vec())),
        HarnessAction::Shutdown => return Ok(HarnessValue::Unit),
    };
    result
        .map(|()| HarnessValue::Unit)
        .map_err(|error| error.to_string())
}

/// Controls one headless instance of the real production desktop graph.
///
/// This API is compile-time absent unless the non-default
/// `interaction-harness` feature is enabled.
pub struct DesktopInteractionHarness {
    commands: mpsc::Sender<HarnessCommand>,
    thread: Option<thread::JoinHandle<Result<(), String>>>,
    controls: ProductionControls,
    fixture: NativeFixture,
}

impl DesktopInteractionHarness {
    pub fn launch(
        application_root: impl AsRef<Path>,
        request: LaunchRequest,
    ) -> Result<Self, InteractionHarnessError> {
        let application_root = application_root.as_ref();
        let configuration = application_root.join("configuration");
        let data = application_root.join("data");
        let cache = application_root.join("cache");
        for path in [&configuration, &data, &cache] {
            fs::create_dir_all(path).map_err(|error| {
                InteractionHarnessError::new(format!(
                    "could not create harness directory {}: {error}",
                    path.display()
                ))
            })?;
        }

        let fixture = NativeFixture::with_application_paths(ApplicationPaths::new(
            configuration,
            data,
            cache,
        ));
        let controls = ProductionControls::default();
        let (command_sender, command_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::channel();
        let driver = Arc::new(HeadlessDesktopDriver {
            commands: Mutex::new(Some(command_receiver)),
            ready: ready_sender.clone(),
        });
        let bootstrap = assemble_interaction_harness(controls.clone(), fixture.platform(), driver)?;
        let thread = thread::Builder::new()
            .name("parchmint-interaction-harness".to_owned())
            .spawn(move || {
                let result = bootstrap
                    .run(request)
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                if let Err(error) = &result {
                    let _ = ready_sender.send(HarnessReady::Failed(error.clone()));
                }
                result
            })
            .map_err(|error| {
                InteractionHarnessError::new(format!(
                    "could not start interaction harness thread: {error}"
                ))
            })?;

        match ready_receiver.recv_timeout(START_TIMEOUT) {
            Ok(HarnessReady::Started) => Ok(Self {
                commands: command_sender,
                thread: Some(thread),
                controls,
                fixture,
            }),
            Ok(HarnessReady::Failed(error)) => {
                let _ = thread.join();
                Err(InteractionHarnessError::new(error))
            }
            Err(error) => {
                let _ = thread.join();
                Err(InteractionHarnessError::new(format!(
                    "interaction harness did not start: {error}"
                )))
            }
        }
    }

    pub fn has_window(&self, window: HarnessWindow) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::HasWindow(window))?.into_bool()
    }

    pub fn click_text(
        &self,
        window: HarnessWindow,
        label: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ClickText(window, label.into()))?
            .into_unit()
    }

    pub fn click_target(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ClickTarget(window, target))?
            .into_unit()
    }

    /// Closes a particular author-visible tab by its stable document ID.
    pub fn close_editor_tab(
        &self,
        window: HarnessWindow,
        pane: EditorPane,
        document_id: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::CloseEditorTab(
            window,
            pane,
            document_id.into(),
        ))?
        .into_unit()
    }

    pub fn right_click_text(
        &self,
        window: HarnessWindow,
        label: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::RightClickText(window, label.into()))?
            .into_unit()
    }

    pub fn right_click_target(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::RightClickTarget(window, target))?
            .into_unit()
    }

    pub fn right_click_target_at(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
        position: (f32, f32),
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::RightClickTargetAt(window, target, position))?
            .into_unit()
    }

    pub fn type_into(
        &self,
        window: HarnessWindow,
        placeholder: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::TypeInto(
            window,
            placeholder.into(),
            value.into(),
        ))?
        .into_unit()
    }

    pub fn type_at(
        &self,
        window: HarnessWindow,
        point: (f32, f32),
        value: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::TypeAt(window, point, value.into()))?
            .into_unit()
    }

    pub fn type_into_target(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
        value: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::TypeIntoTarget(window, target, value.into()))?
            .into_unit()
    }

    pub fn type_focused(
        &self,
        window: HarnessWindow,
        value: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::TypeFocused(window, value.into()))?
            .into_unit()
    }

    pub fn press_key(
        &self,
        window: HarnessWindow,
        key: HarnessKey,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::PressKey(window, key))?
            .into_unit()
    }

    /// Sends a Shift-modified named key to the focused control.
    pub fn press_shift_key(
        &self,
        window: HarnessWindow,
        key: HarnessKey,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::PressShiftKey(window, key))?
            .into_unit()
    }

    pub fn press_command_key(
        &self,
        window: HarnessWindow,
        key: char,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::PressCommandKey(window, key))?
            .into_unit()
    }

    pub fn press_command_shift_key(
        &self,
        window: HarnessWindow,
        key: char,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::PressCommandShiftKey(window, key))?
            .into_unit()
    }

    pub fn replace_text(
        &self,
        window: HarnessWindow,
        current_value: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ReplaceText(
            window,
            current_value.into(),
            replacement.into(),
        ))?
        .into_unit()
    }

    pub fn replace_target(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
        replacement: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ReplaceTarget(
            window,
            target,
            replacement.into(),
        ))?
        .into_unit()
    }

    pub fn target_is_focused(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
    ) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::TargetIsFocused(window, target))?
            .into_bool()
    }

    /// Returns the shell's semantic keyboard-focus owner.
    pub fn focus_target(
        &self,
        window: HarnessWindow,
    ) -> Result<FocusTarget, InteractionHarnessError> {
        self.request(HarnessAction::FocusTarget(window))?
            .into_focus_target()
    }

    /// Returns whether a production target is currently rendered.
    pub fn target_is_visible(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
    ) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::TargetIsVisible(window, target))?
            .into_bool()
    }

    /// Returns whether a document is presently rendered in a pane's tab strip.
    pub fn editor_tab_is_visible(
        &self,
        window: HarnessWindow,
        pane: EditorPane,
        document_id: impl Into<String>,
    ) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::EditorTabIsVisible(
            window,
            pane,
            document_id.into(),
        ))?
        .into_bool()
    }

    pub fn replace_text_and_submit(
        &self,
        window: HarnessWindow,
        current_value: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ReplaceTextAndSubmit(
            window,
            current_value.into(),
            replacement.into(),
        ))?
        .into_unit()
    }

    pub fn drag_text_to_text(
        &self,
        window: HarnessWindow,
        source: impl Into<String>,
        destination: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::DragTextToText(
            window,
            source.into(),
            destination.into(),
        ))?
        .into_unit()
    }

    pub fn drag_text_to_text_at(
        &self,
        window: HarnessWindow,
        source: impl Into<String>,
        destination: impl Into<String>,
        position: HarnessDropPosition,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::DragTextToTextAt(
            window,
            source.into(),
            destination.into(),
            position,
        ))?
        .into_unit()
    }

    pub fn drag_within_target(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
        from: (f32, f32),
        to: (f32, f32),
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::DragWithinTarget(window, target, from, to))?
            .into_unit()
    }

    /// Moves the pointer over a stable production target without clicking it.
    pub fn move_pointer_to_target(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
        position: (f32, f32),
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::MovePointerToTarget(window, target, position))?
            .into_unit()
    }

    /// Moves the pointer beyond the current window bounds.
    pub fn move_pointer_outside(
        &self,
        window: HarnessWindow,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::MovePointerOutside(window))?
            .into_unit()
    }

    /// Moves the pointer over one uniquely occurring live prose run.
    pub fn move_pointer_to_editor_text(
        &self,
        window: HarnessWindow,
        pane: EditorPane,
        text: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::MovePointerToEditorText(
            window,
            pane,
            text.into(),
        ))?
        .into_unit()
    }

    /// Moves the pointer to the single visible attached comment anchor.
    pub fn move_pointer_to_comment_anchor(
        &self,
        window: HarnessWindow,
        pane: EditorPane,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::MovePointerToCommentAnchor(window, pane))?
            .into_unit()
    }

    /// Uses real native pointer events to double- or triple-click one unique
    /// prose run in the mounted editor.
    pub fn multi_click_editor_text(
        &self,
        window: HarnessWindow,
        pane: EditorPane,
        text: impl Into<String>,
        clicks: u8,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::MultiClickEditorText(
            window,
            pane,
            text.into(),
            clicks,
        ))?
        .into_unit()
    }

    /// Selects a uniquely occurring prose run by semantic document text,
    /// keeping comment and popover workflows independent of pixel geometry.
    pub fn select_editor_text(
        &self,
        window: HarnessWindow,
        pane: EditorPane,
        text: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::SelectEditorText(window, pane, text.into()))?
            .into_unit()
    }

    pub fn hierarchy_node(
        &self,
        title: impl Into<String>,
    ) -> Result<HarnessNode, InteractionHarnessError> {
        self.request(HarnessAction::HierarchyNode(title.into()))?
            .into_node()
    }

    /// Selects a hierarchy node previously resolved from the live project.
    pub fn click_hierarchy_node(
        &self,
        window: HarnessWindow,
        node: HarnessNode,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ClickHierarchyNode(window, node))?
            .into_unit()
    }

    /// Clicks a mounted Cards row through its rendered, stable node target.
    /// This avoids ambiguous visible titles that may also occur in Explorer.
    pub fn click_cards_node(
        &self,
        window: HarnessWindow,
        node: HarnessNode,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ClickCardsNode(window, node))?
            .into_unit()
    }

    /// Double-clicks a mounted Cards row through its stable node target.
    pub fn double_click_cards_node(
        &self,
        window: HarnessWindow,
        node: HarnessNode,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::DoubleClickCardsNode(window, node))?
            .into_unit()
    }

    /// Applies an explicit selection gesture to a hierarchy node through the
    /// production selection reducer.
    pub fn select_hierarchy_node(
        &self,
        window: HarnessWindow,
        node: HarnessNode,
        gesture: HarnessSelectionGesture,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::SelectHierarchyNode(window, node, gesture))?
            .into_unit()
    }

    /// Opens the context menu for one resolved Explorer row.
    pub fn right_click_hierarchy_node(
        &self,
        window: HarnessWindow,
        node: HarnessNode,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::RightClickHierarchyNode(window, node))?
            .into_unit()
    }

    /// Returns whether one resolved Explorer row is rendered at the moment.
    pub fn hierarchy_node_is_visible(
        &self,
        window: HarnessWindow,
        node: HarnessNode,
    ) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::HierarchyNodeIsVisible(window, node))?
            .into_bool()
    }

    /// Reports whether a resolved hierarchy row is mounted in Cards.
    pub fn cards_node_is_visible(
        &self,
        window: HarnessWindow,
        node: HarnessNode,
    ) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::CardsNodeIsVisible(window, node))?
            .into_bool()
    }

    /// Scrolls a semantic production region using a native wheel event.
    pub fn scroll_target_by(
        &self,
        window: HarnessWindow,
        target: HarnessTarget,
        delta_y: f32,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ScrollTargetBy(window, target, delta_y))?
            .into_unit()
    }

    /// Selects one loaded History row without relying on its repeated label.
    pub fn click_history_checkpoint(
        &self,
        window: HarnessWindow,
        position: usize,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ClickHistoryCheckpoint(window, position))?
            .into_unit()
    }

    /// Selects a loaded History row by stable checkpoint ID rather than its
    /// repeated author-visible label or a virtualized list position.
    pub fn click_history_checkpoint_by_id(
        &self,
        window: HarnessWindow,
        checkpoint_id: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ClickHistoryCheckpointById(
            window,
            checkpoint_id.into(),
        ))?
        .into_unit()
    }

    pub fn drag_hierarchy_node(
        &self,
        window: HarnessWindow,
        surface: HarnessHierarchySurface,
        source: HarnessNode,
        destination: HarnessNode,
        position: HarnessDropPosition,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::DragHierarchyNode(
            window,
            surface,
            source,
            destination,
            position,
        ))?
        .into_unit()
    }

    /// Drags an Explorer document onto a production editor-pane drop target.
    pub fn drag_hierarchy_node_to_pane(
        &self,
        window: HarnessWindow,
        source: HarnessNode,
        pane: EditorPane,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::DragHierarchyNodeToPane(window, source, pane))?
            .into_unit()
    }

    pub fn contains_text(
        &self,
        window: HarnessWindow,
        text: impl Into<String>,
    ) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::ContainsText(window, text.into()))?
            .into_bool()
    }

    /// Resizes the rendered window and dispatches its real resize event.
    pub fn resize(
        &self,
        window: HarnessWindow,
        width: f32,
        height: f32,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::Resize(window, width, height))?
            .into_unit()
    }

    /// Advances the native surface by one Iced render frame.
    pub fn redraw(&self, window: HarnessWindow) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::Redraw(window))?.into_unit()
    }

    pub fn elapse_autosave_idle(&self) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ElapseAutosaveIdle)?.into_unit()
    }

    /// Advances the production recovery cadence without waiting on wall time.
    pub fn elapse_recovery_capture(&self) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ElapseRecoveryCapture)?
            .into_unit()
    }

    /// Advances the production autosave clock without a wall-clock wait.
    pub fn advance_autosave_clock(
        &self,
        first_dirty_age: Duration,
        last_edit_age: Duration,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::AdvanceAutosaveClock(
            first_dirty_age,
            last_edit_age,
        ))?
        .into_unit()
    }

    pub fn close(&self, window: HarnessWindow) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::Close(window))?.into_unit()
    }

    pub fn active_editor_body(&self) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::ActiveEditorBody)?.into_text()
    }

    pub fn active_editor_tab_title(&self) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::ActiveEditorTabTitle)?
            .into_text()
    }

    /// Returns the stable ID of the document active in an authoring pane.
    pub fn active_editor_document_id(
        &self,
        pane: EditorPane,
    ) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::ActiveEditorDocumentId(pane))?
            .into_text()
    }

    /// Reports whether both mounted panes share a live document session.
    pub fn editor_panes_share_session(&self) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::EditorPanesShareSession)?
            .into_bool()
    }

    /// Returns the live state of the project-wide replacement preview.
    pub fn replacement_status(&self) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::ReplacementStatus)?.into_text()
    }

    /// Returns the live project-wide search state for workflow diagnostics.
    pub fn global_search_status(&self) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::GlobalSearchStatus)?.into_text()
    }

    /// Returns the live export state for workflow diagnostics.
    pub fn export_status(&self) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::ExportStatus)?.into_text()
    }

    pub fn hierarchy_titles(&self) -> Result<Vec<String>, InteractionHarnessError> {
        self.request(HarnessAction::HierarchyTitles)?.into_texts()
    }

    pub fn tab_titles(&self) -> Result<Vec<String>, InteractionHarnessError> {
        self.request(HarnessAction::TabTitles)?.into_texts()
    }

    /// Returns the active comment-composer feedback, if any.
    pub fn comment_feedback(&self) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::CommentFeedback)?.into_text()
    }

    /// Returns transient comment-hover state for author-flow diagnostics.
    pub fn comment_hover_status(&self) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::CommentHoverStatus)?.into_text()
    }

    /// Returns the selected checkpoint and comparison state for diagnostics.
    pub fn history_status(&self) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::HistoryStatus)?.into_text()
    }

    /// Returns the authoritative loaded History rows rather than a rendered
    /// window of repeated labels.
    pub fn history_checkpoints(
        &self,
    ) -> Result<Vec<HarnessHistoryCheckpoint>, InteractionHarnessError> {
        self.request(HarnessAction::HistoryCheckpoints)?
            .into_history_checkpoints()
    }

    /// Returns the Explorer projection with stable identity and selection
    /// state for multi-select, cut, and ordering assertions.
    pub fn hierarchy(&self) -> Result<Vec<HarnessHierarchyEntry>, InteractionHarnessError> {
        self.request(HarnessAction::Hierarchy)?.into_hierarchy()
    }

    /// Configures the next deterministic native dialog response.
    pub fn set_next_path_selection(&self, path: impl Into<PathBuf>) {
        self.fixture.set_next_path_selection(path.into());
    }

    /// Seeds the deterministic system clipboard used by production paste.
    pub fn seed_clipboard(&self, plain_text: Option<&str>, html: Option<&str>) {
        let mut content = UntrustedClipboardContent::empty();
        if let Some(plain_text) = plain_text {
            content = content.with_plain_text(plain_text);
        }
        if let Some(html) = html {
            content = content.with_html(html);
        }
        self.fixture.seed_external_clipboard(content);
    }

    /// Reads the deterministic system clipboard written by production copy or
    /// cut commands. HTML is absent because ParchMint's v1 clipboard writer
    /// intentionally publishes plain text only.
    pub fn clipboard_contents(&self) -> (Option<String>, Option<String>) {
        let content = self.fixture.clipboard_contents();
        (
            content.plain_text().map(str::to_owned),
            content.html().map(str::to_owned),
        )
    }

    /// Causes the next specified production boundary operation to fail.
    pub fn fail_next(&self, point: ProductionFaultPoint, kind: ProductionFaultKind) {
        self.controls.fail_next(point, kind);
    }

    pub fn snapshot(
        &self,
        window: HarnessWindow,
        path: impl Into<PathBuf>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::Snapshot(window, path.into()))?
            .into_unit()
    }

    pub fn trace(&self) -> Result<Vec<HarnessTraceEntry>, InteractionHarnessError> {
        self.request(HarnessAction::Trace)?.into_trace()
    }

    pub fn observations(&self) -> Vec<ProductionObservation> {
        self.controls.observations()
    }

    pub fn take_diagnostics(&self) -> Vec<DiagnosticEvent> {
        parchmint_diagnostics::take_captured_events()
    }

    pub fn shutdown(mut self) -> Result<(), InteractionHarnessError> {
        self.stop()
    }

    /// Stops the headless process without dispatching a project close request
    /// or its final-save path. Recovery flows use this to model an abandoned
    /// session; platform-backed tests remain responsible for real OS kills.
    pub fn abandon(mut self) -> Result<(), InteractionHarnessError> {
        self.stop()
    }

    fn request(&self, action: HarnessAction) -> Result<HarnessValue, InteractionHarnessError> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        self.commands
            .send(HarnessCommand {
                action,
                reply: reply_sender,
            })
            .map_err(|_| InteractionHarnessError::new("interaction harness has stopped"))?;
        reply_receiver
            .recv()
            .map_err(|_| InteractionHarnessError::new("interaction harness dropped its reply"))?
            .map_err(InteractionHarnessError::new)
    }

    fn stop(&mut self) -> Result<(), InteractionHarnessError> {
        if self.thread.is_none() {
            return Ok(());
        }
        let _ = self.request(HarnessAction::Shutdown);
        let thread = self.thread.take().expect("thread presence was checked");
        thread
            .join()
            .map_err(|_| InteractionHarnessError::new("interaction harness thread panicked"))?
            .map_err(InteractionHarnessError::new)
    }
}

impl Drop for DesktopInteractionHarness {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl HarnessValue {
    fn into_unit(self) -> Result<(), InteractionHarnessError> {
        match self {
            Self::Unit => Ok(()),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_bool(self) -> Result<bool, InteractionHarnessError> {
        match self {
            Self::Bool(value) => Ok(value),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_focus_target(self) -> Result<FocusTarget, InteractionHarnessError> {
        match self {
            Self::FocusTarget(target) => Ok(target),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_text(self) -> Result<String, InteractionHarnessError> {
        match self {
            Self::Text(value) => Ok(value),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_texts(self) -> Result<Vec<String>, InteractionHarnessError> {
        match self {
            Self::Texts(values) => Ok(values),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_history_checkpoints(
        self,
    ) -> Result<Vec<HarnessHistoryCheckpoint>, InteractionHarnessError> {
        match self {
            Self::HistoryCheckpoints(checkpoints) => Ok(checkpoints),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_hierarchy(self) -> Result<Vec<HarnessHierarchyEntry>, InteractionHarnessError> {
        match self {
            Self::Hierarchy(entries) => Ok(entries),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_node(self) -> Result<HarnessNode, InteractionHarnessError> {
        match self {
            Self::Node(node) => Ok(node),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_trace(self) -> Result<Vec<HarnessTraceEntry>, InteractionHarnessError> {
        match self {
            Self::Trace(value) => Ok(value),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }
}
