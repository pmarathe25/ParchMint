use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use parchmint_app::LazyTreeSnapshot;

/// Rust-owned state exposed through a generated QObject.
pub struct ParchMintBackendRust {
    status: QString,
    node_count: i32,
    revision: u64,
    outline: LazyTreeSnapshot,
}

impl Default for ParchMintBackendRust {
    fn default() -> Self {
        let outline = LazyTreeSnapshot::stress_fixture(10_000);
        Self {
            status: QString::from("Ready"),
            node_count: i32::try_from(outline.len()).expect("spike fixture fits i32"),
            revision: 0,
            outline,
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
        type ParchMintBackend = super::ParchMintBackendRust;

        #[qinvokable]
        #[cxx_name = "nodeTitle"]
        fn node_title(self: &ParchMintBackend, row: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "nodeDepth"]
        fn node_depth(self: &ParchMintBackend, row: i32) -> i32;

        #[qinvokable]
        #[cxx_name = "nodeParent"]
        fn node_parent(self: &ParchMintBackend, row: i32) -> i32;

        #[qinvokable]
        #[cxx_name = "performCommand"]
        fn perform_command(self: Pin<&mut ParchMintBackend>, command: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "demonstrateError"]
        fn demonstrate_error(self: Pin<&mut ParchMintBackend>);

        #[qsignal]
        #[cxx_name = "commandCompleted"]
        fn command_completed(self: Pin<&mut ParchMintBackend>, command: QString, revision: u64);

        #[qsignal]
        #[cxx_name = "operationFailed"]
        fn operation_failed(self: Pin<&mut ParchMintBackend>, message: QString);
    }
}

impl qobject::ParchMintBackend {
    /// Lazily returns one visible row title from the Rust snapshot.
    pub fn node_title(&self, row: i32) -> QString {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.rust().outline.visible_rows(row, 1).first())
            .map_or_else(QString::default, |node| QString::from(&node.title))
    }

    /// Returns the cached outline depth without materializing a Qt row object.
    pub fn node_depth(&self, row: i32) -> i32 {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.rust().outline.visible_rows(row, 1).first())
            .map_or(0, |node| i32::from(node.depth))
    }

    /// Returns a stable parent row identifier, or -1 for a root.
    pub fn node_parent(&self, row: i32) -> i32 {
        usize::try_from(row)
            .ok()
            .and_then(|row| self.rust().outline.visible_rows(row, 1).first())
            .and_then(|node| node.parent)
            .and_then(|parent| i32::try_from(parent).ok())
            .unwrap_or(-1)
    }

    /// Demonstrates a typed invokable, property mutation, and completion signal.
    pub fn perform_command(mut self: Pin<&mut Self>, command: &QString) -> bool {
        if command.to_string().trim().is_empty() {
            self.as_mut()
                .operation_failed(QString::from("Command must not be empty"));
            return false;
        }
        let revision = self.revision().saturating_add(1);
        self.as_mut().rust_mut().revision = revision;
        self.as_mut().revision_changed();
        self.as_mut()
            .set_status(QString::from(format!("Completed: {command}")));
        self.as_mut().command_completed(command.clone(), revision);
        true
    }

    /// Demonstrates user-displayable Rust error propagation into QML.
    pub fn demonstrate_error(mut self: Pin<&mut Self>) {
        self.as_mut().operation_failed(QString::from(
            "Demonstration error: canonical files were not changed",
        ));
    }
}
