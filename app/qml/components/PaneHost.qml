pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// One host is instantiated twice by Main.qml. It intentionally owns its
// TextArea for its whole lifetime: replacing the other pane cannot discard its
// Qt undo stack, cursor, or scroll position.
Pane {
    id: root
    required property var backend
    required property var model
    required property int paneIndex
    required property string nodeId
    required property string viewName
    required property bool pinned
    property bool focused: backend.focused_pane === paneIndex
    property string loadedNode: ""
    padding: 0

    function reloadBody() {
        if (viewName === "editor" && nodeId.length > 0 && loadedNode !== nodeId) {
            editor.text = backend.paneDocumentBody(paneIndex)
            loadedNode = nodeId
        }
    }
    onNodeIdChanged: reloadBody()
    onViewNameChanged: reloadBody()
    Component.onCompleted: reloadBody()
    background: Rectangle {
        color: root.focused ? root.palette.base : root.palette.alternateBase
        border.width: root.focused ? 1 : 0
        border.color: root.palette.highlight
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        ToolBar {
            Layout.fillWidth: true
            RowLayout {
                anchors.fill: parent
                ToolButton { text: root.pinned ? "●" : "○"; checkable: true; checked: root.pinned; Accessible.name: qsTr("Pin pane"); onClicked: root.backend.setPanePinned(root.paneIndex, checked) }
                Label { text: root.viewName === "attachment" ? qsTr("Attachment") : root.viewName; Layout.fillWidth: true; elide: Text.ElideRight }
                ToolButton { text: "×"; Accessible.name: qsTr("Close pane"); onClicked: root.backend.closePane(root.paneIndex) }
            }
        }
        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: root.viewName === "outline" ? 1 : root.viewName === "cards" ? 2 : root.viewName === "attachment" ? 3 : 0
            TextArea {
                id: editor
                textFormat: TextEdit.PlainText
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                persistentSelection: true
                placeholderText: root.nodeId.length ? qsTr("Markdown research or manuscript note") : qsTr("Select a document")
                Accessible.name: qsTr("Document editor")
                onActiveFocusChanged: {
                    if (activeFocus)
                        root.backend.focusPane(root.paneIndex)
                    else if (root.loadedNode === root.nodeId && root.nodeId.length > 0)
                        root.backend.savePaneBody(root.paneIndex, text)
                }
            }
            OutlineView { backend: root.backend; model: root.model }
            CardsView { backend: root.backend; model: root.model }
            ColumnLayout {
                spacing: 12
                Label { text: qsTr("Safe attachment preview"); font.bold: true; font.pixelSize: 20 }
                Label { text: root.backend.paneAttachmentDescription(root.paneIndex); wrapMode: Text.Wrap; Layout.fillWidth: true }
                Label { text: qsTr("Images, PDFs where the platform supports them, and plain text are previewed passively. Other files require an explicit system-open action."); wrapMode: Text.Wrap; Layout.fillWidth: true; opacity: .7 }
                Button { text: qsTr("Open in system application…"); enabled: root.backend.paneAttachmentUrl(root.paneIndex).length > 0; onClicked: Qt.openUrlExternally(root.backend.paneAttachmentUrl(root.paneIndex)) }
            }
        }
    }
}
