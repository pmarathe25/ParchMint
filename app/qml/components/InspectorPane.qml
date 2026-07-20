pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var backend
    padding: 16
    background: Rectangle { color: root.palette.alternateBase }
    ColumnLayout {
        anchors.fill: parent
        Label { text: qsTr("Inspector"); font.pixelSize: 20; font.bold: true }
        Label { text: root.backend.selected_count === 1 ? root.backend.selected_title : root.backend.selected_count > 1 ? qsTr("%1 items selected").arg(root.backend.selected_count) : qsTr("No selection"); wrapMode: Text.Wrap; font.bold: true; Layout.fillWidth: true }
        Label { text: qsTr("Synopsis") }
        TextArea { text: root.backend.selected_synopsis; enabled: root.backend.selected_id.length > 0; Layout.fillWidth: true; Layout.preferredHeight: 140; wrapMode: TextEdit.Wrap; onActiveFocusChanged: if (!activeFocus) root.backend.editSummary(root.backend.selected_id, text); Accessible.name: qsTr("Selected synopsis") }
        Label { text: qsTr("Status") }
        TextField { text: root.backend.selected_status; enabled: root.backend.selected_id.length > 0; Layout.fillWidth: true; onEditingFinished: root.backend.editStatus(root.backend.selected_id, text) }
        Label { text: qsTr("Label") }
        TextField { text: root.backend.selected_label; enabled: root.backend.selected_id.length > 0; Layout.fillWidth: true; onEditingFinished: root.backend.editLabel(root.backend.selected_id, text) }
        CheckBox { text: qsTr("Include in compile"); checked: true; enabled: root.backend.selected_id.length > 0 }
        Label { text: qsTr("Tags and notes will be available here."); opacity: .6; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Item { Layout.fillHeight: true }
        Button { text: qsTr("Move to Trash"); enabled: root.backend.selected_id.length > 0; Layout.fillWidth: true; onClicked: root.backend.trashNode(root.backend.selected_id) }
    }
}
