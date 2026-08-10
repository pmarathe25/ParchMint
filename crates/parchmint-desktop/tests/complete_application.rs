//! Requirements-first integration gates for the complete desktop application.
//!
//! These tests deliberately live at the desktop boundary. They must drive the
//! production graph only; component fakes and injected desktop services prove
//! their own contracts elsewhere. The production graph is available; scenario
//! gates remain ignored until their named native or rendered evidence exists.

use parchmint_desktop::DesktopBootstrap;
use parchmint_editor_core::feasibility::{PlatformMeasurements, all_platform_budgets_pass};
use parchmint_test_support::complete_application::{
    CompleteApplicationFixture, LARGE_DOCUMENT_WORDS, NORMAL_DOCUMENT_WORDS, ViewCount,
    ViewTopology,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanonicalFlow {
    CreateAndWrite,
    Organize,
    Compare,
    SameDocumentTwice,
    PlanInCards,
    Comment,
    SearchAndReplace,
    Recover,
    DeleteAndRestore,
    Spellcheck,
    Appearance,
    Export,
    MovePlatform,
}

impl CanonicalFlow {
    const ALL: [Self; 13] = [
        Self::CreateAndWrite,
        Self::Organize,
        Self::Compare,
        Self::SameDocumentTwice,
        Self::PlanInCards,
        Self::Comment,
        Self::SearchAndReplace,
        Self::Recover,
        Self::DeleteAndRestore,
        Self::Spellcheck,
        Self::Appearance,
        Self::Export,
        Self::MovePlatform,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureScale {
    Normal,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FixtureProfile {
    scale: FixtureScale,
    topology: ViewTopology,
}

impl FixtureProfile {
    fn build(self) -> CompleteApplicationFixture {
        match self.scale {
            FixtureScale::Normal => CompleteApplicationFixture::normal_with_topology(self.topology),
            FixtureScale::Large => CompleteApplicationFixture::large_with_topology(self.topology),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScenarioRequirement {
    flow: CanonicalFlow,
    fixture: FixtureProfile,
    observations: &'static [&'static str],
}

const NORMAL_ONE_VIEW: FixtureProfile = FixtureProfile {
    scale: FixtureScale::Normal,
    topology: ViewTopology::OneView,
};

const NORMAL_TWO_VIEWS: FixtureProfile = FixtureProfile {
    scale: FixtureScale::Normal,
    topology: ViewTopology::SameDocumentTwoViews,
};

const NORMAL_DISTINCT_DOCUMENTS_TWO_VIEWS: FixtureProfile = FixtureProfile {
    scale: FixtureScale::Normal,
    topology: ViewTopology::DistinctDocumentsTwoViews,
};

const LARGE_ONE_VIEW: FixtureProfile = FixtureProfile {
    scale: FixtureScale::Large,
    topology: ViewTopology::OneView,
};

const LARGE_TWO_VIEWS: FixtureProfile = FixtureProfile {
    scale: FixtureScale::Large,
    topology: ViewTopology::SameDocumentTwoViews,
};

const CANONICAL_SCENARIOS: &[ScenarioRequirement] = &[
    ScenarioRequirement {
        flow: CanonicalFlow::CreateAndWrite,
        fixture: NORMAL_ONE_VIEW,
        observations: &[
            "initial Untitled Document",
            "autosave",
            "reopen has typed content",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::Organize,
        fixture: NORMAL_ONE_VIEW,
        observations: &[
            "nested hierarchy",
            "multi-select drag",
            "duplicate and cut/paste moves",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::Compare,
        fixture: NORMAL_DISTINCT_DOCUMENTS_TWO_VIEWS,
        observations: &[
            "distinct documents",
            "focus changes Inspector",
            "toolbar targets focused pane",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::SameDocumentTwice,
        fixture: NORMAL_TWO_VIEWS,
        observations: &[
            "independent scroll",
            "independent selection",
            "shared edit and undo",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::PlanInCards,
        fixture: NORMAL_ONE_VIEW,
        observations: &["Synopsis", "metadata", "reorder", "open document in Editor"],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::Comment,
        fixture: NORMAL_ONE_VIEW,
        observations: &["select", "reply", "resolve", "navigate", "reopen"],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::SearchAndReplace,
        fixture: NORMAL_TWO_VIEWS,
        observations: &[
            "local search per view",
            "global preview",
            "one project replacement operation",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::Recover,
        fixture: NORMAL_ONE_VIEW,
        observations: &[
            "replay unsaved input",
            "canonical consistency",
            "History consistency",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::DeleteAndRestore,
        fixture: NORMAL_ONE_VIEW,
        observations: &[
            "delete subtree",
            "session undo",
            "Recently Deleted or History restore",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::Spellcheck,
        fixture: NORMAL_ONE_VIEW,
        observations: &[
            "in-place misspelling",
            "suggestion",
            "project and global dictionaries",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::Appearance,
        fixture: NORMAL_ONE_VIEW,
        observations: &[
            "System",
            "Light",
            "Dark",
            "all open windows",
            "project and export unchanged",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::Export,
        fixture: NORMAL_ONE_VIEW,
        observations: &[
            "title option",
            "page-break option",
            "one self-contained HTML file",
        ],
    },
    ScenarioRequirement {
        flow: CanonicalFlow::MovePlatform,
        fixture: NORMAL_ONE_VIEW,
        observations: &[
            "clean close",
            "identical hierarchy",
            "identical content and History",
        ],
    },
];

/// Measurements that an agreed reference-hardware runner must hand to this
/// test. Building this descriptor, or timing a headless fixture, is not
/// release evidence.
struct ReferenceHardwareMeasurements {
    hardware_profile: String,
    platform_measurements: Vec<PlatformMeasurements>,
}

fn require_reference_hardware_measurements() -> ReferenceHardwareMeasurements {
    panic!(
        "Stage 38 needs a Windows/macOS/Linux runner that records real key-to-paint, UI-turn, warm-viewport, and memory-cycle measurements on agreed reference hardware"
    );
}

fn require_production_graph() {
    DesktopBootstrap::production().expect(
        "Stage 38 must assemble the production desktop graph before complete-application scenarios can run",
    );
}

#[test]
fn fixture_catalog_covers_normal_and_release_gate_document_sizes_in_one_and_two_views() {
    for views in [ViewCount::One, ViewCount::Two] {
        let normal = CompleteApplicationFixture::normal(views);
        let large = CompleteApplicationFixture::large(views);

        assert_eq!(normal.word_count(), NORMAL_DOCUMENT_WORDS);
        assert_eq!(normal.views(), views);
        assert_eq!(large.word_count(), LARGE_DOCUMENT_WORDS);
        assert_eq!(large.views(), views);
        assert!(!normal.body().is_empty());
        assert!(!large.body().is_empty());
    }
}

#[test]
fn scenario_catalog_maps_each_canonical_flow_to_a_fixture_and_observable_outcome() {
    assert_eq!(CANONICAL_SCENARIOS.len(), CanonicalFlow::ALL.len());

    for flow in CanonicalFlow::ALL {
        let matching: Vec<_> = CANONICAL_SCENARIOS
            .iter()
            .filter(|scenario| scenario.flow == flow)
            .collect();
        assert_eq!(matching.len(), 1, "each canonical flow needs one scenario");
        assert!(!matching[0].observations.is_empty());
        let fixture = matching[0].fixture.build();
        assert!(!fixture.body().is_empty());
        assert_eq!(
            fixture.companion_body().is_some(),
            matching[0].fixture.topology.document_count() == 2
        );
    }

    for profile in [LARGE_ONE_VIEW, LARGE_TWO_VIEWS] {
        let fixture = profile.build();
        assert_eq!(fixture.word_count(), LARGE_DOCUMENT_WORDS);
        assert_eq!(fixture.views(), profile.topology.views());
    }
}

#[test]
fn production_graph_is_available_to_complete_application_tests() {
    // This gate proves the assembled service graph exists. The ignored
    // scenarios below still require their named real interaction evidence.
    require_production_graph();
}

#[test]
#[ignore = "requires a desktop interaction driver"]
fn create_write_autosave_close_and_reopen() {
    let fixture = CompleteApplicationFixture::normal(ViewCount::One);
    assert_eq!(fixture.word_count(), NORMAL_DOCUMENT_WORDS);
    require_production_graph();
    // Drive launch, project creation, typing, autosave, close, and reopen.
}

#[test]
#[ignore = "requires a desktop interaction driver"]
fn organize_nested_content_with_multiselect_drag_duplicate_cut_paste_and_cross_section_move() {
    require_production_graph();
    // Drive the complete Organize canonical flow through the explorer and Cards.
}

#[test]
#[ignore = "requires a desktop interaction driver"]
fn compare_documents_and_route_focus_to_the_matching_inspector_and_toolbar() {
    let fixture = CompleteApplicationFixture::normal(ViewCount::Two);
    assert_eq!(fixture.views().value(), 2);
    require_production_graph();
    // Open Manuscript and Research or another Manuscript document in companion panes.
}

#[test]
#[ignore = "requires a desktop interaction driver"]
fn same_document_in_two_views_keeps_scroll_and_selection_independent_with_shared_edit_and_undo() {
    let fixture = CompleteApplicationFixture::normal(ViewCount::Two);
    assert_eq!(fixture.views(), ViewCount::Two);
    require_production_graph();
    // Verify independent view state and one shared document history.
}

#[test]
#[ignore = "requires a desktop interaction driver"]
fn cards_edits_synopsis_metadata_expansion_order_and_editor_navigation() {
    require_production_graph();
    // Drive Synopsis and metadata edits, expand/collapse, reorder, then open Editor.
}

#[test]
#[ignore = "requires a desktop interaction driver"]
fn comments_select_reply_resolve_navigate_and_reopen() {
    require_production_graph();
    // Drive the editor context menu and assert persisted comment-thread behavior.
}

#[test]
#[ignore = "requires a desktop interaction driver"]
fn local_and_global_search_replace_apply_one_project_operation() {
    require_production_graph();
    // Verify local Find per view and revalidated global replacement preview/application.
}

#[test]
#[ignore = "requires a controllable termination boundary"]
fn recovery_replays_unsaved_input_without_diverging_canonical_files_or_history() {
    require_production_graph();
    // Terminate after recovery receipt and before durable save; reopen and compare all three states.
}

#[test]
#[ignore = "requires a desktop interaction driver to observe recovery fault isolation"]
fn recovery_fault_isolated_from_the_last_valid_record_and_the_user_can_continue_safely() {
    require_production_graph();
    // Corrupt the newest journal record and verify valid recovery plus a visible isolated-record error.
}

#[test]
#[ignore = "requires a desktop interaction driver"]
fn delete_restore_and_history_restore_preserve_the_complete_subtree() {
    require_production_graph();
    // Cover in-session undo, Recently Deleted, and whole-project History restoration.
}

#[test]
#[ignore = "requires a desktop interaction driver to observe History recovery"]
fn history_fault_keeps_the_current_project_openable_and_offers_safe_history_reinitialization() {
    require_production_graph();
    // Corrupt or remove History after a durable save without corrupting current canonical resources.
}

#[test]
#[ignore = "requires a desktop interaction driver to observe search recovery"]
fn search_fault_rebuilds_from_canonical_and_open_editor_sources_without_changing_project_state() {
    require_production_graph();
    // Remove or corrupt the index, rebuild, and verify stale batches cannot alter the current UI.
}

#[test]
#[ignore = "requires a desktop interaction driver"]
fn spellcheck_suggestion_and_project_global_dictionary_mutations_survive_save_and_reopen() {
    require_production_graph();
    // Exercise misspelling decoration, suggestion selection, and project/global dictionary scopes.
}

#[test]
#[ignore = "requires a desktop interaction driver to observe spellcheck recovery"]
fn spellcheck_fault_is_visible_per_view_and_a_later_exact_generation_recovers() {
    require_production_graph();
    // Fail a dictionary reload or worker request, then require a later matching generation to recover.
}

#[test]
#[ignore = "requires a rendered multiwindow desktop driver and approved visual review"]
fn system_light_and_dark_change_all_open_windows_without_changing_project_or_export() {
    require_production_graph();
    // Capture Light/Dark visual references and compare project/export bytes before and after.
}

#[test]
#[ignore = "requires a desktop interaction driver and export sink fault injector"]
fn export_generates_one_self_contained_html_file_and_leaves_project_state_unchanged_on_fault() {
    require_production_graph();
    // Cover title/page-break options plus write, cancellation, and output failures.
}

#[test]
#[ignore = "requires Windows, macOS, and Linux runners"]
fn canonical_project_interchange_is_identical_after_clean_cross_platform_reopen() {
    require_production_graph();
    // Use the same fixture in Windows, macOS, and Linux jobs and compare canonical bytes and History.
}

#[test]
#[ignore = "requires agreed reference hardware, real high-DPI windows, and native clipboard access"]
fn release_hardware_gate_measures_large_one_and_two_view_latency_memory_high_dpi_and_clipboard() {
    let one_view = CompleteApplicationFixture::large(ViewCount::One);
    let two_views = CompleteApplicationFixture::large(ViewCount::Two);
    assert_eq!(one_view.word_count(), LARGE_DOCUMENT_WORDS);
    assert_eq!(two_views.word_count(), LARGE_DOCUMENT_WORDS);
    require_production_graph();
    let report = require_reference_hardware_measurements();
    assert!(
        !report.hardware_profile.trim().is_empty(),
        "the runner must identify the agreed reference hardware profile"
    );
    assert!(
        all_platform_budgets_pass(&report.platform_measurements),
        "all three platform measurements must meet the existing latency and memory evaluator"
    );
    // The runner also records scale-factor and native clipboard observations; those remain
    // separate acceptance evidence because they are not part of the editor budget descriptor.
}

#[test]
fn canonical_flow_catalog_remains_complete() {
    assert_eq!(CanonicalFlow::ALL.len(), 13);
}
