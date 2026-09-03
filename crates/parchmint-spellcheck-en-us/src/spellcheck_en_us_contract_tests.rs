//! Behavioral contracts for the private offline `en-US` spellcheck service.
//!
//! The implementation supplies `SpellcheckTestHarness` only under `cfg(test)`.
//! It must drive the real `EnUsSpellcheckService` while exposing scheduling,
//! persisted-dictionary, and network-observation controls.  Those controls are
//! deliberately ParchMint test values; no spelling-engine type is expected here.

use parchmint_editor_api::DocumentPosition;
use parchmint_spellcheck_api::{
    BlockId, DictionaryRevision, DocumentId, EditorRevision, EditorSelection, LanguageId,
    ProjectId, SpellcheckGeneration, SpellcheckPriority, SpellcheckRequest, SpellcheckService,
    SuggestionRequest,
};

use crate::{EnUsSpellcheckService, SpellcheckTestHarness, block_on_test};

fn document(value: u8) -> DocumentId {
    DocumentId::from_bytes([value; 16])
}

fn block(value: u8) -> BlockId {
    BlockId::from_bytes([value; 16])
}

fn selection(start: u64, end: u64) -> EditorSelection {
    EditorSelection::new(DocumentPosition::from(start), DocumentPosition::from(end))
}

fn check_request(
    document_id: DocumentId,
    document_revision: u64,
    project_dictionary: u64,
    global_dictionary: u64,
    generation: u64,
    priority: SpellcheckPriority,
    text: &str,
) -> SpellcheckRequest {
    SpellcheckRequest {
        language: LanguageId::EnUs,
        document_id,
        project_id: ProjectId::from_bytes([1; 16]),
        document_revision: EditorRevision::from(document_revision),
        blocks: vec![parchmint_spellcheck_api::RevisionedTextRange {
            block_id: block(2),
            range: selection(0, text.len() as u64),
            text: text.into(),
        }],
        project_dictionary: DictionaryRevision::from(project_dictionary),
        global_dictionary: DictionaryRevision::from(global_dictionary),
        generation: SpellcheckGeneration::from(generation),
        priority,
    }
}

fn suggestions_for(
    service: &EnUsSpellcheckService,
    document_id: DocumentId,
    word: &str,
    project_dictionary: u64,
    global_dictionary: u64,
) -> Vec<String> {
    let suggestions = block_on_test(service.suggest(SuggestionRequest {
        document_id,
        project_id: ProjectId::from_bytes([1; 16]),
        block_id: block(2),
        range: selection(0, word.len() as u64),
        word: word.into(),
        document_revision: EditorRevision::from(7),
        project_dictionary: DictionaryRevision::from(project_dictionary),
        global_dictionary: DictionaryRevision::from(global_dictionary),
    }))
    .expect("offline en-US suggestions");

    assert!(
        suggestions
            .windows(2)
            .all(|pair| pair[0].rank < pair[1].rank)
    );
    suggestions
        .into_iter()
        .map(|suggestion| suggestion.word)
        .collect()
}

fn project_suggestions(
    service: &EnUsSpellcheckService,
    document_id: DocumentId,
    project_id: ProjectId,
    word: &str,
    revision: DictionaryRevision,
) -> Vec<String> {
    block_on_test(service.suggest(SuggestionRequest {
        document_id,
        project_id,
        block_id: block(2),
        range: selection(0, word.len() as u64),
        word: word.into(),
        document_revision: EditorRevision::from(1),
        project_dictionary: revision,
        global_dictionary: DictionaryRevision::default(),
    }))
    .expect("project suggestions")
    .into_iter()
    .map(|suggestion| suggestion.word)
    .collect()
}

#[test]
fn bundled_en_us_is_the_only_available_language_and_suggestions_are_ranked() {
    let harness = SpellcheckTestHarness::new();
    let service = harness.service();

    assert_eq!(
        block_on_test(service.available_languages()).expect("bundled language availability"),
        vec![LanguageId::EnUs]
    );
    assert_eq!(
        suggestions_for(service, document(1), "teh", 0, 0),
        vec!["the".to_owned()]
    );
}

#[test]
fn project_and_global_dictionary_actions_change_subsequent_checks() {
    let harness = SpellcheckTestHarness::new();
    let service = harness.service();
    let project = ProjectId::from_bytes([3; 16]);
    let document_id = document(3);

    harness.save_project_word(project, DictionaryRevision::from(1), "ParchMint");
    block_on_test(service.reload_project_dictionary(project, DictionaryRevision::from(1)))
        .expect("project dictionary reload");
    let mut project_request = check_request(
        document_id,
        7,
        1,
        0,
        1,
        SpellcheckPriority::Visible,
        "ParchMint",
    );
    project_request.project_id = project;
    assert!(
        harness
            .check(project_request)
            .expect("project dictionary check")
            .is_empty()
    );

    harness.save_global_word(DictionaryRevision::from(1), "Quillflux");
    block_on_test(service.reload_global_dictionary(DictionaryRevision::from(1)))
        .expect("global dictionary reload");
    assert!(
        harness
            .check(check_request(
                document_id,
                8,
                1,
                1,
                2,
                SpellcheckPriority::Visible,
                "Quillflux",
            ))
            .expect("global dictionary check")
            .is_empty()
    );
}

