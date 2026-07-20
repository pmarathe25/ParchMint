pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: root
    required property var styleModel
    property string selectedStyleId: ""
    property string selectedName: ""
    property bool selectedBuiltin: true
    signal createStyle(string name, string kind)
    signal renameStyle(string styleId, string name)
    signal deleteStyle(string styleId, string replacementId)
    title: qsTr("Manage styles")
    modal: true
    standardButtons: Dialog.Close

    contentItem: RowLayout {
        ListView {
            id: styles
            Layout.preferredWidth: 240
            Layout.preferredHeight: 360
            model: root.styleModel
            delegate: ItemDelegate {
                required property int index
                required property string name
                required property string id
                required property bool builtin
                width: ListView.view.width
                text: name
                highlighted: ListView.isCurrentItem
                onClicked: {
                    styles.currentIndex = index
                    root.selectedStyleId = id
                    root.selectedName = name
                    root.selectedBuiltin = builtin
                }
            }
        }
        ColumnLayout {
            Layout.preferredWidth: 300
            Label { text: qsTr("Display name") }
            TextField {
                id: displayName
                text: root.selectedName
                Accessible.name: qsTr("Style display name")
            }
            Label {
                text: qsTr("Renaming does not rewrite documents; stable IDs remain unchanged.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Button {
                text: qsTr("Rename")
                enabled: root.selectedStyleId.length > 0 && displayName.text.trim().length > 0
                onClicked: root.renameStyle(root.selectedStyleId, displayName.text.trim())
            }
            Item { Layout.fillHeight: true }
            Button {
                text: qsTr("Delete and replace…")
                enabled: root.selectedStyleId.length > 0 && !root.selectedBuiltin
                onClicked: replacement.open()
            }
        }
    }

    Dialog {
        id: replacement
        title: qsTr("Replace deleted style")
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        ComboBox {
            id: replacementStyle
            model: root.styleModel
            textRole: "name"
            valueRole: "id"
            Accessible.name: qsTr("Replacement style")
        }
        onAccepted: root.deleteStyle(root.selectedStyleId, replacementStyle.currentValue)
    }
}
