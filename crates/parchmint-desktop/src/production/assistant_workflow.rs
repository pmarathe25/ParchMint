//! Session-specific project reads and provider requests.

use std::collections::BTreeSet;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use chrono::{Datelike, Utc};
use parchmint_application::AuthoredProjectSnapshot;
use parchmint_assistant_api::{
    AssistantError, AssistantMode, AssistantRequest, ContextPassage, ConversationMessage,
    MessageRole, ProjectSection as ContextSection, SecretString, ToolDefinition,
    note_creation_targets_current_selection, note_intent, note_read_scope_is_current_or_project,
    note_reads_across_project,
};
use parchmint_assistant_budget::{BudgetError, BudgetJournal, Pricing, Usage as BudgetUsage};
use parchmint_domain::{DocumentId, NodeId, NodeKind, Project};
use parchmint_preferences::{AssistantAuthentication, PreferenceService};
use parchmint_ui_api::{
    AssistantWorkflowPort, AssistantWorkflowRequest, AssistantWorkflowResult, PlatformServices,
    ProjectSnapshotQuery,
};

use super::{assistant_config, block_on};

pub(super) struct ProductionAssistant {
    pub(super) query: Arc<dyn ProjectSnapshotQuery>,
    pub(super) preferences: Arc<dyn PreferenceService>,
    pub(super) platform: PlatformServices,
    pub(super) budget: Option<Arc<BudgetJournal>>,
    pub(super) conversations:
        Option<Result<super::assistant_conversations::ConversationStore, String>>,
}

fn pricing(profile: &parchmint_preferences::AssistantPreferences) -> Option<Pricing> {
    (profile.provider == parchmint_preferences::AssistantProvider::OpenAi
        && profile.model.trim() == "gpt-6.1-terra")
        .then_some(Pricing {
            input_micros_per_million: 1_000_000,
            output_micros_per_million: 5_000_000,
            search_call_micros: 10_000,
        })
}

// https://developers.openai.com/api/docs/models/gpt-6.1-terra
// The long-context tier applies to the entire request when its input exceeds
// 272,000 tokens, including tokens supplied by web search.
fn pricing_for_usage(rates: Pricing, input_tokens: u64) -> Pricing {
    if input_tokens > 272_000 {
        Pricing {
            input_micros_per_million: rates.input_micros_per_million * 2,
            output_micros_per_million: rates.output_micros_per_million * 3 / 2,
            ..rates
        }
    } else {
        rates
    }
}

fn period(binding: &str, reset_day: u8) -> Result<String, AssistantError> {
    if !(1..=28).contains(&reset_day) {
        return Err(AssistantError::InvalidConfiguration);
    }
    let now = Utc::now();
    let (mut year, mut month) = (now.year(), now.month());
    if now.day() < u32::from(reset_day) {
        if month == 1 {
            year -= 1;
            month = 12;
        } else {
            month -= 1;
        }
    }
    Ok(format!("{binding}-{year:04}-{month:02}-{reset_day:02}"))
}

fn note_tree_hash(project: &Project) -> [u8; 32] {
    let entries = project
        .nodes
        .iter()
        .map(|(id, node)| {
            let (kind, root_section, document) = match node.kind {
                NodeKind::Root(section) => (0, Some(section), None),
                NodeKind::Group => (1, None, None),
                NodeKind::Document(document) => (2, None, Some(document)),
            };
            parchmint_ui_api::AssistantNoteTreeEntry {
                node: *id,
                parent: project.nodes.parent(*id),
                kind,
                root_section,
                document,
            }
        })
        .collect();
    parchmint_ui_api::assistant_note_tree_hash(project.id, entries)
}

fn target_documents(project: &Project, root: NodeId) -> Option<BTreeSet<DocumentId>> {
    if !matches!(
        project.nodes.section(root),
        Some(
            parchmint_domain::ProjectSection::Manuscript
                | parchmint_domain::ProjectSection::Lore
                | parchmint_domain::ProjectSection::Research
        )
    ) {
        return None;
    }
    project.nodes.get(root)?;
    let mut result = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        let node = project.nodes.get(id)?;
        if let NodeKind::Document(document) = node.kind {
            result.insert(document);
        }
        pending.extend(project.nodes.children(id).iter().rev().copied());
    }
    Some(result)
}

fn target_documents_for_context(
    project: &Project,
    node: NodeId,
) -> Result<BTreeSet<DocumentId>, AssistantError> {
    target_documents(project, node).ok_or(AssistantError::ContextUnavailable)
}

