pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtCore

Item {
    id: root
    required property var backend
    required property var model

    property var splitTree: ({ "kind": "leaf", "pane": 0 })
    property var paneRects: []
    property var dividerRects: []
    property var paneHosts: []
    property int layoutRevision: 0
    property bool changingPanes: false
    property string layoutProjectKey: ""
    property int layoutPaneCount: 0
    readonly property var focusedHost: {
        const pane = backend.focused_pane
        return pane >= 0 && pane < paneHosts.length ? paneHosts[pane] : null
    }
    readonly property int dividerSize: 5
    readonly property int minimumPaneSize: 220

    Settings {
        id: layoutSettings
        category: "editor-split-layouts"
        property string projectLayoutsJson: "{}"
    }
    FormattingBar {
        id: formattingBar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        z: 40
        adapter: root.focusedHost ? root.focusedHost.activeAdapter : null
        styleModel: root.focusedHost ? root.focusedHost.paragraphStyles : []
        sourceMode: root.focusedHost ? root.focusedHost.activeSourceMode : false
        onSourceModeRequested: { if (root.focusedHost) root.focusedHost.beginActiveSource() }
        onHeightChanged: root.updateGeometry()
    }


    Component {
        id: paneHostComponent
        PaneHost {
            backend: root.backend
            model: root.model
            backendSyncSuspended: root.changingPanes
            splitRequestHandler: function(direction, nodeId) {
                return root.splitPane(paneIndex, direction, nodeId)
            }
            onCloseRequested: root.closePane(paneIndex)
        }
    }

    function leaf(pane) {
        return { "kind": "leaf", "pane": pane }
    }

    function branch(axis, first, second, ratio) {
        return {
            "kind": "branch",
            "axis": axis,
            "first": first,
            "second": second,
            "ratio": ratio
        }
    }

    function defaultTree(count) {
        let tree = leaf(0)
        for (let pane = 1; pane < Math.max(1, count); ++pane)
            tree = branch("horizontal", tree, leaf(pane), .67)
        return tree
    }

    function validateSplitTree(candidate, count) {
        if (!candidate || count < 1)
            return false
        const pending = [candidate]
        const seen = ({})
        let visited = 0
        const visitLimit = count * 2 + 1
        while (pending.length) {
            const node = pending.pop()
            visited += 1
            if (!node || visited > visitLimit)
                return false
            if (node.kind === "leaf") {
                const pane = node.pane
                if (typeof pane !== "number" || Math.floor(pane) !== pane
                        || pane < 0 || pane >= count || seen[pane])
                    return false
                seen[pane] = true
            } else if (node.kind === "branch") {
                if ((node.axis !== "horizontal" && node.axis !== "vertical")
                        || typeof node.ratio !== "number" || !isFinite(node.ratio)
                        || node.ratio <= 0 || node.ratio >= 1
                        || !node.first || !node.second)
                    return false
                pending.push(node.second)
                pending.push(node.first)
            } else {
                return false
            }
        }
        if (visited !== count * 2 - 1)
            return false
        for (let pane = 0; pane < count; ++pane) {
            if (!seen[pane])
                return false
        }
        return true
    }

    function savedLayouts() {
        try {
            const value = JSON.parse(layoutSettings.projectLayoutsJson)
            return value && typeof value === "object" ? value : ({})
        } catch (error) {
            return ({})
        }
    }

    function saveLayoutForKey(key) {
        const count = layoutPaneCount
        if (!key.length || count < 1 || !validateSplitTree(splitTree, count))
            return
        const layouts = savedLayouts()
        layouts[key] = splitTree
        layoutSettings.projectLayoutsJson = JSON.stringify(layouts)
    }

    function saveLayout() {
        saveLayoutForKey(layoutProjectKey)
    }

    function restoreLayout() {
        const count = Math.max(1, backend.pane_count)
        layoutPaneCount = count
        const candidate = layoutProjectKey.length ? savedLayouts()[layoutProjectKey] : null
        splitTree = validateSplitTree(candidate, count) ? candidate : defaultTree(count)
        updateGeometry()
    }

    function resetLayout() {
        layoutPaneCount = Math.max(1, backend.pane_count)
        splitTree = defaultTree(layoutPaneCount)
        updateGeometry()
    }

    function collectPanes(node, values) {
        if (!node)
            return
        if (node.kind === "leaf") {
            values.push(node.pane)
            return
        }
        collectPanes(node.first, values)
        collectPanes(node.second, values)
    }

    function containsPane(node, pane) {
        if (!node)
            return false
        if (node.kind === "leaf")
            return node.pane === pane
        return containsPane(node.first, pane) || containsPane(node.second, pane)
    }

    function replaceLeaf(node, pane, replacement) {
        if (node.kind === "leaf")
            return node.pane === pane ? replacement : node
        return branch(node.axis,
                      replaceLeaf(node.first, pane, replacement),
                      replaceLeaf(node.second, pane, replacement),
                      node.ratio)
    }

    function removeLeaf(node, pane) {
        if (!node)
            return null
        if (node.kind === "leaf")
            return node.pane === pane ? null : node
        const first = removeLeaf(node.first, pane)
        const second = removeLeaf(node.second, pane)
        if (!first)
            return second
        if (!second)
            return first
        return branch(node.axis, first, second, node.ratio)
    }

    function reindexAfterRemoval(node, removedPane) {
        if (node.kind === "leaf")
            return leaf(node.pane > removedPane ? node.pane - 1 : node.pane)
        return branch(node.axis,
                      reindexAfterRemoval(node.first, removedPane),
                      reindexAfterRemoval(node.second, removedPane),
                      node.ratio)
    }

    function createPaneHost(pane) {
        const item = paneHostComponent.createObject(root, {
            "paneIndex": pane
        })
        if (!item)
            console.warn("ParchMint editor: pane host creation failed")
        return item
    }

    function retirePaneHost(item) {
        if (!item)
            return
        item.retiring = true
        item.visible = false
        item.paneIndex = -1
        item.destroy()
    }

    function destroyPaneHosts() {
        const existing = paneHosts
        paneHosts = []
        for (let index = 0; index < existing.length; ++index)
            retirePaneHost(existing[index])
    }

    function reconcilePaneHosts() {
        if (changingPanes)
            return
        const count = backend.project_open ? backend.pane_count : 0
        const next = paneHosts.slice()
        while (next.length > count)
            retirePaneHost(next.pop())
        while (next.length < count) {
            const item = createPaneHost(next.length)
            if (!item)
                break
            next.push(item)
        }
        paneHosts = next
        syncPaneStates()
        updatePaneGeometry()
    }

    function syncPaneStates() {
        if (changingPanes)
            return
        const count = Math.min(backend.pane_count, paneHosts.length)
        for (let pane = 0; pane < count; ++pane) {
            const item = paneHosts[pane]
            item.paneIndex = pane
            item.syncTabs()
        }
    }

    function removePaneHost(pane) {
        const next = paneHosts.slice()
        const removed = next.splice(pane, 1)[0]
        retirePaneHost(removed)
        paneHosts = next
        for (let index = pane; index < paneHosts.length; ++index)
            paneHosts[index].paneIndex = index
    }

    function reconcileLayout() {
        if (changingPanes)
            return
        reconcilePaneHosts()
        const count = Math.max(1, backend.pane_count)
        layoutPaneCount = count
        if (!validateSplitTree(splitTree, count))
            splitTree = defaultTree(count)
        updateGeometry()
    }

    function canSplitPane(pane, direction) {
        const geometry = paneRect(pane)
        const horizontal = direction === "left" || direction === "right"
        const dimension = horizontal ? geometry.width : geometry.height
        return dimension >= minimumPaneSize * 2
    }

    function splitPane(pane, direction, nodeId) {
        if (!containsPane(splitTree, pane) || !canSplitPane(pane, direction))
            return false
        changingPanes = true
        const oldCount = backend.pane_count
        const added = backend.addPane(nodeId)
        let succeeded = false
        if (added === oldCount) {
            const axis = direction === "left" || direction === "right"
                       ? "horizontal" : "vertical"
            const oldLeaf = leaf(pane)
            const newLeaf = leaf(added)
            const replacement = direction === "left" || direction === "up"
                              ? branch(axis, newLeaf, oldLeaf, .5)
                              : branch(axis, oldLeaf, newLeaf, .5)
            splitTree = replaceLeaf(splitTree, pane, replacement)
            succeeded = true
        }
        changingPanes = false
        reconcilePaneHosts()
        syncPaneStates()
        updateGeometry()
        if (succeeded) {
            layoutPaneCount = backend.pane_count
            saveLayout()
        }
        return succeeded
    }

    function splitFocused(direction) {
        return splitPane(backend.focused_pane, direction, "")
    }

    function closePane(pane) {
        if (backend.pane_count <= 1)
            return false
        const closingHost = paneHosts[pane]
        if (closingHost && !closingHost.prepareToClose())
            return false
        if (closingHost)
            closingHost.retiring = true
        changingPanes = true
        const removed = backend.removePane(pane)
        if (removed) {
            removePaneHost(pane)
            const remaining = removeLeaf(splitTree, pane)
            splitTree = reindexAfterRemoval(remaining || leaf(0), pane)
        } else if (closingHost) {
            closingHost.retiring = false
        }
        changingPanes = false
        syncPaneStates()
        updateGeometry()
        if (removed) {
            layoutPaneCount = backend.pane_count
            saveLayout()
        }
        return removed
    }

    function showFindFocused() {
        const item = paneHosts[backend.focused_pane]
        if (item)
            item.showFind()
    }

    function syncLiveBodies() {
        let succeeded = true
        for (let pane = 0; pane < paneHosts.length; ++pane) {
            const item = paneHosts[pane]
            if (item && !item.syncLiveBodies())
                succeeded = false
        }
        return succeeded
    }

    function setRatio(node, path, position, ratio) {
        if (position === path.length)
            return branch(node.axis, node.first, node.second, ratio)
        if (path.charAt(position) === "0")
            return branch(node.axis,
                          setRatio(node.first, path, position + 1, ratio),
                          node.second,
                          node.ratio)
        return branch(node.axis,
                      node.first,
                      setRatio(node.second, path, position + 1, ratio),
                      node.ratio)
    }

    function dividerForPath(path) {
        for (let index = 0; index < dividerRects.length; ++index) {
            if (dividerRects[index].path === path)
                return dividerRects[index]
        }
        return null
    }

    function resizeDivider(path, ratio) {
        const divider = dividerForPath(path)
        if (!divider)
            return
        const dimension = divider.axis === "horizontal"
                        ? divider.branchWidth : divider.branchHeight
        const minimumRatio = dimension > 0
                           ? Math.min(.5, minimumPaneSize / dimension) : .5
        const bounded = Math.max(minimumRatio, Math.min(1 - minimumRatio, ratio))
        splitTree = setRatio(splitTree, path, 0, bounded)
        updateGeometry()
    }

    function effectiveRatio(ratio, dimension) {
        if (dimension <= minimumPaneSize * 2)
            return .5
        const minimumRatio = minimumPaneSize / dimension
        return Math.max(minimumRatio, Math.min(1 - minimumRatio, ratio))
    }

    function updatePaneGeometry() {
        for (let pane = 0; pane < paneHosts.length; ++pane) {
            const item = paneHosts[pane]
            const geometry = paneRects[pane]
            if (!item || !geometry)
                continue
            item.x = geometry.x
            item.y = formattingBar.height + geometry.y
            item.width = geometry.width
            item.height = geometry.height
        }
    }

    function updateGeometry() {
        const panes = []
        const dividers = []
        function visit(node, x, y, width, height, path) {
            if (!node)
                return
            if (node.kind === "leaf") {
                panes[node.pane] = { "x": x, "y": y, "width": width, "height": height }
                return
            }
            if (node.axis === "horizontal") {
                const ratio = root.effectiveRatio(node.ratio, width)
                const firstWidth = Math.round(width * ratio)
                visit(node.first, x, y, firstWidth, height, path + "0")
                visit(node.second, x + firstWidth, y, width - firstWidth, height, path + "1")
                dividers.push({
                    "path": path,
                    "axis": node.axis,
                    "x": x + firstWidth - dividerSize / 2,
                    "y": y,
                    "width": dividerSize,
                    "height": height,
                    "branchX": x,
                    "branchY": y,
                    "branchWidth": width,
                    "branchHeight": height,
                    "ratio": ratio
                })
            } else {
                const ratio = root.effectiveRatio(node.ratio, height)
                const firstHeight = Math.round(height * ratio)
                visit(node.first, x, y, width, firstHeight, path + "0")
                visit(node.second, x, y + firstHeight, width, height - firstHeight, path + "1")
                dividers.push({
                    "path": path,
                    "axis": node.axis,
                    "x": x,
                    "y": y + firstHeight - dividerSize / 2,
                    "width": width,
                    "height": dividerSize,
                    "branchX": x,
                    "branchY": y,
                    "branchWidth": width,
                    "branchHeight": height,
                    "ratio": ratio
                })
            }
        }
        visit(splitTree, 0, 0, width, Math.max(0, height - formattingBar.height), "")
        paneRects = panes
        dividerRects = dividers
        layoutRevision += 1
        updatePaneGeometry()
    }

    function paneRect(pane) {
        const dependency = layoutRevision
        return paneRects[pane] || { "x": 0, "y": 0, "width": width, "height": height }
    }

    onWidthChanged: updateGeometry()
    onHeightChanged: updateGeometry()
    Component.onCompleted: {
        layoutProjectKey = backend.project_path
        layoutPaneCount = backend.pane_count
        reconcilePaneHosts()
        restoreLayout()
    }

    Connections {
        target: root.backend
        function onPane_countChanged() {
            if (!root.changingPanes && root.backend.project_open)
                root.reconcileLayout()
        }
        function onPanes_revisionChanged() {
            if (!root.changingPanes) {
                root.reconcilePaneHosts()
                root.syncPaneStates()
            }
        }
        function onProject_openChanged() {
            if (root.backend.project_open) {
                root.layoutProjectKey = root.backend.project_path
                root.layoutPaneCount = root.backend.pane_count
                root.reconcilePaneHosts()
                Qt.callLater(root.restoreLayout)
            } else {
                root.saveLayout()
                root.destroyPaneHosts()
            }
        }
        function onProject_pathChanged() {
            const nextKey = root.backend.project_path
            const projectChanged = root.layoutProjectKey !== nextKey
            if (root.layoutProjectKey.length && projectChanged)
                root.saveLayoutForKey(root.layoutProjectKey)
            if (projectChanged)
                root.destroyPaneHosts()
            root.layoutProjectKey = nextKey
            root.layoutPaneCount = root.backend.pane_count
            if (root.backend.project_open && nextKey.length) {
                root.reconcilePaneHosts()
                Qt.callLater(root.restoreLayout)
            }
        }
    }

    Repeater {
        model: root.dividerRects
        delegate: Rectangle {
            id: divider
            required property var modelData
            x: modelData.x
            y: formattingBar.height + modelData.y
            width: modelData.width
            height: modelData.height
            z: 20
            color: dragArea.containsMouse || dragArea.pressed
                   ? DesignTokens.accent : DesignTokens.outline
            opacity: dragArea.containsMouse || dragArea.pressed ? 1 : .7
            MouseArea {
                id: dragArea
                anchors.fill: parent
                anchors.margins: -3
                hoverEnabled: true
                cursorShape: divider.modelData.axis === "horizontal"
                           ? Qt.SplitHCursor : Qt.SplitVCursor
                onPositionChanged: function(mouse) {
                    if (!pressed)
                        return
                    const point = mapToItem(root, mouse.x, mouse.y)
                    const ratio = divider.modelData.axis === "horizontal"
                                ? (point.x - divider.modelData.branchX) / divider.modelData.branchWidth
                                : (point.y - formattingBar.height - divider.modelData.branchY) / divider.modelData.branchHeight
                    root.resizeDivider(divider.modelData.path, ratio)
                }
                onReleased: root.saveLayout()
            }
        }
    }
}
