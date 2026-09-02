//! Requirements-first contracts for the Iced editor workspace.
//!
//! These tests intentionally name the editor-workspace presentation contract
//! before the Stage 36 implementation exists.  They stay at the UI boundary:
//! messages yield adapter-facing effects, while fixture and layout assertions
//! remain deterministic and headless.

use parchmint_preferences::ResolvedAppearance;
use parchmint_ui_iced::{
    AsyncEditorCompletion, AsyncEditorPayload, CommentAnchor, EditorCommand, EditorEffect,
    EditorFixture, EditorMessage, EditorPane, EditorTask, EditorWorkspace, FindMatch,
    FormattingCommand, LocalSearchState, Point, Rect, SpellingMenuRequest, StatusCount, TabSpec,
};

#[test]
fn dual_pane_and_same_document_fixtures_remain_distinct_in_visual_and_headless_state() {
    let dual = EditorWorkspace::from_fixture(EditorFixture::DualPane);
    let same_document = EditorWorkspace::from_fixture(EditorFixture::SameDocumentTwoViews);

    assert_eq!(
        dual.fixture_reference(ResolvedAppearance::Light),
        "editor-dual-light"
    );
    assert_eq!(
        dual.fixture_reference(ResolvedAppearance::Dark),
        "editor-dual-dark"
    );
    assert_eq!(
        same_document.fixture_reference(ResolvedAppearance::Light),
        "editor-same-document-two-views-light"
    );

    assert_ne!(
        dual.pane(EditorPane::Primary).active_document(),
        dual.pane(EditorPane::Companion).active_document(),
        "the dual-pane fixture proves two distinct Manuscript documents"
    );
    assert_eq!(
        same_document.pane(EditorPane::Primary).active_document(),
        same_document.pane(EditorPane::Companion).active_document(),
        "the same-document fixture proves two views of one document"
    );
    assert_ne!(
        same_document.pane(EditorPane::Primary).view(),
        same_document.pane(EditorPane::Companion).view(),
        "two mounted views must retain distinct view identities"
    );
    assert_ne!(
        same_document.pane(EditorPane::Primary).scroll_offset(),
        same_document.pane(EditorPane::Companion).scroll_offset(),
        "the fixture makes independent presentation state observable"
    );
}

#[test]
fn formatting_commands_and_undo_route_to_the_focused_pane_without_losing_context() {
    let mut workspace = EditorWorkspace::from_fixture(EditorFixture::SameDocumentTwoViews);
    let companion = workspace.pane(EditorPane::Companion).view();

    workspace.update(EditorMessage::FocusPane(EditorPane::Companion));
    let effects = workspace.update(EditorMessage::Format(FormattingCommand::Bold));
    assert_eq!(
        effects,
        [
            EditorEffect::Command {
                view: companion,
                command: EditorCommand::ToggleBold,
            },
            EditorEffect::RestoreEditorFocus { view: companion },
        ]
    );

    workspace.update(EditorMessage::FocusFormattingToolbar);
    let effects = workspace.update(EditorMessage::Undo);
    assert_eq!(
        effects,
        [EditorEffect::Command {
            view: companion,
            command: EditorCommand::Undo,
        }],
        "toolbar focus must retain the last focused editor view for shared undo"
    );
}

#[test]
fn link_editor_applies_trimmed_targets_removes_links_and_cancels_without_commands() {
    let mut workspace = EditorWorkspace::from_fixture(EditorFixture::SameDocumentTwoViews);
    let companion = workspace.pane(EditorPane::Companion).view();
    workspace.update(EditorMessage::FocusPane(EditorPane::Companion));

    assert!(
        workspace
            .update(EditorMessage::Format(FormattingCommand::Link))
            .is_empty()
    );
    assert!(workspace.link_editor().is_open());
    workspace.update(EditorMessage::SetLinkTarget(
        "  https://example.com/story  ".into(),
    ));
    assert_eq!(
        workspace.update(EditorMessage::ApplyLink),
        [EditorEffect::Command {
            view: companion,
            command: EditorCommand::SetLink {
                target: Some("https://example.com/story".into()),
            },
        }]
    );
    assert!(!workspace.link_editor().is_open());

    workspace.update(EditorMessage::OpenLinkEditor);
    assert_eq!(
        workspace.update(EditorMessage::RemoveLink),
        [EditorEffect::Command {
            view: companion,
            command: EditorCommand::SetLink { target: None },
        }]
    );

    workspace.update(EditorMessage::OpenLinkEditor);
    workspace.update(EditorMessage::SetLinkTarget("   ".into()));
    assert!(workspace.update(EditorMessage::ApplyLink).is_empty());
    assert_eq!(
        workspace.link_editor().validation_error(),
        Some("Enter a URL before applying a link.")
    );
    assert!(workspace.link_editor().is_open());
    assert!(workspace.update(EditorMessage::CancelLinkEditor).is_empty());
    assert!(!workspace.link_editor().is_open());
}

