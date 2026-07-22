pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import QtCore
import org.parchmint.app 1.0
import org.parchmint.adapters 1.0

ApplicationWindow {
    id: window
    width: 1320
    height: 840
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    title: backend.project_open ? qsTr("%1 — ParchMint").arg(backend.project_name) : qsTr("ParchMint")
    Material.accent: DesignTokens.accent
    Material.primary: DesignTokens.accent
    Material.theme: appSettings.theme === "dark" ? Material.Dark : appSettings.theme === "light" ? Material.Light : Material.System
    Material.containerStyle: Material.Dense

    property string transientMessage: ""
    property bool binderVisible: true
    property bool quitApproved: false
    property int workspaceMode: 0
    onWorkspaceModeChanged: {
        if (backend.project_open)
            backend.setFilter(workspaceMode === 1 ? "" : binderPane.filterText)
    }

    function rememberProject(path) {
        const normalized = path.trim()
        if (!normalized.length)
            return
        const values = appSettings.recentProjects.filter(function(value) { return value !== normalized })
        values.unshift(normalized)
        appSettings.recentProjects = values.slice(0, 12)
    }

    function dispatchCommand(id) {
        switch (id) {
        case "project.new": newProjectDialog.open(); break
        case "project.open": openProjectFolderDialog.open(); break
        case "project.close": backend.closeProject(); break
        case "project.save": backend.flushAllDocuments(); break
        case "project.export": exportDialog.open(); break
        case "project.diagnostics": diagnosticsDialog.open(); break
        case "edit.undo": backend.undoStructural(); break
        case "edit.redo": backend.redoStructural(); break
        case "edit.find": editorWorkspace.showFindFocused(); break
        case "edit.replace_project": replaceProjectDialog.open(); break
        case "structure.new_group": backend.createChild(backend.selected_id, qsTr("Untitled Section"), true); break
        case "structure.new_scene": backend.createChild(backend.selected_id, qsTr("Untitled Document"), false); break
        case "structure.move_up": backend.moveUp(backend.selected_id); break
        case "structure.move_down": backend.moveDown(backend.selected_id); break
        case "structure.indent": backend.indentNode(backend.selected_id); break
        case "structure.outdent": backend.outdentNode(backend.selected_id); break
        case "structure.duplicate": backend.duplicateNode(backend.selected_id); break
        case "structure.trash": trashConfirm.open(); break
        case "view.binder": window.binderVisible = !window.binderVisible; break
        case "view.split": editorWorkspace.splitFocused("right"); break
        case "view.next_pane": backend.focusNextPane(); break
        case "view.editor": window.workspaceMode = 0; break
        case "view.cards": window.workspaceMode = 1; break
        case "view.settings": settingsDialog.open(); break
        case "help.keyboard": keyboardDialog.open(); break
        case "help.onboarding": sampleProjectParentDialog.open(); break
        }
    }

    Settings {
        id: appSettings
        category: "version1"
        property string theme: "system"
        property var recentProjects: []
    }

    Connections {
        target: backend
        function onProject_openChanged() {
            if (backend.project_open) {
                binderPane.filterText = ""
                window.workspaceMode = 0
            }
        }
    }

    onClosing: function(close) {
        if (quitApproved) {
            close.accepted = true
            return
        }
        editorWorkspace.syncLiveBodies()
        close.accepted = backend.prepareQuit()
        quitApproved = close.accepted
    }

    Popup {
        id: commandPalette
        anchors.centerIn: Overlay.overlay
        width: Math.min(680, window.width - DesignTokens.space6 * 2)
        height: Math.min(520, window.height - DesignTokens.space6 * 2)
        modal: true
        focus: true
        padding: DesignTokens.space3
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        onAboutToShow: {
            commandField.clear()
            backend.filterCommands("")
            commandField.forceActiveFocus()
        }
        contentItem: ColumnLayout {
            Label { text: qsTr("Command palette"); font.bold: true; Accessible.role: Accessible.Heading }
            TextField {
                id: commandField
                Layout.fillWidth: true
                placeholderText: qsTr("Type a command")
                Accessible.name: qsTr("Command search")
                onTextChanged: backend.filterCommands(text)
                onAccepted: {
                    if (backend.command_count > 0 && backend.requestCommand(backend.commandId(0)))
                        commandPalette.close()
                }
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                reuseItems: true
                model: backend.command_count
                delegate: ItemDelegate {
                    required property int index
                    width: ListView.view.width
                    Accessible.name: backend.commandLabel(index) + (backend.commandShortcut(index).length ? ", " + backend.commandShortcut(index) : "")
                    onClicked: {
                        if (backend.requestCommand(backend.commandId(index)))
                            commandPalette.close()
                    }
                    contentItem: RowLayout {
                        Label { text: backend.commandLabel(index); Layout.fillWidth: true }
                        Label { text: backend.commandShortcut(index); opacity: .7 }
                    }
                }
            }
        }
    }

    Shortcut { sequences: ["Ctrl+Shift+P", "Meta+Shift+P"]; onActivated: commandPalette.open() }

    Popup {
        id: searchPopup
        x: Math.max(DesignTokens.space3, window.width - width - DesignTokens.space3)
        y: projectToolbar.height + DesignTokens.space1
        width: Math.min(520, window.width - DesignTokens.space3 * 2)
        height: 410
        padding: DesignTokens.space3
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        contentItem: ColumnLayout {
            spacing: DesignTokens.space2
            Label { text: backend.search_status; wrapMode: Text.Wrap; Layout.fillWidth: true; opacity: .75 }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: backend.search_result_count
                delegate: ItemDelegate {
                    required property int index
                    width: ListView.view.width
                    onClicked: { backend.openSearchResult(index, false); searchPopup.close() }
                    contentItem: ColumnLayout {
                        Label { text: backend.searchResultTitle(index); font.bold: true; Layout.fillWidth: true; elide: Text.ElideRight }
                        Label { text: backend.searchResultContext(index); opacity: .65; Layout.fillWidth: true; elide: Text.ElideRight }
                        Label { text: backend.searchResultSnippet(index); Layout.fillWidth: true; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight }
                    }
                    ToolButton {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        Accessible.name: qsTr("Open search result in other pane")
                        ToolTip.visible: hovered
                        ToolTip.text: Accessible.name
                        onClicked: { backend.openSearchResult(index, true); searchPopup.close() }
                        contentItem: Image {
                            source: "qrc:/icons/chevron.svg"
                            sourceSize.width: DesignTokens.iconSize
                            sourceSize.height: DesignTokens.iconSize
                            width: DesignTokens.iconSize
                            height: DesignTokens.iconSize
                            anchors.centerIn: parent
                            rotation: -90
                        }
                    }
                }
            }
        }
    }

    ParchMintBackend {
        id: backend
        objectName: "parchmintBackend"
        onCommandRequested: function(id) { window.dispatchCommand(id) }
        onCommandCompleted: function(command, revision) { window.transientMessage = command }
        onOperationFailed: function(message) {
            window.transientMessage = message
            errorPopup.open()
        }
    }

    OutlineModel {
        id: outlineModel
        source: backend
        onModelError: function(message) {
            window.transientMessage = message
            errorPopup.open()
        }
    }

    Popup {
        id: errorPopup
        anchors.centerIn: Overlay.overlay
        modal: true
        focus: true
        padding: DesignTokens.space4
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        contentItem: ColumnLayout {
            spacing: DesignTokens.space3
            Label { text: qsTr("ParchMint could not complete the operation"); font.bold: true }
            Label { text: window.transientMessage; wrapMode: Text.Wrap; Layout.preferredWidth: 440 }
            Button { text: qsTr("Close"); onClicked: errorPopup.close(); Layout.alignment: Qt.AlignRight }
        }
    }

    Dialog {
        id: readOnlyDialog
        visible: backend.read_only_offer
        title: qsTr("Project already open")
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Cancel
        onRejected: backend.dismissReadOnlyOffer()
        contentItem: ColumnLayout {
            Label { text: qsTr("This project is open elsewhere. You can open it read-only."); wrapMode: Text.Wrap; Layout.preferredWidth: 460 }
            Button { text: qsTr("Open Read-Only"); highlighted: true; Layout.alignment: Qt.AlignRight; onClicked: backend.openProjectReadOnly() }
        }
    }

    Dialog {
        id: newProjectDialog
        title: qsTr("Create project")
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: {
            createProjectParentDialog.open()
        }
        contentItem: ColumnLayout {
            Label { text: qsTr("Project name") }
            TextField { id: nameField; text: qsTr("Untitled Novel"); Layout.preferredWidth: 440; selectByMouse: true }
        }
    }

    FolderDialog {
        id: createProjectParentDialog
        title: qsTr("Choose the parent folder for the new project")
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: {
            if (backend.createProject(selectedFolder.toString(), nameField.text))
                window.rememberProject(backend.project_path)
        }
    }

    FolderDialog {
        id: openProjectFolderDialog
        title: qsTr("Open a ParchMint project folder")
        currentFolder: backend.project_open ? backend.project_path : StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: {
            if (backend.openProject(selectedFolder.toString()))
                window.rememberProject(backend.project_path)
        }
    }

    FileDialog {
        id: attachmentFileDialog
        title: qsTr("Import research attachment")
        fileMode: FileDialog.OpenFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: backend.importAttachment(backend.selected_id, selectedFile.toString())
    }

    Dialog {
        id: exportDialog
        title: qsTr("Export manuscript")
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: exportFileDialog.open()
        function rebuildPresets() {
            exportPresets.clear()
            for (let index = 0; index < backend.compile_preset_count; ++index) {
                const id = backend.compilePresetId(index)
                exportPresets.append({ "presetId": id, "name": backend.compilePresetName(index) })
            }
            presetChooser.currentIndex = Math.max(0, presetChooser.indexOfValue(backend.selected_compile_preset))
        }
        onOpened: rebuildPresets()
        contentItem: GridLayout {
            columns: 2
            rowSpacing: DesignTokens.space3
            columnSpacing: DesignTokens.space3
            Label { text: qsTr("Format") }
            ComboBox {
                id: exportFormat
                Layout.preferredWidth: 300
                textRole: "label"
                valueRole: "value"
                model: [
                    { label: qsTr("Markdown"), value: "markdown" },
                    { label: qsTr("Plain text"), value: "plain_text" },
                    { label: qsTr("HTML"), value: "html" },
                    { label: qsTr("PDF"), value: "pdf" },
                    { label: qsTr("EPUB"), value: "epub" },
                    { label: qsTr("DOCX"), value: "docx" }
                ]
                Accessible.name: qsTr("Export format")
            }
            Label { text: qsTr("Compile preset") }
            RowLayout {
                Layout.preferredWidth: 300
                ComboBox {
                    id: presetChooser
                    Layout.fillWidth: true
                    model: ListModel { id: exportPresets }
                    textRole: "name"
                    valueRole: "presetId"
                    Accessible.name: qsTr("Compile preset")
                    onActivated: backend.selectCompilePreset(currentValue)
                }
                Button {
                    text: qsTr("Edit…")
                    enabled: presetChooser.currentIndex >= 0
                    Accessible.name: qsTr("Edit compile preset")
                    onClicked: {
                        presetName.presetId = presetChooser.currentValue
                        presetName.text = presetChooser.currentText
                        presetName.open()
                    }
                }
            }
        }
    }

    Dialog {
        id: presetName
        property string presetId: ""
        title: qsTr("Compile preset name")
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        TextField {
            id: presetNameField
            width: 320
            placeholderText: qsTr("Preset name")
            Accessible.name: qsTr("Compile preset name")
        }
        property alias text: presetNameField.text
        onAccepted: {
            if (backend.renameCompilePreset(presetId, text))
                exportDialog.rebuildPresets()
        }
    }

    FileDialog {
        id: exportFileDialog
        title: qsTr("Save exported manuscript")
        fileMode: FileDialog.SaveFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        nameFilters: {
            switch (exportFormat.currentValue) {
            case "markdown": return [qsTr("Markdown files (*.md)")]
            case "plain_text": return [qsTr("Text files (*.txt)")]
            case "html": return [qsTr("HTML files (*.html)")]
            case "pdf": return [qsTr("PDF files (*.pdf)")]
            case "epub": return [qsTr("EPUB files (*.epub)")]
            case "docx": return [qsTr("Word documents (*.docx)")]
            }
            return [qsTr("All files (*)")]
        }
        onAccepted: {
            if (backend.exportDestinationExists(selectedFile.toString()))
                overwriteExportDialog.open()
            else
                backend.exportProjectWithOverwrite(exportFormat.currentValue, selectedFile.toString(), false)
        }
    }

    Dialog {
        id: overwriteExportDialog
        title: qsTr("Replace existing export?")
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: backend.exportProjectWithOverwrite(exportFormat.currentValue, exportFileDialog.selectedFile.toString(), true)
        contentItem: Label {
            width: 420
            wrapMode: Text.Wrap
            text: qsTr("A file already exists at this location. Replace it?")
        }
    }

    Dialog {
        id: replaceProjectDialog
        title: qsTr("Replace across project")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(760, window.width - DesignTokens.space6 * 2)
        height: Math.min(620, window.height - DesignTokens.space6 * 2)
        standardButtons: Dialog.Close
        contentItem: ColumnLayout {
            RowLayout {
                Layout.fillWidth: true
                TextField { id: projectFind; Layout.fillWidth: true; placeholderText: qsTr("Find literal text"); Accessible.name: qsTr("Project replacement search text") }
                TextField { id: projectReplace; Layout.fillWidth: true; placeholderText: qsTr("Replace with"); Accessible.name: qsTr("Project replacement value") }
                CheckBox { id: projectCase; text: qsTr("Case sensitive") }
                Button { text: qsTr("Preview"); enabled: projectFind.text.length > 0; onClicked: backend.previewProjectReplace(projectFind.text, projectReplace.text, projectCase.checked) }
            }
            Label {
                text: backend.replace_count === 0 ? qsTr("No previewed changes") : qsTr("%1 previewed changes; select each change before applying").arg(backend.replace_count)
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                reuseItems: true
                model: backend.replace_count
                delegate: CheckDelegate {
                    required property int index
                    width: ListView.view.width
                    checked: backend.replaceSelected(index)
                    onToggled: backend.setReplaceSelected(index, checked)
                    Accessible.name: backend.replaceTitle(index) + ": " + backend.replaceContext(index)
                    contentItem: ColumnLayout {
                        Label { text: backend.replaceTitle(index); font.bold: true; Layout.fillWidth: true; elide: Text.ElideRight }
                        Label { text: backend.replaceContext(index); Layout.fillWidth: true; elide: Text.ElideRight }
                    }
                }
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                Button { text: qsTr("Undo last project replacement"); onClicked: backend.undoProjectReplace() }
                Button { text: qsTr("Apply selected changes"); enabled: backend.replace_count > 0; highlighted: true; onClicked: { if (backend.applyProjectReplace()) replaceProjectDialog.close() } }
            }
        }
    }

    FileDialog {
        id: diagnosticsDialog
        title: qsTr("Export diagnostics")
        fileMode: FileDialog.SaveFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        nameFilters: [qsTr("Text files (*.txt)")]
        onAccepted: backend.exportDiagnostics(selectedFile.toString())
    }

    Dialog {
        id: settingsDialog
        title: qsTr("Settings")
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Close
        contentItem: GridLayout {
            columns: 2
            Label { text: qsTr("Theme") }
            ComboBox {
                Layout.preferredWidth: 260
                textRole: "label"
                valueRole: "value"
                model: [
                    { label: qsTr("Follow system"), value: "system" },
                    { label: qsTr("Light"), value: "light" },
                    { label: qsTr("Dark"), value: "dark" }
                ]
                Component.onCompleted: currentIndex = Math.max(0, indexOfValue(appSettings.theme))
                onActivated: appSettings.theme = currentValue
                Accessible.name: qsTr("Application theme")
            }
        }
    }

    Dialog {
        id: keyboardDialog
        title: qsTr("Keyboard shortcuts")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: 560
        height: 520
        standardButtons: Dialog.Close
        contentItem: ListView {
            clip: true
            model: backend.command_count
            delegate: ItemDelegate {
                required property int index
                width: ListView.view.width
                contentItem: RowLayout {
                    Label { text: backend.commandLabel(index); Layout.fillWidth: true }
                    Label { text: backend.commandShortcut(index); opacity: .7 }
                }
            }
        }
        onOpened: backend.filterCommands("")
    }

    Dialog {
        id: recentDialog
        title: qsTr("Recent projects")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(520, window.width - DesignTokens.space3 * 2)
        height: 440
        standardButtons: Dialog.Close
        contentItem: ListView {
            clip: true
            model: appSettings.recentProjects
            delegate: ItemDelegate {
                required property string modelData
                width: ListView.view.width
                text: modelData
                Accessible.name: qsTr("Open recent project %1").arg(modelData)
                onClicked: { if (backend.openProject(modelData)) recentDialog.close() }
            }
        }
    }


    FolderDialog {
        id: sampleProjectParentDialog
        title: qsTr("Choose the parent folder for the sample project")
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: {
            if (backend.createSampleProject(selectedFolder.toString()))
                window.rememberProject(backend.project_path)
        }
    }

    Dialog {
        id: recoveryDialog
        objectName: "recoveryDialog"
        visible: backend.recovery_count > 0
        title: backend.recovery_corrupt ? qsTr("Corrupt recovery record") : qsTr("Recover unsaved writing")
        modal: true
        anchors.centerIn: Overlay.overlay
        closePolicy: Popup.NoAutoClose
        contentItem: ColumnLayout {
            spacing: DesignTokens.space3
            Label { text: backend.recovery_title; font.bold: true; Layout.fillWidth: true }
            ScrollView {
                Layout.preferredWidth: 620
                Layout.preferredHeight: 300
                TextArea { text: backend.recovery_preview; readOnly: true; wrapMode: TextEdit.Wrap; Accessible.name: qsTr("Recovery preview") }
            }
            Label {
                text: backend.recovery_corrupt ? qsTr("This recovery record cannot be read.")
                                               : qsTr("Restore, save a copy, or discard this version.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                Button { text: qsTr("Discard"); onClicked: backend.discardRecovery() }
                Button { text: qsTr("Save Copy…"); visible: !backend.recovery_corrupt; onClicked: recoveryCopyDialog.open() }
                Button { text: qsTr("Restore"); visible: !backend.recovery_corrupt; highlighted: true; onClicked: backend.restoreRecovery() }
            }
        }
    }

    FileDialog {
        id: recoveryCopyDialog
        title: qsTr("Save recovered writing as a copy")
        fileMode: FileDialog.SaveFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        nameFilters: [qsTr("Markdown files (*.md)"), qsTr("Text files (*.txt)")]
        onAccepted: backend.saveRecoveryCopy(selectedFile.toString())
    }

    Dialog {
        id: externalConflictDialog
        visible: backend.external_conflict
        title: qsTr("%1 changed outside ParchMint").arg(backend.external_conflict_title)
        modal: true
        anchors.centerIn: Overlay.overlay
        closePolicy: Popup.NoAutoClose
        contentItem: ColumnLayout {
            spacing: DesignTokens.space3
            RowLayout {
                Layout.preferredWidth: 760
                Layout.preferredHeight: 320
                ColumnLayout {
                    Layout.fillWidth: true; Layout.fillHeight: true
                    Label { text: qsTr("Your unsaved version"); font.bold: true }
                    ScrollView { Layout.fillWidth: true; Layout.fillHeight: true; TextArea { text: backend.external_local_preview; readOnly: true; wrapMode: TextEdit.Wrap } }
                }
                ColumnLayout {
                    Layout.fillWidth: true; Layout.fillHeight: true
                    Label { text: qsTr("Version on disk"); font.bold: true }
                    ScrollView { Layout.fillWidth: true; Layout.fillHeight: true; TextArea { text: backend.external_disk_preview; readOnly: true; wrapMode: TextEdit.Wrap } }
                }
            }
            Label { text: qsTr("Choose which version to keep, or save a copy first."); wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                Button { text: qsTr("Save Copy…"); onClicked: externalCopyDialog.open() }
                Button { text: qsTr("Reload Disk Version"); onClicked: backend.resolveExternalReload() }
                Button { text: qsTr("Overwrite with Mine"); highlighted: true; onClicked: backend.resolveExternalOverwrite() }
            }
        }
    }

    FileDialog {
        id: externalCopyDialog
        title: qsTr("Save your version as a copy")
        fileMode: FileDialog.SaveFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        nameFilters: [qsTr("Markdown files (*.md)"), qsTr("Text files (*.txt)")]
        onAccepted: backend.saveExternalCopy(selectedFile.toString())
    }

    Dialog {
        id: trashConfirm
        title: qsTr("Move selection to project trash?")
        modal: true
        anchors.centerIn: Overlay.overlay
        standardButtons: Dialog.Yes | Dialog.Cancel
        onAccepted: backend.trashNode(backend.selected_id)
        Label { text: qsTr("You can restore it later from project trash."); wrapMode: Text.Wrap; width: 440 }
    }

    Timer {
        interval: 100
        running: backend.export_in_progress
        repeat: true
        onTriggered: backend.pollExport()
    }

    Timer {
        interval: 100
        running: backend.project_open
        repeat: true
        onTriggered: backend.pollDocumentLifecycle()
    }

    Shortcut { sequences: [StandardKey.New]; onActivated: backend.requestCommand("project.new") }
    Shortcut { sequences: [StandardKey.Open]; onActivated: backend.requestCommand("project.open") }
    Shortcut { sequences: [StandardKey.Close]; enabled: backend.project_open; onActivated: backend.requestCommand("project.close") }
    Shortcut { sequences: [StandardKey.Save]; enabled: backend.project_open; onActivated: backend.requestCommand("project.save") }
    Shortcut { sequences: ["Ctrl+Shift+E", "Meta+Shift+E"]; enabled: backend.project_open; onActivated: backend.requestCommand("project.export") }
    Shortcut { sequences: ["Ctrl+Shift+Up", "Meta+Shift+Up"]; enabled: backend.selected_id.length > 0; onActivated: backend.requestCommand("structure.move_up") }
    Shortcut { sequences: ["Ctrl+Shift+Down", "Meta+Shift+Down"]; enabled: backend.selected_id.length > 0; onActivated: backend.requestCommand("structure.move_down") }
    Shortcut { sequences: ["Ctrl+]", "Meta+]"]; enabled: backend.selected_id.length > 0; onActivated: backend.requestCommand("structure.indent") }
    Shortcut { sequences: ["Ctrl+[", "Meta+["]; enabled: backend.selected_id.length > 0; onActivated: backend.requestCommand("structure.outdent") }
    Shortcut { sequences: [StandardKey.Delete]; enabled: backend.selected_id.length > 0; onActivated: backend.requestCommand("structure.trash") }
    Shortcut { sequence: "Ctrl+Tab"; enabled: backend.pane_count > 1; onActivated: backend.requestCommand("view.next_pane") }
    Shortcut { sequences: [StandardKey.Find]; enabled: backend.project_open; onActivated: backend.requestCommand("edit.find") }
    Shortcut { sequences: ["Ctrl+Alt+F", "Meta+Alt+F"]; enabled: backend.project_open; onActivated: backend.requestCommand("edit.replace_project") }
    Shortcut { sequences: [StandardKey.Preferences]; onActivated: backend.requestCommand("view.settings") }
    Shortcut { sequences: ["Ctrl+Shift+F", "Meta+Shift+F"]; enabled: backend.project_open; onActivated: projectSearchField.forceActiveFocus() }
    Shortcut { sequences: ["Ctrl+?", "Meta+?"]; onActivated: backend.requestCommand("help.keyboard") }
    Shortcut { sequences: ["Ctrl+1", "Meta+1"]; enabled: backend.project_open; onActivated: backend.requestCommand("view.editor") }
    Shortcut { sequences: ["Ctrl+2", "Meta+2"]; enabled: backend.project_open; onActivated: backend.requestCommand("view.cards") }

    Item {
        id: projectShell
        anchors.fill: parent
        visible: backend.project_open

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            ToolBar {
                id: projectToolbar
                Layout.fillWidth: true
                Layout.preferredHeight: 34
                contentItem: RowLayout {
                    spacing: DesignTokens.space1
                    ToolButton {
                        text: qsTr("New")
                        font.pixelSize: DesignTokens.typeCaption
                        Accessible.name: qsTr("Create new project")
                        onClicked: newProjectDialog.open()
                    }
                    ToolButton {
                        text: qsTr("Open")
                        font.pixelSize: DesignTokens.typeCaption
                        Accessible.name: qsTr("Open project")
                        onClicked: openProjectFolderDialog.open()
                    }
                    ToolButton {
                        text: qsTr("Close")
                        font.pixelSize: DesignTokens.typeCaption
                        Accessible.name: qsTr("Close project")
                        onClicked: backend.closeProject()
                    }
                    ToolButton {
                        text: qsTr("Save")
                        font.pixelSize: DesignTokens.typeCaption
                        Accessible.name: qsTr("Save project")
                        onClicked: backend.flushAllDocuments()
                    }
                    ToolButton {
                        text: qsTr("Export")
                        font.pixelSize: DesignTokens.typeCaption
                        Accessible.name: qsTr("Export manuscript")
                        enabled: !backend.export_in_progress
                        onClicked: exportDialog.open()
                    }
                    ToolButton {
                        visible: window.workspaceMode === 0
                        checked: window.binderVisible
                        checkable: true
                        font.pixelSize: DesignTokens.typeCaption
                        text: qsTr("Files")
                        Accessible.name: checked ? qsTr("Hide file tree") : qsTr("Show file tree")
                        onClicked: window.binderVisible = checked
                    }
                    ButtonGroup { id: workspaceModeGroup }
                    Item { Layout.preferredWidth: DesignTokens.space1 }
                    ToolButton {
                        text: qsTr("Editor")
                        font.pixelSize: DesignTokens.typeCaption
                        checkable: true
                        checked: window.workspaceMode === 0
                        ButtonGroup.group: workspaceModeGroup
                        onClicked: window.workspaceMode = 0
                    }
                    ToolButton {
                        text: qsTr("Cards")
                        font.pixelSize: DesignTokens.typeCaption
                        checkable: true
                        checked: window.workspaceMode === 1
                        ButtonGroup.group: workspaceModeGroup
                        onClicked: window.workspaceMode = 1
                    }
                    Item { Layout.fillWidth: true }
                    TextField {
                        id: projectSearchField
                        Layout.fillWidth: true
                        Layout.maximumWidth: 280
                        Layout.preferredHeight: 28
                        font.pixelSize: DesignTokens.typeCaption
                        placeholderText: qsTr("Search project…")
                        selectByMouse: true
                        onTextChanged: {
                            if (text.trim().length) {
                                backend.projectSearch(text)
                                searchPopup.open()
                            } else {
                                searchPopup.close()
                            }
                        }
                        onAccepted: {
                            if (text.trim().length) {
                                backend.projectSearch(text)
                                searchPopup.open()
                            }
                        }
                        Accessible.name: qsTr("Search project")
                    }
                }
            }

            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: window.workspaceMode

                Item {
                    RowLayout {
                        anchors.fill: parent
                        spacing: 0
                        BinderPane {
                            id: binderPane
                            Layout.preferredWidth: window.binderVisible ? 268 : 0
                            Layout.fillHeight: true
                            visible: window.binderVisible
                            backend: backend
                            onOpenInSplitRequested: function(nodeId) { editorWorkspace.splitPane(backend.focused_pane, "right", nodeId) }
                            model: outlineModel
                        }
                        Rectangle {
                            Layout.preferredWidth: window.binderVisible ? 1 : 0
                            Layout.fillHeight: true
                            color: DesignTokens.outline
                        }
                        EditorWorkspace {
                            id: editorWorkspace
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            backend: backend
                            model: outlineModel
                        }
                    }
                }

                CardsView {
                    backend: backend
                    model: outlineModel
                    onOpenRequested: function(nodeId) {
                        backend.selectNode(nodeId, false)
                        window.workspaceMode = 0
                    }
                }
            }
        }
    }

    Rectangle {
        id: startView
        anchors.fill: parent
        visible: !backend.project_open
        color: DesignTokens.base
        Accessible.name: qsTr("ParchMint start view")
        ColumnLayout {
            anchors.centerIn: parent
            width: Math.min(640, parent.width - DesignTokens.space4 * 2)
            spacing: DesignTokens.space4
            Label {
                text: qsTr("ParchMint")
                font.pixelSize: DesignTokens.typeDisplay
                font.bold: true
                Accessible.role: Accessible.Heading
            }
            RowLayout {
                Layout.fillWidth: true
                Button { text: qsTr("New project…"); highlighted: true; onClicked: newProjectDialog.open() }
                Button { text: qsTr("Open project…"); onClicked: openProjectFolderDialog.open() }
                Button { text: qsTr("Create sample project…"); onClicked: sampleProjectParentDialog.open() }
            }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: DesignTokens.outline
                Layout.topMargin: DesignTokens.space2
            }
            Label { text: qsTr("Recent projects"); font.bold: true; visible: appSettings.recentProjects.length > 0 }
            ListView {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(contentHeight, 220)
                clip: true
                visible: appSettings.recentProjects.length > 0
                model: appSettings.recentProjects
                delegate: ItemDelegate {
                    required property string modelData
                    width: ListView.view.width
                    text: modelData
                    Accessible.name: qsTr("Open recent project %1").arg(modelData)
                    onClicked: {
                        if (backend.openProject(modelData))
                            window.rememberProject(backend.project_path)
                    }
                }
            }
            Label {
                visible: appSettings.recentProjects.length === 0
                text: qsTr("Recent projects will appear here after you open one.")
                color: DesignTokens.textMuted
            }
        }
    }
}
