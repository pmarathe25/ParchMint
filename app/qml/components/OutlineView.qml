pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var backend
    required property var model
    padding: 12
    ColumnLayout {
        anchors.fill: parent
        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTr("Summary outline"); font.pixelSize: 20; font.bold: true; Layout.fillWidth: true }
            ComboBox { model: [qsTr("Binder order"), qsTr("Title"), qsTr("Status")]; onActivated: root.backend.setOutlineSort(currentIndex === 1 ? "title" : currentIndex === 2 ? "status" : "binder"); Accessible.name: qsTr("Outline sort") }
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            Label { text: qsTr("Title"); Layout.preferredWidth: 190; font.bold: true }
            Label { text: qsTr("Synopsis"); Layout.fillWidth: true; font.bold: true }
            Label { text: qsTr("Status"); Layout.preferredWidth: 100; font.bold: true }
            Label { text: qsTr("Label"); Layout.preferredWidth: 100; font.bold: true }
            Label { text: qsTr("Words"); Layout.preferredWidth: 52; font.bold: true }
        }
        ListView {
            Layout.fillWidth: true; Layout.fillHeight: true
            clip: true; reuseItems: true; model: root.model
            delegate: ItemDelegate {
                required property string nodeId
                required property string title
                required property string synopsis
                required property string status
                required property string label
                required property int depth
                required property bool isRoot
                width: ListView.view.width
                highlighted: root.backend.selected_id === nodeId
                contentItem: RowLayout {
                    spacing: 8
                    TextField { text: title; readOnly: isRoot; Layout.preferredWidth: 190; leftPadding: depth * 12; onEditingFinished: root.backend.renameNode(nodeId, text); Accessible.name: qsTr("Title for %1").arg(title) }
                    TextField { text: synopsis; readOnly: isRoot; Layout.fillWidth: true; onEditingFinished: root.backend.editSummary(nodeId, text); Accessible.name: qsTr("Synopsis for %1").arg(title) }
                    TextField { text: status; readOnly: isRoot; Layout.preferredWidth: 100; onEditingFinished: root.backend.editStatus(nodeId, text); Accessible.name: qsTr("Status for %1").arg(title) }
                    TextField { text: label; readOnly: isRoot; Layout.preferredWidth: 100; onEditingFinished: root.backend.editLabel(nodeId, text); Accessible.name: qsTr("Label for %1").arg(title) }
                    Label { text: qsTr("—"); Layout.preferredWidth: 52; horizontalAlignment: Text.AlignHCenter }
                }
                onClicked: root.backend.selectNode(nodeId, false)
            }
        }
    }
}
