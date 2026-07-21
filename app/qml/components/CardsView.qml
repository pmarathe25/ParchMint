pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.parchmint.adapters 1.0

Pane {
    id: root
    required property var backend
    required property var model
    padding: DesignTokens.space4
    background: Rectangle { color: DesignTokens.surface }
    CardsModel { id: cardsModel; source: root.model }
    ColumnLayout {
        anchors.fill: parent
        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTr("Cards"); font.pixelSize: DesignTokens.typeTitle; font.bold: true; Layout.fillWidth: true; Accessible.role: Accessible.Heading }
            Label { text: qsTr("Ordered cards"); color: DesignTokens.textMuted }
        }
        GridView {
            Layout.fillWidth: true; Layout.fillHeight: true
            cellWidth: 250; cellHeight: 176
            clip: true; reuseItems: true; model: cardsModel
            delegate: ItemDelegate {
                required property string nodeId
                required property string title
                required property string synopsis
                required property string status
                required property string label
                width: 234; height: 160
                highlighted: root.backend.selected_id === nodeId
                Accessible.name: qsTr("Card %1").arg(title)
                contentItem: ColumnLayout {
                    spacing: DesignTokens.space2
                    Label { text: title; font.bold: true; Layout.fillWidth: true; elide: Text.ElideRight }
                    Label { text: synopsis.length ? synopsis : qsTr("No synopsis"); Layout.fillWidth: true; Layout.fillHeight: true; wrapMode: Text.Wrap; maximumLineCount: 4; elide: Text.ElideRight; color: DesignTokens.textMuted }
                    RowLayout {
                        Layout.fillWidth: true
                        Label { text: status; Layout.fillWidth: true; color: DesignTokens.textMuted }
                        Label { text: label; color: DesignTokens.textMuted }
                    }
                }
                onClicked: root.backend.selectNode(nodeId, false)
            }
        }
    }
}
