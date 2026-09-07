//! Session authorization and appearance delivery at the UI service boundary.

use parchmint_platform_api::WindowCapability;
use parchmint_preferences::{ResolvedAppearance, ThemeSnapshot};
use parchmint_ui_api::{ProjectSessionRegistry, apply_appearance_events};

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
