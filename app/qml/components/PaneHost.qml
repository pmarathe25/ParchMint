pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.parchmint.adapters 1.0

// One host is instantiated twice by Main.qml. It intentionally owns its
// TextArea for its whole lifetime: replacing the other pane cannot discard its
// Qt undo stack, cursor, or scroll position.
Pane {
    id: root
    objectName: "paneHost" + paneIndex
    required property var backend
    required property var model
    required property int paneIndex
    required property string nodeId
    required property string viewName
    required property bool pinned
    property bool focused: backend.focused_pane === paneIndex
    property string loadedNode: ""
    property int loadedRevision: 0
    property bool loadingBody: false
    property bool findVisible: false
    property string pendingExternalUrl: ""
    property bool sourceMode: false
    property string sourceBuffer: ""
    property string sourceError: ""
    readonly property var paragraphStyles: [
        { "id": "body", "name": qsTr("Body") },
        { "id": "heading-1", "name": qsTr("Heading 1") },
        { "id": "heading-2", "name": qsTr("Heading 2") }
    ]
    padding: 0

    function showFind() {
        findVisible = true
        findField.forceActiveFocus()
    }
    function localStatistics(value) {
        const words = value.trim().length ? value.trim().split(/\s+/).length : 0
        return qsTr("%1 words · %2 characters").arg(words).arg(value.length)
    }

    function reloadBody(force) {
        if (!sourceMode && viewName === "editor" && nodeId.length > 0
                && (force || loadedNode !== nodeId)) {
            const liveBody = backend.paneDocumentBody(paneIndex)
            const liveRevision = backend.paneDocumentRevision(paneIndex)
            if (loadedNode !== nodeId || editor.text !== liveBody) {
                loadingBody = true
                editor.text = liveBody
                loadingBody = false
            }
            loadedNode = nodeId
            loadedRevision = liveRevision
        } else if (!nodeId.length) {
            loadingBody = true
            editor.clear()
            loadingBody = false
            loadedNode = ""
            loadedRevision = 0
        }
    }
    function syncLiveBody() {
        if (!sourceMode && viewName === "editor" && loadedNode === nodeId && nodeId.length)
            return backend.updatePaneBody(paneIndex, editor.text, 0, Math.max(1, editor.lineCount))
        return true
    }
    function beginSource() {
        if (!syncLiveBody())
            return
        sourceBuffer = backend.paneDocumentBody(paneIndex)
        sourceError = backend.validateMarkdown(sourceBuffer)
        if (!sourceError.length && backend.beginSourceMode(sourceBuffer))
            sourceMode = true
    }
    function commitSource(source) {
        sourceError = backend.validateMarkdown(source)
        if (sourceError.length)
            return
        if (backend.commitSourceMode(source)) {
            loadingBody = true
            editor.text = source
            loadingBody = false
            sourceMode = false
            backend.updatePaneBody(paneIndex, source, 0, Math.max(1, editor.lineCount))
        }
    }
    function discardSource() {
        sourceMode = false
        reloadBody(true)
    }
    function findNext() {
        const query = findField.text
        if (!query.length)
            return
        const source = caseCheck.checked ? editor.text : editor.text.toLocaleLowerCase()
        const needle = caseCheck.checked ? query : query.toLocaleLowerCase()
        let start = editor.selectionEnd
        let found = source.indexOf(needle, start)
        if (found < 0 && start > 0)
            found = source.indexOf(needle)
        if (found >= 0) {
            editor.select(found, found + needle.length)
            editor.forceActiveFocus()
        }
    }
    function replaceSelection() {
        const selected = caseCheck.checked ? editor.selectedText : editor.selectedText.toLocaleLowerCase()
        const query = caseCheck.checked ? findField.text : findField.text.toLocaleLowerCase()
        if (selected.length && selected === query) {
            const start = editor.selectionStart
            editor.remove(editor.selectionStart, editor.selectionEnd)
            editor.insert(start, replaceField.text)
            editor.cursorPosition = start + replaceField.text.length
            findNext()
        } else {
            findNext()
        }
    }
    function replaceAll() {
        const query = findField.text
        if (!query.length)
            return
        let source = caseCheck.checked ? editor.text : editor.text.toLocaleLowerCase()
        const needle = caseCheck.checked ? query : query.toLocaleLowerCase()
        const positions = []
        let start = 0
        while (positions.length < 10000) {
            const found = source.indexOf(needle, start)
            if (found < 0)
                break
            positions.push(found)
            start = found + needle.length
        }
        for (let index = positions.length - 1; index >= 0; --index) {
            editor.remove(positions[index], positions[index] + query.length)
            editor.insert(positions[index], replaceField.text)
        }
        editor.cursorPosition = positions.length ? positions[0] + replaceField.text.length : editor.cursorPosition
    }
    onNodeIdChanged: reloadBody(false)
    onViewNameChanged: reloadBody(false)
    Component.onCompleted: {
        editorAdapter.defineStyle("body", {}, true, "body")
        editorAdapter.defineStyle("heading-1", { "font-weight": 700, "font-size": 24 }, true, "body")
        editorAdapter.defineStyle("heading-2", { "font-weight": 700, "font-size": 20 }, true, "body")
        reloadBody(false)
    }
    background: Rectangle {
        color: DesignTokens.surface
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        ToolBar {
            Layout.fillWidth: true
            RowLayout {
                anchors.fill: parent
                ToolButton { checkable: true; checked: root.pinned; Accessible.name: root.pinned ? qsTr("Unpin pane") : qsTr("Pin pane"); ToolTip.visible: hovered; ToolTip.text: Accessible.name; onClicked: root.backend.setPanePinned(root.paneIndex, checked); contentItem: Image { source: "qrc:/icons/pin.svg"; width: 18; height: 18; anchors.centerIn: parent } }
                Label { text: root.viewName === "attachment" ? qsTr("Attachment") : (root.backend.paneTitle(root.paneIndex).length ? root.backend.paneTitle(root.paneIndex) : qsTr("No document")); Layout.fillWidth: true; elide: Text.ElideRight; font.bold: true }
                ButtonGroup { id: viewGroup }
                ToolButton { text: qsTr("Editor"); checkable: true; checked: root.viewName === "editor"; ButtonGroup.group: viewGroup; onClicked: root.backend.setPaneView(root.paneIndex, "editor"); Accessible.name: qsTr("Show editor") }
                ToolButton { text: qsTr("Outline"); checkable: true; checked: root.viewName === "outline"; ButtonGroup.group: viewGroup; onClicked: root.backend.setPaneView(root.paneIndex, "outline"); Accessible.name: qsTr("Show outline") }
                ToolButton { text: qsTr("Cards"); checkable: true; checked: root.viewName === "cards"; ButtonGroup.group: viewGroup; onClicked: root.backend.setPaneView(root.paneIndex, "cards"); Accessible.name: qsTr("Show cards") }
                Label {
                    visible: root.viewName === "editor" && root.nodeId.length > 0
                    text: root.backend.document_revision >= 0 ? root.backend.paneSaveStatus(root.paneIndex) : ""
                    opacity: .7
                    Accessible.name: qsTr("Pane save status") + ": " + text
                }
                ToolButton { Accessible.name: qsTr("Find and replace in document"); ToolTip.visible: hovered; ToolTip.text: Accessible.name; onClicked: root.findVisible = !root.findVisible; contentItem: Image { source: "qrc:/icons/search.svg"; width: 18; height: 18; anchors.centerIn: parent } }
                ToolButton { Accessible.name: qsTr("Close pane"); ToolTip.visible: hovered; ToolTip.text: Accessible.name; onClicked: root.backend.closePane(root.paneIndex); contentItem: Image { source: "qrc:/icons/close.svg"; width: 18; height: 18; anchors.centerIn: parent } }
            }
        }
        RowLayout {
            visible: root.findVisible && root.viewName === "editor"
            Layout.fillWidth: true
            Layout.margins: 6
            TextField { id: findField; Layout.fillWidth: true; placeholderText: qsTr("Find"); onAccepted: root.findNext() }
            TextField { id: replaceField; Layout.preferredWidth: 120; placeholderText: qsTr("Replace") }
            CheckBox { id: caseCheck; text: qsTr("Case") }
            Button { text: qsTr("Next"); onClicked: root.findNext() }
            Button { text: qsTr("Replace"); onClicked: root.replaceSelection() }
            Button { text: qsTr("Replace all"); onClicked: root.replaceAll() }
        }
        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: root.viewName === "outline" ? 1 : root.viewName === "cards" ? 2 : root.viewName === "attachment" ? 3 : 0
            Item {
                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0
                    FormattingBar {
                        Layout.fillWidth: true
                        adapter: editorAdapter
                        visible: !root.sourceMode
                        styleModel: root.paragraphStyles
                        onSourceModeRequested: root.beginSource()
                    }
                    TextArea {
                    id: editor
                    objectName: "paneEditor" + root.paneIndex
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    // The live-session contract transports the exact Markdown
                    // source. EditorAdapter applies semantic QTextDocument
                    // formats directly; Qt's Markdown importer is never a
                    // persistence conversion step.
                    textFormat: TextEdit.PlainText
                    visible: !root.sourceMode
                    wrapMode: TextEdit.Wrap
                    Layout.maximumWidth: 880
                    Layout.alignment: Qt.AlignHCenter
                    leftPadding: 36
                    rightPadding: 36
                    topPadding: 28
                    bottomPadding: 56
                    font.pixelSize: 16
                    selectByMouse: true
                    persistentSelection: true
                    readOnly: root.backend.project_read_only
                    placeholderText: root.nodeId.length ? qsTr("Markdown research or manuscript note") : qsTr("Select a document")
                    Accessible.name: qsTr("Document editor")
                    onActiveFocusChanged: {
                        if (activeFocus)
                            root.backend.focusPane(root.paneIndex)
                        else if (root.loadedNode === root.nodeId && root.nodeId.length > 0)
                            root.backend.flushPane(root.paneIndex, text)
                    }
                    onTextChanged: {
                        liveCounts.text = root.localStatistics(selectedText.length ? selectedText : text)
                        if (!root.loadingBody && root.loadedNode === root.nodeId && root.nodeId.length > 0)
                            root.backend.updatePaneBody(root.paneIndex, text, 0, Math.max(1, lineCount))
                    }
                    onSelectedTextChanged: liveCounts.text = root.localStatistics(selectedText.length ? selectedText : text)
                    Keys.priority: Keys.BeforeItem
                    Keys.onPressed: function(event) {
                        if (event.key === Qt.Key_Backspace) {
                            editorAdapter.deletePreviousSemanticUnit()
                            event.accepted = true
                        } else if (event.key === Qt.Key_Delete) {
                            editorAdapter.deleteNextSemanticUnit()
                            event.accepted = true
                        }
                    }
                    }
                    SourceEditor {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        visible: root.sourceMode
                        text: root.sourceBuffer
                        valid: root.sourceError.length === 0
                        diagnostics: root.sourceError.length ? [{ start: 0, message: root.sourceError }] : []
                        onTextChanged: { root.sourceBuffer = text; root.sourceError = root.backend.validateMarkdown(text) }
                        onAcceptRequested: function(source) { root.commitSource(source) }
                        onDiscardRequested: root.discardSource()
                    }
                }
                EditorAdapter {
                    id: editorAdapter
                    textDocument: editor.textDocument
                    focused: editor.activeFocus
                    onAdapterError: function(message) { console.warn("ParchMint editor:", message) }
                    onIncrementalDirty: function(revision, position, removed, added, firstBlock, lastBlockExclusive) {
                        root.backend.noteEditorDelta(revision, firstBlock, lastBlockExclusive)
                    }
                }
                Connections {
                    target: editor
                    function onCursorPositionChanged() { editorAdapter.cursorPosition = editor.cursorPosition }
                    function onSelectionStartChanged() { editorAdapter.selectionStart = editor.selectionStart }
                    function onSelectionEndChanged() { editorAdapter.selectionEnd = editor.selectionEnd }
                }
                Label {
                    id: liveCounts
                    text: root.localStatistics(editor.selectedText.length ? editor.selectedText : editor.text)
                    Accessible.name: qsTr("Live document statistics") + ": " + text
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 8
                    z: 1
                    opacity: .75
                }
            }
            OutlineView { backend: root.backend; model: root.model }
            CardsView { backend: root.backend; model: root.model }
            ColumnLayout {
                spacing: 12
                Label { text: qsTr("Safe attachment preview"); font.bold: true; font.pixelSize: 20 }
                Label { text: root.backend.paneAttachmentDescription(root.paneIndex); wrapMode: Text.Wrap; Layout.fillWidth: true }
                Label { text: qsTr("Images, PDFs where the platform supports them, and plain text are previewed passively. Other files require an explicit system-open action."); wrapMode: Text.Wrap; Layout.fillWidth: true; opacity: .7 }
                Button {
                    text: qsTr("Open in system application…")
                    enabled: root.backend.paneAttachmentUrl(root.paneIndex).length > 0
                    onClicked: {
                        root.pendingExternalUrl = root.backend.paneAttachmentUrl(root.paneIndex)
                        externalOpenConfirm.open()
                    }
                }
            }
        }
    }

    Dialog {
        id: externalOpenConfirm
        anchors.centerIn: Overlay.overlay
        title: qsTr("Open attachment outside ParchMint?")
        modal: true
        standardButtons: Dialog.Open | Dialog.Cancel
        onAccepted: Qt.openUrlExternally(root.pendingExternalUrl)
        Label {
            width: 420
            wrapMode: Text.Wrap
            text: qsTr("The system application may execute or transmit content according to its own settings. Only continue if you trust this attachment.")
        }
    }
}