#[test]
fn local_find_and_replace_are_independent_per_view_and_replacement_is_editor_scoped() {
    let mut workspace = EditorWorkspace::from_fixture(EditorFixture::SameDocumentTwoViews);
    let primary = workspace.pane(EditorPane::Primary).view();
    let companion = workspace.pane(EditorPane::Companion).view();

    workspace.update(EditorMessage::FocusPane(EditorPane::Primary));
    workspace.update(EditorMessage::OpenLocalFind);
    workspace.update(EditorMessage::SetFindQuery("river".into()));
    workspace.update(EditorMessage::SetFindMatches(vec![FindMatch::new(3, 8)]));

    workspace.update(EditorMessage::FocusPane(EditorPane::Companion));
    workspace.update(EditorMessage::OpenLocalFind);
    workspace.update(EditorMessage::SetFindQuery("mountain".into()));
    workspace.update(EditorMessage::SetFindMatches(vec![FindMatch::new(11, 19)]));
    workspace.update(EditorMessage::SetReplaceVisible(true));

    assert_eq!(
        workspace.local_search(primary),
        &LocalSearchState::open("river", vec![FindMatch::new(3, 8)])
    );
    assert_eq!(workspace.local_search(companion).query(), "mountain");
    assert!(workspace.local_search(companion).replace_visible());
    assert!(!workspace.local_search(primary).replace_visible());

    let effects = workspace.update(EditorMessage::ReplaceActiveMatch("valley".into()));
    assert_eq!(
        effects,
        [EditorEffect::Command {
            view: companion,
            command: EditorCommand::ReplaceActiveFindMatch {
                replacement: "valley".into(),
            },
        }],
        "local replacement must originate from the focused view and enter document undo"
    );
}

#[test]
fn selecting_a_comment_navigates_and_highlights_its_anchor_in_the_last_focused_view() {
    let mut workspace = EditorWorkspace::from_fixture(EditorFixture::SameDocumentTwoViews);
    let companion = workspace.pane(EditorPane::Companion).view();
    workspace.update(EditorMessage::FocusPane(EditorPane::Companion));
    workspace.update(EditorMessage::SetCommentAnchor {
        comment_id: "comment-17".into(),
        anchor: CommentAnchor::Range {
            document_id: "chapter-one".into(),
            range: FindMatch::new(17, 24),
            quote: "highlighted passage".into(),
        },
    });

    let effects = workspace.update(EditorMessage::SelectComment("comment-17".into()));
    assert_eq!(
        effects,
        [EditorEffect::NavigateCommentAnchor {
            view: companion,
            comment_id: "comment-17".into(),
            highlight: true,
        }]
    );
}

#[test]
fn spelling_menu_uses_the_misspelled_word_geometry_and_stays_inside_its_pane() {
    let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
    let pane_bounds = Rect::new(720.0, 84.0, 360.0, 784.0);
    let word_bounds = Rect::new(1038.0, 812.0, 38.0, 18.0);

    let effects = workspace.update(EditorMessage::OpenSpellingMenu(SpellingMenuRequest::new(
        EditorPane::Companion,
        "teh",
        word_bounds,
        pane_bounds,
    )));
    let menu = match effects.as_slice() {
        [EditorEffect::ShowSpellingMenu(menu)] => menu,
        other => panic!("expected one spelling-menu effect, got {other:?}"),
    };

    assert_eq!(menu.anchor_bounds(), word_bounds);
    assert_eq!(menu.pane(), EditorPane::Companion);
    assert!(menu.bounds().left() >= pane_bounds.left());
    assert!(menu.bounds().right() <= pane_bounds.right());
    assert!(menu.bounds().top() >= pane_bounds.top());
    assert!(menu.bounds().bottom() <= pane_bounds.bottom());
    assert_eq!(menu.invocation_point(), Point::new(1057.0, 821.0));
}

