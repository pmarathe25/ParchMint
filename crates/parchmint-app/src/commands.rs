//! Central product command catalog used by menus and the command palette.

/// Stable command metadata. Identifiers are intentionally not translated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    /// Stable dispatch identifier.
    pub id: &'static str,
    /// English source label translated by the UI.
    pub label: &'static str,
    /// Portable shortcut spelling shown by the palette.
    pub shortcut: &'static str,
    /// Whether the command is unavailable without an open project.
    pub requires_project: bool,
    /// Whether the command is unavailable without a selected node.
    pub requires_selection: bool,
}

/// Version-1 command catalog. UI surfaces dispatch these stable identifiers;
/// they must not maintain independent command names or availability rules.
pub const COMMANDS: &[CommandSpec] = &[
    command("project.new", "New Project…", "Ctrl+N", false, false),
    command("project.open", "Open Project…", "Ctrl+O", false, false),
    command("project.close", "Close Project", "Ctrl+W", true, false),
    command(
        "project.export",
        "Export manuscript…",
        "Ctrl+Shift+E",
        true,
        false,
    ),
    command(
        "project.diagnostics",
        "Export diagnostics…",
        "",
        false,
        false,
    ),
    command("edit.undo", "Undo structural change", "Ctrl+Z", true, false),
    command(
        "edit.redo",
        "Redo structural change",
        "Ctrl+Shift+Z",
        true,
        false,
    ),
    command("edit.find", "Find in document", "Ctrl+F", true, false),
    command(
        "edit.replace_project",
        "Replace across project…",
        "Ctrl+Alt+F",
        true,
        false,
    ),
    command("structure.new_group", "New Group", "", true, true),
    command("structure.new_scene", "New Scene", "", true, true),
    command("structure.move_up", "Move Up", "Ctrl+Shift+Up", true, true),
    command(
        "structure.move_down",
        "Move Down",
        "Ctrl+Shift+Down",
        true,
        true,
    ),
    command("structure.indent", "Indent", "Ctrl+]", true, true),
    command("structure.outdent", "Outdent", "Ctrl+[", true, true),
    command("structure.duplicate", "Duplicate", "", true, true),
    command("structure.trash", "Move to Trash", "Delete", true, true),
    command("view.binder", "Toggle binder", "", false, false),
    command("view.inspector", "Toggle inspector", "", false, false),
    command("view.split", "Split workspace", "", true, false),
    command("view.next_pane", "Focus next pane", "Ctrl+Tab", true, false),
    command("view.swap_panes", "Swap panes", "", true, false),
    command("view.settings", "Settings…", "Ctrl+,", false, false),
    command(
        "help.keyboard",
        "Keyboard shortcuts",
        "Ctrl+?",
        false,
        false,
    ),
    command("help.onboarding", "ParchMint tour", "", false, false),
];

const fn command(
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
    requires_project: bool,
    requires_selection: bool,
) -> CommandSpec {
    CommandSpec {
        id,
        label,
        shortcut,
        requires_project,
        requires_selection,
    }
}

/// Returns palette results in stable catalog order using all query words.
pub fn matching_commands(query: &str, project_open: bool, has_selection: bool) -> Vec<CommandSpec> {
    let words = query
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    COMMANDS
        .iter()
        .copied()
        .filter(|item| !item.requires_project || project_open)
        .filter(|item| !item.requires_selection || has_selection)
        .filter(|item| {
            let haystack =
                format!("{} {} {}", item.id, item.label, item.shortcut).to_ascii_lowercase();
            words.iter().all(|word| haystack.contains(word))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn command_ids_are_unique_and_palette_respects_context() {
        let ids = COMMANDS.iter().map(|item| item.id).collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), COMMANDS.len());
        assert!(matching_commands("replace project", false, false).is_empty());
        assert_eq!(
            matching_commands("replace project", true, false)[0].id,
            "edit.replace_project"
        );
        assert!(matching_commands("move up", true, false).is_empty());
        assert_eq!(
            matching_commands("move up", true, true)[0].id,
            "structure.move_up"
        );
    }
}
