pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.parchmint.adapters 1.0

// Each host owns its TextArea for its whole lifetime so split layout changes
// cannot discard the Qt undo stack, cursor, or scroll position.
Pane {
    id: root
    objectName: "paneHost" + paneIndex
    required property var backend
    required property var model
    required property int paneIndex
    required property string nodeId
    required property string viewName
    required property bool pinned
    required property var splitRequestHandler
    signal closeRequested()
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
    property bool dropActive: false
    property string dropDirection: "center"
    property bool retiring: false
    readonly property var paragraphStyles: [
        { "id": "body", "name": qsTr("Body") },
        { "id": "heading-1", "name": qsTr("Heading 1") },
        { "id": "heading-2", "name": qsTr("Heading 2") }
    ]
    padding: 0

    function requestSplit(direction, nodeId) {
        return typeof splitRequestHandler === "function"
                && splitRequestHandler(direction, nodeId) === true
    }
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
    function dropDirectionAt(x, y) {
        const threshold = Math.min(72, Math.max(32, Math.min(width, height) * .2))
        const left = x
        const right = width - x
        const up = y
        const down = height - y
        const nearest = Math.min(left, right, up, down)
        if (nearest >= threshold)
            return "center"
        if (nearest === left)
            return "left"
        if (nearest === right)
            return "right"
        if (nearest === up)
            return "up"
        return "down"
    }

    function reloadBody(force) {
        if (!sourceMode && viewName !== "attachment" && nodeId.length > 0
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
        if (!sourceMode && viewName !== "attachment" && loadedNode === nodeId && nodeId.length)
            return backend.updatePaneBody(paneIndex, editor.text, 0, Math.max(1, editor.lineCount))
        return true
    }
    function prepareToClose() {
        editorAdapter.flushPendingChanges()
        return !sourceMode && syncLiveBody()
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
                ToolButton { Layout.preferredWidth: 32; Layout.preferredHeight: 32; text: "→"; Accessible.name: qsTr("Split editor right"); ToolTip.visible: hovered; ToolTip.text: Accessible.name; onClicked: root.requestSplit("right", "") }
                ToolButton { Layout.preferredWidth: 32; Layout.preferredHeight: 32; text: "↓"; Accessible.name: qsTr("Split editor down"); ToolTip.visible: hovered; ToolTip.text: Accessible.name; onClicked: root.requestSplit("down", "") }
                Label {
                    visible: root.width >= 420 && root.viewName !== "attachment" && root.nodeId.length > 0
                    text: root.backend.document_revision >= 0 ? root.backend.paneSaveStatus(root.paneIndex) : ""
                    opacity: .7
                    Accessible.name: qsTr("Pane save status") + ": " + text
                }
                ToolButton { Accessible.name: qsTr("Find and replace in document"); ToolTip.visible: hovered; ToolTip.text: Accessible.name; onClicked: root.findVisible = !root.findVisible; contentItem: Image { source: "qrc:/icons/search.svg"; width: 18; height: 18; anchors.centerIn: parent } }
                ToolButton { visible: root.backend.pane_count > 1; Accessible.name: qsTr("Close pane"); ToolTip.visible: hovered; ToolTip.text: Accessible.name; onClicked: root.closeRequested(); contentItem: Image { source: "qrc:/icons/close.svg"; width: 18; height: 18; anchors.centerIn: parent } }
            }
        }
        RowLayout {
            visible: root.findVisible && root.viewName !== "attachment"
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
            currentIndex: root.viewName === "attachment" ? 1 : 0
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
                    placeholderText: qsTr("Select a document")
                    Accessible.name: qsTr("Document editor")
                    onActiveFocusChanged: {
                        if (activeFocus)
                            root.backend.focusPane(root.paneIndex)
                        else if (!root.retiring && root.loadedNode === root.nodeId && root.nodeId.length > 0) {
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
            ColumnLayout {
                spacing: 12
                Label { text: qsTr("Attachment"); font.bold: true; font.pixelSize: DesignTokens.typeTitle }
                Label { text: root.backend.paneAttachmentDescription(root.paneIndex); wrapMode: Text.Wrap; Layout.fillWidth: true }
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

    Rectangle {
        id: dropAffordance
        z: 20
        visible: root.dropActive
        x: root.dropDirection === "right" ? root.width / 2
           : root.dropDirection === "center" ? 24 : 0
        y: root.dropDirection === "down" ? root.height / 2
           : root.dropDirection === "center" ? 24 : 0
        width: root.dropDirection === "left" || root.dropDirection === "right"
               ? root.width / 2
               : root.dropDirection === "center" ? Math.max(0, root.width - 48) : root.width
        height: root.dropDirection === "up" || root.dropDirection === "down"
                ? root.height / 2
                : root.dropDirection === "center" ? Math.max(0, root.height - 48) : root.height
        color: DesignTokens.accentContainer
        opacity: .72
        border.width: 2
        border.color: DesignTokens.accent
        radius: DesignTokens.radiusMedium
        Label {
            anchors.centerIn: parent
            text: root.dropDirection === "center" ? qsTr("Open here")
                  : root.dropDirection === "left" ? qsTr("Split left")
                  : root.dropDirection === "right" ? qsTr("Split right")
                  : root.dropDirection === "up" ? qsTr("Split up") : qsTr("Split down")
            font.bold: true
            color: DesignTokens.text
        }
    }

    DropArea {
        z: 21
        anchors.fill: parent
        keys: ["application/x-parchmint-node-id"]
        onEntered: function(drag) {
            root.dropActive = true
            root.dropDirection = root.dropDirectionAt(drag.x, drag.y)
        }
        onPositionChanged: function(drag) {
            root.dropDirection = root.dropDirectionAt(drag.x, drag.y)
        }
        onExited: {
            root.dropActive = false
            root.dropDirection = "center"
        }
        onDropped: function(drop) {
            const id = drop.getDataAsString("application/x-parchmint-node-id")
            if (id.length) {
                if (root.dropDirection === "center")
                    drop.accepted = root.backend.openNodeInPane(root.paneIndex, id)
                else {
                    drop.accepted = root.requestSplit(root.dropDirection, id)
                }
            }
            root.dropActive = false
            root.dropDirection = "center"
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
