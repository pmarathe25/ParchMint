//! Session authorization and appearance delivery at the UI service boundary.

use parchmint_platform_api::WindowCapability;
use parchmint_preferences::{ResolvedAppearance, ThemeSnapshot};
use parchmint_ui_api::{ProjectSessionRegistry, apply_appearance_events};

#[test]
fn note_tree_identity_detects_scope_changes_even_when_revisions_match() {
    use parchmint_domain::{DocumentId, NodeId, ProjectId, ProjectSection};
    use parchmint_ui_api::{AssistantNoteTreeEntry, assistant_note_tree_hash};
    let project = ProjectId::from_bytes([1; 16]);
    let root = NodeId::manuscript_root();
    let group = NodeId::from_bytes([2; 16]);
    let document = NodeId::from_bytes([3; 16]);
    let entries = vec![
        AssistantNoteTreeEntry {
            node: root,
            parent: None,
            kind: 0,
            root_section: Some(ProjectSection::Manuscript),
            document: None,
        },
        AssistantNoteTreeEntry {
            node: group,
            parent: Some(root),
            kind: 1,
            root_section: None,
            document: None,
        },
        AssistantNoteTreeEntry {
            node: document,
            parent: Some(group),
            kind: 2,
            root_section: None,
            document: Some(DocumentId::from_bytes([4; 16])),
        },
    ];
    let expected = assistant_note_tree_hash(project, entries.clone());
    let mut reordered = entries.clone();
    reordered.reverse();
    assert_eq!(assistant_note_tree_hash(project, reordered), expected);
    assert_ne!(
        assistant_note_tree_hash(ProjectId::from_bytes([5; 16]), entries.clone()),
        expected
    );
    let mut moved = entries.clone();
    moved[2].parent = Some(root);
    assert_ne!(assistant_note_tree_hash(project, moved), expected);
    let mut rebound = entries.clone();
    rebound[2].document = Some(DocumentId::from_bytes([6; 16]));
    assert_ne!(assistant_note_tree_hash(project, rebound), expected);
    let mut section_changed = entries.clone();
    section_changed[0].root_section = Some(ProjectSection::Lore);
    assert_ne!(assistant_note_tree_hash(project, section_changed), expected);
    let mut removed = entries.clone();
    removed.pop();
    assert_ne!(assistant_note_tree_hash(project, removed), expected);
}

#[test]
fn project_sessions_reject_stale_generations_after_recreation() {
    let mut sessions = ProjectSessionRegistry::new();
    let first = sessions.register(12);

    assert_eq!((first.session_id(), first.generation()), (12, 1));
    assert!(sessions.retire(first));
    assert!(!sessions.is_current(first));

    let replacement = sessions.register(12);
    assert_eq!(
        (replacement.session_id(), replacement.generation()),
        (12, 2)
    );
    assert!(sessions.is_current(replacement));
    assert!(!sessions.retire(first));
}

#[test]
fn appearance_events_apply_each_generation_in_window_id_order() {
    let snapshots = [
        ThemeSnapshot::new(ResolvedAppearance::Light, 3),
        ThemeSnapshot::new(ResolvedAppearance::Dark, 4),
    ];
    let mut applied = Vec::new();

    apply_appearance_events(
        &snapshots,
        &[
            WindowCapability::new(9, 5),
            WindowCapability::new(2, 8),
            WindowCapability::new(7, 3),
        ],
        |window, snapshot| {
            applied.push((snapshot.generation, window.window_id(), window.generation()));
        },
    );

    assert_eq!(
        applied,
        [
            (3, 2, 8),
            (3, 7, 3),
            (3, 9, 5),
            (4, 2, 8),
            (4, 7, 3),
            (4, 9, 5),
        ]
    );
}
