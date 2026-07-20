pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import QtQuick.Window
import org.parchmint.app 1.0
import org.parchmint.adapters 1.0

ApplicationWindow {
    id: window
    width: 1320
    height: 840
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    title: backend.project_open ? qsTr("%1 — ParchMint").arg(backend.project_name) : qsTr("ParchMint")
    Material.accent: DesignTokens.accent
    Material.primary: DesignTokens.accent

    property string transientMessage: ""
    property bool binderVisible: true
    property bool inspectorVisible: true

    ParchMintBackend {
        id: backend
        onCommandCompleted: function(command, revision) {
            window.transientMessage = qsTr("%1 at revision %2").arg(command).arg(revision)
        }
        onOperationFailed: function(message) {
            window.transientMessage = message
            errorPopup.open()
        }
    }

    OutlineModel {
        id: outlineModel
        source: backend
        onModelError: function(message) {
            window.transientMessage = message
            errorPopup.open()
        }
    }

    Popup {
        id: errorPopup
        anchors.centerIn: Overlay.overlay
        modal: true
        focus: true
        padding: DesignTokens.space4
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        contentItem: ColumnLayout {
            spacing: DesignTokens.space3
            Label { text: qsTr("ParchMint could not complete the operation"); font.bold: true }
            Label { text: window.transientMessage; wrapMode: Text.Wrap; Layout.preferredWidth: 440 }
            Button { text: qsTr("Close"); onClicked: errorPopup.close(); Layout.alignment: Qt.AlignRight }
        }
    }

    Dialog {
        id: projectDialog
        title: qsTr("Create project")
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Ok | Dialog.Cancel
        property bool openExisting: false
        onAccepted: {
            if (openExisting)
                backend.openProject(pathField.text)
            else
                backend.createProject(pathField.text, nameField.text)
        }
        contentItem: GridLayout {
            columns: 2
            rowSpacing: DesignTokens.space3
            columnSpacing: DesignTokens.space3
            Label { text: qsTr("Project folder") }
            TextField { id: pathField; Layout.preferredWidth: 440; placeholderText: qsTr("/path/to/My Novel") }
            Label { visible: !projectDialog.openExisting; text: qsTr("Project name") }
            TextField { id: nameField; visible: !projectDialog.openExisting; text: qsTr("Untitled Novel"); Layout.fillWidth: true }
        }
    }

    menuBar: MenuBar {
        Menu {
            title: qsTr("Project")
            Action { text: qsTr("New Project…"); onTriggered: { projectDialog.openExisting = false; projectDialog.open() } }
            Action { text: qsTr("Open Project…"); onTriggered: { projectDialog.openExisting = true; projectDialog.open() } }
            Action { text: qsTr("Close Project"); enabled: backend.project_open; onTriggered: backend.closeProject() }
        }
        Menu {
            title: qsTr("Structure")
            Action { text: qsTr("New Group"); enabled: backend.selected_id.length > 0; onTriggered: backend.createChild(backend.selected_id, qsTr("Untitled Group"), true) }
            Action { text: qsTr("New Scene"); enabled: backend.selected_id.length > 0; onTriggered: backend.createChild(backend.selected_id, qsTr("Untitled Scene"), false) }
            MenuSeparator {}
            Action { text: qsTr("Move Up"); enabled: backend.selected_id.length > 0; onTriggered: backend.moveUp(backend.selected_id) }
            Action { text: qsTr("Move Down"); enabled: backend.selected_id.length > 0; onTriggered: backend.moveDown(backend.selected_id) }
            Action { text: qsTr("Indent"); enabled: backend.selected_id.length > 0; onTriggered: backend.indentNode(backend.selected_id) }
            Action { text: qsTr("Outdent"); enabled: backend.selected_id.length > 0; onTriggered: backend.outdentNode(backend.selected_id) }
            Action { text: qsTr("Duplicate"); enabled: backend.selected_id.length > 0; onTriggered: backend.duplicateNode(backend.selected_id) }
            Action { text: qsTr("Move to Trash"); enabled: backend.selected_id.length > 0; onTriggered: backend.trashNode(backend.selected_id) }
        }
        Menu {
            title: qsTr("View")
            Action { text: qsTr("Binder"); checkable: true; checked: window.binderVisible; onTriggered: window.binderVisible = !window.binderVisible }
            Action { text: qsTr("Inspector"); checkable: true; checked: window.inspectorVisible; onTriggered: window.inspectorVisible = !window.inspectorVisible }
        }
    }

    Shortcut { sequence: StandardKey.Undo; enabled: backend.project_open; onActivated: backend.undoStructural() }
    Shortcut { sequence: StandardKey.Redo; enabled: backend.project_open; onActivated: backend.redoStructural() }
    Shortcut { sequence: "Ctrl+Shift+Up"; enabled: backend.selected_id.length > 0; onActivated: backend.moveUp(backend.selected_id) }
    Shortcut { sequence: "Ctrl+Shift+Down"; enabled: backend.selected_id.length > 0; onActivated: backend.moveDown(backend.selected_id) }
    Shortcut { sequence: "Ctrl+]"; enabled: backend.selected_id.length > 0; onActivated: backend.indentNode(backend.selected_id) }
    Shortcut { sequence: "Ctrl+["; enabled: backend.selected_id.length > 0; onActivated: backend.outdentNode(backend.selected_id) }
    Shortcut { sequence: StandardKey.Delete; enabled: backend.selected_id.length > 0; onActivated: backend.trashNode(backend.selected_id) }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: DesignTokens.space2
            ToolButton { text: "☰"; Accessible.name: qsTr("Toggle binder"); onClicked: window.binderVisible = !window.binderVisible }
            ToolButton { text: "ⓘ"; Accessible.name: qsTr("Toggle inspector"); onClicked: window.inspectorVisible = !window.inspectorVisible }
            TextField {
                Layout.preferredWidth: 260
                placeholderText: qsTr("Filter title, synopsis, status, label")
                enabled: backend.project_open
                onTextChanged: backend.setFilter(text)
                Accessible.name: qsTr("Filter outline")
            }
            Item { Layout.fillWidth: true }
            Label { text: backend.project_open ? backend.project_name : qsTr("No project open"); font.bold: true }
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0
        BinderPane { Layout.preferredWidth: 276; Layout.fillHeight: true; visible: window.binderVisible; backend: backend; model: outlineModel }
        Rectangle { Layout.preferredWidth: window.binderVisible ? 1 : 0; Layout.fillHeight: true; color: window.palette.mid; opacity: .35 }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0
            TabBar {
                id: views
                Layout.fillWidth: true
                enabled: backend.project_open
                TabButton { text: qsTr("Outline") }
                TabButton { text: qsTr("Cards") }
                TabButton { text: qsTr("Editor") }
            }
            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: views.currentIndex
                OutlineView { backend: backend; model: outlineModel }
                CardsView { backend: backend; model: outlineModel }
                Pane {
                    padding: DesignTokens.space6
                    ColumnLayout {
                        anchors.fill: parent
                        Label { text: backend.selected_title.length ? backend.selected_title : qsTr("Select a manuscript document"); font.pixelSize: 24; font.bold: true }
                        Label { text: qsTr("Document bodies remain owned by the Rust document session."); opacity: .7 }
                        TextArea { Layout.fillWidth: true; Layout.fillHeight: true; readOnly: true; text: backend.selected_id.length ? qsTr("Open the selected document from the binder to begin writing.") : ""; placeholderText: qsTr("Select a scene in the binder.") }
                    }
                }
            }
        }

        Rectangle { Layout.preferredWidth: window.inspectorVisible ? 1 : 0; Layout.fillHeight: true; color: window.palette.mid; opacity: .35 }
        InspectorPane { Layout.preferredWidth: 310; Layout.fillHeight: true; visible: window.inspectorVisible; backend: backend }
    }

    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: DesignTokens.space3
            anchors.rightMargin: DesignTokens.space3
            Label { text: window.transientMessage.length ? window.transientMessage : qsTr("Local-first · structural changes are saved canonically"); Layout.fillWidth: true; elide: Text.ElideRight }
            Label { text: qsTr("%1 visible · %2 selected").arg(backend.node_count).arg(backend.selected_count) }
        }
    }
}
