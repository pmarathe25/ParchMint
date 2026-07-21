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
    property int liveWords: 0
    property int liveCharacters: 0
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
    function refreshStatistics() {
        if (nodeId.length > 0) {
            liveWords = backend.paneWordCount(paneIndex)
            liveCharacters = backend.paneCharacterCount(paneIndex)
        } else {
            liveWords = 0
            liveCharacters = 0
        }
    }
    function statisticsText(selection) {
        if (selection.length) {
            const words = selection.trim().length ? selection.trim().split(/\s+/).length : 0
            return qsTr("%1 words · %2 characters").arg(words).arg(selection.length)
        }
        return qsTr("%1 words · %2 characters").arg(liveWords).arg(liveCharacters)
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
            refreshStatistics()
        } else if (!nodeId.length) {
            loadingBody = true
            editor.clear()
            loadingBody = false
            loadedNode = ""
            loadedRevision = 0
            refreshStatistics()
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
    // Case-insensitive matching must never lowercase the whole document:
    // Unicode case folding can change string length (for example İ folds to
    // two code units), which shifts every later match offset and would
    // corrupt selections and replacements. A RegExp with the Unicode flag
    // matches case-insensitively while reporting UTF-16 offsets in the
    // original string.
    function escapeRegExp(text) {
        return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
    }
    function findRegExp(query) {
        return new RegExp(escapeRegExp(query), caseCheck.checked ? "gu" : "giu")
    }
    function queryMatches(text) {
        return text.length > 0
                && new RegExp("^(?:" + escapeRegExp(findField.text) + ")$",
                              caseCheck.checked ? "u" : "iu").test(text)
    }
    function findNext() {
        const query = findField.text
        if (!query.length)
            return
        const matcher = findRegExp(query)
        matcher.lastIndex = editor.selectionEnd
        let match = matcher.exec(editor.text)
        if (!match && editor.selectionEnd > 0) {
            matcher.lastIndex = 0
            match = matcher.exec(editor.text)
        }
        if (match) {
            editor.select(match.index, match.index + match[0].length)
            editor.forceActiveFocus()
        }
    }
    function replaceSelection() {
        if (queryMatches(editor.selectedText)) {
            const start = editor.selectionStart
            editor.remove(editor.selectionStart, editor.selectionEnd)
            editor.insert(start, replaceField.text)
            editor.cursorPosition = start + replaceField.text.length
            findNext()
        } else {
            findNext()
        }
    }
    readonly property int replaceAllLimit: 10000
    function replaceAll() {
        const query = findField.text
        if (!query.length)
            return
        const matcher = findRegExp(query)
        const positions = []
        let truncated = false
        let match
        while ((match = matcher.exec(editor.text)) !== null) {
            if (positions.length >= replaceAllLimit) {
                truncated = true
                break
            }
            positions.push([match.index, match[0].length])
            if (match[0].length === 0)
                matcher.lastIndex += 1
        }
        for (let index = positions.length - 1; index >= 0; --index) {
            editor.remove(positions[index][0], positions[index][0] + positions[index][1])
            editor.insert(positions[index][0], replaceField.text)
        }
        editor.cursorPosition = positions.length ? positions[0][0] + replaceField.text.length : editor.cursorPosition
        replaceStatus.text = truncated
            ? qsTr("Replaced the first %1 matches; more remain").arg(positions.length)
            : qsTr("Replaced %1 matches").arg(positions.length)
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
            Label { id: replaceStatus; opacity: .7 }
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
                        else if (root.loadedNode === root.nodeId && root.nodeId.length > 0) {
                            editorAdapter.flushPendingChanges()
                            root.backend.flushPane(root.paneIndex, text)
                        }
                    }
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
                    onIncrementalDirty: function(revision, position, removed, added, insertedText, firstBlock, lastBlockExclusive) {
                        if (root.loadingBody || root.loadedNode !== root.nodeId || root.nodeId.length === 0)
                            return
                        if (root.backend.applyPaneTextDelta(root.paneIndex, position, removed,
                                                            insertedText, firstBlock,
                                                            lastBlockExclusive)) {
                            root.refreshStatistics()
                        } else {
                            // The workspace rejected the delta; the editor and
                            // the live document would otherwise drift apart.
                            // Force a full-body resync and show one non-modal
                            // notice instead of a popup per keystroke.
                            resyncNotice.visible = true
                            resyncNoticeTimer.restart()
                            root.reloadBody(true)
                        }
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
                    text: root.statisticsText(editor.selectedText)
                    Accessible.name: qsTr("Live document statistics") + ": " + text
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 8
                    z: 1
                    opacity: .75
                }
                Label {
                    id: resyncNotice
                    visible: false
                    text: qsTr("Editor resynchronized after a sync error")
                    Accessible.name: text
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.margins: 8
                    z: 1
                    padding: 6
                    background: Rectangle { color: DesignTokens.overlay; radius: DesignTokens.radiusSmall }
                    Timer {
                        id: resyncNoticeTimer
                        interval: 4000
                        onTriggered: resyncNotice.visible = false
                    }
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
