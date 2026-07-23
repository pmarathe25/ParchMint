pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ToolBar {
    id: root
    property var adapter: null
    property var styleModel: []
    property bool sourceMode: false
    signal sourceModeRequested()

    readonly property bool editable: adapter !== null && !sourceMode
    implicitHeight: 38
    topPadding: 0
    bottomPadding: 0

    contentItem: Flickable {
        id: scroller
        clip: true
        contentWidth: controls.implicitWidth
        contentHeight: height
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentWidth > width
        Accessible.name: qsTr("Text formatting controls")

        ScrollBar.horizontal: ScrollBar {
            policy: ScrollBar.AsNeeded
        }

        RowLayout {
            id: controls
            height: scroller.height
            spacing: 2

            ToolButton {
                text: qsTr("B")
                checkable: true
                checked: root.adapter !== null && root.adapter.boldState === 1
                opacity: root.adapter !== null && root.adapter.boldState === -1 ? 0.65 : 1
                enabled: root.editable
                Accessible.name: qsTr("Toggle bold")
                onClicked: root.adapter.toggleBold()
            }
            ToolButton {
                text: qsTr("I")
                checkable: true
                checked: root.adapter !== null && root.adapter.italicState === 1
                opacity: root.adapter !== null && root.adapter.italicState === -1 ? 0.65 : 1
                enabled: root.editable
                Accessible.name: qsTr("Toggle italic")
                onClicked: root.adapter.toggleItalic()
            }
            ToolButton {
                text: qsTr("U")
                checkable: true
                checked: root.adapter !== null && root.adapter.underline
                enabled: root.editable
                font.underline: true
                Accessible.name: qsTr("Toggle underline")
                onClicked: root.adapter.toggleUnderline()
            }
            ToolButton {
                text: qsTr("Sup")
                enabled: root.editable
                Accessible.name: qsTr("Superscript")
                onClicked: root.adapter.setVerticalAlignment(1)
            }
            ToolButton {
                text: qsTr("Sub")
                enabled: root.editable
                Accessible.name: qsTr("Subscript")
                onClicked: root.adapter.setVerticalAlignment(2)
            }
            ToolSeparator {}
            Loader {
                active: root.adapter !== null
                sourceComponent: Component {
                    StylePicker {
                        adapter: root.adapter
                        model: root.styleModel
                        enabled: root.editable
                    }
                }
            }
            ToolButton {
                text: qsTr("Clear formatting")
                enabled: root.editable
                Accessible.name: qsTr("Clear direct formatting")
                onClicked: root.adapter.clearDirectFormatting()
            }
            ToolButton {
                text: qsTr("Bulleted list")
                enabled: root.editable
                Accessible.name: qsTr("Toggle bulleted list")
                onClicked: root.adapter.toggleList(false)
            }
            ToolButton {
                text: qsTr("Numbered list")
                enabled: root.editable
                Accessible.name: qsTr("Toggle numbered list")
                onClicked: root.adapter.toggleList(true)
            }
            ToolButton {
                text: qsTr("Left")
                enabled: root.editable
                Accessible.name: qsTr("Align paragraph left")
                onClicked: root.adapter.setParagraphAlignment(Qt.AlignLeft)
            }
            ToolButton {
                text: qsTr("Center")
                enabled: root.editable
                Accessible.name: qsTr("Align paragraph center")
                onClicked: root.adapter.setParagraphAlignment(Qt.AlignHCenter)
            }
            ToolButton {
                text: qsTr("Right")
                enabled: root.editable
                Accessible.name: qsTr("Align paragraph right")
                onClicked: root.adapter.setParagraphAlignment(Qt.AlignRight)
            }
            ToolButton {
                text: qsTr("Link…")
                enabled: root.editable
                Accessible.name: qsTr("Set link destination")
                onClicked: linkDialog.open()
            }
            ToolButton {
                text: qsTr("Image…")
                enabled: root.editable
                Accessible.name: qsTr("Insert image")
                onClicked: imageDialog.open()
            }
            ToolButton {
                text: qsTr("Scene break")
                enabled: root.editable
                Accessible.name: qsTr("Insert scene break")
                onClicked: root.adapter.insertSceneBreak()
            }
            ToolButton {
                text: qsTr("Page break")
                enabled: root.editable
                Accessible.name: qsTr("Insert page break")
                onClicked: root.adapter.insertPageBreak()
            }
            ToolButton {
                text: qsTr("Undo")
                enabled: root.adapter !== null && root.adapter.canUndo && !root.sourceMode
                Accessible.name: qsTr("Undo editor change")
                onClicked: root.adapter.undo()
            }
            ToolButton {
                text: qsTr("Redo")
                enabled: root.adapter !== null && root.adapter.canRedo && !root.sourceMode
                Accessible.name: qsTr("Redo editor change")
                onClicked: root.adapter.redo()
            }
            ToolButton {
                text: qsTr("Source")
                enabled: root.editable
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
        onAccepted: {
            if (root.adapter)
                root.adapter.setLink(linkDestination.text.trim())
        }
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
        onAccepted: {
            if (root.adapter)
                root.adapter.insertImage(imageAsset.text.trim(), imageAlt.text)
        }
    }
}