fn prepend_selection(
    context: &mut parchmint_assistant_core::RetrievedContext,
    selection: &parchmint_ui_api::AssistantSelectedText,
    title: &str,
    section: ContextSection,
) {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut scalars = 0_usize;
    for character in selection.text.chars() {
        if scalars == parchmint_assistant_core::MAX_SCALARS_PER_PASSAGE {
            chunks.push(std::mem::take(&mut chunk));
            scalars = 0;
        }
        chunk.push(character);
        scalars += 1;
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    let mut passages = Vec::new();
    let mut bytes = 0_usize;
    for text in chunks {
        bytes += text.len();
        passages.push(ContextPassage {
            source_id: String::new(),
            document_id: parchmint_domain::encode_stable_id(selection.document.as_bytes()),
            document_revision: selection.revision.value(),
            section,
            title: format!("{title} · editor selection"),
            anchor: None,
            text,
            original: None,
        });
    }
    for passage in std::mem::take(&mut context.passages) {
        if passages.len() >= parchmint_assistant_core::MAX_PASSAGES
            || bytes.saturating_add(passage.text.len()) > parchmint_assistant_core::MAX_TOTAL_BYTES
        {
            context.truncated = true;
        } else {
            bytes += passage.text.len();
            passages.push(passage);
        }
    }
    for (index, passage) in passages.iter_mut().enumerate() {
        passage.source_id = format!("S{}", index + 1);
    }
    context.passages = passages;
}

/// Notes share the request's passage and byte limits with authored evidence.
/// Leave space for requested notes before filling the remainder with passages.
fn append_notes(
    context: &mut parchmint_assistant_core::RetrievedContext,
    notes: Vec<ContextPassage>,
) {
    use parchmint_assistant_core::{MAX_PASSAGES, MAX_TOTAL_BYTES};
    if notes.is_empty() {
        return;
    }
    let note_bytes = notes.iter().map(|note| note.text.len()).sum::<usize>();
    let reserved = note_bytes.min(MAX_TOTAL_BYTES / 4);
    let note_slots = notes.len().min(MAX_PASSAGES);
    let mut bytes = 0_usize;
    let mut passages = Vec::new();
    for passage in std::mem::take(&mut context.passages) {
        if passages.len() >= MAX_PASSAGES - note_slots
            || bytes.saturating_add(passage.text.len()) > MAX_TOTAL_BYTES - reserved
        {
            context.truncated = true;
            continue;
        }
        bytes += passage.text.len();
        passages.push(passage);
    }
    for mut note in notes {
        if passages.len() == MAX_PASSAGES || bytes == MAX_TOTAL_BYTES {
            context.truncated = true;
            break;
        }
        let remaining = MAX_TOTAL_BYTES - bytes;
        if note.text.len() > remaining {
            let mut end = remaining;
            while !note.text.is_char_boundary(end) {
                end -= 1;
            }
            note.text.truncate(end);
            context.truncated = true;
        }
        if !note.text.is_empty() {
            bytes += note.text.len();
            passages.push(note);
        }
    }
    for (index, passage) in passages.iter_mut().enumerate() {
        passage.source_id = format!("S{}", index + 1);
    }
    context.passages = passages;
}

/// Extract only allowed, live originals, after verifying them against the
/// captured project metadata. Limits bound both individual reads and aggregate
/// work; unsupported sources are reported rather than silently claimed read.
fn extracted_originals(
    query: &dyn ProjectSnapshotQuery,
    snapshot: &AuthoredProjectSnapshot,
    pinned: &BTreeSet<DocumentId>,
    excluded: &BTreeSet<DocumentId>,
    cancelled: &AtomicBool,
) -> Result<
    (
        Vec<parchmint_assistant_core::ExtractedOriginal>,
        Vec<String>,
    ),
    AssistantError,
> {
    use parchmint_assistant_core::research_text::{self, ExtractionError};
    const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
    const MAX_SOURCES: usize = 64;
    const MAX_NOTICES: usize = 16;
    let live_research = snapshot
        .project
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            if let NodeKind::Document(document) = node.kind
                && snapshot.project.nodes.section(node.id)
                    == Some(parchmint_domain::ProjectSection::Research)
            {
                Some(document)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    let mut entries = snapshot
        .project
        .research_originals
        .iter()
        .filter(|(_, original)| {
            !excluded.contains(&original.document) && live_research.contains(&original.document)
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(_, original)| !pinned.contains(&original.document));
    let mut extracted = Vec::new();
    let mut notices = Vec::new();
    let mut omitted_notices = 0_usize;
    let mut read_bytes = 0_u64;
    let mut read_sources = 0_usize;
    for (source_id, original) in entries {
        if cancelled.load(Ordering::Acquire) {
            return Err(AssistantError::Cancelled);
        }
        let result = if research_text::extractor_version(&original.media_type).is_none() {
            Err("this source format is not yet searchable")
        } else if original.byte_length > research_text::MAX_TEXT_BYTES as u64 {
            Err("source exceeds the text extraction limit")
        } else if read_sources >= MAX_SOURCES {
            Err("too many originals for one request")
        } else if read_bytes.saturating_add(original.byte_length) > MAX_TOTAL_BYTES {
            Err("the request's original-source read limit was reached")
        } else {
            read_sources += 1;
            read_bytes += original.byte_length;
            match query.research_original_bytes(*source_id) {
                Ok(bytes)
                    if bytes.len() as u64 == original.byte_length
                        && parchmint_project_format::ContentHash::of_bytes(&bytes).as_bytes()
                            == &original.sha256 =>
                {
                    research_text::extract(&original.media_type, &bytes).map_err(
                        |error| match error {
                            ExtractionError::UnsupportedFormat => {
                                "this source format is not yet searchable"
                            }
                            ExtractionError::UnsupportedEncoding => {
                                "source encoding is unsupported or not declared"
                            }
                            ExtractionError::InvalidMarkup => "source markup could not be parsed",
                            ExtractionError::TooLarge => "source exceeds text extraction limits",
                            ExtractionError::NoText => "source contains no extractable text",
                        },
                    )
                }
                _ => Err("the saved source could not be verified or read"),
            }
        };
        match result {
            Ok(text) => extracted.push(parchmint_assistant_core::ExtractedOriginal {
                source_id: *source_id,
                text,
            }),
            Err(reason) if notices.len() < MAX_NOTICES => {
                notices.push(format!("{}: {reason}.", original.filename))
            }
            Err(_) => omitted_notices += 1,
        }
    }
    if omitted_notices > 0 {
        notices.push(format!(
            "{omitted_notices} additional original sources could not be searched."
        ));
    }
    Ok((extracted, notices))
}

/// Extracts names immediately adjacent to explicit identity constructions.
/// These are search hints and never establish canonical identity.
fn identity_hint_names(text: &str) -> BTreeSet<String> {
    fn adjacent_names<'a>(words: impl Iterator<Item = &'a str>, backwards: bool) -> Vec<String> {
        let mut found = Vec::new();
        for word in words.take(4) {
            let delimiter = |ch: char| !ch.is_alphanumeric() && !matches!(ch, '-' | '\'' | '’');
            let boundary = if backwards {
                word.char_indices().rev().find(|(_, ch)| delimiter(*ch))
            } else {
                word.char_indices().find(|(_, ch)| delimiter(*ch))
            };
            let fragment = match boundary {
                Some((index, ch)) if backwards => {
                    let adjacent = &word[index + ch.len_utf8()..];
                    if adjacent.is_empty() && found.is_empty() {
                        &word[..index]
                    } else {
                        adjacent
                    }
                }
                Some((index, _)) => &word[..index],
                None => word,
            };
            if fragment.is_empty() {
                break;
            }
            let word = fragment
                .trim_matches(|ch: char| !ch.is_alphanumeric())
                .split(['\'', '’'])
                .next()
                .unwrap_or("");
            if word.chars().count() < 3
                || !word.chars().next().is_some_and(char::is_uppercase)
                || matches!(
                    word,
                    "The"
                        | "This"
                        | "That"
                        | "Both"
                        | "Neither"
                        | "After"
                        | "Before"
                        | "When"
                        | "While"
                        | "Someone"
                        | "Winter"
                        | "Spring"
                        | "Summer"
                        | "Autumn"
                        | "Fall"
                )
            {
                break;
            }
            found.push(word.to_owned());
            if boundary.is_some() {
                break;
            }
        }
        found
    }
    let mut result = BTreeSet::new();
    for sentence in text.split(['.', '!', '?', '\n']) {
        let lower = sentence.to_lowercase();
        if ['"', '“', '”', '`'].iter().any(|ch| sentence.contains(*ch))
            || lower
                .split(|ch: char| !ch.is_alphanumeric())
                .any(|word| matches!(word, "said" | "says" | "asked" | "whispered" | "shouted"))
        {
            continue;
        }
        for marker in [
            " is known as ",
            " was known as ",
            " is called ",
            " was called ",
            " known as ",
            " is ",
            " was ",
        ] {
            let Some(index) = lower.find(marker) else {
                continue;
            };
            let Some(prefix) = sentence.get(..index) else {
                continue;
            };
            let Some(suffix) = sentence.get(index + marker.len()..) else {
                continue;
            };
            // Unicode case folding can change byte lengths; skip if the
            // marker's offset does not identify the same original prefix.
            if prefix.to_lowercase() != lower[..index] {
                continue;
            }
            let left = adjacent_names(prefix.split_whitespace().rev(), true);
            let suffix = suffix.trim_start();
            let suffix = suffix
                .strip_prefix("the ")
                .or_else(|| suffix.strip_prefix("The "))
                .unwrap_or(suffix);
            if suffix
                .split_whitespace()
                .next()
                .is_some_and(|word| word.contains("'s") || word.contains("’s"))
                && !lower.contains("nickname")
            {
                continue;
            }
            let right = adjacent_names(suffix.split_whitespace(), false);
            let explicit_alias = matches!(
                marker,
                " is known as " | " was known as " | " is called " | " was called " | " known as "
            );
            let nickname = lower.contains("nickname") && !right.is_empty();
            if (!left.is_empty() && !right.is_empty())
                || (explicit_alias && (!left.is_empty() || !right.is_empty()))
                || nickname
            {
                result.extend(left);
                result.extend(right);
            }
            break;
        }
    }
    result
}

/// Expands local retrieval with target terms and names from a few already
/// relevant identity statements. These are search hints, not canon facts.
fn retrieval_expansion(
    initial: &parchmint_assistant_core::RetrievedContext,
    target_ids: &[String],
) -> (String, String) {
    const TARGET_BYTES: usize = 16 * 1024;
    const NAME_BYTES: usize = 4 * 1024;
    const RELATED_PASSAGES: usize = 8;
    fn append(expanded: &mut String, text: &str, limit: usize) {
        if expanded.len() >= limit {
            return;
        }
        expanded.push(' ');
        for ch in text.chars() {
            if expanded.len() + ch.len_utf8() > limit {
                break;
            }
            expanded.push(ch);
        }
    }
    let mut target_terms = String::new();
    let mut names = String::new();
    for passage in initial
        .passages
        .iter()
        .filter(|passage| target_ids.contains(&passage.document_id))
    {
        append(&mut target_terms, &passage.text, TARGET_BYTES);
    }
    for passage in initial
        .passages
        .iter()
        .filter(|passage| !target_ids.contains(&passage.document_id))
        .take(RELATED_PASSAGES)
    {
        for word in identity_hint_names(&passage.text) {
            append(&mut names, &word, NAME_BYTES);
        }
    }
    (target_terms, names)
}

fn permitted_history(
    history: Vec<parchmint_ui_api::AssistantHistoryTurn>,
    binding: &str,
) -> Vec<ConversationMessage> {
    history
        .into_iter()
        // Never forward note-derived history. A fresh explicit note request
        // reads only its currently authorized scope.
        .filter(|turn| !turn.notes_derived && turn.provider_binding == binding)
        .flat_map(|turn| {
            [
                ConversationMessage {
                    role: MessageRole::User,
                    text: turn.question,
                },
                ConversationMessage {
                    role: MessageRole::Assistant,
                    text: turn.answer,
                },
            ]
        })
        .collect()
}

fn mark_check_target_coverage(
    context: &mut parchmint_assistant_core::RetrievedContext,
    target_ids: &[String],
) {
    if target_ids.iter().any(|target| {
        !context.passages.iter().any(|passage| {
            passage.section != ContextSection::Notes && &passage.document_id == target
        })
    }) {
        context.truncated = true;
    }
}

impl AssistantWorkflowPort for ProductionAssistant {
    fn conversation_snapshot(
        &self,
    ) -> Result<parchmint_ui_api::AssistantConversationState, String> {
        match self.conversations.as_ref() {
            Some(Ok(store)) => store.snapshot(),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(Default::default()),
        }
    }

    fn queue_conversation(
        &self,
        snapshot: parchmint_ui_api::AssistantConversationState,
    ) -> Result<(), String> {
        match self.conversations.as_ref() {
            Some(Ok(store)) => store.queue(snapshot),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn flush_conversation(&self) -> Result<(), String> {
        match self.conversations.as_ref() {
            Some(Ok(store)) => store.flush(),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn conversation_write_pending(&self) -> Result<bool, String> {
        match self.conversations.as_ref() {
            Some(Ok(store)) => store.write_pending(),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(false),
        }
    }

    fn run(
        &self,
        request: AssistantWorkflowRequest,
        cancelled: &AtomicBool,
    ) -> Result<AssistantWorkflowResult, AssistantError> {
        self.run_inner(request, cancelled, None)
    }

    fn run_with_progress(
        &self,
        request: AssistantWorkflowRequest,
        cancelled: &AtomicBool,
        progress: &mut dyn FnMut(&str),
    ) -> Result<AssistantWorkflowResult, AssistantError> {
        if request.edit_proposal.is_some() {
            self.run_inner(request, cancelled, None)
        } else {
            self.run_inner(request, cancelled, Some(progress))
        }
    }
}

impl ProductionAssistant {
    fn run_inner(
        &self,
        request: AssistantWorkflowRequest,
        cancelled: &AtomicBool,
        progress: Option<&mut dyn FnMut(&str)>,
    ) -> Result<AssistantWorkflowResult, AssistantError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(AssistantError::Cancelled);
        }
        if request.edit_proposal.is_some()
            && (request.mode != AssistantMode::Ask
                || request.include_notes
                || request.explicit_notes.is_some()
                || request.create_note
                || request.selected_text.is_some())
        {
            return Err(AssistantError::InvalidRequest);
        }
        self.flush_conversation()
            .map_err(|_| AssistantError::ConversationUnavailable)?;
        let permitted_notes = note_intent(&request.prompt);
        if request.explicit_notes.is_some_and(|scope| {
            parchmint_assistant_api::explicit_note_scope_conflicts(
                &request.prompt,
                matches!(
                    scope,
                    parchmint_ui_api::AssistantExplicitNotes::Project { .. }
                ),
            )
        }) {
            return Err(AssistantError::InvalidRequest);
        }
        if (request.explicit_notes.is_some() && (!request.include_notes || request.create_note))
            || (request.include_notes
                && request.explicit_notes.is_none()
                && (!permitted_notes.read
                    || !note_read_scope_is_current_or_project(&request.prompt)))
            || (request.create_note
                && (!permitted_notes.create
                    || !note_creation_targets_current_selection(&request.prompt)))
            || ((request.include_notes || request.create_note)
                && request.mode != AssistantMode::Ask)
        {
            return Err(AssistantError::InvalidRequest);
        }
        if let Some(selection) = &request.selected_text {
            if request.mode == AssistantMode::Research || request.context_scope.is_some() {
                return Err(AssistantError::InvalidRequest);
            }
            if request.include_notes {
                return Err(AssistantError::SelectionNoteScope);
            }
            if selection.text.trim().is_empty()
                || selection.text.len() > parchmint_ui_api::MAX_ASSISTANT_SELECTION_BYTES
            {
                return Err(AssistantError::SelectionUnavailable);
            }
        }
        let profile = block_on(self.preferences.load())
            .map_err(|_| AssistantError::InvalidConfiguration)?
            .values
            .assistant;
        let provider = assistant_config::provider(&profile)
            .map_err(|_| AssistantError::InvalidConfiguration)?;
        let binding = provider.credential_binding();
        let history_binding = format!(
            "{}:{}",
            binding,
            profile.credential_reference.as_deref().unwrap_or("no-auth")
        );
        let credential = assistant_config::read_credential(&self.platform, &profile, &binding)
            .map_err(|_| AssistantError::InvalidConfiguration)?;
        if profile.authentication == AssistantAuthentication::ApiKey && credential.is_none() {
            return Err(AssistantError::MissingCredential);
        }
        let mut target_ids = Vec::new();
        let mut context_warnings = Vec::new();
        let mut matching_sources = None;
        let mut proposal_target = None;
        let mut adjacent_context_added = false;
        let mut context = if request.mode == AssistantMode::Research {
            // The author-provided external query is sent without project excerpts.
            parchmint_assistant_core::RetrievedContext {
                passages: Vec::new(),
                truncated: false,
            }
        } else {
            let mut snapshot = self
                .query
                .snapshot_with_documents()
                .map_err(|_| AssistantError::InvalidRequest)?;
            for projection in &request.live_documents {
                if let Some(document) = snapshot
                    .documents
                    .iter_mut()
                    .find(|document| document.document_id == projection.document_id())
                {
                    document.body = projection.body().to_owned();
                    document.comments = projection.comments().to_vec();
                    document.revision = projection.revision();
                }
            }
            let target_documents = match request.target {
                Some(target) => target_documents(&snapshot.project, target)
                    .ok_or(AssistantError::InvalidRequest)?,
                None if request.mode == AssistantMode::Check => {
                    return Err(AssistantError::InvalidRequest);
                }
                None => BTreeSet::new(),
            };
            if let Some(scope) = request.explicit_notes {
                let (document, revision, expected_tree) = match scope {
                    parchmint_ui_api::AssistantExplicitNotes::Document {
                        document,
                        project_revision,
                        tree_hash,
                    } => (Some(document), project_revision, tree_hash),
                    parchmint_ui_api::AssistantExplicitNotes::Project {
                        project_revision,
                        tree_hash,
                    } => (None, project_revision, tree_hash),
                };
                if revision != snapshot.project.revision.value()
                    || expected_tree != note_tree_hash(&snapshot.project)
                    || document.is_some_and(|document| {
                        target_documents.len() != 1 || !target_documents.contains(&document)
                    })
                {
                    return Err(AssistantError::InvalidRequest);
                }
            }
            if request.mode == AssistantMode::Check && target_documents.is_empty() {
                return Err(AssistantError::InvalidRequest);
            }
            let mut excluded = BTreeSet::new();
            for node in &request.excluded_context {
                excluded.extend(target_documents_for_context(&snapshot.project, *node)?);
            }
            if request.mode == AssistantMode::Check && !target_documents.is_disjoint(&excluded) {
                return Err(AssistantError::CheckTargetExcluded);
            }
            let selection_source = if let Some(selection) = &request.selected_text {
                if excluded.contains(&selection.document) {
                    return Err(AssistantError::SelectionExcluded);
                }
                let node = snapshot
                    .project
                    .nodes
                    .iter()
                    .find_map(|(_, node)| {
                        (node.kind == NodeKind::Document(selection.document)).then_some(node)
                    })
                    .ok_or(AssistantError::SelectionUnavailable)?;
                let section = match snapshot.project.nodes.section(node.id) {
                    Some(parchmint_domain::ProjectSection::Manuscript) => {
                        ContextSection::Manuscript
                    }
                    Some(parchmint_domain::ProjectSection::Lore) => ContextSection::Lore,
                    Some(parchmint_domain::ProjectSection::Research) => ContextSection::Research,
                    _ => return Err(AssistantError::SelectionUnavailable),
                };
                if !snapshot.documents.iter().any(|document| {
                    document.document_id == selection.document
                        && document.revision == selection.revision
                }) {
                    return Err(AssistantError::SelectionUnavailable);
                }
                Some((node.title.clone(), section))
            } else {
                None
            };
            if let Some(scope) = request.context_scope {
                let mut allowed = target_documents_for_context(&snapshot.project, scope)?;
                if request.mode == AssistantMode::Check {
                    allowed.extend(target_documents.iter().copied());
                }
                for (_, node) in snapshot.project.nodes.iter() {
                    if let NodeKind::Document(document) = node.kind
                        && !allowed.contains(&document)
                    {
                        excluded.insert(document);
                    }
                }
            }
            if request.selected_text.is_some() {
                for (_, node) in snapshot.project.nodes.iter() {
                    if let NodeKind::Document(document) = node.kind
                        && !(request.mode == AssistantMode::Check
                            && target_documents.contains(&document))
                    {
                        excluded.insert(document);
                    }
                }
            }
            let mut pinned = target_documents.clone();
            for node in &request.pinned_context {
                pinned.extend(target_documents_for_context(&snapshot.project, *node)?);
            }
            pinned.retain(|id| !excluded.contains(id));
            target_ids = target_documents
                .iter()
                .map(|id| parchmint_domain::encode_stable_id(id.as_bytes()))
                .collect();
            let authored = AuthoredProjectSnapshot {
                project: snapshot.project,
                document_summaries: Vec::new(),
                documents: snapshot.documents,
            };
            let (originals, notices) = extracted_originals(
                self.query.as_ref(),
                &authored,
                &pinned,
                &excluded,
                cancelled,
            )?;
            context_warnings = notices;
            let initial = if let Some(proposal) = &request.edit_proposal {
                context_warnings.clear();
                let encoded = |id: NodeId| parchmint_domain::encode_stable_id(id.as_bytes());
                if request.target.map(encoded).as_deref() != Some(proposal.scope.target.as_str())
                    || request.context_scope.map(encoded) != proposal.scope.evidence_scope
                    || request
                        .excluded_context
                        .iter()
                        .copied()
                        .map(encoded)
                        .collect::<BTreeSet<_>>()
                        != proposal
                            .scope
                            .excluded
                            .iter()
                            .cloned()
                            .collect::<BTreeSet<_>>()
                    || proposal.context.len() > parchmint_assistant_core::MAX_PASSAGES
                    || proposal
                        .context
                        .iter()
                        .map(|item| item.text.len())
                        .sum::<usize>()
                        > parchmint_assistant_core::MAX_TOTAL_BYTES
                {
                    return Err(AssistantError::InvalidRequest);
                }
                let target = parchmint_assistant_core::edit_proposal::target(
                    &proposal.finding,
                    &proposal.scope,
                    &proposal.context,
                    proposal.evidence_index,
                    &authored,
                    &originals,
                )
                .ok_or(AssistantError::InvalidRequest)?;
                proposal_target = Some(parchmint_ui_api::AssistantEditProposalTarget {
                    document: target.document,
                    revision: target.revision,
                    start: target.start,
                    end: target.end,
                    original: target.original,
                });
                let mut passages = proposal.finding.evidence.to_vec();
                for (index, passage) in passages.iter_mut().enumerate() {
                    passage.source_id = format!("S{}", index + 1);
                }
                parchmint_assistant_core::RetrievedContext {
                    passages,
                    truncated: false,
                }
            } else {
                parchmint_assistant_core::retrieve_with_originals(
                    &authored,
                    &request.prompt,
                    &pinned,
                    &excluded,
                    &originals,
                )
                .map_err(|_| AssistantError::InvalidRequest)?
            };
            let mut selected = if !initial.passages.is_empty() && request.edit_proposal.is_none() {
                let (expanded, names) = retrieval_expansion(&initial, &target_ids);
                parchmint_assistant_core::retrieve_with_identity_expansion(
                    &authored,
                    &request.prompt,
                    &expanded,
                    &names,
                    &pinned,
                    &excluded,
                    &originals,
                )
                .map_err(|_| AssistantError::InvalidRequest)?
            } else {
                initial
            };
            if let (Some(selection), Some((title, section))) =
                (&request.selected_text, &selection_source)
            {
                prepend_selection(&mut selected, selection, title, *section);
            }
            if request.include_notes {
                let all_notes = match request.explicit_notes {
                    Some(parchmint_ui_api::AssistantExplicitNotes::Project { .. }) => true,
                    Some(parchmint_ui_api::AssistantExplicitNotes::Document { .. }) => false,
                    None => note_reads_across_project(&request.prompt),
                };
                if !all_notes {
                    if target_documents.is_empty() {
                        return Err(AssistantError::InvalidRequest);
                    }
                    if !target_documents.is_disjoint(&excluded) {
                        return Err(AssistantError::NoteTargetExcluded);
                    }
                }
                let mut notes = Vec::new();
                for document in &authored.documents {
                    if excluded.contains(&document.document_id)
                        || (!all_notes && !target_documents.contains(&document.document_id))
                    {
                        continue;
                    }
                    let Some((_, node)) =
                        authored.project.nodes.iter().find(|(_, node)| {
                            node.kind == NodeKind::Document(document.document_id)
                        })
                    else {
                        continue;
                    };
                    if authored.project.nodes.section(node.id)
                        == Some(parchmint_domain::ProjectSection::Unfiled)
                    {
                        continue;
                    }
                    for message in document.comments.iter().flat_map(|thread| &thread.messages) {
                        if notes.len() >= 16 {
                            selected.truncated = true;
                            break;
                        }
                        let text = message.body.chars().take(1200).collect::<String>();
                        if text.len() < message.body.len() {
                            selected.truncated = true;
                        }
                        if text.is_empty() {
                            continue;
                        }
                        notes.push(ContextPassage {
                            source_id: String::new(),
                            document_id: parchmint_domain::encode_stable_id(
                                document.document_id.as_bytes(),
                            ),
                            document_revision: document.revision.value(),
                            section: ContextSection::Notes,
                            title: format!("Note in {}", node.title),
                            anchor: None,
                            text,
                            original: None,
                        });
                    }
                }
                append_notes(&mut selected, notes);
            }
            if request.selected_text.is_none() && request.edit_proposal.is_none() {
                adjacent_context_added = parchmint_assistant_core::append_adjacent_context(
                    &authored,
                    &request.prompt,
                    &excluded,
                    &mut selected,
                )
                .map_err(|_| AssistantError::InvalidRequest)?
                    > 0;
            }
            matching_sources = Some((authored, originals));
            selected
        };
        if request.mode == AssistantMode::Check {
            mark_check_target_coverage(&mut context, &target_ids);
        }
        let mut prompt = if let Some(proposal) = &request.edit_proposal {
            format!(
                "Suggest a replacement for the selected passage to address this possible continuity conflict. Return only JSON with exactly one string field named replacement. Keep it a single paragraph and preserve the author's voice. The first or second supplied source is the selected passage, indicated by its source ID below. Do not provide document IDs or instructions.\n\n{}",
                serde_json::json!({"finding":proposal.finding.explanation,"uncertainty":proposal.finding.uncertainty,"selected_source":format!("S{}", proposal.evidence_index + 1),"selected_quote":proposal.finding.evidence[proposal.evidence_index].text})
            )
        } else {
            match request.mode {
                AssistantMode::Check => format!(
                    "Review the selected material for possible continuity conflicts against the supplied project evidence. The selected document IDs are {}. This is not an exhaustive check. Return only JSON: {{\"summary\":\"...\",\"findings\":[{{\"category\":\"other\",\"explanation\":\"...\",\"uncertainty\":\"...\",\"evidence\":[{{\"source_id\":\"S1\",\"quote\":\"exact text\"}},{{\"source_id\":\"S2\",\"quote\":\"exact text\"}}]}}]}}. Categories: identity_appearance, time_travel, objects_possession, rules, relationships, knowledge, other. Each finding must cite two exact, unambiguous quotations from supplied sources, at least one from the selected documents. Include uncertainty where appropriate. Do not invent sources or quotes. Return an empty findings array if the supplied evidence supports no finding.\n\n{}",
                    target_ids.join(", "),
                    request.prompt
                ),
                AssistantMode::Research => format!(
                    "Find credible sources and produce a concise, source-attributed synthesis with useful distilled notes. Distinguish sourced facts from interpretation.\n\n{}",
                    request.prompt
                ),
                AssistantMode::Ask => request.prompt,
            }
        };
        if !context_warnings.is_empty() {
            prompt = format!(
                "Some saved research originals could not be searched. Do not claim to have reviewed them.\n\n{prompt}"
            );
        }
        if adjacent_context_added {
            prompt = format!(
                "Neighboring paragraphs are additional context and may describe different events. Do not assume a relationship without evidence.\n\n{prompt}"
            );
        }
        if context.truncated {
            prompt = format!(
                "Some relevant project passages or requested notes were omitted due to context limits. Do not claim that your answer or review is exhaustive.\n\n{prompt}"
            );
        }
        let provider_request = AssistantRequest {
            mode: request.mode,
            prompt,
            context: context.passages.clone(),
            history: if request.edit_proposal.is_some()
                || request.mode == AssistantMode::Research
                || !request.excluded_context.is_empty()
                || request.context_scope.is_some()
                || request.selected_text.is_some()
            {
                Vec::new()
            } else {
                permitted_history(request.history, &history_binding)
            },
            tools: if request.create_note {
                vec![ToolDefinition {
                    name: "create_note".into(),
                    description: "Create the single note explicitly requested by the author at their captured editor selection.".into(),
                    parameters: serde_json::json!({"type":"object", "properties":{"body":{"type":"string"}}, "required":["body"], "additionalProperties":false}),
                }]
            } else {
                Vec::new()
            },
            max_output_tokens: 2048,
        };
        let secret = SecretString::new(credential.as_ref().map_or("", |value| value.expose()));
        let prepared = provider.prepare(&provider_request, &secret, cancelled)?;
        let rates = pricing(&profile);
        let mut reservation = None;
        let mut period_key = None;
        if let Some(rates) = rates {
            let journal = self
                .budget
                .as_ref()
                .ok_or(AssistantError::BudgetUnavailable)?;
            let limit = profile
                .monthly_budget_cents
                .checked_mul(10_000)
                .ok_or(AssistantError::InvalidConfiguration)?;
            let key = period(&binding, profile.reset_day)?;
            // Search content is supplied by the provider, so its size cannot
            // be bounded from our request body. Reserve the model's full
            // context window at the long-context rate for each request.
            let maximum_input_tokens = 1_050_000;
            let maximum = pricing_for_usage(rates, maximum_input_tokens)
                .estimate(BudgetUsage {
                    input_tokens: maximum_input_tokens,
                    output_tokens: u64::from(provider_request.max_output_tokens),
                    search_calls: u64::from(request.mode == AssistantMode::Research),
                })
                .map_err(|_| AssistantError::InvalidConfiguration)?;
            let id = journal
                .reserve(&key, limit, maximum)
                .map_err(|error| match error {
                    BudgetError::LimitReached => AssistantError::BudgetLimitReached,
                    _ => AssistantError::BudgetUnavailable,
                })?;
            reservation = Some(id);
            period_key = Some(key);
        }
        let dispatched = match progress {
            Some(progress) => {
                provider.dispatch_with_progress(prepared, &secret, cancelled, progress)
            }
            None => provider.dispatch(prepared, &secret, cancelled),
        };
        let mut response = match dispatched {
            Ok(response) => response,
            Err(parchmint_assistant_http::DispatchFailure::NotSubmitted(error)) => {
                if let (Some(journal), Some(id)) = (self.budget.as_ref(), reservation.as_deref())
                    && journal.settle(id, 0).is_err()
                {
                    return Err(AssistantError::ReservationHeld(Box::new(error)));
                }
                return Err(error);
            }
            Err(parchmint_assistant_http::DispatchFailure::Submitted(error)) => {
                return Err(if reservation.is_some() {
                    AssistantError::ReservationHeld(Box::new(error))
                } else {
                    error
                });
            }
        };
        let mut estimated_cost_micros = None;
        let mut estimated_period_total_micros = None;
        let mut reservation_held = false;
        let mut usage_warning = if rates.is_none() {
            Some("No cost estimate or local spending limit.".to_owned())
        } else {
            None
        };
        if let (Some(rates), Some(journal), Some(id), Some(period_key)) = (
            rates,
            self.budget.as_ref(),
            reservation.as_deref(),
            period_key.as_deref(),
        ) {
            if let Some(usage) = response.usage {
                let actual = pricing_for_usage(rates, usage.input_tokens)
                    .estimate(BudgetUsage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        search_calls: u64::from(request.mode == AssistantMode::Research),
                    })
                    .map_err(|_| {
                        AssistantError::ReservationHeld(Box::new(AssistantError::InvalidResponse))
                    })?;
                estimated_cost_micros = Some(actual);
                if journal.settle(id, actual).is_err() {
                    reservation_held = true;
                    usage_warning =
                        Some("Usage could not be reconciled; the reservation remains held.".into());
                }
            } else {
                reservation_held = true;
                usage_warning =
                    Some("Provider usage was unavailable; the reservation remains held.".into());
            }
            if let Ok(total) = journal.reserved_or_spent(period_key) {
                estimated_period_total_micros = Some(total);
                if total >= profile.warning_cents.saturating_mul(10_000) && usage_warning.is_none()
                {
                    usage_warning =
                        Some("Estimated ParchMint usage has reached your warning amount.".into());
                }
            }
        }
        if let Some(target) = &proposal_target {
            response.text =
                parchmint_assistant_core::edit_proposal::validate(&response.text, &target.original)
                    .map_err(|_| {
                        if reservation_held {
                            AssistantError::ReservationHeld(Box::new(
                                AssistantError::InvalidResponse,
                            ))
                        } else {
                            AssistantError::InvalidResponse
                        }
                    })?;
            response.project_citations.clear();
            response.unverified_project_citations.clear();
        }
        let (check_report, check_report_invalid) = if request.mode == AssistantMode::Check {
            let targets = target_ids.iter().cloned().collect();
            match parchmint_assistant_core::check_report::validate(
                &response.text,
                &context.passages,
                &targets,
            ) {
                Ok(mut report) => {
                    report.scope =
                        request
                            .target
                            .map(|target| parchmint_assistant_api::CheckScope {
                                target: parchmint_domain::encode_stable_id(target.as_bytes()),
                                evidence_scope: request
                                    .context_scope
                                    .map(|id| parchmint_domain::encode_stable_id(id.as_bytes())),
                                excluded: request
                                    .excluded_context
                                    .iter()
                                    .map(|id| parchmint_domain::encode_stable_id(id.as_bytes()))
                                    .collect(),
                                selection_only: request.selected_text.is_some(),
                            });
                    if let Some((authored, originals)) = &matching_sources {
                        parchmint_assistant_core::continuity_match::attach_matches(
                            &mut report,
                            &context.passages,
                            authored,
                            originals,
                        );
                    }
                    response.text.clone_from(&report.summary);
                    response.project_citations.clear();
                    response.unverified_project_citations.clear();
                    (Some(report), false)
                }
                Err(_) => (None, true),
            }
        } else {
            (None, false)
        };
        Ok(AssistantWorkflowResult {
            response,
            check_report,
            check_report_invalid,
            context: context.passages,
            context_truncated: context.truncated,
            context_warnings,
            estimated_cost_micros,
            estimated_period_total_micros,
            usage_warning,
            web_retrieved_at_unix_seconds: (request.mode == AssistantMode::Research).then(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            }),
            provider_binding: history_binding,
            notes_derived: request.include_notes || request.create_note,
            edit_proposal_target: proposal_target,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_application::{DocumentSnapshot, DocumentVisibility};
    use parchmint_domain::{ProjectId, ProjectSection};
    use parchmint_editor_api::{
        BlockId, CanonicalComment, CanonicalProjection, CommentId, EditorRevision, EditorSelection,
    };
    use parchmint_platform_native::{SessionCredentialStore, testing::NativeFixture};
    use parchmint_preferences::{
        AssistantPreferences, AssistantProvider, FilePreferenceStore, PreferenceCommand,
        PreferenceCoordinator,
    };
    use parchmint_ui_api::{ProjectQueryError, ProjectSnapshot};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        time::{Duration, Instant},
    };

    include!("assistant_retrieval_eval_tests.rs");

    struct SnapshotQuery(
        ProjectSnapshot,
        std::collections::BTreeMap<parchmint_domain::ResearchSourceId, Vec<u8>>,
    );

    #[test]
    fn identity_hints_use_only_local_explicit_constructions() {
        assert!(identity_hint_names("Mara is the Lantern Keeper.").contains("Mara"));
        assert!(identity_hint_names("The Lantern Keeper is Mara.").contains("Mara"));
        assert!(identity_hint_names("The night watch is Eren's nickname.").contains("Eren"));
        assert!(identity_hint_names("Mara is known as Fox.").contains("Fox"));
        assert!(identity_hint_names("Mara is called Fox.").contains("Fox"));
        let local = identity_hint_names("Mara is the Lantern Keeper, Lysa waited.");
        assert!(local.contains("Mara"));
        assert!(!local.contains("Lysa"));
        for sentence in [
            "Mara, Eren is known as Fox.",
            "Mara; Eren is known as Fox.",
            "Mara—Eren is known as Fox.",
            "Mara;Eren is known as Fox.",
        ] {
            let local = identity_hint_names(sentence);
            assert!(local.contains("Eren"));
            assert!(!local.contains("Mara"));
        }
        for sentence in [
            "Eren is known as Fox—Lysa left.",
            "Eren is known as Fox;Lysa left.",
            "Eren is known as Fox،Lysa left.",
        ] {
            let local = identity_hint_names(sentence);
            assert!(local.contains("Fox"));
            assert!(!local.contains("Lysa"));
        }
        for ordinary in [
            "Mara was tired. Winter arrived early.",
            "Mara was tired at Winter Harbor.",
            "Mara was Eren's friend. Winter arrived.",
            "Mara said, the Lantern Keeper is Lysa.",
            "Mara asked: the Lantern Keeper is Lysa?",
            "Mara was tired of Eren's nickname.",
            "Mara called Eren to warn him.",
        ] {
            assert!(
                identity_hint_names(ordinary).is_empty(),
                "unexpected hint: {ordinary}"
            );
        }
        let _ = identity_hint_names("İnci is the Lantern Keeper.");
    }

    #[test]
    fn long_context_pricing_applies_to_all_tokens_above_the_threshold() {
        let rates = Pricing {
            input_micros_per_million: 1_000_000,
            output_micros_per_million: 5_000_000,
            search_call_micros: 10_000,
        };
        assert_eq!(pricing_for_usage(rates, 272_000), rates);
        let long = pricing_for_usage(rates, 272_001);
        assert_eq!(long.input_micros_per_million, 2_000_000);
        assert_eq!(long.output_micros_per_million, 7_500_000);
        assert_eq!(long.search_call_micros, 10_000);
    }

    #[test]
    fn project_wide_notes_require_an_explicit_scope() {
        assert!(!note_reads_across_project("Read my notes on this scene"));
        assert!(note_reads_across_project(
            "Summarize notes across the project"
        ));
        assert!(!note_reads_across_project(
            "Read notes on this scene: compare events across the project"
        ));
        assert!(!note_reads_across_project(
            "Read notes on this scene across the project"
        ));
    }

    #[test]
    fn history_does_not_leak_notes_or_follow_a_provider_change() {
        use parchmint_ui_api::AssistantHistoryTurn;
        let turn =
            |binding: &str, notes_derived, question: &str, answer: &str| AssistantHistoryTurn {
                provider_binding: binding.into(),
                notes_derived,
                question: question.into(),
                answer: answer.into(),
            };
        let history = vec![
            turn("provider-a", false, "Question", "Answer"),
            turn(
                "provider-a",
                true,
                "Summarize my notes",
                "Private annotation",
            ),
            turn(
                "provider-b",
                false,
                "Other provider question",
                "Other provider answer",
            ),
        ];
        let permitted = permitted_history(history, "provider-a");
        assert_eq!(permitted.len(), 2);
        assert_eq!(permitted[0].text, "Question");
        assert_eq!(permitted[1].text, "Answer");
    }

    #[test]
    fn original_extraction_rejects_changed_bytes_and_skips_excluded_sources() {
        let mut project = Project::new(ProjectId::from_bytes([40; 16]));
        let document = DocumentId::from_bytes([41; 16]);
        let source = parchmint_domain::ResearchSourceId::from_bytes([42; 16]);
        project
            .nodes
            .try_insert_document(
                NodeId::from_bytes([43; 16]),
                document,
                ProjectSection::Research.root_id(),
                0,
                "Archive",
            )
            .unwrap();
        project.research_originals.insert(
            source,
            parchmint_domain::ResearchOriginal {
                document,
                filename: "archive.txt".into(),
                media_type: "text/plain".into(),
                source_url: None,
                retrieved_at_unix_seconds: 1,
                byte_length: 4,
                sha256: *parchmint_project_format::ContentHash::of_bytes(b"good").as_bytes(),
            },
        );
        let authored = AuthoredProjectSnapshot {
            project: project.clone(),
            document_summaries: Vec::new(),
            documents: vec![DocumentSnapshot {
                document_id: document,
                body: "<p>Archive</p>".into(),
                comments: Vec::new(),
                revision: EditorRevision::from(1),
                visibility: DocumentVisibility::Closed,
            }],
        };
        let query = SnapshotQuery(
            ProjectSnapshot {
                project,
                document_summaries: Vec::new(),
                documents: authored.documents.clone(),
                styles_css: String::new(),
            },
            std::collections::BTreeMap::from([(source, b"evil".to_vec())]),
        );
        let (text, warnings) = extracted_originals(
            &query,
            &authored,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(text.is_empty());
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("could not be verified"))
        );
        let (text, warnings) = extracted_originals(
            &query,
            &authored,
            &BTreeSet::new(),
            &BTreeSet::from([document]),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(text.is_empty() && warnings.is_empty());
        assert_eq!(
            extracted_originals(
                &query,
                &authored,
                &BTreeSet::new(),
                &BTreeSet::new(),
                &AtomicBool::new(true)
            ),
            Err(AssistantError::Cancelled)
        );
    }

    #[test]
    fn html_original_is_verified_extracted_and_cited_without_changing_its_bytes() {
        let bytes = "<!doctype html><body><p>Saffron arrived by sea at the café.</p><script>secret()</script></body>".as_bytes().to_vec();
        let mut project = Project::new(ProjectId::from_bytes([50; 16]));
        let document = DocumentId::from_bytes([51; 16]);
        let source = parchmint_domain::ResearchSourceId::from_bytes([52; 16]);
        project
            .nodes
            .try_insert_document(
                NodeId::from_bytes([53; 16]),
                document,
                ProjectSection::Research.root_id(),
                0,
                "Archive",
            )
            .unwrap();
        project.research_originals.insert(
            source,
            parchmint_domain::ResearchOriginal {
                document,
                filename: "archive.html".into(),
                media_type: "text/html; charset=utf-8".into(),
                source_url: Some("https://example.test/archive".into()),
                retrieved_at_unix_seconds: 1,
                byte_length: bytes.len() as u64,
                sha256: *parchmint_project_format::ContentHash::of_bytes(&bytes).as_bytes(),
            },
        );
        let authored = AuthoredProjectSnapshot {
            project: project.clone(),
            document_summaries: Vec::new(),
            documents: vec![DocumentSnapshot {
                document_id: document,
                body: "<p>Archive</p>".into(),
                comments: Vec::new(),
                revision: EditorRevision::from(1),
                visibility: DocumentVisibility::Closed,
            }],
        };
        let query = SnapshotQuery(
            ProjectSnapshot {
                project,
                document_summaries: Vec::new(),
                documents: authored.documents.clone(),
                styles_css: String::new(),
            },
            std::collections::BTreeMap::from([(source, bytes.clone())]),
        );
        let (extracted, warnings) = extracted_originals(
            &query,
            &authored,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(warnings.is_empty());
        assert_eq!(extracted.len(), 1);
        assert!(
            extracted[0]
                .text
                .contains("Saffron arrived by sea at the café.")
        );
        assert!(!extracted[0].text.contains("secret"));
        assert_eq!(query.1.get(&source).unwrap(), &bytes);
        let context = parchmint_assistant_core::retrieve_with_originals(
            &authored,
            "saffron",
            &BTreeSet::new(),
            &BTreeSet::new(),
            &extracted,
        )
        .unwrap();
        let evidence = context
            .passages
            .iter()
            .find_map(|passage| passage.original.as_ref())
            .unwrap();
        assert_eq!(
            evidence.extractor,
            parchmint_assistant_core::research_text::HTML_EXTRACTOR_VERSION
        );
        let (excluded, notices) = extracted_originals(
            &query,
            &authored,
            &BTreeSet::new(),
            &BTreeSet::from([document]),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(excluded.is_empty() && notices.is_empty());
        let mut incompatible = authored.clone();
        incompatible
            .project
            .research_originals
            .get_mut(&source)
            .unwrap()
            .media_type = "text/html; charset=windows-1252".into();
        let (omitted, notices) = extracted_originals(
            &query,
            &incompatible,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(omitted.is_empty());
        assert!(notices.iter().any(|notice| notice.contains("encoding")));
    }

    #[test]
    fn missing_group_targets_are_reported_as_incomplete_context() {
        let mut context = parchmint_assistant_core::RetrievedContext {
            passages: vec![ContextPassage {
                source_id: "S1".into(),
                document_id: "first".into(),
                document_revision: 0,
                section: ContextSection::Manuscript,
                title: "First".into(),
                anchor: None,
                text: "A scene.".into(),
                original: None,
            }],
            truncated: false,
        };
        mark_check_target_coverage(&mut context, &["first".into()]);
        assert!(!context.truncated);
        mark_check_target_coverage(&mut context, &["first".into(), "second".into()]);
        assert!(context.truncated);
        context.truncated = false;
        context.passages[0].section = ContextSection::Notes;
        mark_check_target_coverage(&mut context, &["first".into()]);
        assert!(context.truncated);
    }

    #[test]
    fn selected_text_is_preserved_before_the_bounded_check_target() {
        let selected = parchmint_ui_api::AssistantSelectedText {
            document: DocumentId::from_bytes([33; 16]),
            revision: EditorRevision::from(2),
            text: "é".repeat(parchmint_ui_api::MAX_ASSISTANT_SELECTION_BYTES / 2),
        };
        let target = ContextPassage {
            source_id: "old".into(),
            document_id: "target".into(),
            document_revision: 1,
            section: ContextSection::Manuscript,
            title: "Chapter".into(),
            anchor: None,
            text: "x".repeat(1200),
            original: None,
        };
        let mut context = parchmint_assistant_core::RetrievedContext {
            passages: vec![target; 48],
            truncated: false,
        };
        prepend_selection(&mut context, &selected, "Selection", ContextSection::Lore);
        assert!(context.truncated);
        assert!(context.passages.len() <= parchmint_assistant_core::MAX_PASSAGES);
        assert!(
            context.passages.iter().map(|p| p.text.len()).sum::<usize>()
                <= parchmint_assistant_core::MAX_TOTAL_BYTES
        );
        let actual = context
            .passages
            .iter()
            .filter(|p| p.title.contains("editor selection"))
            .map(|p| p.text.as_str())
            .collect::<String>();
        assert_eq!(actual, selected.text);
        assert!(context.passages.iter().any(|p| p.document_id == "target"));
        assert!(
            context
                .passages
                .iter()
                .enumerate()
                .all(|(index, p)| p.source_id == format!("S{}", index + 1))
        );
    }

    #[test]
    fn requested_notes_share_the_bounded_context_and_keep_valid_utf8() {
        let passage = ContextPassage {
            source_id: String::new(),
            document_id: "document".into(),
            document_revision: 1,
            section: ContextSection::Lore,
            title: "Evidence".into(),
            anchor: None,
            text: "a".repeat(1200),
            original: None,
        };
        let mut context = parchmint_assistant_core::RetrievedContext {
            passages: vec![passage.clone(); 48],
            truncated: false,
        };
        let mut note = passage;
        note.section = ContextSection::Notes;
        note.text = "🌊".repeat(1200);
        append_notes(&mut context, vec![note; 16]);
        assert!(context.truncated);
        assert!(context.passages.len() <= parchmint_assistant_core::MAX_PASSAGES);
        assert!(
            context.passages.iter().map(|p| p.text.len()).sum::<usize>()
                <= parchmint_assistant_core::MAX_TOTAL_BYTES
        );
        assert!(
            context
                .passages
                .iter()
                .any(|p| p.section == ContextSection::Notes)
        );
        assert!(
            context
                .passages
                .iter()
                .any(|p| p.section == ContextSection::Lore)
        );
        assert!(
            context
                .passages
                .iter()
                .enumerate()
                .all(|(index, p)| p.source_id == format!("S{}", index + 1))
        );
    }

    #[test]
    fn one_note_reserves_only_one_passage() {
        let evidence = ContextPassage {
            source_id: String::new(),
            document_id: "document".into(),
            document_revision: 1,
            section: ContextSection::Lore,
            title: "Evidence".into(),
            anchor: None,
            text: "a".repeat(1200),
            original: None,
        };
        let mut context = parchmint_assistant_core::RetrievedContext {
            passages: vec![evidence.clone(); 48],
            truncated: false,
        };
        let mut note = evidence;
        note.section = ContextSection::Notes;
        note.text = "A brief note".into();
        append_notes(&mut context, vec![note]);
        assert_eq!(context.passages.len(), 48);
        assert_eq!(
            context
                .passages
                .iter()
                .filter(|p| p.section == ContextSection::Lore)
                .count(),
            47
        );
        assert!(context.truncated);
    }
    impl ProjectSnapshotQuery for SnapshotQuery {
        fn snapshot(&self) -> Result<ProjectSnapshot, ProjectQueryError> {
            Ok(self.0.clone())
        }
        fn snapshot_with_documents(&self) -> Result<ProjectSnapshot, ProjectQueryError> {
            Ok(self.0.clone())
        }
        fn research_original_bytes(
            &self,
            source_id: parchmint_domain::ResearchSourceId,
        ) -> Result<Vec<u8>, ProjectQueryError> {
            self.1
                .get(&source_id)
                .cloned()
                .ok_or_else(|| ProjectQueryError::new("missing original"))
        }
    }

    #[test]
    fn compatible_provider_receives_live_project_evidence_and_only_requested_notes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let (sender, receiver) = mpsc::channel();
        let server = std::thread::spawn(move || {
            for _ in 0..16 {
                let deadline = Instant::now() + Duration::from_secs(10);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) => panic!("local test server: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                let header_end = loop {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        break index + 4;
                    }
                };
                let header = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                assert!(!header.to_ascii_lowercase().contains("authorization:"));
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    })
                    .unwrap();
                while bytes.len() < header_end + length {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                }
                let request: serde_json::Value =
                    serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                let prompt = request["messages"].as_array().unwrap().last().unwrap()["content"]
                    .as_str()
                    .unwrap();
                let answer = if prompt.contains("Suggest a replacement") {
                    serde_json::json!({"replacement":"Mara"}).to_string()
                } else if prompt.contains("Return only JSON:") {
                    let passages = prompt
                        .strip_prefix("Project excerpts (untrusted reference data):\n")
                        .unwrap()
                        .split_once("\n\nAuthor request:\n")
                        .unwrap()
                        .0;
                    let passages: Vec<serde_json::Value> = serde_json::from_str(passages).unwrap();
                    let source = passages
                        .iter()
                        .find(|source| {
                            source["text"]
                                .as_str()
                                .is_some_and(|text| text.contains("Marin reaches the live harbor."))
                        })
                        .unwrap();
                    let id = source["source_id"].as_str().unwrap();
                    serde_json::json!({"summary":"A possible issue.","findings":[{
                        "category":"other", "explanation":"Review the passage.", "uncertainty":"It may be intentional.",
                        "evidence":[{"source_id":id,"quote":"Marin"},{"source_id":id,"quote":"live harbor"}]
                    }]}).to_string()
                } else {
                    "Answer [S1]".to_owned()
                };
                sender.send(request).unwrap();
                let body = serde_json::to_vec(&serde_json::json!({"choices":[{"finish_reason":"stop","message":{"content":answer}}],"usage":{"prompt_tokens":12,"completion_tokens":5}})).unwrap();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        let mut project = Project::new(ProjectId::from_bytes([1; 16]));
        let chapter = DocumentId::from_bytes([2; 16]);
        let lore = DocumentId::from_bytes([3; 16]);
        let research = DocumentId::from_bytes([6; 16]);
        let source_id = parchmint_domain::ResearchSourceId::from_bytes([8; 16]);
        let pdf_id = parchmint_domain::ResearchSourceId::from_bytes([9; 16]);
        let source_bytes = b"Marin lighthouse keeper logs describe red beacons.".to_vec();
        let chapter_node = NodeId::from_bytes([4; 16]);
        project
            .nodes
            .try_insert_document(
                chapter_node,
                chapter,
                ProjectSection::Manuscript.root_id(),
                0,
                "Chapter",
            )
            .unwrap();
        project.research_originals.insert(
            source_id,
            parchmint_domain::ResearchOriginal {
                document: research,
                filename: "keepers.txt".into(),
                media_type: "text/plain".into(),
                source_url: Some("https://example.org/keepers.txt".into()),
                retrieved_at_unix_seconds: 123,
                byte_length: source_bytes.len() as u64,
                sha256: *parchmint_project_format::ContentHash::of_bytes(&source_bytes).as_bytes(),
            },
        );
        project.research_originals.insert(
            pdf_id,
            parchmint_domain::ResearchOriginal {
                document: research,
                filename: "archive.pdf".into(),
                media_type: "application/pdf".into(),
                source_url: None,
                retrieved_at_unix_seconds: 124,
                byte_length: 4,
                sha256: [0; 32],
            },
        );
        project
            .nodes
            .try_insert_document(
                NodeId::from_bytes([7; 16]),
                research,
                ProjectSection::Research.root_id(),
                0,
                "Research",
            )
            .unwrap();
        project
            .nodes
            .try_insert_document(
                NodeId::from_bytes([5; 16]),
                lore,
                ProjectSection::Lore.root_id(),
                0,
                "Keeper",
            )
            .unwrap();
        let documents = vec![
            DocumentSnapshot {
                document_id: chapter,
                body: "<p>old harbor</p>".into(),
                comments: Vec::new(),
                revision: EditorRevision::from(0),
                visibility: DocumentVisibility::Open,
            },
            DocumentSnapshot {
                document_id: lore,
                body: "<p>Marin has a silver key.</p><p>The story may be unreliable.</p>".into(),
                comments: vec![CanonicalComment::new(
                    CommentId::from_bytes([8; 16]),
                    EditorSelection::new(0.into(), 5.into()),
                    "Private lore note",
                    BlockId::from_bytes(*lore.as_bytes()),
                )],
                revision: EditorRevision::from(0),
                visibility: DocumentVisibility::Closed,
            },
            DocumentSnapshot {
                document_id: research,
                body: "<p>Marin outside research reference</p>".into(),
                comments: Vec::new(),
                revision: EditorRevision::from(0),
                visibility: DocumentVisibility::Closed,
            },
        ];
        let query = Arc::new(SnapshotQuery(
            ProjectSnapshot {
                project,
                document_summaries: Vec::new(),
                documents,
                styles_css: String::new(),
            },
            std::collections::BTreeMap::from([(source_id, source_bytes)]),
        ));
        let path = std::env::temp_dir().join(format!(
            "parchmint-assistant-workflow-{}-{}.json",
            std::process::id(),
            listener_port(&endpoint)
        ));
        let preferences: Arc<dyn PreferenceService> = Arc::new(PreferenceCoordinator::new(
            Arc::new(FilePreferenceStore::new(&path)),
        ));
        let current = block_on(preferences.load()).unwrap();
        let profile = AssistantPreferences {
            provider: AssistantProvider::OpenAiCompatible,
            endpoint,
            model: "mock".into(),
            authentication: AssistantAuthentication::None,
            ..Default::default()
        };
        let provider_binding = format!(
            "{}:no-auth",
            assistant_config::provider(&profile)
                .unwrap()
                .credential_binding()
        );
        block_on(preferences.update(current.revision, PreferenceCommand::SetAssistant(profile)))
            .unwrap();
        let fixture = NativeFixture::new();
        let platform = PlatformServices::new(
            fixture.menus(),
            fixture.dialogs(),
            fixture.clipboard(),
            fixture.external_open(),
            fixture.application_paths(),
            fixture.appearance(),
        )
        .with_session_credentials(Arc::new(SessionCredentialStore::default()));
        let assistant = ProductionAssistant {
            query,
            preferences,
            platform,
            budget: None,
            conversations: None,
        };
        let note = CanonicalComment::new(
            CommentId::from_bytes([7; 16]),
            EditorSelection::new(0.into(), 5.into()),
            "Private harbor note",
            BlockId::from_bytes(*chapter.as_bytes()),
        );
        let projection = CanonicalProjection::new(
            chapter,
            EditorRevision::from(1),
            "<p>Marin reaches the live harbor.</p>",
            vec![note],
            Vec::new(),
            5,
        );
        let make_request = |prompt: &str, include_notes| AssistantWorkflowRequest {
            mode: AssistantMode::Ask,
            prompt: prompt.into(),
            target: Some(chapter_node),
            context_scope: None,
            selected_text: None,
            pinned_context: Vec::new(),
            excluded_context: Vec::new(),
            history: Vec::new(),
            include_notes,
            explicit_notes: None,
            create_note: false,
            live_documents: vec![projection.clone()],
            edit_proposal: None,
        };
        let first = assistant
            .run(
                make_request("What happened to Marin?", false),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(
            first
                .context
                .iter()
                .any(|passage| passage.text.contains("live harbor")
                    && passage.document_revision == 1)
        );
        assert!(first.context.iter().any(|passage| {
            passage
                .original
                .as_ref()
                .is_some_and(|origin| origin.filename == "keepers.txt")
                && passage.text.contains("red beacons")
        }));
        assert!(
            first
                .context
                .iter()
                .any(|passage| passage.text == "The story may be unreliable.")
        );
        assert!(
            first
                .context_warnings
                .iter()
                .any(|warning| warning.contains("archive.pdf"))
        );
        let mut current_notes = make_request("Read my notes: what do they say about Marin?", true);
        current_notes
            .pinned_context
            .push(ProjectSection::Lore.root_id());
        let second = assistant
            .run(current_notes, &AtomicBool::new(false))
            .unwrap();
        assert!(
            second
                .context
                .iter()
                .any(|passage| passage.text.contains("Private harbor note"))
        );
        for prompt in [
            "Read notes on this passage: summarize the rebellion across the project",
            "Read my notes here: summarize the entire project",
        ] {
            assert_eq!(
                assistant
                    .run(make_request(prompt, true), &AtomicBool::new(false))
                    .unwrap_err(),
                AssistantError::InvalidRequest
            );
        }
        let across = assistant
            .run(
                make_request("Read notes: across the project", true),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(
            across
                .context
                .iter()
                .any(|p| p.text.contains("Private lore note"))
        );
        let captured_project = assistant.query.snapshot_with_documents().unwrap().project;
        let project_revision = captured_project.revision.value();
        let tree_hash = note_tree_hash(&captured_project);
        let mut explicit_current = make_request("What might Marin have forgotten?", true);
        explicit_current.explicit_notes =
            Some(parchmint_ui_api::AssistantExplicitNotes::Document {
                document: chapter,
                project_revision,
                tree_hash,
            });
        let current_result = assistant
            .run(explicit_current.clone(), &AtomicBool::new(false))
            .unwrap();
        assert!(current_result.notes_derived);
        assert!(
            current_result
                .context
                .iter()
                .any(|p| p.text.contains("Private harbor note"))
        );
        assert!(
            !current_result
                .context
                .iter()
                .any(|p| p.text.contains("Private lore note"))
        );
        let mut wrong_document = explicit_current.clone();
        wrong_document.explicit_notes = Some(parchmint_ui_api::AssistantExplicitNotes::Document {
            document: lore,
            project_revision,
            tree_hash,
        });
        assert_eq!(
            assistant
                .run(wrong_document, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::InvalidRequest
        );
        let mut stale_tree = explicit_current.clone();
        stale_tree.explicit_notes = Some(parchmint_ui_api::AssistantExplicitNotes::Document {
            document: chapter,
            project_revision: project_revision.saturating_add(1),
            tree_hash,
        });
        assert_eq!(
            assistant
                .run(stale_tree, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::InvalidRequest
        );
        let mut changed_tree = explicit_current.clone();
        let mut other_hash = tree_hash;
        other_hash[0] ^= 1;
        changed_tree.explicit_notes = Some(parchmint_ui_api::AssistantExplicitNotes::Document {
            document: chapter,
            project_revision,
            tree_hash: other_hash,
        });
        assert_eq!(
            assistant
                .run(changed_tree, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::InvalidRequest
        );
        let mut selection_with_notes = explicit_current.clone();
        selection_with_notes.selected_text = Some(parchmint_ui_api::AssistantSelectedText {
            document: chapter,
            revision: EditorRevision::from(1),
            text: "Marin".into(),
        });
        assert_eq!(
            assistant
                .run(selection_with_notes, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::SelectionNoteScope
        );
        let mut excluded_document = explicit_current;
        excluded_document.excluded_context.push(chapter_node);
        assert_eq!(
            assistant
                .run(excluded_document, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::NoteTargetExcluded
        );
        let mut explicit_project = make_request("What should Marin remember?", true);
        explicit_project.explicit_notes = Some(parchmint_ui_api::AssistantExplicitNotes::Project {
            project_revision,
            tree_hash,
        });
        for (prompt, scope) in [
            (
                "Do not read my notes",
                parchmint_ui_api::AssistantExplicitNotes::Project {
                    project_revision,
                    tree_hash,
                },
            ),
            (
                "Read notes on this scene only",
                parchmint_ui_api::AssistantExplicitNotes::Project {
                    project_revision,
                    tree_hash,
                },
            ),
            (
                "Answer without consulting notes",
                parchmint_ui_api::AssistantExplicitNotes::Document {
                    document: chapter,
                    project_revision,
                    tree_hash,
                },
            ),
            (
                "Answer without consulting annotations",
                parchmint_ui_api::AssistantExplicitNotes::Project {
                    project_revision,
                    tree_hash,
                },
            ),
            (
                "Use manuscript text only",
                parchmint_ui_api::AssistantExplicitNotes::Project {
                    project_revision,
                    tree_hash,
                },
            ),
            (
                "Do not read confidential material; answer from the manuscript",
                parchmint_ui_api::AssistantExplicitNotes::Project {
                    project_revision,
                    tree_hash,
                },
            ),
            (
                "Never access these; answer the question from the story",
                parchmint_ui_api::AssistantExplicitNotes::Project {
                    project_revision,
                    tree_hash,
                },
            ),
            (
                "Summarize these, omitting confidential entries",
                parchmint_ui_api::AssistantExplicitNotes::Project {
                    project_revision,
                    tree_hash,
                },
            ),
            (
                "Use the context but skip anything private",
                parchmint_ui_api::AssistantExplicitNotes::Project {
                    project_revision,
                    tree_hash,
                },
            ),
            (
                "Summarize them without revealing personal details",
                parchmint_ui_api::AssistantExplicitNotes::Project {
                    project_revision,
                    tree_hash,
                },
            ),
        ] {
            let mut conflicting = make_request(prompt, true);
            conflicting.explicit_notes = Some(scope);
            assert_eq!(
                assistant
                    .run(conflicting, &AtomicBool::new(false))
                    .unwrap_err(),
                AssistantError::InvalidRequest
            );
        }
        let project_result = assistant
            .run(explicit_project.clone(), &AtomicBool::new(false))
            .unwrap();
        assert!(project_result.notes_derived);
        assert!(
            project_result
                .context
                .iter()
                .any(|p| p.text.contains("Private lore note"))
        );
        let mut excluded_project = explicit_project.clone();
        excluded_project
            .excluded_context
            .push(NodeId::from_bytes([5; 16]));
        let excluded_result = assistant
            .run(excluded_project, &AtomicBool::new(false))
            .unwrap();
        assert!(
            !excluded_result
                .context
                .iter()
                .any(|p| p.text.contains("Private lore note"))
        );
        let mut scoped_project = explicit_project;
        scoped_project.context_scope = Some(ProjectSection::Lore.root_id());
        let scoped_result = assistant
            .run(scoped_project, &AtomicBool::new(false))
            .unwrap();
        assert!(
            !scoped_result
                .context
                .iter()
                .any(|p| p.text.contains("Private harbor note"))
        );
        assert!(
            scoped_result
                .context
                .iter()
                .any(|p| p.text.contains("Private lore note"))
        );
        let mut followup = make_request("Answer without using notes", false);
        followup.history = vec![
            parchmint_ui_api::AssistantHistoryTurn {
                question: "Ordinary earlier question".into(),
                answer: "Ordinary earlier answer".into(),
                provider_binding: provider_binding.clone(),
                notes_derived: false,
            },
            parchmint_ui_api::AssistantHistoryTurn {
                question: "Summarize my notes".into(),
                answer: "Private note summary".into(),
                provider_binding: provider_binding.clone(),
                notes_derived: true,
            },
            parchmint_ui_api::AssistantHistoryTurn {
                question: "Question from a different provider".into(),
                answer: "Answer from a different provider".into(),
                provider_binding: "different-provider".into(),
                notes_derived: false,
            },
        ];
        assistant.run(followup, &AtomicBool::new(false)).unwrap();
        let mut pinned = make_request("Describe customs", false);
        pinned.pinned_context.push(ProjectSection::Lore.root_id());
        let pinned_result = assistant.run(pinned, &AtomicBool::new(false)).unwrap();
        assert!(
            pinned_result
                .context
                .iter()
                .any(|p| p.text.contains("silver key"))
        );
        let mut excluded = make_request("Read notes: across the project", true);
        excluded.pinned_context.push(ProjectSection::Lore.root_id());
        excluded.excluded_context.push(NodeId::from_bytes([5; 16]));
        excluded
            .history
            .push(parchmint_ui_api::AssistantHistoryTurn {
                question: "Previous question".into(),
                answer: "Earlier secret silver key".into(),
                provider_binding: provider_binding.clone(),
                notes_derived: false,
            });
        let excluded_result = assistant.run(excluded, &AtomicBool::new(false)).unwrap();
        assert!(
            !excluded_result
                .context
                .iter()
                .any(|p| p.document_id == parchmint_domain::encode_stable_id(lore.as_bytes()))
        );
        let mut scoped = make_request("Read notes across the project", true);
        scoped.context_scope = Some(ProjectSection::Lore.root_id());
        scoped.pinned_context.extend([
            ProjectSection::Manuscript.root_id(),
            ProjectSection::Lore.root_id(),
        ]);
        scoped.history.push(parchmint_ui_api::AssistantHistoryTurn {
            question: "Previous unrestricted question".into(),
            answer: "Earlier unrestricted harbor".into(),
            provider_binding: provider_binding.clone(),
            notes_derived: false,
        });
        let scoped_result = assistant.run(scoped, &AtomicBool::new(false)).unwrap();
        assert!(scoped_result.context_warnings.is_empty());
        assert!(
            scoped_result
                .context
                .iter()
                .all(|p| p.document_id == parchmint_domain::encode_stable_id(lore.as_bytes()))
        );
        let mut scoped_check = make_request("Marin", false);
        scoped_check.mode = AssistantMode::Check;
        scoped_check.context_scope = Some(ProjectSection::Lore.root_id());
        let scoped_check_result = assistant
            .run(scoped_check, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(
            scoped_check_result
                .check_report
                .as_ref()
                .unwrap()
                .findings
                .len(),
            1
        );
        assert!(!scoped_check_result.check_report_invalid);
        assert_eq!(
            scoped_check_result
                .check_report
                .as_ref()
                .unwrap()
                .scope
                .as_ref()
                .unwrap()
                .target,
            parchmint_domain::encode_stable_id(chapter_node.as_bytes())
        );
        assert_eq!(scoped_check_result.response.text, "A possible issue.");
        assert!(
            scoped_check_result
                .context
                .iter()
                .any(|p| p.document_id == parchmint_domain::encode_stable_id(chapter.as_bytes()))
        );
        assert!(
            scoped_check_result
                .context
                .iter()
                .any(|p| p.document_id == parchmint_domain::encode_stable_id(lore.as_bytes()))
        );
        assert!(
            !scoped_check_result
                .context
                .iter()
                .any(|p| p.document_id == parchmint_domain::encode_stable_id(research.as_bytes()))
        );
        let check = scoped_check_result.check_report.as_ref().unwrap();
        let mut proposed = make_request("Propose edit", false);
        proposed.context_scope = Some(ProjectSection::Lore.root_id());
        proposed
            .history
            .push(parchmint_ui_api::AssistantHistoryTurn {
                question: "Unrelated private history".into(),
                answer: "secret history".into(),
                provider_binding: provider_binding.clone(),
                notes_derived: false,
            });
        proposed.edit_proposal = Some(parchmint_ui_api::AssistantEditProposalRequest {
            finding: check.findings[0].clone(),
            scope: check.scope.clone().unwrap(),
            context: scoped_check_result.context.clone(),
            evidence_index: 0,
        });
        let mut debug_snapshot = assistant.query.snapshot_with_documents().unwrap();
        debug_snapshot
            .documents
            .iter_mut()
            .find(|doc| doc.document_id == chapter)
            .unwrap()
            .body = projection.body().to_owned();
        debug_snapshot
            .documents
            .iter_mut()
            .find(|doc| doc.document_id == chapter)
            .unwrap()
            .revision = projection.revision();
        let authored = AuthoredProjectSnapshot {
            project: debug_snapshot.project,
            document_summaries: Vec::new(),
            documents: debug_snapshot.documents,
        };
        let spec = proposed.edit_proposal.as_ref().unwrap();
        let candidate = parchmint_assistant_core::continuity_match::capture(
            &spec.finding,
            &spec.scope,
            &spec.context,
            parchmint_domain::ContinuityDecision::Intentional,
        )
        .expect("capture");
        assert!(
            parchmint_assistant_core::continuity_match::evidence_is_current(
                &candidate,
                &authored,
                &[]
            ),
            "candidate: {candidate:?}"
        );
        assert!(
            parchmint_assistant_core::edit_proposal::target(
                &spec.finding,
                &spec.scope,
                &spec.context,
                0,
                &authored,
                &[]
            )
            .is_some(),
            "target: {candidate:?}"
        );
        let mut stale = proposed.clone();
        stale.live_documents.clear();
        assert_eq!(
            assistant.run(stale, &AtomicBool::new(false)).unwrap_err(),
            AssistantError::InvalidRequest
        );
        let proposal_result = assistant.run(proposed, &AtomicBool::new(false)).unwrap();
        assert_eq!(proposal_result.response.text, "Mara");
        assert_eq!(
            proposal_result
                .edit_proposal_target
                .as_ref()
                .unwrap()
                .original,
            "Marin"
        );
        let mut selection_ask = make_request("What is selected?", false);
        selection_ask.selected_text = Some(parchmint_ui_api::AssistantSelectedText {
            document: chapter,
            revision: EditorRevision::from(1),
            text: "live harbor".into(),
        });
        selection_ask
            .pinned_context
            .push(ProjectSection::Lore.root_id());
        selection_ask
            .history
            .push(parchmint_ui_api::AssistantHistoryTurn {
                question: "Earlier unrelated question".into(),
                answer: "Earlier silver key".into(),
                provider_binding,
                notes_derived: false,
            });
        let selection_result = assistant
            .run(selection_ask.clone(), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(selection_result.context.len(), 1);
        assert_eq!(selection_result.context[0].text, "live harbor");
        assert!(selection_result.context_warnings.is_empty());
        let mut selection_check = make_request("Check this", false);
        selection_check.mode = AssistantMode::Check;
        selection_check.selected_text = Some(parchmint_ui_api::AssistantSelectedText {
            document: lore,
            revision: EditorRevision::from(0),
            text: "silver key".into(),
        });
        let selection_check_result = assistant
            .run(selection_check, &AtomicBool::new(false))
            .unwrap();
        assert!(selection_check_result.check_report.is_some());
        assert_eq!(selection_check_result.context[0].text, "silver key");
        assert!(
            selection_check_result
                .context
                .iter()
                .any(|p| p.text.contains("live harbor"))
        );
        assert!(
            !selection_check_result
                .context
                .iter()
                .any(|p| p.text.contains("outside research"))
        );
        let many_notes = (0..17)
            .map(|index| {
                CanonicalComment::new(
                    CommentId::from_bytes([index; 16]),
                    EditorSelection::new(0.into(), 5.into()),
                    format!("Note {index}"),
                    BlockId::from_bytes(*chapter.as_bytes()),
                )
            })
            .collect();
        let mut bounded_notes = make_request("Read notes on this scene", true);
        bounded_notes.live_documents = vec![CanonicalProjection::new(
            chapter,
            EditorRevision::from(1),
            "<p>Marin reaches the live harbor.</p>",
            many_notes,
            Vec::new(),
            5,
        )];
        let bounded_result = assistant
            .run(bounded_notes, &AtomicBool::new(false))
            .unwrap();
        assert!(bounded_result.context_truncated);
        let mut denied_selection = selection_ask.clone();
        denied_selection
            .excluded_context
            .push(ProjectSection::Manuscript.root_id());
        assert_eq!(
            assistant
                .run(denied_selection, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::SelectionExcluded
        );
        let mut stale_selection = selection_ask.clone();
        stale_selection.selected_text.as_mut().unwrap().revision = EditorRevision::from(999);
        assert_eq!(
            assistant
                .run(stale_selection, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::SelectionUnavailable
        );
        let mut selection_notes = make_request("Read notes on this scene", true);
        selection_notes.selected_text = selection_ask.selected_text;
        assert_eq!(
            assistant
                .run(selection_notes, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::SelectionNoteScope
        );
        let mut scoped_local_notes = make_request("Read notes on this scene", true);
        scoped_local_notes.context_scope = Some(ProjectSection::Lore.root_id());
        assert_eq!(
            assistant
                .run(scoped_local_notes, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::NoteTargetExcluded
        );
        let mut excluded_target = make_request("Check continuity", false);
        excluded_target.mode = AssistantMode::Check;
        excluded_target
            .excluded_context
            .push(ProjectSection::Manuscript.root_id());
        assert_eq!(
            assistant
                .run(excluded_target, &AtomicBool::new(false))
                .unwrap_err(),
            AssistantError::CheckTargetExcluded
        );
        let mut missing = make_request("What happened?", false);
        missing.excluded_context.push(NodeId::from_bytes([99; 16]));
        assert_eq!(
            assistant.run(missing, &AtomicBool::new(false)).unwrap_err(),
            AssistantError::ContextUnavailable
        );
        assert_eq!(
            assistant
                .run(
                    make_request("Read my notes in Chapter 2", true),
                    &AtomicBool::new(false)
                )
                .unwrap_err(),
            AssistantError::InvalidRequest
        );
        assert_eq!(
            assistant
                .run(
                    make_request("Read my notes: only from Chapter 2", true),
                    &AtomicBool::new(false)
                )
                .unwrap_err(),
            AssistantError::InvalidRequest
        );
        assert_eq!(
            assistant
                .run(
                    make_request("Read notes across the project: Chapter 2 only", true),
                    &AtomicBool::new(false)
                )
                .unwrap_err(),
            AssistantError::InvalidRequest
        );
        for prompt in [
            "Read notes across the project: except private notes",
            "Read notes across the project: skip confidential notes",
            "Read notes across the project: avoid confidential notes",
            "Read notes across the project but avoid confidential notes: what do they say?",
            "Read notes across the project; avoid confidential notes",
            "Read notes on this scene but avoid confidential notes",
        ] {
            assert_eq!(
                assistant
                    .run(make_request(prompt, true), &AtomicBool::new(false))
                    .unwrap_err(),
                AssistantError::InvalidRequest
            );
        }
        let sent_without_notes = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_with_notes = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_across = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_explicit_current = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_explicit_project = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_explicit_excluded = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_explicit_scoped = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_followup = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_pinned = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_excluded = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_scoped = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_scoped_check = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_proposal = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_selection = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_selection_check = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        let sent_bounded_notes = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .to_string();
        assert!(sent_without_notes.contains("live harbor"));
        assert!(sent_without_notes.contains("red beacons"));
        assert!(sent_without_notes.contains("keepers.txt"));
        assert!(sent_without_notes.contains("silver key"));
        assert!(sent_without_notes.contains("Neighboring paragraphs are additional context"));
        assert!(!sent_without_notes.contains("old harbor"));
        assert!(!sent_without_notes.contains("Private harbor note"));
        assert!(sent_with_notes.contains("Private harbor note"));
        assert!(!sent_with_notes.contains("Private lore note"));
        assert!(sent_across.contains("Private harbor note"));
        assert!(sent_across.contains("Private lore note"));
        assert!(sent_explicit_current.contains("Private harbor note"));
        assert!(!sent_explicit_current.contains("Private lore note"));
        assert!(sent_explicit_project.contains("Private harbor note"));
        assert!(sent_explicit_project.contains("Private lore note"));
        assert!(sent_explicit_excluded.contains("Private harbor note"));
        assert!(!sent_explicit_excluded.contains("Private lore note"));
        assert!(!sent_explicit_scoped.contains("Private harbor note"));
        assert!(sent_explicit_scoped.contains("Private lore note"));
        assert!(sent_followup.contains("Ordinary earlier answer"));
        assert!(!sent_followup.contains("Private note summary"));
        assert!(!sent_followup.contains("Private harbor note"));
        assert!(!sent_followup.contains("Question from a different provider"));
        assert!(sent_pinned.contains("silver key"));
        assert!(!sent_excluded.contains("silver key"));
        assert!(!sent_excluded.contains("The story may be unreliable."));
        assert!(!sent_excluded.contains("Private lore note"));
        assert!(!sent_excluded.contains("Previous question"));
        assert!(sent_excluded.contains("Private harbor note"));
        assert!(!sent_scoped.contains("live harbor"));
        assert!(!sent_scoped.contains("Private harbor note"));
        assert!(!sent_scoped.contains("Previous unrestricted question"));
        assert!(sent_scoped.contains("Private lore note"));
        assert!(sent_scoped_check.contains("live harbor"));
        assert!(sent_proposal.contains("Suggest a replacement"));
        assert!(!sent_proposal.contains("secret history"));
        assert!(!sent_proposal.contains("silver key"));
        assert!(!sent_proposal.contains("red beacons"));
        assert!(!sent_scoped_check.contains("outside research reference"));
        assert!(sent_selection.contains("live harbor"));
        assert!(!sent_selection.contains("Neighboring paragraphs are additional context"));
        assert!(!sent_selection.contains("silver key"));
        assert!(!sent_selection.contains("Earlier unrelated question"));
        assert!(!sent_selection.contains("red beacons"));
        assert!(sent_selection_check.contains("silver key"));
        assert!(!sent_selection_check.contains("Neighboring paragraphs are additional context"));
        assert!(sent_selection_check.contains("live harbor"));
        assert!(!sent_selection_check.contains("red beacons"));
        assert!(sent_bounded_notes.contains("requested notes were omitted"));
        assert!(sent_bounded_notes.contains("Note 0"));
        assert!(!sent_bounded_notes.contains("Note 16"));
        server.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    fn listener_port(endpoint: &str) -> String {
        endpoint
            .chars()
            .filter(|character| character.is_ascii_digit())
            .collect()
    }
}
