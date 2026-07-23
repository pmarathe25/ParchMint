pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.parchmint.adapters 1.0

Pane {
    id: root
    objectName: "paneHost" + paneIndex
    required property var backend
    required property var model
    required property int paneIndex
    required property var splitRequestHandler
    signal closeRequested()
    property var tabs: []
    property int tabsRevision: 0
    property bool findVisible: false
    property bool retiring: false
    property bool backendSyncSuspended: false
    property bool dropActive: false
    property string dropDirection: "center"
    property int tabCount: 0
    property int activeTab: -1
    readonly property var activeHost: {
        const revision = tabsRevision
        for (let index = 0; index < tabs.length; ++index) {
            if (tabs[index].tabIndex === activeTab)
                return tabs[index]
        }
        return null
    }
    readonly property var activeAdapter: activeHost
                                         && activeHost.nodeId.length > 0
                                         && activeHost.viewName !== "attachment"
                                       ? activeHost.adapter : null
    readonly property bool activeSourceMode: activeHost ? activeHost.sourceMode : false
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
    function focusPane() {
        if (paneIndex >= 0)
            backend.focusPane(paneIndex)
    }
    function hostFor(nodeId) {
        for (let index = 0; index < tabs.length; ++index) {
            if (tabs[index].nodeId === nodeId)
                return tabs[index]
        }
        return null
    }
    function syncTabs() {
        if (retiring || paneIndex < 0)
            return
        const nextTabCount = backend.paneTabCount(paneIndex)
        const nextActiveTab = backend.paneActiveTab(paneIndex)
        const ids = []
        for (let index = 0; index < nextTabCount; ++index)
            ids.push(backend.paneTabId(paneIndex, index))
        const next = []
        for (let index = 0; index < tabs.length; ++index) {
            const host = tabs[index]
            if (ids.indexOf(host.nodeId) < 0) {
                host.retiring = true
                host.destroy()
            } else {
                next.push(host)
            }
        }
        for (let index = 0; index < ids.length; ++index) {
            let host = hostFor(ids[index])
            if (!host) {
                host = tabHost.createObject(content, { "nodeId": ids[index] })
                if (host)
                    next.push(host)
            }
            if (host) {
                host.tabIndex = index
                host.viewName = backend.paneTabView(paneIndex, index)
                host.reload(false)
            }
        }
        tabCount = nextTabCount
        activeTab = nextActiveTab
        tabs = next
        tabsRevision += 1
    }
    function reloadTabs() {
        for (let index = 0; index < tabs.length; ++index)
            tabs[index].reload(false)
    }
    function focusActiveEditor() {
        if (activeHost)
            activeHost.focusEditor()
    }
    function activate(tab) {
        if (backend.activatePaneTab(paneIndex, tab)) {
            focusPane()
            syncTabs()
            Qt.callLater(focusActiveEditor)
            return true
        }
        return false
    }
    function closeTab(tab) {
        let host = null
        for (let index = 0; index < tabs.length; ++index) {
            if (tabs[index].tabIndex === tab) {
                host = tabs[index]
                break
            }
        }
        if (host && !host.prepare())
            return false
        if (tabCount === 1 && backend.pane_count > 1) {
            closeRequested()
            return true
        }
        const closed = backend.closePaneTab(paneIndex, tab)
        if (closed) Qt.callLater(syncTabs)
        return closed
    }
    function showFind() {
        if (!activeHost || activeHost.viewName === "attachment")
            return
        focusPane()
        findVisible = true
        findField.forceActiveFocus()
    }
    function beginActiveSource() {
        if (activeHost)
            activeHost.beginSource()
    }
    function syncLiveBodies() {
        let succeeded = true
        for (let index = 0; index < tabs.length; ++index) {
            if (!tabs[index].sync())
                succeeded = false
        }
        return succeeded
    }
    function prepareToClose() {
        let succeeded = true
        for (let index = 0; index < tabs.length; ++index) {
            if (!tabs[index].prepare())
                succeeded = false
        }
        return succeeded
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
    Component.onCompleted: syncTabs()
    background: Rectangle {
        color: DesignTokens.surface
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        ToolBar {
            Layout.fillWidth: true
            Layout.preferredHeight: 38
            Layout.minimumHeight: 38
            Layout.maximumHeight: 38
            RowLayout {
                anchors.fill: parent
                spacing: 2
                Flickable {
                    id: tabScroller
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    contentWidth: tabRow.width
                    contentHeight: height
                    flickableDirection: Flickable.HorizontalFlick
                    boundsBehavior: Flickable.StopAtBounds
                    interactive: contentWidth > width
                    Accessible.name: qsTr("Editor tabs")
                    ScrollBar.horizontal: ScrollBar {
                        policy: ScrollBar.AsNeeded
                    }
                    Row {
                        id: tabRow
                        height: tabScroller.height
                        Repeater {
                            model: root.tabCount
                            delegate: Item {
                                required property int index
                                width: 170
                                height: tabRow.height
                                Button {
                                    id: tabButton
                                    anchors.fill: parent
                                    checkable: true
                                    checked: root.activeTab === index
                                    rightPadding: 30
                                    onClicked: root.activate(index)
                                    Accessible.name: qsTr("Open %1 tab").arg(
                                                         tabTitle.text)
                                    contentItem: Label {
                                        id: tabTitle
                                        text: {
                                            const revision = root.tabsRevision
                                            return root.backend.paneTabTitle(root.paneIndex,
                                                                             index)
                                        }
                                        color: tabButton.checked
                                               ? tabButton.palette.highlightedText
                                               : tabButton.palette.buttonText
                                        elide: Text.ElideRight
                                        verticalAlignment: Text.AlignVCenter
                                        leftPadding: 8
                                    }
                                    background: Rectangle {
                                        color: tabButton.checked
                                               ? tabButton.palette.highlight
                                               : tabButton.hovered
                                                 ? tabButton.palette.midlight
                                                 : "transparent"
                                        border.width: tabButton.checked ? 1 : 0
                                        border.color: tabButton.palette.highlight
                                        radius: DesignTokens.radiusSmall
                                    }
                                }
                                ToolButton {
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 28
                                    height: 28
                                    text: "×"
                                    Accessible.name: qsTr("Close %1 tab").arg(
                                                         tabTitle.text)
                                    ToolTip.visible: hovered
                                    ToolTip.text: Accessible.name
                                    onClicked: root.closeTab(index)
                                }
                            }
                        }
                        Label {
                            visible: root.tabCount === 0
                            width: visible ? 150 : 0
                            height: parent.height
                            text: qsTr("No document")
                            color: palette.windowText
                            opacity: .7
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                }
                Label {
                    visible: root.width >= 520 && root.activeHost !== null
                             && root.activeHost.viewName !== "attachment"
                             && root.activeHost.nodeId.length > 0
                    text: {
                        const revision = root.backend.document_revision
                        return root.activeHost
                                ? root.backend.tabSaveStatus(root.paneIndex,
                                                             root.activeHost.tabIndex)
                                : ""
                    }
                    opacity: .7
                    Accessible.name: qsTr("Tab save status") + ": " + text
                }
                ToolButton {
                    enabled: root.activeHost !== null
                             && root.activeHost.viewName !== "attachment"
                    Accessible.name: qsTr("Find and replace in document")
                    ToolTip.visible: hovered
                    ToolTip.text: Accessible.name
                    onClicked: {
                        root.focusPane()
                        root.findVisible = !root.findVisible
                        if (root.findVisible)
                            findField.forceActiveFocus()
                    }
                    contentItem: Image {
                        source: "qrc:/icons/search.svg"
                        width: 18
                        height: 18
                        anchors.centerIn: parent
                    }
                }
                ToolButton {
                    text: "→"
                    Accessible.name: qsTr("Split editor right")
                    ToolTip.visible: hovered
                    ToolTip.text: Accessible.name
                    onClicked: {
                        root.focusPane()
                        root.requestSplit("right", "")
                    }
                }
                ToolButton {
                    text: "↓"
                    Accessible.name: qsTr("Split editor down")
                    ToolTip.visible: hovered
                    ToolTip.text: Accessible.name
                    onClicked: {
                        root.focusPane()
                        root.requestSplit("down", "")
                    }
                }
            }
        }
        RowLayout {
            visible: root.findVisible && root.activeHost !== null
                     && root.activeHost.viewName !== "attachment"
            Layout.fillWidth: true
            Layout.margins: 6
            TextField {
                id: findField
                Layout.fillWidth: true
                placeholderText: qsTr("Find")
                Accessible.name: qsTr("Find text")
                onTextChanged: replaceStatus.text = ""
                onAccepted: root.activeHost.findNext()
            }
            TextField {
                id: replaceField
                Layout.preferredWidth: 120
                placeholderText: qsTr("Replace")
                Accessible.name: qsTr("Replacement text")
            }
            CheckBox {
                id: caseCheck
                text: qsTr("Case")
                Accessible.name: qsTr("Match case")
            }
            Button {
                text: qsTr("Next")
                onClicked: root.activeHost.findNext()
            }
            Button {
                text: qsTr("Replace")
                onClicked: root.activeHost.replaceSelection()
            }
            Button {
                text: qsTr("Replace all")
                onClicked: root.activeHost.replaceAll()
            }
            Label {
                id: replaceStatus
                opacity: .7
                Accessible.name: text
            }
        }
        Item {
            id: content
            Layout.fillWidth: true
            Layout.fillHeight: true
        }
    }

    Component {
        id: tabHost
        Item {
            id: tab
            required property string nodeId
            property int tabIndex: -1
            property string viewName: ""
            property string loadedNode: ""
            property double loadedRevision: 0
            property bool loadingBody: false
            property bool applyingLocalDelta: false
            property bool sourceMode: false
            property double sourceBaseRevision: 0
            property string pendingExternalUrl: ""
            property string sourceBuffer: ""
            property string sourceError: ""
            property bool retiring: false
            property int liveWords: 0
            property int liveCharacters: 0
            readonly property bool active: root.activeTab === tabIndex
            readonly property var adapter: editorAdapter

            anchors.fill: parent
            visible: active

            function focusEditor() {
                if (sourceMode)
                    sourceEditor.focusEditor()
                else if (viewName !== "attachment")
                    editor.forceActiveFocus()
            }

            function refreshStatistics() {
                if (tabIndex >= 0 && nodeId.length > 0 && viewName !== "attachment") {
                    liveWords = root.backend.tabWordCount(root.paneIndex, tabIndex)
                    liveCharacters = root.backend.tabCharacterCount(root.paneIndex,
                                                                    tabIndex)
                } else {
                    liveWords = 0
                    liveCharacters = 0
                }
            }
            function statisticsText(selection) {
                if (selection.length) {
                    const words = selection.trim().length
                                ? selection.trim().split(/\s+/).length : 0
                    return qsTr("%1 words · %2 characters").arg(words).arg(
                                selection.length)
                }
                return qsTr("%1 words · %2 characters").arg(liveWords).arg(
                            liveCharacters)
            }
            function reload(force) {
                if (sourceMode || viewName === "attachment" || tabIndex < 0)
                    return
                if (!nodeId.length) {
                    if (!editor.activeFocus) {
                        loadingBody = true
                        editorAdapter.loadPlainText("")
                        loadingBody = false
                    }
                    loadedNode = ""
                    loadedRevision = 0
                    refreshStatistics()
                    return
                }
                const liveRevision = root.backend.tabDocumentRevision(root.paneIndex,
                                                                       tabIndex)
                if (!force && loadedNode === nodeId
                        && loadedRevision === liveRevision)
                    return
                if (!force && applyingLocalDelta && loadedNode === nodeId) {
                    loadedRevision = liveRevision
                    refreshStatistics()
                    return
                }
                const liveBody = root.backend.tabDocumentBody(root.paneIndex,
                                                               tabIndex)
                const needsBody = force || loadedNode !== nodeId
                                || loadedRevision !== liveRevision
                                || editor.text !== liveBody
                if (needsBody) {
                    loadingBody = true
                    editorAdapter.loadPlainText(liveBody)
                    loadingBody = false
                }
                loadedNode = nodeId
                loadedRevision = liveRevision
                refreshStatistics()
            }
            function sync() {
                if (sourceMode)
                    return false
                if (viewName === "attachment" || !nodeId.length || tabIndex < 0)
                    return true
                if (loadedNode !== nodeId)
                    return false
                return root.backend.updateTabBody(root.paneIndex, tabIndex,
                                                  editor.text, 0,
                                                  Math.max(1, editor.lineCount))
            }
            function prepare() {
                editorAdapter.flushPendingChanges()
                if (!sync())
                    return false
                if (viewName === "attachment" || !nodeId.length || tabIndex < 0)
                    return true
                return root.backend.flushTab(root.paneIndex, tabIndex, editor.text)
            }
            function beginSource() {
                if (!root.activate(tabIndex) || !sync())
                    return
                sourceBuffer = root.backend.tabDocumentBody(root.paneIndex, tabIndex)
                sourceError = root.backend.validateMarkdown(sourceBuffer)
                if (!sourceError.length) {
                    sourceBaseRevision = root.backend.tabDocumentRevision(root.paneIndex,
                                                                           tabIndex)
                    sourceMode = true
                }
            }
            function commitSource(value) {
                sourceError = root.backend.validateMarkdown(value)
                if (sourceError.length || !root.activate(tabIndex))
                    return
                if (root.backend.tabDocumentRevision(root.paneIndex, tabIndex)
                        !== sourceBaseRevision) {
                    sourceError = qsTr("The document changed in another pane; discard or copy these source edits and reopen source mode.")
                    return
                }
                const sourceLines = Math.max(1, value.split("\n").length)
                if (root.backend.updateTabBody(root.paneIndex, tabIndex, value,
                                               0, sourceLines)) {
                    loadingBody = true
                    editorAdapter.loadPlainText(value)
                    loadingBody = false
                    sourceMode = false
                    reload(false)
                    Qt.callLater(focusEditor)
                }
            }
            function discardSource() {
                sourceMode = false
                reload(true)
                Qt.callLater(focusEditor)
            }
            function escapeRegExp(text) {
                const special = "\\^$.*+?()[]{}|"
                let escaped = ""
                for (let index = 0; index < text.length; ++index) {
                    const value = text.charAt(index)
                    if (special.indexOf(value) >= 0)
                        escaped += "\\"
                    escaped += value
                }
                return escaped
            }
            function findRegExp(query) {
                return new RegExp(escapeRegExp(query),
                                  caseCheck.checked ? "gu" : "giu")
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
                    editor.remove(positions[index][0],
                                  positions[index][0] + positions[index][1])
                    editor.insert(positions[index][0], replaceField.text)
                }
                if (positions.length)
                    editor.cursorPosition = positions[0][0]
                                          + replaceField.text.length
                replaceStatus.text = truncated
                        ? qsTr("Replaced the first %1 matches; more remain").arg(
                              positions.length)
                        : qsTr("Replaced %1 matches").arg(positions.length)
            }
            onTabIndexChanged: reload(true)
            onViewNameChanged: reload(true)
            onActiveChanged: {
                if (active)
                    reload(false)
            }
            Component.onCompleted: {
                editorAdapter.defineStyle("body", {}, true, "body")
                editorAdapter.defineStyle("heading-1",
                                          { "font-weight": 700, "font-size": 24 },
                                          true, "body")
                editorAdapter.defineStyle("heading-2",
                                          { "font-weight": 700, "font-size": 20 },
                                          true, "body")
                reload(false)
            }
            StackLayout {
                anchors.fill: parent
                currentIndex: tab.viewName === "attachment" ? 1 : 0
                Item {
                    TextArea {
                        id: editor
                        objectName: tab.active
                                  ? "paneEditor" + root.paneIndex
                                  : "paneEditor" + root.paneIndex + "_" + tab.nodeId
                        anchors.top: parent.top
                        anchors.bottom: parent.bottom
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Math.min(parent.width, 880)
                        textFormat: TextEdit.PlainText
                        visible: !tab.sourceMode
                        wrapMode: TextEdit.Wrap
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
                            if (activeFocus) {
                                root.activate(tab.tabIndex)
                            } else if (!root.retiring && !tab.retiring
                                       && tab.loadedNode === tab.nodeId
                                       && tab.nodeId.length > 0
                                       && tab.tabIndex >= 0) {
                                editorAdapter.flushPendingChanges()
                                root.backend.flushTab(root.paneIndex, tab.tabIndex,
                                                      text)
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
                        id: sourceEditor
                        anchors.fill: parent
                        visible: tab.sourceMode
                        text: tab.sourceBuffer
                        valid: !tab.sourceError.length
                        diagnostics: tab.sourceError.length
                                     ? [{ "start": 0, "message": tab.sourceError }]
                                     : []
                        onTextChanged: {
                            tab.sourceBuffer = text
                            tab.sourceError = root.backend.validateMarkdown(text)
                        }
                        onAcceptRequested: function(value) {
                            tab.commitSource(value)
                        }
                        onDiscardRequested: tab.discardSource()
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.margins: DesignTokens.space4
                    spacing: 12
                    Label {
                        text: qsTr("Attachment")
                        font.bold: true
                        font.pixelSize: DesignTokens.typeTitle
                    }
                    Label {
                        text: root.backend.tabAttachmentDescription(root.paneIndex,
                                                                    tab.tabIndex)
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                        Accessible.name: qsTr("Attachment description") + ": " + text
                    }
                    Button {
                        text: qsTr("Open in system application…")
                        enabled: root.backend.tabAttachmentUrl(root.paneIndex,
                                                               tab.tabIndex).length > 0
                        Accessible.name: text
                        onClicked: {
                            tab.pendingExternalUrl =
                                    root.backend.tabAttachmentUrl(root.paneIndex,
                                                                  tab.tabIndex)
                            externalOpenConfirm.open()
                        }
                    }
                    Item {
                        Layout.fillHeight: true
                    }
                }
            }
            Dialog {
                id: externalOpenConfirm
                anchors.centerIn: Overlay.overlay
                title: qsTr("Open attachment outside ParchMint?")
                modal: true
                standardButtons: Dialog.Open | Dialog.Cancel
                onAccepted: Qt.openUrlExternally(tab.pendingExternalUrl)
                Label {
                    width: 420
                    wrapMode: Text.Wrap
                    text: qsTr("The system application may execute or transmit content according to its own settings. Only continue if you trust this attachment.")
                }
            }
            EditorAdapter {
                id: editorAdapter
                textDocument: editor.textDocument
                focused: tab.active && root.backend.focused_pane === root.paneIndex
                onAdapterError: function(message) {
                    console.warn("ParchMint editor:", message)
                }
                onIncrementalDirty: function(revision, position, removed, added,
                                             insertedText, firstBlock,
                                             lastBlockExclusive) {
                    if (root.retiring || tab.retiring || tab.loadingBody
                            || tab.loadedNode !== tab.nodeId
                            || !tab.nodeId.length || tab.tabIndex < 0)
                        return
                    tab.applyingLocalDelta = true
                    const applied = root.backend.applyTabTextDelta(
                                root.paneIndex, tab.tabIndex, position, removed,
                                insertedText, firstBlock)
                    tab.applyingLocalDelta = false
                    if (applied) {
                        tab.loadedRevision = root.backend.tabDocumentRevision(
                                    root.paneIndex, tab.tabIndex)
                        tab.refreshStatistics()
                    } else {
                        resyncNotice.visible = true
                        resyncNoticeTimer.restart()
                        tab.reload(true)
                    }
                }
            }
            Connections {
                target: editor
                function onCursorPositionChanged() {
                    editorAdapter.cursorPosition = editor.cursorPosition
                }
                function onSelectionStartChanged() {
                    editorAdapter.selectionStart = editor.selectionStart
                }
                function onSelectionEndChanged() {
                    editorAdapter.selectionEnd = editor.selectionEnd
                }
            }
            Label {
                visible: tab.viewName !== "attachment" && tab.nodeId.length > 0
                text: tab.statisticsText(editor.selectedText)
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
                background: Rectangle {
                    color: DesignTokens.overlay
                    radius: DesignTokens.radiusSmall
                }
                Timer {
                    id: resyncNoticeTimer
                    interval: 4000
                    onTriggered: resyncNotice.visible = false
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
               : root.dropDirection === "center" ? Math.max(0, root.width - 48)
                                                 : root.width
        height: root.dropDirection === "up" || root.dropDirection === "down"
                ? root.height / 2
                : root.dropDirection === "center" ? Math.max(0, root.height - 48)
                                                  : root.height
        color: DesignTokens.accentContainer
        opacity: .72
        border.width: 2
        border.color: DesignTokens.accent
        radius: DesignTokens.radiusMedium
        Label {
            anchors.centerIn: parent
            text: root.dropDirection === "center" ? qsTr("Open in new tab")
                  : root.dropDirection === "left" ? qsTr("Split left")
                  : root.dropDirection === "right" ? qsTr("Split right")
                  : root.dropDirection === "up" ? qsTr("Split up")
                                               : qsTr("Split down")
            font.bold: true
            color: DesignTokens.text
        }
    }
    DropArea {
        anchors.fill: parent
        z: 21
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
            const nodeId =
                    drop.getDataAsString("application/x-parchmint-node-id")
            if (nodeId.length) {
                drop.accepted = root.dropDirection === "center"
                        ? root.backend.openNodeInPane(root.paneIndex, nodeId)
                        : root.requestSplit(root.dropDirection, nodeId)
            }
            root.dropActive = false
            root.dropDirection = "center"
        }
    }
    Connections {
        target: root.backend
        function onPanes_revisionChanged() {
            if (!root.backendSyncSuspended)
                root.syncTabs()
        }
        function onDocument_revisionChanged() {
            if (!root.backendSyncSuspended)
                root.reloadTabs()
        }
        function onExternal_conflictChanged() {
            if (!root.backendSyncSuspended && !root.backend.external_conflict)
                Qt.callLater(root.reloadTabs)
        }
    }
}
