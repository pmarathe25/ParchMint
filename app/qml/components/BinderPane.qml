pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var backend
    required property var model
    padding: 0
    background: Rectangle { color: DesignTokens.surface }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: DesignTokens.space3
            Label {
                text: qsTr("BINDER")
                font.bold: true
                font.pixelSize: DesignTokens.typeCaption
                color: DesignTokens.textMuted
                Layout.fillWidth: true
                Accessible.role: Accessible.Heading
            }
            Label { text: qsTr("%1 items").arg(root.backend.node_count); color: DesignTokens.textFaint; font.pixelSize: DesignTokens.typeCaption }
        }
        TextField {
            id: filter
            Layout.fillWidth: true
            Layout.leftMargin: DesignTokens.space3
            Layout.rightMargin: DesignTokens.space3
            Layout.bottomMargin: DesignTokens.space2
            placeholderText: qsTr("Filter binder")
            Accessible.name: qsTr("Filter binder")
            onTextChanged: root.backend.setFilter(text)
        }
        Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: DesignTokens.outline }
        ListView {
            id: tree
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            reuseItems: true
            model: root.model
            keyNavigationEnabled: true
            highlightFollowsCurrentItem: true
            highlight: Rectangle { color: DesignTokens.accentContainer; radius: DesignTokens.radiusSmall }
            focus: true
            Accessible.name: qsTr("Project binder")
            Keys.onPressed: function(event) {
                if (event.key === Qt.Key_F2 && currentItem && !currentItem.isRoot) {
                    currentItem.beginRename()
                    event.accepted = true
                } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && currentItem) {
                    currentItem.openNode()
                    event.accepted = true
                } else if (event.key === Qt.Key_Left && currentIndex > 0) {
                    currentIndex -= 1
                    event.accepted = true
                } else if (event.key === Qt.Key_Right && currentIndex + 1 < count) {
                    currentIndex += 1
                    event.accepted = true
                }
            }
            delegate: ItemDelegate {
                id: delegateRoot
                required property string nodeId
                required property string title
                required property string synopsis
                required property int depth
                required property bool isRoot
                required property bool isGroup
                required property bool includeInCompile
                width: ListView.view.width
                height: Math.max(34, contentItem.implicitHeight + DesignTokens.space1)
                leftPadding: DesignTokens.space2 + depth * DesignTokens.space3
                highlighted: root.backend.selected_id === nodeId
                property bool editing: false
                property string bufferedNodeId: ""
                property string bufferedTitle: ""
                property string dropPlacement: ""
                Accessible.name: qsTr("Binder item %1").arg(title)
                Accessible.description: synopsis

                function syncBuffer() {
                    if (!editing) {
                        bufferedNodeId = nodeId
                        bufferedTitle = title
                    }
                }
                function beginRename() {
                    if (!isRoot) {
                        editing = true
                        bufferedNodeId = nodeId
                        bufferedTitle = title
                        rename.forceActiveFocus()
                        rename.selectAll()
                    }
                }
                function commitRename() {
                    if (editing && bufferedNodeId === nodeId && bufferedTitle.trim().length)
                        root.backend.renameNode(bufferedNodeId, bufferedTitle.trim())
                    editing = false
                }
                function openNode() { root.backend.selectNode(nodeId, false) }
                onNodeIdChanged: syncBuffer()
                Component.onCompleted: syncBuffer()
                TapHandler {
                    acceptedButtons: Qt.LeftButton
                    onTapped: function(eventPoint, button) {
                        ListView.view.currentIndex = index
                        root.backend.selectNode(nodeId, (eventPoint.modifiers & Qt.ControlModifier) !== 0)
                    }
                }
                contentItem: RowLayout {
                    spacing: DesignTokens.space2
                    Image {
                        source: delegateRoot.isGroup || delegateRoot.isRoot
                                ? "qrc:/icons/chevron.svg" : "qrc:/icons/document.svg"
                        sourceSize.width: DesignTokens.iconSize
                        sourceSize.height: DesignTokens.iconSize
                        Layout.preferredWidth: DesignTokens.iconSize
                        Layout.preferredHeight: DesignTokens.iconSize
                        rotation: delegateRoot.isGroup || delegateRoot.isRoot ? 90 : 0
                        Accessible.ignored: true
                    }
                    TextField {
                        id: rename
                        Layout.fillWidth: true
                        visible: delegateRoot.editing
                        text: delegateRoot.bufferedTitle
                        onTextEdited: delegateRoot.bufferedTitle = text
                        onEditingFinished: delegateRoot.commitRename()
                        Keys.onEscapePressed: delegateRoot.editing = false
                        Accessible.name: qsTr("Rename %1").arg(delegateRoot.title)
                    }
                    Label {
                        visible: !delegateRoot.editing
                        text: delegateRoot.title
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        color: delegateRoot.isRoot ? DesignTokens.textMuted : DesignTokens.text
                        font.bold: delegateRoot.isRoot
                    }
                    Label { visible: !delegateRoot.isRoot && !delegateRoot.includeInCompile; text: qsTr("Excluded"); font.pixelSize: DesignTokens.typeCaption; color: DesignTokens.textFaint }
                }
                Drag.active: dragHandler.active && !isRoot
                Drag.supportedActions: Qt.MoveAction
                Drag.mimeData: ({ "application/x-parchmint-node-id": nodeId })
                Drag.hotSpot.x: width / 2
                Drag.hotSpot.y: height / 2
                DragHandler { id: dragHandler; enabled: !delegateRoot.isRoot }
                DropArea {
                    anchors.fill: parent
                    keys: ["application/x-parchmint-node-id"]
                    onEntered: function(drag) {
                        if (drag.source && drag.source.nodeId === delegateRoot.nodeId) {
                            drag.accepted = false
                            return
                        }
                        delegateRoot.dropPlacement = drag.y < height * .25 ? "before" : drag.y > height * .75 ? "after" : (delegateRoot.isGroup ? "inside" : "after")
                    }
                    onExited: delegateRoot.dropPlacement = ""
                    onDropped: function(drop) {
                        const id = drop.getDataAsString("application/x-parchmint-node-id")
                        if (id.length && root.backend.moveNode(id, delegateRoot.nodeId, delegateRoot.dropPlacement))
                            drop.accepted = true
                        delegateRoot.dropPlacement = ""
                    }
                }
                Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: 2; visible: delegateRoot.dropPlacement === "before"; color: DesignTokens.accent }
                Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 2; visible: delegateRoot.dropPlacement === "after"; color: DesignTokens.accent }
                Rectangle { anchors.fill: parent; anchors.margins: 2; radius: DesignTokens.radiusSmall; visible: delegateRoot.dropPlacement === "inside"; color: "transparent"; border.width: 1; border.color: DesignTokens.accent }
                Menu {
                    id: contextMenu
                    MenuItem { text: qsTr("Rename"); enabled: !delegateRoot.isRoot; onTriggered: delegateRoot.beginRename() }
                    MenuItem { text: qsTr("New group"); onTriggered: root.backend.createChild(delegateRoot.nodeId, qsTr("Untitled Group"), true) }
                    MenuItem { text: qsTr("New scene"); onTriggered: root.backend.createChild(delegateRoot.nodeId, qsTr("Untitled Scene"), false) }
                    MenuSeparator {}
                    MenuItem { text: qsTr("Open in other pane"); onTriggered: root.backend.openInOtherPane(delegateRoot.nodeId) }
                    MenuItem { text: qsTr("Move to trash"); enabled: !delegateRoot.isRoot; onTriggered: root.backend.trashNode(delegateRoot.nodeId) }
                }
                onPressAndHold: contextMenu.open()
            }
        }
    }
}
