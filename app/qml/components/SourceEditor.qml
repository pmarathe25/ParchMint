pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.parchmint.adapters 1.0

Item {
    id: root
    property alias text: source.text
    property var diagnostics: []
    property bool valid: true
    signal acceptRequested(string source)
    signal discardRequested()

    function focusEditor() {
        source.forceActiveFocus()
    }

    ColumnLayout {
        anchors.fill: parent
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: root.valid ? qsTr("Markdown source") : qsTr("Source has errors — buffer retained")
                color: root.valid ? DesignTokens.text : DesignTokens.danger
                Layout.fillWidth: true
            }
            Button {
                text: qsTr("Return to editor")
                enabled: root.valid
                onClicked: root.acceptRequested(source.text)
            }
            Button {
                text: qsTr("Discard source changes")
                onClicked: root.discardRequested()
            }
        }
        TextArea {
            id: source
            Layout.fillWidth: true
            Layout.fillHeight: true
            textFormat: TextEdit.PlainText
            wrapMode: TextEdit.NoWrap
            selectByMouse: true
            persistentSelection: true
            font.family: "monospace"
            Accessible.name: qsTr("Raw Markdown source editor")
        }
        MarkdownHighlighter {
            textDocument: source.textDocument
        }
        ListView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, 120)
            model: root.diagnostics
            delegate: Label {
                required property var modelData
                width: ListView.view.width
                text: qsTr("Byte %1: %2").arg(modelData.start).arg(modelData.message)
                wrapMode: Text.Wrap
                Accessible.name: text
            }
        }
    }
}
