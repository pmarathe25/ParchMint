pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var backend
    required property var model
    signal openInSplitRequested(string nodeId)
    property var collapsedNodes: ({})
    property alias filterText: filter.text
    property string propertiesNodeId: ""
    property string propertiesTitle: ""
    property string originalSynopsis: ""
    property string originalStatus: ""
    property string originalLabel: ""
    property bool originalIncludeInCompile: false
    padding: 0
    background: Rectangle { color: DesignTokens.surface }

    function isExpanded(nodeId) {
        return !collapsedNodes[nodeId]
    }

    function setExpanded(nodeId, expanded) {
        const next = Object.assign({}, collapsedNodes)
        if (expanded)
            delete next[nodeId]
        else
            next[nodeId] = true
        collapsedNodes = next
    }

    function beginProperties(nodeId, title, synopsis, status, labelValue, includeInCompile) {
        propertiesNodeId = nodeId
        propertiesTitle = title
        originalSynopsis = synopsis
        originalStatus = status
        originalLabel = labelValue
        originalIncludeInCompile = includeInCompile
        propertiesSummaryEditor.text = synopsis
        propertiesStatusEditor.text = status
        propertiesLabelEditor.text = labelValue
        propertiesIncludeEditor.checked = includeInCompile
        propertiesDialog.open()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: DesignTokens.space2
            Layout.rightMargin: DesignTokens.space2
            Layout.topMargin: DesignTokens.space2
            Layout.bottomMargin: DesignTokens.space1
            Label {
                text: qsTr("Project")
                font.bold: true
                font.pixelSize: DesignTokens.typeCaption
                color: DesignTokens.textMuted
                Layout.fillWidth: true
                Accessible.role: Accessible.Heading
            }
            Label {
                text: root.backend.node_count
                color: DesignTokens.textFaint
                font.pixelSize: DesignTokens.typeCaption
            }
        }
        TextField {
            id: filter
            Layout.fillWidth: true
            Layout.leftMargin: DesignTokens.space2
            Layout.rightMargin: DesignTokens.space2
            Layout.bottomMargin: DesignTokens.space2
            placeholderText: qsTr("Filter files")
            Accessible.name: qsTr("Filter project files")
            onTextChanged: root.backend.setFilter(text)
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: DesignTokens.outline
        }
        ListView {
            id: tree
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            reuseItems: true
            model: root.model
            keyNavigationEnabled: false
            highlightFollowsCurrentItem: true
            highlight: Rectangle {
                color: DesignTokens.accentContainer
                radius: DesignTokens.radiusSmall
            }
            focus: true
            Accessible.name: qsTr("Project files")

            function moveCurrent(delta) {
                let candidate = currentIndex + delta
                while (candidate >= 0 && candidate < count) {
                    if (root.model.ancestorsExpanded(candidate, root.collapsedNodes)) {
                        currentIndex = candidate
                        return
                    }
                    candidate += delta
                }
            }

            Keys.onPressed: function(event) {
                if (event.key === Qt.Key_F2 && currentItem && !currentItem.isRoot) {
                    currentItem.beginRename()
                    event.accepted = true
                } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                           && currentItem) {
                    currentItem.openNode()
                    event.accepted = true
                } else if (event.key === Qt.Key_Left && currentItem) {
                    if (currentItem.hasChildren && currentItem.expanded)
                        root.setExpanded(currentItem.nodeId, false)
                    else if (currentItem.parentId >= 0)
                        currentIndex = currentItem.parentId
                    event.accepted = true
                } else if (event.key === Qt.Key_Right && currentItem) {
                    if (currentItem.hasChildren && !currentItem.expanded)
                        root.setExpanded(currentItem.nodeId, true)
                    else
                        moveCurrent(1)
                    event.accepted = true
                } else if (event.key === Qt.Key_Up) {
                    moveCurrent(-1)
                    event.accepted = true
                } else if (event.key === Qt.Key_Down) {
                    moveCurrent(1)
                    event.accepted = true
                }
            }

            delegate: ItemDelegate {
                id: delegateRoot
                required property int index
                required property string nodeId
                required property string parentNodeId
                required property string title
                required property string synopsis
                required property int depth
                required property int parentId
                required property bool isRoot
                required property bool isGroup
                required property bool hasChildren
                required property bool includeInCompile
                readonly property bool hierarchyVisible: root.model.ancestorsExpanded(index, root.collapsedNodes)
                readonly property bool expanded: root.isExpanded(nodeId)
                readonly property string creationParentId: isRoot || isGroup ? nodeId : parentNodeId
                width: ListView.view.width
                height: hierarchyVisible ? 30 : 0
                visible: hierarchyVisible
                leftPadding: DesignTokens.space1 + depth * DesignTokens.space3
                rightPadding: DesignTokens.space2
                highlighted: root.backend.selected_id === nodeId
                property bool editing: false
                property string bufferedNodeId: ""
                property string bufferedTitle: ""
                property string dropPlacement: ""
                Accessible.name: qsTr("Project item %1").arg(title)
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
                function openNode() {
                    root.backend.selectNode(nodeId, false)
                }
                function toggleExpanded() {
                    if (hasChildren)
                        root.setExpanded(nodeId, !expanded)
                }

                onNodeIdChanged: syncBuffer()
                Component.onCompleted: syncBuffer()

                TapHandler {
                    acceptedButtons: Qt.LeftButton
                    onTapped: function(eventPoint) {
                        ListView.view.currentIndex = delegateRoot.index
                        root.backend.selectNode(delegateRoot.nodeId,
                                                (eventPoint.modifiers & Qt.ControlModifier) !== 0)
                    }
                    onLongPressed: contextMenu.popup()
                }
                TapHandler {
                    acceptedButtons: Qt.RightButton
                    onTapped: {
                        ListView.view.currentIndex = delegateRoot.index
                        contextMenu.popup()
                    }
                }

                contentItem: RowLayout {
                    spacing: DesignTokens.space1
                    ToolButton {
                        Layout.preferredWidth: 24
                        Layout.preferredHeight: 24
                        enabled: delegateRoot.hasChildren
                        flat: true
                        padding: 3
                        Accessible.name: delegateRoot.expanded ? qsTr("Collapse %1").arg(delegateRoot.title)
                                                               : qsTr("Expand %1").arg(delegateRoot.title)
                        onClicked: delegateRoot.toggleExpanded()
                        contentItem: Image {
                            visible: delegateRoot.hasChildren
                            source: "qrc:/icons/chevron.svg"
                            sourceSize.width: 14
                            sourceSize.height: 14
                            width: 14
                            height: 14
                            anchors.centerIn: parent
                            rotation: delegateRoot.expanded ? 90 : 0
                        }
                    }
                    Image {
                        visible: !delegateRoot.isGroup && !delegateRoot.isRoot
                        source: "qrc:/icons/document.svg"
                        sourceSize.width: 15
                        sourceSize.height: 15
                        Layout.preferredWidth: 15
                        Layout.preferredHeight: 15
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
                        font.bold: delegateRoot.isRoot || delegateRoot.isGroup
                    }
                }

                Drag.active: dragHandler.active && !isRoot
                Drag.supportedActions: Qt.MoveAction
                Drag.mimeData: ({ "application/x-parchmint-node-id": nodeId })
                Drag.hotSpot.x: width / 2
                Drag.hotSpot.y: height / 2
                DragHandler {
                    id: dragHandler
                    enabled: !delegateRoot.isRoot && !delegateRoot.editing
                }
                DropArea {
                    anchors.fill: parent
                    keys: ["application/x-parchmint-node-id"]
                    onEntered: function(drag) {
                        if (drag.source && drag.source.nodeId === delegateRoot.nodeId) {
                            drag.accepted = false
                            return
                        }
                        delegateRoot.dropPlacement = delegateRoot.isRoot
                                ? "inside"
                                : drag.y < height * .25 ? "before"
                                : drag.y > height * .75 ? "after"
                                : delegateRoot.isGroup ? "inside" : "after"
                    }
                    onPositionChanged: function(drag) {
                        delegateRoot.dropPlacement = delegateRoot.isRoot
                                ? "inside"
                                : drag.y < height * .25 ? "before"
                                : drag.y > height * .75 ? "after"
                                : delegateRoot.isGroup ? "inside" : "after"
                    }
                    onExited: delegateRoot.dropPlacement = ""
                    onDropped: function(drop) {
                        const id = drop.getDataAsString("application/x-parchmint-node-id")
                        if (id.length && root.backend.moveNode(id, delegateRoot.nodeId,
                                                               delegateRoot.dropPlacement))
                            drop.accepted = true
                        delegateRoot.dropPlacement = ""
                    }
                }
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 2
                    visible: delegateRoot.dropPlacement === "before"
                    color: DesignTokens.accent
                }
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 2
                    visible: delegateRoot.dropPlacement === "after"
                    color: DesignTokens.accent
                }
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 2
                    radius: DesignTokens.radiusSmall
                    visible: delegateRoot.dropPlacement === "inside"
                    color: "transparent"
                    border.width: 1
                    border.color: DesignTokens.accent
                }

                Menu {
                    id: contextMenu
                    MenuItem {
                        text: qsTr("New section")
                        enabled: delegateRoot.creationParentId.length > 0
                        onTriggered: root.backend.createChild(delegateRoot.creationParentId,
                                                              qsTr("Untitled Section"), true)
                    }
                    MenuItem {
                        text: qsTr("New document")
                        enabled: delegateRoot.creationParentId.length > 0
                        onTriggered: root.backend.createChild(delegateRoot.creationParentId,
                                                              qsTr("Untitled Document"), false)
                    }
                    MenuSeparator {}
                    MenuItem {
                        text: qsTr("Rename")
                        enabled: !delegateRoot.isRoot
                        onTriggered: delegateRoot.beginRename()
                    }
                    MenuItem {
                        text: qsTr("Properties…")
                        enabled: !delegateRoot.isRoot
                        onTriggered: root.beginProperties(delegateRoot.nodeId, delegateRoot.title,
                                                           delegateRoot.synopsis, delegateRoot.status,
                                                           delegateRoot.label, delegateRoot.includeInCompile)
                    }
                    MenuItem {
                        text: qsTr("Open in split")
                        enabled: !delegateRoot.isRoot
                        onTriggered: root.openInSplitRequested(delegateRoot.nodeId)
                    }
                    MenuItem {
                        text: qsTr("Move to Trash")
                        enabled: !delegateRoot.isRoot
                        onTriggered: root.backend.trashNode(delegateRoot.nodeId)
                    }
                }
            }
        }
    }

    Dialog {
        id: propertiesDialog
        title: qsTr("Properties — %1").arg(root.propertiesTitle)
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: {
            if (propertiesSummaryEditor.text !== root.originalSynopsis)
                root.backend.editSummary(root.propertiesNodeId, propertiesSummaryEditor.text)
            if (propertiesStatusEditor.text !== root.originalStatus)
                root.backend.editStatus(root.propertiesNodeId, propertiesStatusEditor.text)
            if (propertiesLabelEditor.text !== root.originalLabel)
                root.backend.editLabel(root.propertiesNodeId, propertiesLabelEditor.text)
            if (propertiesIncludeEditor.checked !== root.originalIncludeInCompile)
                root.backend.setIncludeInCompile(root.propertiesNodeId, propertiesIncludeEditor.checked)
        }
        contentItem: ColumnLayout {
            spacing: DesignTokens.space2
            Label { text: qsTr("Summary") }
            TextArea {
                id: propertiesSummaryEditor
                Layout.preferredWidth: 460
                Layout.preferredHeight: 150
                wrapMode: TextEdit.Wrap
                placeholderText: qsTr("Add a short summary")
                Accessible.name: qsTr("Summary")
            }
            GridLayout {
                columns: 2
                Layout.fillWidth: true
                Label { text: qsTr("Status") }
                TextField { id: propertiesStatusEditor; Layout.fillWidth: true; Accessible.name: qsTr("Status") }
                Label { text: qsTr("Label") }
                TextField { id: propertiesLabelEditor; Layout.fillWidth: true; Accessible.name: qsTr("Label") }
            }
            CheckBox {
                id: propertiesIncludeEditor
                text: qsTr("Include in compile")
            }
        }
    }
}
