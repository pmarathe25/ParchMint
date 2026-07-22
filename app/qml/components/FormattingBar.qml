pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ToolBar {
    id: root
    required property var adapter
    property var styleModel: []
    signal sourceModeRequested()

    contentItem: Flickable {
        id: scroller
        implicitHeight: controls.implicitHeight
                        + (horizontalScroll.visible ? horizontalScroll.implicitHeight : 0)
        clip: true
        contentWidth: controls.implicitWidth
        contentHeight: controls.implicitHeight
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentWidth > width
        ScrollBar.horizontal: ScrollBar {
            id: horizontalScroll
            policy: ScrollBar.AsNeeded
        }

        RowLayout {
            id: controls
            height: scroller.height - (horizontalScroll.visible ? horizontalScroll.implicitHeight : 0)
            ToolButton {
                text: qsTr("B")
                checkable: true
                checked: root.adapter.boldState === 1
                opacity: root.adapter.boldState === -1 ? 0.65 : 1
                enabled: root.adapter.focused
                Accessible.name: qsTr("Toggle bold")
                onClicked: root.adapter.toggleBold()
            }
            ToolButton {
                text: qsTr("I")
                checkable: true
                checked: root.adapter.italicState === 1
                opacity: root.adapter.italicState === -1 ? 0.65 : 1
                enabled: root.adapter.focused
                Accessible.name: qsTr("Toggle italic")
                onClicked: root.adapter.toggleItalic()
            }
            ToolButton {
                text: qsTr("U")
                checkable: true
                checked: root.adapter.underline
                enabled: root.adapter.focused
                font.underline: true
                Accessible.name: qsTr("Toggle underline")
                onClicked: root.adapter.toggleUnderline()
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
                text: qsTr("Bulleted list")
                enabled: root.adapter.focused
                Accessible.name: qsTr("Toggle bulleted list")
                onClicked: root.adapter.toggleList(false)
            }
            ToolButton {
                text: qsTr("Numbered list")
                enabled: root.adapter.focused
                Accessible.name: qsTr("Toggle numbered list")
                onClicked: root.adapter.toggleList(true)
            }
            ToolButton {
                text: qsTr("Left")
                enabled: root.adapter.focused
                Accessible.name: qsTr("Align paragraph left")
                onClicked: root.adapter.setParagraphAlignment(Qt.AlignLeft)
            }
            ToolButton {
                text: qsTr("Center")
                enabled: root.adapter.focused
                Accessible.name: qsTr("Align paragraph center")
                onClicked: root.adapter.setParagraphAlignment(Qt.AlignHCenter)
            }
            ToolButton {
                text: qsTr("Right")
                enabled: root.adapter.focused
                Accessible.name: qsTr("Align paragraph right")
                onClicked: root.adapter.setParagraphAlignment(Qt.AlignRight)
            }
            ToolButton {
                text: qsTr("Link…")
                enabled: root.adapter.focused
                Accessible.name: qsTr("Set link destination")
                onClicked: linkDialog.open()
            }
            ToolButton {
                text: qsTr("Image…")
                enabled: root.adapter.focused
                Accessible.name: qsTr("Insert image")
                onClicked: imageDialog.open()
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

    Dialog {
        id: linkDialog
        title: qsTr("Link destination")
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        TextField {
            id: linkDestination
            width: 360
            placeholderText: qsTr("https://, mailto:, or asset:")
            Accessible.name: qsTr("Link destination")
        }
        onAccepted: root.adapter.setLink(linkDestination.text.trim())
    }

    Dialog {
        id: imageDialog
        title: qsTr("Insert image")
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        ColumnLayout {
            width: 360
            TextField {
                id: imageAsset
                Layout.fillWidth: true
                placeholderText: qsTr("Asset identifier")
                Accessible.name: qsTr("Image asset identifier")
            }
            TextField {
                id: imageAlt
                Layout.fillWidth: true
                placeholderText: qsTr("Alternative text")
                Accessible.name: qsTr("Image alternative text")
            }
        }
        onAccepted: root.adapter.insertImage(imageAsset.text.trim(), imageAlt.text)
    }
}
