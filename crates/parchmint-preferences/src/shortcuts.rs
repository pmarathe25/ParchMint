//! Application command catalog and portable, persisted shortcut overrides.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Keybindings = BTreeMap<String, Option<Shortcut>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    pub key: String,
    /// Control = 1, Alt = 2, Shift = 4, Super/Command = 8.
    pub modifiers: u8,
}
impl Shortcut {
    pub fn new(key: impl Into<String>, modifiers: u8) -> Self {
        Self {
            key: {
                let key = key.into();
                if key.chars().count() == 1 {
                    key.to_uppercase()
                } else {
                    key
                }
            },
            modifiers,
        }
    }
    pub fn is_assignable(&self) -> bool {
        let function = self
            .key
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
        (self.key.chars().count() != 1 || self.key == self.key.to_uppercase())
            && !self.key.chars().any(char::is_whitespace)
            && self.modifiers <= 15
            && (function || self.key == "Delete" || self.modifiers & 11 != 0)
            && !matches!(self.key.as_str(), "Escape" | "Tab" | "Backspace")
            && (function
                || self.key.chars().count() == 1
                || matches!(
                    self.key.as_str(),
                    "Enter" | "PageUp" | "PageDown" | "Delete"
                ))
    }
}
impl std::fmt::Display for Shortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (flag, label) in [
            (1, "Ctrl+"),
            (
                8,
                if cfg!(target_os = "macos") {
                    "Cmd+"
                } else {
                    "Super+"
                },
            ),
            (2, "Alt+"),
            (4, "Shift+"),
        ] {
            if self.modifiers & flag != 0 {
                f.write_str(label)?;
            }
        }
        f.write_str(&self.key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutScope {
    Global,
    Editor,
    Overview,
}
impl ShortcutScope {
    pub fn is_active(self, active: Self) -> bool {
        self == Self::Global || self == active
    }
    fn overlaps(self, other: Self) -> bool {
        self == Self::Global || other == Self::Global || self == other
    }
}

#[derive(Debug, Clone)]
pub struct ShortcutCommand {
    pub scope: ShortcutScope,
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub default: Option<Shortcut>,
}

pub fn shortcut_commands() -> Vec<ShortcutCommand> {
    let primary = if cfg!(target_os = "macos") { 8 } else { 1 };
    let specs = [
        ("file.new", "New project", "Projects", "N", 0),
        ("file.open", "Open project", "Projects", "O", 0),
        ("file.projects", "Recent projects", "Projects", "O", 4),
        ("file.save", "Save", "Projects", "S", 0),
        ("file.close", "Close window", "Projects", "W", 4),
        ("file.new-tab", "New document tab", "Documents", "T", 0),
        ("tab.close", "Close tab", "Documents", "W", 0),
        (
            "tab.move-pane",
            "Move tab to other pane",
            "Documents",
            "\\",
            4,
        ),
        ("search.next", "Next local match", "Editing", "G", 0),
        ("search.previous", "Previous local match", "Editing", "G", 4),
        (
            "outline.open-beside",
            "Open selected document beside",
            "Overview",
            "Enter",
            2,
        ),
        ("tab.next", "Next tab", "Documents", "PageDown", 0),
        ("tab.previous", "Previous tab", "Documents", "PageUp", 0),
        ("view.editor", "Editor", "Navigation", "1", 0),
        ("view.overview", "Overview", "Navigation", "2", 0),
        ("view.history", "History", "Navigation", "3", 0),
        ("view.deleted", "Recently deleted", "Navigation", "4", 0),
        ("view.export", "Export", "Navigation", "X", 4),
        ("view.settings", "Settings", "Navigation", ",", 0),
        ("view.explorer", "Toggle Explorer", "Navigation", "E", 4),
        ("view.comments", "Toggle comments", "Navigation", "C", 4),
        (
            "view.companion",
            "Toggle companion pane",
            "Navigation",
            "\\",
            0,
        ),
        ("view.focus", "Focus document", "Navigation", "F11", 16),
        (
            "view.next-region",
            "Next focus region",
            "Navigation",
            "F6",
            16,
        ),
        ("zoom.in", "Zoom in", "Navigation", "=", 0),
        ("zoom.out", "Zoom out", "Navigation", "-", 0),
        ("zoom.reset", "Reset zoom", "Navigation", "0", 0),
        ("edit.undo", "Undo", "Editing", "Z", 0),
        (
            "edit.redo",
            "Redo",
            "Editing",
            if cfg!(target_os = "macos") { "Z" } else { "Y" },
            if cfg!(target_os = "macos") { 4 } else { 0 },
        ),
        ("edit.cut", "Cut", "Editing", "X", 0),
        ("edit.copy", "Copy", "Editing", "C", 0),
        ("edit.paste", "Paste", "Editing", "V", 0),
        (
            "edit.paste-plain",
            "Paste without formatting",
            "Editing",
            "V",
            4,
        ),
        ("edit.select-all", "Select all", "Editing", "A", 0),
        ("search.local", "Find in document", "Search", "F", 0),
        ("search.replace", "Replace in document", "Search", "H", 0),
        ("search.global", "Search project", "Search", "F", 4),
        ("format.bold", "Bold", "Formatting", "B", 0),
        ("format.italic", "Italic", "Formatting", "I", 0),
        ("format.underline", "Underline", "Formatting", "U", 0),
        (
            "format.strikethrough",
            "Strikethrough",
            "Formatting",
            "S",
            4,
        ),
        ("format.link", "Link", "Formatting", "K", 0),
        ("format.comment", "Add comment", "Formatting", "M", 2),
        ("format.numbered", "Numbered list", "Formatting", "7", 4),
        ("format.bulleted", "Bulleted list", "Formatting", "8", 4),
        ("format.quote", "Block quote", "Formatting", "9", 4),
        ("format.align-left", "Align left", "Formatting", "L", 0),
        ("format.align-center", "Center", "Formatting", "E", 0),
        ("format.align-right", "Align right", "Formatting", "R", 0),
        ("format.align-justify", "Justify", "Formatting", "J", 0),
        ("format.page-break", "Page break", "Formatting", "Enter", 0),
        (
            "format.scene-break",
            "Scene break",
            "Formatting",
            "Enter",
            4,
        ),
        ("format.body", "Body style", "Formatting", "0", 2),
        ("format.heading-1", "Heading 1", "Formatting", "1", 2),
        ("format.heading-2", "Heading 2", "Formatting", "2", 2),
        ("format.heading-3", "Heading 3", "Formatting", "3", 2),
        (
            "format.spacing-single",
            "Single line spacing",
            "Formatting",
            "",
            0,
        ),
        (
            "format.spacing-double",
            "Double line spacing",
            "Formatting",
            "",
            0,
        ),
        ("format.styles", "Manage styles", "Formatting", "", 0),
        (
            "outline.document",
            "New outline document",
            "Overview",
            "N",
            4,
        ),
        (
            "outline.next-document",
            "Create next document",
            "Overview",
            "Enter",
            0,
        ),
        (
            "outline.next-group",
            "Create next group",
            "Overview",
            "Enter",
            4,
        ),
        (
            "outline.delete",
            "Delete selected documents or groups",
            "Overview",
            "Delete",
            16,
        ),
        ("outline.rename", "Rename", "Overview", "F2", 16),
        ("outline.group", "New outline group", "Overview", "N", 6),
        (
            "outline.fields",
            "Manage metadata fields",
            "Overview",
            "F",
            2,
        ),
    ];
    specs
        .into_iter()
        .map(|(id, label, group, key, extra)| ShortcutCommand {
            scope: if id.starts_with("format.") && id != "format.styles"
                || id.starts_with("tab.")
                || matches!(
                    id,
                    "search.local"
                        | "search.replace"
                        | "search.next"
                        | "search.previous"
                        | "view.focus"
                        | "view.companion"
                ) {
                ShortcutScope::Editor
            } else if matches!(id, "outline.next-document" | "outline.next-group") {
                ShortcutScope::Overview
            } else {
                ShortcutScope::Global
            },
            id,
            label,
            group,
            default: (!key.is_empty())
                .then(|| Shortcut::new(key, if extra == 16 { 0 } else { primary | extra })),
        })
        .collect()
}

pub fn effective_shortcut(command: &ShortcutCommand, bindings: &Keybindings) -> Option<Shortcut> {
    bindings
        .get(command.id)
        .cloned()
        .unwrap_or_else(|| command.default.clone())
}

pub fn validate_keybindings(bindings: &Keybindings) -> Result<(), String> {
    let commands = shortcut_commands();
    for id in bindings.keys() {
        if !commands.iter().any(|command| command.id == id) {
            return Err(format!("Unknown command: {id}"));
        }
    }
    let mut assigned = Vec::new();
    for command in &commands {
        if let Some(shortcut) = effective_shortcut(command, bindings) {
            if !shortcut.is_assignable() {
                return Err("Use Ctrl, Alt, or Command with a key, or a function key.".into());
            }
            if let Some((_, label, _)) =
                assigned
                    .iter()
                    .find(|(key, _, scope): &&(Shortcut, &str, ShortcutScope)| {
                        *key == shortcut && scope.overlaps(command.scope)
                    })
            {
                return Err(format!("{shortcut} is already assigned to {label}."));
            }
            assigned.push((shortcut, command.label, command.scope));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_unique_and_overrides_detect_conflicts() {
        assert_eq!(validate_keybindings(&Keybindings::new()), Ok(()));
        let mut bindings = Keybindings::new();
        let commands = shortcut_commands();
        bindings.insert(
            "file.save".into(),
            commands
                .iter()
                .find(|c| c.id == "file.open")
                .unwrap()
                .default
                .clone(),
        );
        assert!(
            validate_keybindings(&bindings)
                .unwrap_err()
                .contains("Open project")
        );
        bindings.insert("file.open".into(), None);
        assert!(validate_keybindings(&bindings).is_ok());
    }
}
