pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var backend
    required property var model
    padding: 0
    background: Rectangle { color: root.palette.alternateBase }
    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        Label { text: qsTr("BINDER"); font.bold: true; font.pixelSize: 11; opacity: .7; Layout.fillWidth: true; leftPadding: 16; topPadding: 16; bottomPadding: 8 }
        ListView {
            id: list
            Layout.fillWidth: true; Layout.fillHeight: true
            clip: true; reuseItems: true; model: root.model
            delegate: ItemDelegate {
                required property string nodeId
                required property string title
                required property string synopsis
                required property int depth
                required property bool isRoot
                width: ListView.view.width
                text: title
                leftPadding: 14 + depth * 14
                highlighted: root.backend.selected_id === nodeId
                Accessible.name: qsTr("Binder item %1").arg(title)
                Accessible.description: synopsis
                onClicked: root.backend.selectNode(nodeId, false)
                Menu { id: contextMenu
                    MenuItem { text: qsTr("New group"); onTriggered: root.backend.createChild(nodeId, qsTr("Untitled Group"), true) }
                    MenuItem { text: qsTr("New scene"); onTriggered: root.backend.createChild(nodeId, qsTr("Untitled Scene"), false) }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Duplicate"); onTriggered: root.backend.duplicateNode(nodeId) }
                    MenuItem { text: qsTr("Move up"); onTriggered: root.backend.moveUp(nodeId) }
                    MenuItem { text: qsTr("Move down"); onTriggered: root.backend.moveDown(nodeId) }
                    MenuItem { text: qsTr("Indent"); onTriggered: root.backend.indentNode(nodeId) }
                    MenuItem { text: qsTr("Outdent"); onTriggered: root.backend.outdentNode(nodeId) }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Move to trash"); onTriggered: root.backend.trashNode(nodeId) }
                }
                onPressAndHold: contextMenu.open()
            }
        }
    }
}