#[test]
fn overflowing_tabs_shrink_uniformly_with_titles_tooltips_and_close_targets_preserved() {
    let tabs = [
        TabSpec::new("one", "A Very Long Chapter One"),
        TabSpec::new("two", "A Very Long Chapter Two"),
        TabSpec::new("three", "A Very Long Chapter Three"),
        TabSpec::new("four", "A Very Long Chapter Four"),
    ];
    let layout = EditorWorkspace::tab_strip_layout(260.0, &tabs, "three");

    let widths = layout
        .tabs()
        .iter()
        .map(|tab| tab.bounds().width())
        .collect::<Vec<_>>();
    assert!(widths.windows(2).all(|pair| pair[0] == pair[1]));
    for tab in layout.tabs() {
        assert!(
            tab.display_title()
                .starts_with(tab.full_title().chars().next().unwrap())
        );
        assert!(tab.display_title().ends_with('…'));
        assert_eq!(tab.tooltip(), Some(tab.full_title()));
        assert!(tab.close_bounds().width() > 0.0);
        assert!(tab.close_bounds().left() >= tab.bounds().left());
        assert!(tab.close_bounds().right() <= tab.bounds().right());
    }
    assert_eq!(layout.active_tab().id(), "three");
}

#[test]
fn focused_pane_is_visibly_distinguishable_in_light_and_dark_themes() {
    let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
    workspace.update(EditorMessage::FocusPane(EditorPane::Companion));

    for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
        let primary = workspace.pane_focus_style(EditorPane::Primary, appearance);
        let companion = workspace.pane_focus_style(EditorPane::Companion, appearance);

        assert!(!primary.is_focused());
        assert!(companion.is_focused());
        assert!(companion.is_visible_against(appearance));
        assert_ne!(primary, companion);
    }
}

#[test]
fn status_bar_prefers_selection_then_active_document_and_always_exposes_manuscript_total() {
    let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);

    assert_eq!(
        workspace.status_bar().current_count(),
        StatusCount::ActiveDocument(412)
    );
    assert_eq!(workspace.status_bar().manuscript_total(), 1_204);

    workspace.update(EditorMessage::SetSelectionWordCount {
        pane: EditorPane::Primary,
        words: Some(7),
    });
    assert_eq!(
        workspace.status_bar().current_count(),
        StatusCount::Selection(7)
    );
    assert_eq!(workspace.status_bar().manuscript_total(), 1_204);

    workspace.update(EditorMessage::SetSelectionWordCount {
        pane: EditorPane::Primary,
        words: None,
    });
    assert_eq!(
        workspace.status_bar().current_count(),
        StatusCount::ActiveDocument(412)
    );
}

#[test]
fn asynchronous_editor_messages_apply_only_to_the_exact_live_request_and_view() {
    let mut workspace = EditorWorkspace::from_fixture(EditorFixture::SameDocumentTwoViews);
    let view = workspace.pane(EditorPane::Companion).view();
    workspace.update(EditorMessage::FocusPane(EditorPane::Companion));
    let first = workspace.begin_task(EditorTask::RefreshSpellcheck { view });
    let second = workspace.begin_task(EditorTask::RefreshSpellcheck { view });

    assert!(
        !workspace.accept_completion(AsyncEditorCompletion::for_ticket(
            first,
            AsyncEditorPayload::SpellcheckApplied,
        ))
    );
    assert!(
        workspace.accept_completion(AsyncEditorCompletion::for_ticket(
            second,
            AsyncEditorPayload::SpellcheckApplied,
        ))
    );

    let claimed_view = workspace.pane(EditorPane::Primary).view();
    assert!(!workspace.accept_completion(AsyncEditorCompletion::new(
        claimed_view,
        EditorTask::RefreshSpellcheck { view },
        AsyncEditorPayload::SpellcheckApplied,
    )));
}
