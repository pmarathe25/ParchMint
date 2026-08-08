//! Requirements-first tests for the public spellcheck boundary.
//!
//! These tests use only ParchMint values. The private spelling runtime must
//! preserve the same stale-result, priority, and cancellation rules.

use std::collections::{BTreeSet, VecDeque};

use parchmint_domain::{BlockId, DocumentId, ProjectId};
use parchmint_editor_api::{DocumentPosition, EditorRevision, EditorSelection};

use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkItem {
    handle: SpellcheckHandle,
    priority: SpellcheckPriority,
}

#[derive(Default)]
struct WorkQueue {
    visible: VecDeque<WorkItem>,
    recently_changed: VecDeque<WorkItem>,
    background: VecDeque<WorkItem>,
    cancelled: BTreeSet<SpellcheckHandle>,
}

impl WorkQueue {
    fn push(&mut self, item: WorkItem) {
        match item.priority {
            SpellcheckPriority::Visible => self.visible.push_back(item),
            SpellcheckPriority::RecentlyChanged => self.recently_changed.push_back(item),
            SpellcheckPriority::Background => self.background.push_back(item),
        }
    }

    fn cancel(&mut self, handle: SpellcheckHandle) {
        self.cancelled.insert(handle);
    }

    fn pop(&mut self) -> Option<WorkItem> {
        loop {
            let item = self
                .visible
                .pop_front()
                .or_else(|| self.recently_changed.pop_front())
                .or_else(|| self.background.pop_front())?;
            if !self.cancelled.contains(&item.handle) {
                return Some(item);
            }
        }
    }
}

fn document(value: u8) -> DocumentId {
    DocumentId::from_bytes([value; 16])
}

fn block(value: u8) -> BlockId {
    BlockId::from_bytes([value; 16])
}

fn selection(start: u64, end: u64) -> EditorSelection {
    EditorSelection::new(DocumentPosition::from(start), DocumentPosition::from(end))
}

fn request(document_id: DocumentId) -> SpellcheckRequest {
    SpellcheckRequest {
        language: LanguageId::EnUs,
        document_id,
        document_revision: EditorRevision::from(7),
        blocks: vec![RevisionedTextRange {
            block_id: block(2),
            range: selection(4, 12),
            text: "teh quik".into(),
        }],
        project_dictionary: DictionaryRevision::from(3),
        global_dictionary: DictionaryRevision::from(9),
        generation: SpellcheckGeneration::from(11),
        priority: SpellcheckPriority::Visible,
    }
}

fn result(request: &SpellcheckRequest) -> SpellcheckResult {
    SpellcheckResult {
        document_id: request.document_id,
        document_revision: request.document_revision,
        project_dictionary: request.project_dictionary,
        global_dictionary: request.global_dictionary,
        generation: request.generation,
        issues: vec![SpellingIssue {
            block_id: request.blocks[0].block_id,
            range: request.blocks[0].range,
            word: "teh".into(),
            category: SpellingCategory::Misspelling,
            suggestions: vec![SpellingSuggestion {
                word: "the".into(),
                rank: SuggestionRank::from(1),
            }],
        }],
    }
}

#[test]
fn en_us_request_result_and_suggestion_values_preserve_editor_context() {
    let request = request(document(1));
    let result = result(&request);
    let suggestion = SuggestionRequest {
        document_id: request.document_id,
        block_id: block(7),
        range: selection(10, 13),
        word: "teh".into(),
        document_revision: EditorRevision::from(14),
        project_dictionary: DictionaryRevision::from(4),
        global_dictionary: DictionaryRevision::from(8),
    };

    assert_eq!(LanguageId::EnUs.as_str(), "en-US");
    assert_eq!(request.blocks[0].range, selection(4, 12));
    assert_eq!(request.blocks[0].text, "teh quik");
    assert_eq!(result.issues[0].word, "teh");
    assert_eq!(result.issues[0].suggestions[0].word, "the");
    assert_eq!(
        result.issues[0].suggestions[0].rank,
        SuggestionRank::from(1)
    );
    assert_eq!(suggestion.range, selection(10, 13));
    assert_eq!(suggestion.word, "teh");
    assert_eq!(suggestion.document_revision, EditorRevision::from(14));
    assert_eq!(suggestion.project_dictionary, DictionaryRevision::from(4));
    assert_eq!(suggestion.global_dictionary, DictionaryRevision::from(8));
}

#[test]
fn dictionary_reloads_are_project_or_global_and_revisioned() {
    let project = DictionaryReload {
        project: Some(ProjectId::from_bytes([1; 16])),
        revision: DictionaryRevision::from(6),
    };
    let global = DictionaryReload {
        project: None,
        revision: DictionaryRevision::from(10),
    };

    assert_eq!(project.project, Some(ProjectId::from_bytes([1; 16])));
    assert_eq!(project.revision, DictionaryRevision::from(6));
    assert_eq!(global.project, None);
    assert_eq!(global.revision, DictionaryRevision::from(10));
}

#[test]
fn results_match_only_the_exact_text_and_dictionary_generation() {
    let request = request(document(8));
    let mut stale = [
        result(&request),
        result(&request),
        result(&request),
        result(&request),
        result(&request),
    ];
    stale[0].document_id = document(9);
    stale[1].document_revision = EditorRevision::from(8);
    stale[2].project_dictionary = DictionaryRevision::from(4);
    stale[3].global_dictionary = DictionaryRevision::from(10);
    stale[4].generation = SpellcheckGeneration::from(12);

    assert!(stale.iter().all(|result| !request.accepts(result)));
    assert!(request.accepts(&result(&request)));
}

#[test]
fn visible_work_runs_first_and_cancelled_work_never_runs() {
    let mut queue = WorkQueue::default();
    let cancelled = SpellcheckHandle::new(1);
    queue.push(WorkItem {
        handle: cancelled,
        priority: SpellcheckPriority::Visible,
    });
    queue.push(WorkItem {
        handle: SpellcheckHandle::new(2),
        priority: SpellcheckPriority::Background,
    });
    queue.push(WorkItem {
        handle: SpellcheckHandle::new(3),
        priority: SpellcheckPriority::RecentlyChanged,
    });
    queue.push(WorkItem {
        handle: SpellcheckHandle::new(4),
        priority: SpellcheckPriority::Visible,
    });
    queue.cancel(cancelled);

    assert_eq!(
        queue.pop().expect("visible work").handle,
        SpellcheckHandle::new(4)
    );
    assert_eq!(
        queue.pop().expect("recently changed work").priority,
        SpellcheckPriority::RecentlyChanged
    );
    assert_eq!(
        queue.pop().expect("background work").priority,
        SpellcheckPriority::Background
    );
    assert!(queue.pop().is_none());
}

#[test]
fn contract_values_are_engine_neutral() {
    fn assert_contract_value<T: Send + Sync + 'static>() {}

    assert_contract_value::<LanguageId>();
    assert_contract_value::<SpellcheckRequest>();
    assert_contract_value::<RevisionedTextRange>();
    assert_contract_value::<SpellcheckResult>();
    assert_contract_value::<SpellingIssue>();
    assert_contract_value::<SpellingSuggestion>();
    assert_contract_value::<SuggestionRequest>();
    assert_contract_value::<DictionaryReload>();
    assert_contract_value::<SpellcheckHandle>();
    assert_contract_value::<SpellcheckGeneration>();
    assert_contract_value::<DictionaryRevision>();
}
