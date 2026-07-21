pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var backend
    padding: DesignTokens.space4
    background: Rectangle { color: DesignTokens.surface }
    property string bufferNodeId: ""
    property bool editing: false
    property string synopsisBuffer: ""
    property string statusBuffer: ""
    property string labelBuffer: ""
    function resetBuffers() {
        if (!editing) {
            bufferNodeId = backend.selected_id
            synopsisBuffer = backend.selected_synopsis
            statusBuffer = backend.selected_status
            labelBuffer = backend.selected_label
        }
    }
    onVisibleChanged: resetBuffers()
    Connections {
        target: root.backend
        function onSelectedIdChanged() { root.resetBuffers() }
        function onSelectedSynopsisChanged() { root.resetBuffers() }
        function onSelectedStatusChanged() { root.resetBuffers() }
        function onSelectedLabelChanged() { root.resetBuffers() }
    }
    Component.onCompleted: resetBuffers()

    ColumnLayout {
        anchors.fill: parent
        spacing: DesignTokens.space2
        Label { text: qsTr("Inspector"); font.pixelSize: DesignTokens.typeTitle; font.bold: true; Accessible.role: Accessible.Heading }
        Label { text: root.backend.selected_count === 1 ? root.backend.selected_title : root.backend.selected_count > 1 ? qsTr("%1 items selected").arg(root.backend.selected_count) : qsTr("No selection"); wrapMode: Text.Wrap; font.bold: true; Layout.fillWidth: true }
        Label { text: qsTr("Synopsis"); font.bold: true; topPadding: DesignTokens.space2 }
        TextArea {
            id: synopsis
            text: root.synopsisBuffer
            enabled: root.bufferNodeId.length > 0
            Layout.fillWidth: true; Layout.preferredHeight: 132; wrapMode: TextEdit.Wrap
            onTextChanged: if (activeFocus) root.synopsisBuffer = text
            onActiveFocusChanged: {
                root.editing = activeFocus
                if (!activeFocus && root.bufferNodeId === root.backend.selected_id)
                    root.backend.editSummary(root.bufferNodeId, root.synopsisBuffer)
            }
            Accessible.name: qsTr("Selected synopsis")
        }
        Label { text: qsTr("Metadata"); font.bold: true; topPadding: DesignTokens.space2 }
        TextField {
            id: status
            text: root.statusBuffer
            enabled: root.bufferNodeId.length > 0
            placeholderText: qsTr("Status")
            Layout.fillWidth: true
            onTextChanged: if (activeFocus) root.statusBuffer = text
            onActiveFocusChanged: root.editing = activeFocus
            onEditingFinished: if (root.bufferNodeId === root.backend.selected_id) root.backend.editStatus(root.bufferNodeId, root.statusBuffer)
            Accessible.name: qsTr("Selected status")
        }
        TextField {
            id: label
            text: root.labelBuffer
            enabled: root.bufferNodeId.length > 0
            placeholderText: qsTr("Label")
            Layout.fillWidth: true
            onTextChanged: if (activeFocus) root.labelBuffer = text
            onActiveFocusChanged: root.editing = activeFocus
            onEditingFinished: if (root.bufferNodeId === root.backend.selected_id) root.backend.editLabel(root.bufferNodeId, root.labelBuffer)
            Accessible.name: qsTr("Selected label")
        }
        CheckBox {
            text: qsTr("Include in compile")
            checked: root.backend.selected_include_in_compile
            enabled: root.bufferNodeId.length > 0
            onToggled: root.backend.setIncludeInCompile(root.bufferNodeId, checked)
            Accessible.name: qsTr("Include selected document in compile")
        }
        Label { text: qsTr("Statistics"); font.bold: true; topPadding: DesignTokens.space2 }
        Label { text: qsTr("%1 selected · %2 visible nodes").arg(root.backend.selected_count).arg(root.backend.node_count); color: DesignTokens.textMuted; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Item { Layout.fillHeight: true }
        Button { text: qsTr("Move to Trash…"); enabled: root.bufferNodeId.length > 0; Layout.fillWidth: true; onClicked: root.backend.trashNode(root.bufferNodeId) }
    }
}
