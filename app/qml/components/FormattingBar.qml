pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ToolBar {
    id: root
    required property var adapter
    property var styleModel: []
    signal sourceModeRequested()

    RowLayout {
        anchors.fill: parent
        ToolButton {
            text: qsTr("Bold")
            checkable: true
            checked: root.adapter.boldState === 1
            opacity: root.adapter.boldState === -1 ? 0.65 : 1
            enabled: root.adapter.focused
            Accessible.name: qsTr("Toggle bold")
            onClicked: root.adapter.toggleBold()
        }
        ToolButton {
            text: qsTr("Italic")
            checkable: true
            checked: root.adapter.italicState === 1
            opacity: root.adapter.italicState === -1 ? 0.65 : 1
            enabled: root.adapter.focused
            Accessible.name: qsTr("Toggle italic")
            onClicked: root.adapter.toggleItalic()
        }
        ToolButton {
            text: qsTr("Sup")
            enabled: root.adapter.focused
            Accessible.name: qsTr("Superscript")
            onClicked: root.adapter.setVerticalAlignment(1)
        }
        ToolButton {
            text: qsTr("Sub")
            enabled: root.adapter.focused
            Accessible.name: qsTr("Subscript")
            onClicked: root.adapter.setVerticalAlignment(2)
        }
        ToolSeparator {}
        StylePicker {
            adapter: root.adapter
            model: root.styleModel
        }
        ToolButton {
            text: qsTr("Clear formatting")
            enabled: root.adapter.focused
            onClicked: root.adapter.clearDirectFormatting()
        }
        ToolButton {
            text: qsTr("Scene break")
            enabled: root.adapter.focused
            onClicked: root.adapter.insertSceneBreak()
        }
        ToolButton {
            text: qsTr("Page break")
            enabled: root.adapter.focused
            onClicked: root.adapter.insertPageBreak()
        }
        ToolButton {
            text: qsTr("Undo")
            enabled: root.adapter.canUndo
            onClicked: root.adapter.undo()
        }
        ToolButton {
            text: qsTr("Redo")
            enabled: root.adapter.canRedo
            onClicked: root.adapter.redo()
        }
        ToolButton {
            text: qsTr("Source")
            Accessible.name: qsTr("Edit raw Markdown source")
            onClicked: root.sourceModeRequested()
        }
    }
}
