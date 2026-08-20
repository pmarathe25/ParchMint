use std::sync::Arc;

use parchmint_editor_api::DocumentPosition;
use parchmint_spellcheck_api::{
    BlockId, DictionaryRevision, DocumentId, EditorRevision, EditorSelection, LanguageId,
    ProjectId, SpellcheckGeneration, SpellcheckPriority, SpellcheckRequest, SuggestionRequest,
};

use crate::{
    EnUsSpellcheckConfig, EnUsSpellcheckService, SpellcheckError, SpellcheckTestHarness,
    SpellcheckWorkerLimits, block_on_test, word_tokens,
};

fn request(text: &str) -> SpellcheckRequest {
    SpellcheckRequest {
        language: LanguageId::EnUs,
        document_id: DocumentId::from_bytes([1; 16]),
        project_id: ProjectId::from_bytes([1; 16]),
        document_revision: EditorRevision::from(1),
        blocks: vec![parchmint_spellcheck_api::RevisionedTextRange {
            block_id: BlockId::from_bytes([2; 16]),
            range: EditorSelection::new(DocumentPosition::from(10), DocumentPosition::from(30)),
            text: text.to_owned(),
        }],
        project_dictionary: DictionaryRevision::default(),
        global_dictionary: DictionaryRevision::default(),
        generation: SpellcheckGeneration::from(1),
        priority: SpellcheckPriority::Visible,
    }
}

#[test]
fn unicode_token_ranges_use_scalar_positions_and_keep_inner_apostrophes() {
    let tokens = word_tokens("élan isn’t teh");
    assert_eq!(
        tokens
            .iter()
            .map(|token| (token.word, token.start, token.end))
            .collect::<Vec<_>>(),
        vec![("élan", 0, 4), ("isn’t", 5, 10), ("teh", 11, 14)]
    );
}

#[test]
fn invalid_request_is_rejected_before_it_enters_the_worker() {
    let service = EnUsSpellcheckService::new(EnUsSpellcheckConfig {
        worker_limits: SpellcheckWorkerLimits {
            max_chars_per_block: 2,
            ..SpellcheckWorkerLimits::default()
        },
        ..EnUsSpellcheckConfig::default()
    })
    .expect("bounded service");

    assert!(matches!(
        block_on_test(service.check(request("three"))),
        Err(SpellcheckError::InvalidRequest(_))
    ));
}

#[test]
fn global_reload_failure_is_recoverable_without_erasing_saved_words() {
    let harness = SpellcheckTestHarness::new();
    let revision = DictionaryRevision::from(4);
    harness.save_global_word(revision, "Quillflux");
    harness.fail_next_global_dictionary_reload();
    assert!(block_on_test(harness.service().reload_global_dictionary(revision)).is_err());
    block_on_test(harness.service().reload_global_dictionary(revision)).expect("retry reload");

    let suggestions = block_on_test(harness.service().suggest(SuggestionRequest {
        document_id: DocumentId::from_bytes([1; 16]),
        project_id: ProjectId::from_bytes([1; 16]),
        block_id: BlockId::from_bytes([2; 16]),
        range: EditorSelection::new(DocumentPosition::from(0), DocumentPosition::from(9)),
        word: "Quillflux".to_owned(),
        document_revision: EditorRevision::from(1),
        project_dictionary: DictionaryRevision::default(),
        global_dictionary: revision,
    }))
    .expect("dictionary-backed suggestion request");
    assert!(suggestions.is_empty());
}

#[test]
fn zero_worker_limits_fail_at_startup() {
    let result = EnUsSpellcheckService::new(EnUsSpellcheckConfig {
        worker_limits: SpellcheckWorkerLimits {
            queue_capacity: 0,
            ..SpellcheckWorkerLimits::default()
        },
        saved_dictionaries: Arc::new(crate::EmptyDictionarySource),
        ..EnUsSpellcheckConfig::default()
    });
    assert!(result.is_err());
}

#[test]
fn oversized_suggestion_words_are_rejected_before_engine_work() {
    let service = EnUsSpellcheckService::default();
    let result = block_on_test(service.suggest(SuggestionRequest {
        document_id: DocumentId::from_bytes([1; 16]),
        project_id: ProjectId::from_bytes([1; 16]),
        block_id: BlockId::from_bytes([2; 16]),
        range: EditorSelection::new(DocumentPosition::from(0), DocumentPosition::from(257)),
        word: "x".repeat(257),
        document_revision: EditorRevision::from(1),
        project_dictionary: DictionaryRevision::default(),
        global_dictionary: DictionaryRevision::default(),
    }));
    assert!(matches!(result, Err(SpellcheckError::InvalidRequest(_))));
}

#[test]
fn saved_project_dictionary_is_selected_by_loaded_revision() {
    let harness = SpellcheckTestHarness::new();
    let project = ProjectId::from_bytes([9; 16]);
    harness.save_project_word(project, DictionaryRevision::from(2), "Zzquillfluxzz");
    block_on_test(
        harness
            .service()
            .reload_project_dictionary(project, DictionaryRevision::from(2)),
    )
    .expect("project dictionary reload");

    let mut current = request("Zzquillfluxzz");
    current.project_id = project;
    current.project_dictionary = DictionaryRevision::from(2);
    assert!(harness.check(current).expect("current revision").is_empty());

    let mut old = request("Zzquillfluxzz");
    old.project_id = project;
    old.document_id = DocumentId::from_bytes([2; 16]);
    old.project_dictionary = DictionaryRevision::from(1);
    assert!(!harness.check(old).expect("old revision").is_empty());
}
