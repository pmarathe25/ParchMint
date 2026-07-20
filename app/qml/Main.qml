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
    width: 1280
    height: 800
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    title: qsTr("ParchMint")
    Material.accent: DesignTokens.accent
    Material.primary: DesignTokens.accent

    property string transientMessage: ""

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
            Label {
                text: qsTr("ParchMint could not complete the operation")
                font.bold: true
            }
            Label {
                text: window.transientMessage
                wrapMode: Text.Wrap
                Layout.preferredWidth: 420
            }
            Button {
                text: qsTr("Close")
                onClicked: errorPopup.close()
                Layout.alignment: Qt.AlignRight
            }
        }
    }

    header: FormattingBar {
        adapter: primaryAdapter
        styleModel: [
            { "id": "018f0be2-a8ea-7d2d-89ea-45aa663708d4", "name": qsTr("Body"), "previewSize": 14 },
            { "id": "018f0be2-a8ea-7d2d-89ea-45aa663708d5", "name": qsTr("Heading"), "previewSize": 18 }
        ]
        onSourceModeRequested: window.transientMessage = qsTr("Raw source mode is available in the isolated document harness")
    }

    Shortcut { sequence: StandardKey.Bold; onActivated: primaryAdapter.toggleBold() }
    Shortcut { sequence: StandardKey.Italic; onActivated: primaryAdapter.toggleItalic() }
    Shortcut { sequence: StandardKey.Undo; enabled: primaryAdapter.canUndo; onActivated: primaryAdapter.undo() }
    Shortcut { sequence: StandardKey.Redo; enabled: primaryAdapter.canRedo; onActivated: primaryAdapter.redo() }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Pane {
            Layout.preferredWidth: 248
            Layout.fillHeight: true
            padding: 0
            background: Rectangle {
                color: window.palette.alternateBase
            }
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                Label {
                    text: qsTr("MANUSCRIPT — %1 NODES").arg(backend.node_count)
                    font.bold: true
                    font.pixelSize: 11
                    opacity: 0.72
                    Layout.fillWidth: true
                    leftPadding: DesignTokens.space4
                    topPadding: DesignTokens.space4
                    bottomPadding: DesignTokens.space2
                }
                ListView {
                    id: binder
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: outlineModel
                    reuseItems: true
                    delegate: ItemDelegate {
                        required property int index
                        required property string title
                        required property int depth
                        width: ListView.view.width
                        text: title
                        leftPadding: DesignTokens.space4 + depth * DesignTokens.space3
                        Accessible.name: text
                        onClicked: backend.performCommand(qsTr("Open %1").arg(text))
                    }
                }
            }
        }

        Rectangle {
            Layout.preferredWidth: 1
            Layout.fillHeight: true
            color: window.palette.mid
            opacity: 0.35
        }

        SplitView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: Qt.Horizontal

            Pane {
                SplitView.fillWidth: true
                SplitView.minimumWidth: 360
                padding: DesignTokens.space6
                ColumnLayout {
                    anchors.fill: parent
                    Label {
                        text: qsTr("Chapter One")
                        font.pixelSize: 24
                        font.bold: true
                        Accessible.role: Accessible.Heading
                    }
                    TextArea {
                        id: primaryEditor
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        wrapMode: TextEdit.Wrap
                        textFormat: TextEdit.RichText
                        selectByMouse: true
                        persistentSelection: true
                        placeholderText: qsTr("Begin writing…")
                        text: qsTr("<h2>The Glass Orchard</h2><p>Begin writing your story here.</p>")
                        onCursorPositionChanged: primaryAdapter.cursorPosition = cursorPosition
                        onSelectionStartChanged: primaryAdapter.selectionStart = selectionStart
                        onSelectionEndChanged: primaryAdapter.selectionEnd = selectionEnd
                        onActiveFocusChanged: primaryAdapter.focused = activeFocus
                        Keys.onPressed: function(event) {
                            if (event.key === Qt.Key_Backspace && event.modifiers === Qt.NoModifier) {
                                primaryAdapter.deletePreviousSemanticUnit()
                                event.accepted = true
                            } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                                       && event.modifiers === Qt.NoModifier) {
                                primaryAdapter.insertParagraphBreak()
                                event.accepted = true
                            }
                        }
                        Accessible.name: qsTr("Manuscript editor")
                    }
                    EditorAdapter {
                        id: primaryAdapter
                        textDocument: primaryEditor.textDocument
                        onIncrementalDirty: function(revision, position, removed, added, firstBlock, lastBlockExclusive) {
                            backend.noteEditorDelta(revision, firstBlock, lastBlockExclusive)
                        }
                        onAdapterError: function(message) {
                            window.transientMessage = message
                            errorPopup.open()
                        }
                    }
                }
            }

            Pane {
                SplitView.preferredWidth: 390
                SplitView.minimumWidth: 280
                padding: DesignTokens.space4
                ColumnLayout {
                    anchors.fill: parent
                    Label {
                        text: qsTr("Research — Orchard Notes")
                        font.pixelSize: 18
                        font.bold: true
                    }
                    TextArea {
                        id: researchEditor
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        wrapMode: TextEdit.Wrap
                        selectByMouse: true
                        persistentSelection: true
                        text: qsTr("Keep research notes visible while writing.")
                        onCursorPositionChanged: researchAdapter.cursorPosition = cursorPosition
                        onSelectionStartChanged: researchAdapter.selectionStart = selectionStart
                        onSelectionEndChanged: researchAdapter.selectionEnd = selectionEnd
                        onActiveFocusChanged: researchAdapter.focused = activeFocus
                        Accessible.name: qsTr("Research editor")
                    }
                    EditorAdapter {
                        id: researchAdapter
                        textDocument: researchEditor.textDocument
                    }
                }
            }
        }
    }

    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: DesignTokens.space3
            anchors.rightMargin: DesignTokens.space3
            Label {
                text: window.transientMessage.length > 0 ? window.transientMessage : qsTr("Local-first · no network requests")
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            Label { text: qsTr("Document revision %1 · %2").arg(backend.document_revision).arg(backend.save_status) }
        }
    }
}
