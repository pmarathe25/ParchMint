pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var backend
    required property var model
    padding: DesignTokens.space3
    background: Rectangle { color: DesignTokens.surface }
    ColumnLayout {
        anchors.fill: parent
        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTr("Summary outline"); font.pixelSize: DesignTokens.typeTitle; font.bold: true; Layout.fillWidth: true; Accessible.role: Accessible.Heading }
            ComboBox { model: [qsTr("Binder order"), qsTr("Title"), qsTr("Status")]; onActivated: root.backend.setOutlineSort(currentIndex === 1 ? "title" : currentIndex === 2 ? "status" : "binder"); Accessible.name: qsTr("Outline sort") }
        }
        RowLayout {
            Layout.fillWidth: true; spacing: DesignTokens.space2
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
                id: rowRoot
                required property string nodeId
                required property string title
                required property string synopsis
                required property string status
                required property string label
                required property int depth
                required property bool isRoot
                required property int wordCount
                required property bool includeInCompile
                width: ListView.view.width
                highlighted: root.backend.selected_id === nodeId
                property string bufferNode: ""
                property bool editing: false
                property string titleBuffer: ""
                property string synopsisBuffer: ""
                property string statusBuffer: ""
                property string labelBuffer: ""
                function refreshBuffers() {
                    if (!editing) {
                        bufferNode = nodeId
                        titleBuffer = title
                        synopsisBuffer = synopsis
                        statusBuffer = status
                        labelBuffer = label
                    }
                }
                onNodeIdChanged: refreshBuffers()
                Component.onCompleted: refreshBuffers()
                contentItem: RowLayout {
                    spacing: DesignTokens.space2
                    TextField {
                        id: titleField
                        text: rowRoot.titleBuffer
                        readOnly: rowRoot.isRoot
                        Layout.preferredWidth: 190
                        leftPadding: rowRoot.depth * DesignTokens.space3
                        onActiveFocusChanged: rowRoot.editing = activeFocus
                        onTextEdited: rowRoot.titleBuffer = text
                        onEditingFinished: if (rowRoot.bufferNode === rowRoot.nodeId) root.backend.renameNode(rowRoot.bufferNode, rowRoot.titleBuffer)
                        Accessible.name: qsTr("Title for %1").arg(rowRoot.title)
                    }
                    TextField {
                        id: synopsisField
                        text: rowRoot.synopsisBuffer
                        readOnly: rowRoot.isRoot
                        Layout.fillWidth: true
                        onActiveFocusChanged: rowRoot.editing = activeFocus
                        onTextEdited: rowRoot.synopsisBuffer = text
                        onEditingFinished: if (rowRoot.bufferNode === rowRoot.nodeId) root.backend.editSummary(rowRoot.bufferNode, rowRoot.synopsisBuffer)
                        Accessible.name: qsTr("Synopsis for %1").arg(rowRoot.title)
                    }
                    TextField {
                        text: rowRoot.statusBuffer
                        readOnly: rowRoot.isRoot
                        Layout.preferredWidth: 100
                        onActiveFocusChanged: rowRoot.editing = activeFocus
                        onTextEdited: rowRoot.statusBuffer = text
                        onEditingFinished: if (rowRoot.bufferNode === rowRoot.nodeId) root.backend.editStatus(rowRoot.bufferNode, rowRoot.statusBuffer)
                        Accessible.name: qsTr("Status for %1").arg(rowRoot.title)
                    }
                    TextField {
                        text: rowRoot.labelBuffer
                        readOnly: rowRoot.isRoot
                        Layout.preferredWidth: 100
                        onActiveFocusChanged: rowRoot.editing = activeFocus
                        onTextEdited: rowRoot.labelBuffer = text
                        onEditingFinished: if (rowRoot.bufferNode === rowRoot.nodeId) root.backend.editLabel(rowRoot.bufferNode, rowRoot.labelBuffer)
                        Accessible.name: qsTr("Label for %1").arg(rowRoot.title)
                    }
                    CheckBox {
                        visible: !rowRoot.isRoot
                        checked: rowRoot.includeInCompile
                        onToggled: root.backend.setIncludeInCompile(rowRoot.nodeId, checked)
                        Accessible.name: qsTr("Include %1 in compile").arg(rowRoot.title)
                    }
                    Label { text: rowRoot.wordCount > 0 ? rowRoot.wordCount : qsTr("—"); Layout.preferredWidth: 52; horizontalAlignment: Text.AlignHCenter }
                }
                onClicked: root.backend.selectNode(nodeId, false)
            }
        }
    }
}
