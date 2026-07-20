pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

ComboBox {
    id: root
    required property var adapter
    textRole: "name"
    valueRole: "id"
    Accessible.name: qsTr("Paragraph style")
    displayText: currentIndex >= 0 ? currentText : qsTr("Missing style")
    Component.onCompleted: currentIndex = indexOfValue(adapter.paragraphStyle)
    Connections {
        target: root.adapter
        function onSelectionFormatChanged() {
            root.currentIndex = root.indexOfValue(root.adapter.paragraphStyle)
        }
    }
    delegate: ItemDelegate {
        required property var model
        width: ListView.view.width
        text: model.name
        font.family: model.fontFamily ?? ""
        font.pixelSize: model.previewSize ?? 14
        Accessible.name: text
    }
    onActivated: root.adapter.setParagraphStyle(currentValue)
}
