pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.parchmint.adapters 1.0

Pane {
    id: root
    required property var backend
    required property var model
    signal openRequested(string nodeId)
    property var collapsedNodes: ({})
    property string summaryNodeId: ""
    property string summaryTitle: ""
    property string summaryBuffer: ""
    padding: DesignTokens.space3
    background: Rectangle { color: DesignTokens.base }

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
    function beginSummaryEdit(nodeId, title, synopsis) {
        summaryNodeId = nodeId
        summaryTitle = title
        summaryBuffer = synopsis
        summaryEditor.text = synopsis
        summaryDialog.open()
    }


    CardsModel {
        id: cardsModel
        source: root.model
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: DesignTokens.space2

        Label {
            text: qsTr("Cards")
            font.pixelSize: DesignTokens.typeTitle
            font.bold: true
            Accessible.role: Accessible.Heading
        }

        ListView {
            id: cards
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            reuseItems: true
            spacing: 0
            model: cardsModel
            Accessible.name: qsTr("Manuscript cards")

            delegate: ItemDelegate {
                id: cardRoot
                required property int index
                required property string nodeId
                required property string parentNodeId
                required property string title
                required property string synopsis
                required property int depth
                required property bool isGroup
                required property bool hasChildren
                readonly property bool hierarchyVisible: cardsModel.ancestorsExpanded(index, root.collapsedNodes)
                readonly property bool expanded: root.isExpanded(nodeId)
                readonly property string creationParentId: isGroup ? nodeId : parentNodeId
                readonly property int visibleDepth: Math.max(0, depth - 1)
                width: ListView.view.width
                height: hierarchyVisible ? 108 : 0
                visible: hierarchyVisible
                leftPadding: DesignTokens.space3 + visibleDepth * DesignTokens.space5
                rightPadding: DesignTokens.space3
                topPadding: DesignTokens.space2
                bottomPadding: DesignTokens.space2
                highlighted: root.backend.selected_id === nodeId
                hoverEnabled: true
                property bool editing: false
                property string bufferedNodeId: ""
                property string bufferedTitle: ""
                property string dropPlacement: ""
                Accessible.name: qsTr("Card %1").arg(title)
                Accessible.description: synopsis

                function syncBuffer() {
                    if (!editing) {
                        bufferedNodeId = nodeId
                        bufferedTitle = title
                    }
                }
                function beginRename() {
                    editing = true
                    bufferedNodeId = nodeId
                    bufferedTitle = title
                    titleEditor.forceActiveFocus()
                    titleEditor.selectAll()
                }
                function commitRename() {
                    if (editing && bufferedNodeId === nodeId && bufferedTitle.trim().length)
                        root.backend.renameNode(bufferedNodeId, bufferedTitle.trim())
                    editing = false
                }
                function toggleExpanded() {
                    if (hasChildren)
                        root.setExpanded(nodeId, !expanded)
                }

                onNodeIdChanged: syncBuffer()
                Component.onCompleted: syncBuffer()

                background: Rectangle {
                    radius: DesignTokens.radiusMedium
                    color: cardRoot.highlighted ? DesignTokens.accentContainer
                                                : cardRoot.hovered ? DesignTokens.overlay
                                                                   : DesignTokens.raised
                    border.width: 1
                    border.color: cardRoot.highlighted ? DesignTokens.accent : DesignTokens.outline
                }

                TapHandler {
                    acceptedButtons: Qt.LeftButton
                    onTapped: root.backend.selectNode(cardRoot.nodeId, false)
                    onDoubleTapped: root.openRequested(cardRoot.nodeId)
                    onLongPressed: cardMenu.popup()
                }
                TapHandler {
                    acceptedButtons: Qt.RightButton
                    onTapped: {
                        root.backend.selectNode(cardRoot.nodeId, false)
                        cardMenu.popup()
                    }
                }

                contentItem: RowLayout {
                    spacing: DesignTokens.space2
                    ToolButton {
                        Layout.preferredWidth: 26
                        Layout.preferredHeight: 26
                        Layout.alignment: Qt.AlignTop
                        enabled: cardRoot.hasChildren
                        flat: true
                        padding: 4
                        Accessible.name: cardRoot.expanded ? qsTr("Collapse %1").arg(cardRoot.title)
                                                          : qsTr("Expand %1").arg(cardRoot.title)
                        onClicked: cardRoot.toggleExpanded()
                        contentItem: Image {
                            visible: cardRoot.hasChildren
                            source: "qrc:/icons/chevron.svg"
                            sourceSize.width: 15
                            sourceSize.height: 15
                            width: 15
                            height: 15
                            anchors.centerIn: parent
                            rotation: cardRoot.expanded ? 90 : 0
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: DesignTokens.space1
                        TextField {
                            id: titleEditor
                            Layout.fillWidth: true
                            visible: cardRoot.editing
                            text: cardRoot.bufferedTitle
                            onTextEdited: cardRoot.bufferedTitle = text
                            onEditingFinished: cardRoot.commitRename()
                            Keys.onEscapePressed: cardRoot.editing = false
                            Accessible.name: qsTr("Rename %1").arg(cardRoot.title)
                        }
                        Label {
                            visible: !cardRoot.editing
                            text: cardRoot.title
                            font.bold: true
                            font.pixelSize: DesignTokens.typeBody
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                        Label {
                            text: cardRoot.synopsis.length ? cardRoot.synopsis : qsTr("No synopsis")
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            wrapMode: Text.Wrap
                            maximumLineCount: 3
                            elide: Text.ElideRight
                            color: cardRoot.synopsis.length ? DesignTokens.textMuted : DesignTokens.textFaint
                        }
                    }
                }

                Drag.active: dragHandler.active
                Drag.supportedActions: Qt.MoveAction
                Drag.mimeData: ({ "application/x-parchmint-node-id": nodeId })
                Drag.hotSpot.x: width / 2
                Drag.hotSpot.y: height / 2
                DragHandler {
                    id: dragHandler
                    enabled: !cardRoot.editing
                }

                DropArea {
                    anchors.fill: parent
                    keys: ["application/x-parchmint-node-id"]
                    onEntered: function(drag) {
                        if (drag.source && drag.source.nodeId === cardRoot.nodeId) {
                            drag.accepted = false
                            return
                        }
                        cardRoot.dropPlacement = drag.y < height * .25
                                ? "before"
                                : drag.y > height * .75
                                  ? "after"
                                  : (cardRoot.isGroup ? "inside" : "after")
                    }
                    onPositionChanged: function(drag) {
                        cardRoot.dropPlacement = drag.y < height * .25
                                ? "before"
                                : drag.y > height * .75
                                  ? "after"
                                  : (cardRoot.isGroup ? "inside" : "after")
                    }
                    onExited: cardRoot.dropPlacement = ""
                    onDropped: function(drop) {
                        const id = drop.getDataAsString("application/x-parchmint-node-id")
                        if (id.length && root.backend.moveNode(id, cardRoot.nodeId,
                                                               cardRoot.dropPlacement))
                            drop.accepted = true
                        cardRoot.dropPlacement = ""
                    }
                }
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 3
                    visible: cardRoot.dropPlacement === "before"
                    color: DesignTokens.accent
                }
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 3
                    visible: cardRoot.dropPlacement === "after"
                    color: DesignTokens.accent
                }
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 3
                    radius: DesignTokens.radiusMedium
                    visible: cardRoot.dropPlacement === "inside"
                    color: "transparent"
                    border.width: 2
                    border.color: DesignTokens.accent
                }

                Menu {
                    id: cardMenu
                    MenuItem {
                        text: qsTr("New section")
                        enabled: cardRoot.creationParentId.length > 0
                        onTriggered: root.backend.createChild(cardRoot.creationParentId,
                                                              qsTr("Untitled Section"), true)
                    }
                    MenuItem {
                        text: qsTr("New document")
                        enabled: cardRoot.creationParentId.length > 0
                        onTriggered: root.backend.createChild(cardRoot.creationParentId,
                                                              qsTr("Untitled Document"), false)
                    }
                    MenuSeparator {}
                    MenuItem {
                        text: qsTr("Rename")
                        onTriggered: cardRoot.beginRename()
                    }
                    MenuItem {
                        text: qsTr("Edit summary…")
                        onTriggered: root.beginSummaryEdit(cardRoot.nodeId, cardRoot.title, cardRoot.synopsis)
                    }
                    MenuItem {
                        text: qsTr("Move to Trash")
                        onTriggered: root.backend.trashNode(cardRoot.nodeId)
                    }
                }
            }
        }
    }

    Dialog {
        id: summaryDialog
        title: qsTr("Summary — %1").arg(root.summaryTitle)
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: root.backend.editSummary(root.summaryNodeId, summaryEditor.text)
        contentItem: ColumnLayout {
            spacing: DesignTokens.space2
            Label { text: qsTr("Summary") }
            TextArea {
                id: summaryEditor
                Layout.preferredWidth: 460
                Layout.preferredHeight: 180
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                placeholderText: qsTr("Add a short summary")
                Accessible.name: qsTr("Section summary")
            }
        }
    }
}