#[test]
fn project_dictionaries_do_not_leak_between_projects_at_same_revision() {
    let harness = SpellcheckTestHarness::new();
    let service = harness.service();
    let first = ProjectId::from_bytes([1; 16]);
    let second = ProjectId::from_bytes([2; 16]);
    let revision = DictionaryRevision::from(1);
    harness.save_project_word(first, revision, "Zzquillfluxzz");
    harness.save_project_word(second, revision, "Zzotherwordzz");
    block_on_test(service.reload_project_dictionary(first, revision)).expect("first reload");
    block_on_test(service.reload_project_dictionary(second, revision)).expect("second reload");
    let misspelling = "Zzquillfluz";

    let mut first_request = check_request(
        document(20),
        1,
        1,
        0,
        1,
        SpellcheckPriority::Visible,
        "Zzquillfluxzz",
    );
    first_request.project_id = first;
    assert!(
        harness
            .check(first_request)
            .expect("first project check")
            .is_empty()
    );
    let mut second_request = check_request(
        document(21),
        1,
        1,
        0,
        1,
        SpellcheckPriority::Visible,
        "Zzquillfluxzz",
    );
    second_request.project_id = second;
    assert_eq!(
        harness
            .check(second_request)
            .expect("second project check")
            .len(),
        1
    );

    let first_suggestions =
        project_suggestions(service, document(20), first, misspelling, revision);
    assert_eq!(first_suggestions, vec!["Zzquillfluxzz"]);
    let second_suggestions =
        project_suggestions(service, document(21), second, misspelling, revision);
    assert!(
        !second_suggestions
            .iter()
            .any(|word| word == "Zzquillfluxzz")
    );
}

#[test]
fn stale_results_are_not_delivered_after_a_newer_text_or_dictionary_generation() {
    let harness = SpellcheckTestHarness::new();
    let document_id = document(4);
    let stale = check_request(document_id, 7, 0, 0, 1, SpellcheckPriority::Visible, "teh");
    let current = check_request(document_id, 8, 1, 0, 2, SpellcheckPriority::Visible, "teh");

    harness.pause_worker();
    harness.enqueue(stale);
    harness.enqueue(current.clone());
    harness.resume_worker();

    let results = harness.finish_queued_checks().expect("newest check result");
    assert_eq!(results.len(), 1);
    assert!(current.accepts(&results[0]));
}

#[test]
fn a_failed_reload_does_not_lose_the_saved_project_dictionary_change() {
    let harness = SpellcheckTestHarness::new();
    let service = harness.service();
    let project = ProjectId::from_bytes([5; 16]);
    let revision = DictionaryRevision::from(2);

    harness.save_project_word(project, revision, "ParchMint");
    harness.fail_next_project_dictionary_reload();
    assert!(block_on_test(service.reload_project_dictionary(project, revision)).is_err());
    assert!(
        harness
            .saved_project_words(project, revision)
            .iter()
            .any(|word| word == "ParchMint")
    );

    block_on_test(service.reload_project_dictionary(project, revision)).expect("reload retry");
    let mut retry_request = check_request(
        document(5),
        7,
        revision.value(),
        0,
        1,
        SpellcheckPriority::Visible,
        "ParchMint",
    );
    retry_request.project_id = project;
    assert!(
        harness
            .check(retry_request)
            .expect("retry check")
            .is_empty()
    );
}

#[test]
fn cancelled_work_never_delivers_a_result() {
    let harness = SpellcheckTestHarness::new();
    let service = harness.service();

    harness.pause_worker();
    let handle = harness.enqueue(check_request(
        document(7),
        7,
        0,
        0,
        1,
        SpellcheckPriority::Visible,
        "teh",
    ));
    service.cancel(handle);
    harness.resume_worker();

    assert!(
        harness
            .finish_queued_checks()
            .expect("cancelled check completion")
            .is_empty()
    );
}

#[test]
fn bounded_work_prefers_viewport_and_recent_changes_over_background() {
    let harness = SpellcheckTestHarness::with_queue_capacity(2);

    harness.pause_worker();
    harness.enqueue(check_request(
        document(8),
        7,
        0,
        0,
        1,
        SpellcheckPriority::Background,
        "teh",
    ));
    harness.enqueue(check_request(
        document(8),
        8,
        0,
        0,
        2,
        SpellcheckPriority::RecentlyChanged,
        "quik",
    ));
    harness.enqueue(check_request(
        document(8),
        9,
        0,
        0,
        3,
        SpellcheckPriority::Visible,
        "recieve",
    ));

    assert_eq!(harness.queued_request_count(), 2);
    assert_eq!(
        harness.queued_priorities(),
        vec![
            SpellcheckPriority::Visible,
            SpellcheckPriority::RecentlyChanged
        ]
    );
}

#[test]
fn contract_facing_service_is_platform_neutral() {
    fn assert_platform_neutral<T: SpellcheckService + Send + Sync + 'static>() {}

    assert_platform_neutral::<EnUsSpellcheckService>();
}
