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

    property string transientMessage: ""
    property bool binderVisible: true
    property bool inspectorVisible: true
    property bool quitApproved: false
    property bool startViewVisible: !backend.project_open
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
        case "edit.find": (backend.focused_pane === 1 ? paneTwo : paneOne).showFind(); break
        case "edit.replace_project": replaceProjectDialog.open(); break
        case "structure.new_group": backend.createChild(backend.selected_id, qsTr("Untitled Group"), true); break
        case "structure.new_scene": backend.createChild(backend.selected_id, qsTr("Untitled Scene"), false); break
        case "structure.move_up": backend.moveUp(backend.selected_id); break
        case "structure.move_down": backend.moveDown(backend.selected_id); break
        case "structure.indent": backend.indentNode(backend.selected_id); break
        case "structure.outdent": backend.outdentNode(backend.selected_id); break
        case "structure.duplicate": backend.duplicateNode(backend.selected_id); break
        case "structure.trash": trashConfirm.open(); break
        case "view.binder": window.binderVisible = !window.binderVisible; break
        case "view.inspector": window.inspectorVisible = !window.inspectorVisible; break
        case "view.split": backend.setSplit(!backend.split_enabled, "horizontal", 500); break
        case "view.next_pane": backend.focusNextPane(); break
        case "view.swap_panes": backend.swapPanes(); break
        case "view.editor": backend.setPaneView(backend.focused_pane, "editor"); break
        case "view.outline": backend.setPaneView(backend.focused_pane, "outline"); break
        case "view.cards": backend.setPaneView(backend.focused_pane, "cards"); break
        case "view.settings": settingsDialog.open(); break
        case "help.keyboard": keyboardDialog.open(); break
        case "help.onboarding": window.startViewVisible = true; break
        }
    }

    Settings {
        id: appSettings
        category: "version1"
        property string theme: "system"
        property bool onboardingComplete: false
        property var recentProjects: []
    }

    Connections {
        target: backend
        function onProjectOpenChanged() { window.startViewVisible = !backend.project_open }
    }

    onClosing: function(close) {
        if (quitApproved) {
            close.accepted = true
            return
        }
        paneOne.syncLiveBody()
        paneTwo.syncLiveBody()
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
        x: Math.max(DesignTokens.space3, projectSearchField.x)
        y: window.header.height + DesignTokens.space1
        width: 620
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
        onCommandCompleted: function(command, revision) {
            window.transientMessage = qsTr("%1 at revision %2").arg(command).arg(revision)
        }
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
            Label { text: qsTr("Another live process owns this project's writer lock. ParchMint will not break that lock or allow a second writer."); wrapMode: Text.Wrap; Layout.preferredWidth: 460 }
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
            Label { text: qsTr("After continuing, choose the parent folder in the native folder picker."); wrapMode: Text.Wrap; Layout.fillWidth: true; opacity: .7 }
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
            Label {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                text: qsTr("Existing files are left untouched unless a completed export safely replaces them.")
                wrapMode: Text.Wrap
                opacity: .7
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
            text: qsTr("The existing file will remain unchanged until the new export validates. Replace it only if you are sure.")
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
            Label {
                text: qsTr("ParchMint checks every source again, writes recovery backups, and stops on conflicts. The completed replacement can be undone until those documents change.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                opacity: .75
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
            Label { text: qsTr("Motion") }
            Label { text: qsTr("ParchMint uses no nonessential animation and follows the platform focus behavior."); wrapMode: Text.Wrap; Layout.preferredWidth: 360 }
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
        width: 620
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

    Dialog {
        id: onboardingDialog
        title: qsTr("Welcome to ParchMint")
        modal: true
        anchors.centerIn: Overlay.overlay
        closePolicy: Popup.NoAutoClose
        standardButtons: Dialog.Close
        onClosed: appSettings.onboardingComplete = true
        contentItem: ColumnLayout {
            spacing: DesignTokens.space3
            Label { text: qsTr("Plan, write, research, and export—locally."); font.pixelSize: 22; font.bold: true; Accessible.role: Accessible.Heading }
            Label { text: qsTr("Projects are ordinary folders of Markdown and TOML. ParchMint does not need an account or network connection."); wrapMode: Text.Wrap; Layout.preferredWidth: 520 }
            Button {
                text: qsTr("Choose a folder and create the sample…")
                onClicked: sampleProjectParentDialog.open()
            }
        }
    }

    FolderDialog {
        id: sampleProjectParentDialog
        title: qsTr("Choose the parent folder for ParchMint Tour")
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: {
            if (backend.createSampleProject(selectedFolder.toString())) {
                window.rememberProject(backend.project_path)
                onboardingDialog.close()
            }
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
                text: backend.recovery_corrupt
                    ? qsTr("This record is isolated. Discarding it does not affect other recovery records or canonical documents.")
                    : qsTr("Restore returns this text to its live document session. Save a copy preserves it separately; discard removes only this recovery record.")
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
            Label { text: qsTr("Reload uses the disk version. Overwrite explicitly replaces it with your live version after journaling. Save Copy preserves your version separately, then reloads the disk version."); wrapMode: Text.Wrap; Layout.fillWidth: true }
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
        Label { text: qsTr("The document remains recoverable in the project's canonical trash until you explicitly empty it."); wrapMode: Text.Wrap; width: 440 }
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

    menuBar: MenuBar {
        Menu {
            title: qsTr("Project")
            Action { text: qsTr("New Project…"); onTriggered: backend.requestCommand("project.new") }
            Action { text: qsTr("Open Project…"); onTriggered: backend.requestCommand("project.open") }
            Action { text: qsTr("Recent Projects…"); enabled: appSettings.recentProjects.length > 0; onTriggered: recentDialog.open() }
            Action { text: qsTr("Close Project"); enabled: backend.project_open; onTriggered: backend.requestCommand("project.close") }
            MenuSeparator {}
            Action { text: qsTr("Export manuscript…"); enabled: backend.project_open && !backend.export_in_progress; onTriggered: backend.requestCommand("project.export") }
            Action { text: qsTr("Cancel export"); enabled: backend.export_in_progress; onTriggered: backend.cancelExport() }
            MenuSeparator {}
            Action { text: qsTr("Export diagnostics…"); onTriggered: backend.requestCommand("project.diagnostics") }
        }
        Menu {
            title: qsTr("Edit")
            Action { text: qsTr("Undo structural change"); enabled: backend.project_open; onTriggered: backend.requestCommand("edit.undo") }
            Action { text: qsTr("Redo structural change"); enabled: backend.project_open; onTriggered: backend.requestCommand("edit.redo") }
            MenuSeparator {}
            Action { text: qsTr("Find in document"); enabled: backend.project_open; onTriggered: backend.requestCommand("edit.find") }
            Action { text: qsTr("Replace across project…"); enabled: backend.project_open; onTriggered: backend.requestCommand("edit.replace_project") }
        }
        Menu {
            title: qsTr("Structure")
            Action { text: qsTr("New Group"); enabled: backend.selected_id.length > 0; onTriggered: backend.requestCommand("structure.new_group") }
            Action { text: qsTr("New Scene"); enabled: backend.selected_id.length > 0; onTriggered: backend.requestCommand("structure.new_scene") }
            MenuSeparator {}
            Action { text: qsTr("Move Up"); enabled: backend.selected_id.length > 0; onTriggered: backend.requestCommand("structure.move_up") }
            Action { text: qsTr("Move Down"); enabled: backend.selected_id.length > 0; onTriggered: backend.requestCommand("structure.move_down") }
            Action { text: qsTr("Indent"); enabled: backend.selected_id.length > 0; onTriggered: backend.requestCommand("structure.indent") }
            Action { text: qsTr("Outdent"); enabled: backend.selected_id.length > 0; onTriggered: backend.requestCommand("structure.outdent") }
            Action { text: qsTr("Duplicate"); enabled: backend.selected_id.length > 0; onTriggered: backend.requestCommand("structure.duplicate") }
            Action { text: qsTr("Move to Trash"); enabled: backend.selected_id.length > 0; onTriggered: backend.requestCommand("structure.trash") }
        }
        Menu {
            title: qsTr("Research")
            Action { text: qsTr("New research group"); enabled: backend.selected_id.length > 0; onTriggered: backend.createResearchChild(backend.selected_id, qsTr("Untitled Research Group"), true) }
            Action { text: qsTr("New research note"); enabled: backend.selected_id.length > 0; onTriggered: backend.createResearchChild(backend.selected_id, qsTr("Untitled Research Note"), false) }
            Action { text: qsTr("Import attachment…"); enabled: backend.selected_id.length > 0; onTriggered: attachmentFileDialog.open() }
        }
        Menu {
            title: qsTr("View")
            Action { text: qsTr("Binder"); checkable: true; checked: window.binderVisible; onTriggered: backend.requestCommand("view.binder") }
            Action { text: qsTr("Inspector"); checkable: true; checked: window.inspectorVisible; onTriggered: backend.requestCommand("view.inspector") }
            MenuSeparator {}
            Action { text: qsTr("Split workspace"); checkable: true; checked: backend.split_enabled; enabled: backend.project_open; onTriggered: backend.requestCommand("view.split") }
            Action { text: qsTr("Focus next pane"); enabled: backend.split_enabled; onTriggered: backend.requestCommand("view.next_pane") }
            Action { text: qsTr("Swap panes"); enabled: backend.split_enabled; onTriggered: backend.requestCommand("view.swap_panes") }
            MenuSeparator {}
            Action { text: qsTr("Settings…"); onTriggered: backend.requestCommand("view.settings") }
        }
        Menu {
            title: qsTr("Help")
            Action { text: qsTr("Command palette…"); onTriggered: commandPalette.open() }
            Action { text: qsTr("Keyboard shortcuts"); onTriggered: backend.requestCommand("help.keyboard") }
            Action { text: qsTr("ParchMint tour"); onTriggered: backend.requestCommand("help.onboarding") }
        }
    }

    Shortcut { sequences: [StandardKey.Undo]; enabled: backend.project_open; onActivated: backend.requestCommand("edit.undo") }
    Shortcut { sequences: [StandardKey.Redo]; enabled: backend.project_open; onActivated: backend.requestCommand("edit.redo") }
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
    Shortcut { sequence: "Ctrl+Tab"; enabled: backend.split_enabled; onActivated: backend.requestCommand("view.next_pane") }
    Shortcut { sequences: [StandardKey.Find]; enabled: backend.project_open; onActivated: backend.requestCommand("edit.find") }
    Shortcut { sequences: ["Ctrl+Alt+F", "Meta+Alt+F"]; enabled: backend.project_open; onActivated: backend.requestCommand("edit.replace_project") }
    Shortcut { sequences: [StandardKey.Preferences]; onActivated: backend.requestCommand("view.settings") }
    Shortcut { sequences: ["Ctrl+Shift+F", "Meta+Shift+F"]; enabled: backend.project_open; onActivated: projectSearchField.forceActiveFocus() }
    Shortcut { sequences: ["Ctrl+?", "Meta+?"]; onActivated: backend.requestCommand("help.keyboard") }
    Shortcut { sequences: ["Ctrl+1", "Meta+1"]; enabled: backend.project_open; onActivated: backend.requestCommand("view.editor") }
    Shortcut { sequences: ["Ctrl+2", "Meta+2"]; enabled: backend.project_open; onActivated: backend.requestCommand("view.outline") }
    Shortcut { sequences: ["Ctrl+3", "Meta+3"]; enabled: backend.project_open; onActivated: backend.requestCommand("view.cards") }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: DesignTokens.space2
            ToolButton { Accessible.name: qsTr("Toggle binder"); ToolTip.visible: hovered; ToolTip.text: Accessible.name; onClicked: window.binderVisible = !window.binderVisible; contentItem: Image { source: "qrc:/icons/binder.svg"; width: 18; height: 18; anchors.centerIn: parent } }
            ToolButton { Accessible.name: qsTr("Toggle inspector"); ToolTip.visible: hovered; ToolTip.text: Accessible.name; onClicked: window.inspectorVisible = !window.inspectorVisible; contentItem: Image { source: "qrc:/icons/inspector.svg"; width: 18; height: 18; anchors.centerIn: parent } }
            TextField {
                id: projectSearchField
                Layout.preferredWidth: 300
                placeholderText: qsTr("Search project…")
                enabled: backend.project_open
                onTextChanged: {
                    if (text.trim().length) {
                        backend.projectSearch(text)
                        searchPopup.open()
                    }
                }
                onAccepted: { if (text.trim().length) { backend.projectSearch(text); searchPopup.open() } }
                Accessible.name: qsTr("Search project; quote an exact phrase")
            }
            Item { Layout.fillWidth: true }
            Label { text: backend.project_open ? backend.project_name : qsTr("No project open"); font.bold: true }
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0
        BinderPane { Layout.preferredWidth: 276; Layout.fillHeight: true; visible: window.binderVisible; backend: backend; model: outlineModel }
        Rectangle { Layout.preferredWidth: window.binderVisible ? 1 : 0; Layout.fillHeight: true; color: DesignTokens.outline; opacity: .35 }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0
            PaneHost { id: paneOne; Layout.fillWidth: true; Layout.fillHeight: true; backend: backend; model: outlineModel; paneIndex: 0; nodeId: backend.pane_one_id; viewName: backend.pane_one_view; pinned: backend.pane_one_pinned }
            Rectangle { Layout.preferredWidth: backend.split_enabled ? 1 : 0; Layout.fillHeight: true; color: DesignTokens.outline; opacity: .35 }
            PaneHost { id: paneTwo; Layout.preferredWidth: backend.split_enabled ? 520 : 0; Layout.fillHeight: true; visible: backend.split_enabled; backend: backend; model: outlineModel; paneIndex: 1; nodeId: backend.pane_two_id; viewName: backend.pane_two_view; pinned: backend.pane_two_pinned }
        }

        Rectangle { Layout.preferredWidth: window.inspectorVisible ? 1 : 0; Layout.fillHeight: true; color: DesignTokens.outline; opacity: .35 }
        InspectorPane { Layout.preferredWidth: 310; Layout.fillHeight: true; visible: window.inspectorVisible; backend: backend }
    }

    Rectangle {
        id: startView
        anchors.fill: parent
        z: 10
        visible: window.startViewVisible
        color: DesignTokens.base
        Accessible.name: qsTr("ParchMint start view")
        ColumnLayout {
            anchors.centerIn: parent
            width: Math.min(720, parent.width - DesignTokens.space8 * 2)
            spacing: DesignTokens.space4
            Label { text: qsTr("ParchMint"); font.pixelSize: DesignTokens.typeDisplay; font.bold: true; Accessible.role: Accessible.Heading }
            Label { text: qsTr("A calm, local place to plan and write long-form work."); font.pixelSize: DesignTokens.typeTitle; color: DesignTokens.textMuted; wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                Layout.fillWidth: true
                Button { text: qsTr("New project…"); highlighted: true; onClicked: newProjectDialog.open(); Accessible.name: text }
                Button { text: qsTr("Open project…"); onClicked: openProjectFolderDialog.open(); Accessible.name: text }
                Button { text: qsTr("Create sample project…"); onClicked: sampleProjectParentDialog.open(); Accessible.name: text }
            }
            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: DesignTokens.outline; Layout.topMargin: DesignTokens.space2 }
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
                        if (backend.openProject(modelData)) {
                            window.rememberProject(backend.project_path)
                            window.startViewVisible = false
                        }
                    }
                }
            }
            Label { visible: appSettings.recentProjects.length === 0; text: qsTr("Recent projects will appear here after you open one."); color: DesignTokens.textMuted }
            Label { text: qsTr("Projects stay in ordinary folders of Markdown and TOML. ParchMint makes no network request."); wrapMode: Text.Wrap; Layout.fillWidth: true; color: DesignTokens.textMuted }
        }
    }

    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: DesignTokens.space3
            anchors.rightMargin: DesignTokens.space3
            Label { text: backend.export_in_progress ? backend.export_status : (window.transientMessage.length ? window.transientMessage : qsTr("Local-first · structural changes are saved canonically")); Layout.fillWidth: true; elide: Text.ElideRight }
            Label { text: backend.save_status; Accessible.name: qsTr("Save status") + ": " + text }
            Label { text: qsTr("%1 visible · %2 selected").arg(backend.node_count).arg(backend.selected_count) }
        }
    }
}
