//! Bounded retrieval from live authored snapshots; never performs mutations.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use parchmint_application::AuthoredProjectSnapshot;
use parchmint_assistant_api::{
    ContextPassage, OriginalSourceEvidence, ProjectSection as ContextSection,
};
use parchmint_domain::{DocumentId, NodeKind, ProjectSection, ResearchSourceId, encode_stable_id};
use parchmint_editor_api::CanonicalDocumentLoad;
use parchmint_editor_core::EditorCoreSession;

pub mod check_report;
pub mod continuity_match;
pub mod edit_proposal;
pub use parchmint_application::research_text;

pub const MAX_PASSAGES: usize = 48;
pub const MAX_SCALARS_PER_PASSAGE: usize = 1200;
pub const MAX_TOTAL_BYTES: usize = 64 * 1024;
const MAX_ORIGINAL_SEGMENTS: usize = 16_384;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalError {
    IncompleteSnapshot,
    InvalidDocument,
    InvalidOriginal,
}

/// Derived local text. The original remains separately stored and unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedOriginal {
    pub source_id: ResearchSourceId,
    pub text: String,
}

/// Retrieval is bounded and lexical; `truncated` signals omitted candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievedContext {
    pub passages: Vec<ContextPassage>,
    pub truncated: bool,
}

type RetrievalCandidate = (bool, (usize, usize), ContextPassage);
const IDENTITY_RESERVE: usize = 2;
// A few outside-document representatives can surface qualifiers that score
// below many similar passages from one document. Remaining slots stay ranked.
const DIVERSITY_RESERVE: usize = 4;

fn is_identity_only(candidate: &RetrievalCandidate, names: &BTreeSet<String>) -> bool {
    !candidate.0
        && candidate.1.0 == 0
        && !names.is_empty()
        && (score(names, &candidate.2.text) > 0 || score(names, &candidate.2.title) > 0)
}

fn prune_candidates(candidates: &mut Vec<RetrievalCandidate>, names: &BTreeSet<String>) {
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let mut pinned_count = 0;
    let mut other_count = 0;
    let mut identity_count = 0;
    let mut other_documents = BTreeSet::new();
    let mut diverse_count = 0;
    candidates.retain(|candidate| {
        if candidate.0 {
            pinned_count += 1;
            pinned_count <= MAX_PASSAGES
        } else if is_identity_only(candidate, names) && identity_count < IDENTITY_RESERVE {
            identity_count += 1;
            true
        } else {
            other_count += 1;
            if other_count <= MAX_PASSAGES {
                other_documents.insert(candidate.2.document_id.clone());
                true
            } else if diverse_count < MAX_PASSAGES
                && other_documents.insert(candidate.2.document_id.clone())
            {
                diverse_count += 1;
                true
            } else {
                false
            }
        }
    });
}

/// Selects pinned documents and matching paragraphs from other authored sections.
/// Notes are deliberately excluded: reading notes requires a separate request.
pub fn retrieve(
    snapshot: &AuthoredProjectSnapshot,
    query: &str,
    pinned: &BTreeSet<DocumentId>,
    excluded: &BTreeSet<DocumentId>,
) -> Result<RetrievedContext, RetrievalError> {
    retrieve_with_originals(snapshot, query, pinned, excluded, &[])
}

/// Includes locally extracted original text in the same ranking and limits as
/// authored passages. Source identities must still match this snapshot.
pub fn retrieve_with_originals(
    snapshot: &AuthoredProjectSnapshot,
    query: &str,
    pinned: &BTreeSet<DocumentId>,
    excluded: &BTreeSet<DocumentId>,
    originals: &[ExtractedOriginal],
) -> Result<RetrievedContext, RetrievalError> {
    retrieve_with_originals_and_expansion(snapshot, query, "", pinned, excluded, originals)
}

/// Ranks direct question matches before ordinary expansion-only matches.
/// Up to four outside slots prefer otherwise unrepresented documents within
/// those bands, and target-derived terms break ties in relevance.
pub fn retrieve_with_originals_and_expansion(
    snapshot: &AuthoredProjectSnapshot,
    query: &str,
    expansion: &str,
    pinned: &BTreeSet<DocumentId>,
    excluded: &BTreeSet<DocumentId>,
    originals: &[ExtractedOriginal],
) -> Result<RetrievedContext, RetrievalError> {
    retrieve_inner(snapshot, query, expansion, "", pinned, excluded, originals)
}

/// Reserves up to two outside-evidence slots for name-only matches derived
/// from allowed identity statements, in addition to the diversity preference.
pub fn retrieve_with_identity_expansion(
    snapshot: &AuthoredProjectSnapshot,
    query: &str,
    expansion: &str,
    identity_names: &str,
    pinned: &BTreeSet<DocumentId>,
    excluded: &BTreeSet<DocumentId>,
    originals: &[ExtractedOriginal],
) -> Result<RetrievedContext, RetrievalError> {
    retrieve_inner(
        snapshot,
        query,
        expansion,
        identity_names,
        pinned,
        excluded,
        originals,
    )
}

fn retrieve_inner(
    snapshot: &AuthoredProjectSnapshot,
    query: &str,
    expansion: &str,
    identity_names: &str,
    pinned: &BTreeSet<DocumentId>,
    excluded: &BTreeSet<DocumentId>,
    originals: &[ExtractedOriginal],
) -> Result<RetrievedContext, RetrievalError> {
    let terms = terms(query);
    let name_terms = self::terms(identity_names);
    let mut expansion_terms = self::terms(expansion);
    expansion_terms.extend(name_terms.iter().cloned());
    let documents: BTreeMap<_, _> = snapshot
        .documents
        .iter()
        .map(|document| (document.document_id, document))
        .collect();
    let mut candidates = Vec::new();
    let mut truncated = false;
    for (_, node) in snapshot.project.nodes.iter() {
        let NodeKind::Document(document_id) = node.kind else {
            continue;
        };
        if excluded.contains(&document_id) {
            continue;
        }
        let section = match snapshot.project.nodes.section(node.id) {
            Some(ProjectSection::Manuscript) => ContextSection::Manuscript,
            Some(ProjectSection::Lore) => ContextSection::Lore,
            Some(ProjectSection::Research) => ContextSection::Research,
            _ => continue,
        };
        let document = documents
            .get(&document_id)
            .ok_or(RetrievalError::IncompleteSnapshot)?;
        let mut load = CanonicalDocumentLoad::new(document_id, &document.body);
        load.revision = document.revision;
        let session = EditorCoreSession::open(load).map_err(|_| RetrievalError::InvalidDocument)?;
        let projection = session.canonical_projection();
        let title_score = (
            score(&terms, &node.title) * 3,
            score(&expansion_terms, &node.title) * 3,
        );
        let mut block_start = 0_usize;
        for block in projection.semantic().blocks() {
            let start = block_start;
            block_start = block_start
                .saturating_add(block.scalar_len())
                .saturating_add(1);
            let raw_text = block.text();
            let text = raw_text.trim();
            if text.is_empty() {
                continue;
            }
            let leading_bytes = raw_text.len() - raw_text.trim_start().len();
            let mut chunk_start = start + raw_text[..leading_bytes].chars().count();
            let is_pinned = pinned.contains(&document_id);
            let mut chunk = String::new();
            let mut scalar_count = 0;
            for character in text.chars().map(Some).chain(std::iter::once(None)) {
                if let Some(character) = character
                    && scalar_count < MAX_SCALARS_PER_PASSAGE
                {
                    chunk.push(character);
                    scalar_count += 1;
                    continue;
                }
                let relevance = (
                    score(&terms, &chunk) + title_score.0,
                    score(&expansion_terms, &chunk) + title_score.1,
                );
                if !chunk.is_empty() && (is_pinned || relevance != (0, 0)) {
                    candidates.push((
                        is_pinned,
                        relevance,
                        ContextPassage {
                            source_id: String::new(),
                            document_id: encode_stable_id(document_id.as_bytes()),
                            document_revision: document.revision.value(),
                            section,
                            title: node.title.clone(),
                            // Scalar coordinates are meaningful only at this exact revision.
                            anchor: Some(format!(
                                "text:{chunk_start}:{}",
                                chunk_start + scalar_count
                            )),
                            text: std::mem::take(&mut chunk),
                            original: None,
                        },
                    ));
                    if candidates.len() > MAX_PASSAGES * 4 {
                        prune_candidates(&mut candidates, &name_terms);
                        truncated = true;
                    }
                }
                chunk_start += scalar_count;
                chunk.clear();
                scalar_count = 0;
                if let Some(character) = character {
                    chunk.push(character);
                    scalar_count = 1;
                }
            }
        }
    }
    let mut visited_original_segments = 0_usize;
    'originals: for extracted in originals {
        let original = snapshot
            .project
            .research_originals
            .get(&extracted.source_id)
            .ok_or(RetrievalError::InvalidOriginal)?;
        let Some(extractor_version) = research_text::extractor_version(&original.media_type) else {
            continue;
        };
        if excluded.contains(&original.document) {
            continue;
        }
        let node = snapshot
            .project
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.kind == NodeKind::Document(original.document)).then_some(node)
            })
            .ok_or(RetrievalError::InvalidOriginal)?;
        if snapshot.project.nodes.section(node.id) != Some(ProjectSection::Research) {
            return Err(RetrievalError::InvalidOriginal);
        }
        let document = documents
            .get(&original.document)
            .ok_or(RetrievalError::IncompleteSnapshot)?;
        let title = format!("{} · original {}", node.title, original.filename);
        let title_score = (
            score(&terms, &title) * 3,
            score(&expansion_terms, &title) * 3,
        );
        let is_pinned = pinned.contains(&original.document);
        let mut segment = 0_u64;
        for line in extracted.text.lines() {
            let mut chunk = String::new();
            let mut scalar_count = 0;
            for character in line.chars().map(Some).chain(std::iter::once(None)) {
                if let Some(character) = character
                    && scalar_count < MAX_SCALARS_PER_PASSAGE
                {
                    chunk.push(character);
                    scalar_count += 1;
                    continue;
                }
                if !chunk.trim().is_empty() {
                    if visited_original_segments == MAX_ORIGINAL_SEGMENTS {
                        truncated = true;
                        break 'originals;
                    }
                    visited_original_segments += 1;
                    segment += 1;
                    let relevance = (
                        score(&terms, &chunk) + title_score.0,
                        score(&expansion_terms, &chunk) + title_score.1,
                    );
                    if is_pinned || relevance != (0, 0) {
                        candidates.push((
                            is_pinned,
                            relevance,
                            ContextPassage {
                                source_id: String::new(),
                                document_id: encode_stable_id(original.document.as_bytes()),
                                document_revision: document.revision.value(),
                                section: ContextSection::Research,
                                title: title.clone(),
                                anchor: None,
                                text: std::mem::take(&mut chunk),
                                original: Some(OriginalSourceEvidence {
                                    source_id: encode_stable_id(extracted.source_id.as_bytes()),
                                    sha256: original.sha256,
                                    filename: original.filename.clone(),
                                    source_url: original.source_url.clone(),
                                    retrieved_at_unix_seconds: original.retrieved_at_unix_seconds,
                                    extractor: extractor_version.into(),
                                    segment,
                                }),
                            },
                        ));
                        if candidates.len() > MAX_PASSAGES * 4 {
                            prune_candidates(&mut candidates, &name_terms);
                            truncated = true;
                        }
                    }
                }
                chunk.clear();
                scalar_count = 0;
                if let Some(character) = character {
                    chunk.push(character);
                    scalar_count = 1;
                }
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let (pinned_candidates, other_candidates): (Vec<_>, Vec<_>) =
        candidates.into_iter().partition(|(pinned, _, _)| *pinned);
    let mut pinned_candidates: VecDeque<_> = pinned_candidates.into();
    let (identity_candidates, other_candidates): (Vec<_>, Vec<_>) = other_candidates
        .into_iter()
        .partition(|candidate| is_identity_only(candidate, &name_terms));
    let mut identity_candidates: VecDeque<_> = identity_candidates.into();
    let mut other_candidates: VecDeque<_> = other_candidates.into();
    let mut ordered = Vec::new();
    // Reserve up to half the context for outside evidence when it exists,
    // then use any remaining room for target passages.
    for _ in 0..MAX_PASSAGES / 2 {
        if let Some(candidate) = pinned_candidates.pop_front() {
            ordered.push(candidate);
        }
    }
    let available = MAX_PASSAGES.saturating_sub(ordered.len());
    let reserve = IDENTITY_RESERVE
        .min(identity_candidates.len())
        .min(available);
    let outside_slots = available.saturating_sub(reserve);
    let diversity_slots = DIVERSITY_RESERVE.min(outside_slots);
    let mut represented = BTreeSet::new();
    for _ in 0..outside_slots.saturating_sub(diversity_slots) {
        if let Some(candidate) = other_candidates.pop_front() {
            represented.insert(candidate.2.document_id.clone());
            ordered.push(candidate);
        }
    }
    for _ in 0..diversity_slots {
        // Do not let an ordinary expansion-only hit replace a direct match.
        // Identity hints have their own separately bounded reserve below.
        let require_direct_match = other_candidates
            .front()
            .is_some_and(|candidate| candidate.1.0 > 0);
        let position = other_candidates.iter().position(|candidate| {
            (!require_direct_match || candidate.1.0 > 0)
                && !represented.contains(&candidate.2.document_id)
        });
        let candidate = position
            .and_then(|index| other_candidates.remove(index))
            .or_else(|| other_candidates.pop_front());
        if let Some(candidate) = candidate {
            represented.insert(candidate.2.document_id.clone());
            ordered.push(candidate);
        }
    }
    for _ in 0..reserve {
        if let Some(candidate) = identity_candidates.pop_front() {
            ordered.push(candidate);
        }
    }
    ordered.extend(other_candidates);
    ordered.extend(identity_candidates);
    ordered.extend(pinned_candidates);
    let mut passages = Vec::new();
    let mut bytes = 0usize;
    for (_, _, mut passage) in ordered {
        let size = passage.text.len();
        if passages.len() == MAX_PASSAGES || bytes.saturating_add(size) > MAX_TOTAL_BYTES {
            truncated = true;
            continue;
        }
        bytes += size;
        passage.source_id = format!("S{}", passages.len() + 1);
        passages.push(passage);
    }
    Ok(RetrievedContext {
        passages,
        truncated,
    })
}

/// Append a small amount of immediately adjacent authored prose after all
/// ranked evidence. These passages are context, not proof of a relationship.
/// Callers must omit this step for selection-only requests.
pub fn append_adjacent_context(
    snapshot: &AuthoredProjectSnapshot,
    query: &str,
    excluded: &BTreeSet<DocumentId>,
    context: &mut RetrievedContext,
) -> Result<usize, RetrievalError> {
    use parchmint_domain::decode_stable_id;
    use parchmint_editor_api::{SemanticBlock, SemanticBlockKind};

    const MAX_SEEDS: usize = 16;
    const MAX_DOCUMENTS: usize = 8;
    const MAX_SHORTLIST: usize = 32;
    const MAX_ADDED: usize = 2;

    fn range(anchor: &str) -> Option<(usize, usize)> {
        let mut parts = anchor.split(':');
        if parts.next()? != "text" {
            return None;
        }
        let start = parts.next()?.parse().ok()?;
        let end = parts.next()?.parse().ok()?;
        (parts.next().is_none() && start < end).then_some((start, end))
    }

    fn paragraph_excerpt(
        block: &SemanticBlock,
        block_start: usize,
        trailing: bool,
        document: DocumentId,
        revision: u64,
        section: ContextSection,
        title: &str,
    ) -> Option<(usize, usize, ContextPassage)> {
        if block.kind() != SemanticBlockKind::Paragraph {
            return None;
        }
        let raw = block.text();
        let text = raw.trim();
        if text.is_empty() {
            return None;
        }
        let leading_bytes = raw.len() - raw.trim_start().len();
        let leading = raw[..leading_bytes].chars().count();
        let count = text.chars().count();
        let start_offset = if trailing {
            count.saturating_sub(MAX_SCALARS_PER_PASSAGE)
        } else {
            0
        };
        let excerpt = text
            .chars()
            .skip(start_offset)
            .take(MAX_SCALARS_PER_PASSAGE)
            .collect::<String>();
        let start = block_start + leading + start_offset;
        let end = start + excerpt.chars().count();
        Some((
            start,
            end,
            ContextPassage {
                source_id: String::new(),
                document_id: encode_stable_id(document.as_bytes()),
                document_revision: revision,
                section,
                title: title.to_owned(),
                anchor: Some(format!("text:{start}:{end}")),
                text: excerpt,
                original: None,
            },
        ))
    }

    let query_terms = terms(query);
    let mut seeds = Vec::new();
    let mut occupied = Vec::new();
    for (rank, passage) in context.passages.iter().enumerate() {
        if passage.original.is_some() || passage.section == ContextSection::Notes {
            continue;
        }
        let Some(document) = decode_stable_id(&passage.document_id).map(DocumentId::from_bytes)
        else {
            continue;
        };
        let Some((start, end)) = passage.anchor.as_deref().and_then(range) else {
            continue;
        };
        occupied.push((document, passage.document_revision, start, end));
        if !excluded.contains(&document)
            && score(&query_terms, &passage.text) > 0
            && seeds.len() < MAX_SEEDS
        {
            seeds.push((rank, document, passage.document_revision, start, end));
        }
    }
    let mut documents = Vec::new();
    for &(_, document, _, _, _) in &seeds {
        if !documents.contains(&document) {
            if documents.len() == MAX_DOCUMENTS {
                continue;
            }
            documents.push(document);
        }
    }
    let mut shortlist: Vec<(usize, usize, DocumentId, u64, usize, usize, ContextPassage)> =
        Vec::new();
    for document_id in documents {
        let node = snapshot
            .project
            .nodes
            .iter()
            .find_map(|(_, node)| (node.kind == NodeKind::Document(document_id)).then_some(node))
            .ok_or(RetrievalError::InvalidDocument)?;
        let section = match snapshot.project.nodes.section(node.id) {
            Some(ProjectSection::Manuscript) => ContextSection::Manuscript,
            Some(ProjectSection::Lore) => ContextSection::Lore,
            Some(ProjectSection::Research) => ContextSection::Research,
            _ => continue,
        };
        let document = snapshot
            .documents
            .iter()
            .find(|document| document.document_id == document_id)
            .ok_or(RetrievalError::IncompleteSnapshot)?;
        let mut load = CanonicalDocumentLoad::new(document_id, &document.body);
        load.revision = document.revision;
        let session = EditorCoreSession::open(load).map_err(|_| RetrievalError::InvalidDocument)?;
        let projection = session.canonical_projection();
        let blocks = projection.semantic().blocks();
        let mut block_start = 0_usize;
        let mut previous_start = 0_usize;
        for (index, block) in blocks.iter().enumerate() {
            let start = block_start;
            block_start = start.saturating_add(block.scalar_len()).saturating_add(1);
            if block.kind() == SemanticBlockKind::Paragraph {
                let raw = block.text();
                let trimmed = raw.trim();
                let leading_bytes = raw.len() - raw.trim_start().len();
                let paragraph_start = start + raw[..leading_bytes].chars().count();
                let paragraph_end = paragraph_start + trimmed.chars().count();
                for &(rank, seed_document, revision, seed_start, seed_end) in &seeds {
                    if seed_document != document_id
                        || revision != document.revision.value()
                        || seed_start < paragraph_start
                        || seed_end > paragraph_end
                    {
                        continue;
                    }
                    let neighbors = [
                        (
                            seed_start == paragraph_start,
                            index.checked_sub(1),
                            previous_start,
                            true,
                        ),
                        (
                            seed_end == paragraph_end,
                            (index + 1 < blocks.len()).then_some(index + 1),
                            block_start,
                            false,
                        ),
                    ];
                    for (allowed, neighbor_index, neighbor_start, trailing) in neighbors {
                        if !allowed {
                            continue;
                        }
                        let Some(neighbor) = neighbor_index.and_then(|i| blocks.get(i)) else {
                            continue;
                        };
                        if let Some((start, end, passage)) = paragraph_excerpt(
                            neighbor,
                            neighbor_start,
                            trailing,
                            document_id,
                            revision,
                            section,
                            &node.title,
                        ) {
                            let overlaps = |id, rev, existing_start, existing_end| {
                                id == document_id
                                    && rev == revision
                                    && start < existing_end
                                    && existing_start < end
                            };
                            if occupied
                                .iter()
                                .any(|&(id, rev, from, to)| overlaps(id, rev, from, to))
                            {
                                continue;
                            }
                            if shortlist.len() < MAX_SHORTLIST {
                                shortlist.push((
                                    rank,
                                    usize::from(!trailing),
                                    document_id,
                                    revision,
                                    start,
                                    end,
                                    passage,
                                ));
                            } else {
                                context.truncated = true;
                            }
                        }
                    }
                }
            }
            previous_start = start;
        }
    }
    shortlist.sort_by_key(|(rank, side, _, _, _, _, _)| (*rank, *side));
    let mut bytes = context
        .passages
        .iter()
        .map(|passage| passage.text.len())
        .sum::<usize>();
    let mut added = 0;
    for (_, _, document, revision, start, end, mut passage) in shortlist {
        if occupied
            .iter()
            .any(|&(id, rev, existing_start, existing_end)| {
                id == document && rev == revision && start < existing_end && existing_start < end
            })
        {
            continue;
        }
        if added == MAX_ADDED
            || context.passages.len() == MAX_PASSAGES
            || bytes.saturating_add(passage.text.len()) > MAX_TOTAL_BYTES
        {
            context.truncated = true;
            continue;
        }
        bytes += passage.text.len();
        passage.source_id = format!("S{}", context.passages.len() + 1);
        occupied.push((document, revision, start, end));
        context.passages.push(passage);
        added += 1;
    }
    Ok(added)
}

fn terms(text: &str) -> BTreeSet<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.chars().count() >= 2)
        .map(str::to_lowercase)
        // Common function words must not become matches merely because they
        // start a sentence. Ambiguous names such as An need explicit context.
        .filter(|word| {
            word.chars().count() != 2
                || !matches!(
                    word.as_str(),
                    "am" | "an"
                        | "as"
                        | "at"
                        | "be"
                        | "by"
                        | "do"
                        | "go"
                        | "he"
                        | "if"
                        | "in"
                        | "is"
                        | "it"
                        | "me"
                        | "my"
                        | "no"
                        | "of"
                        | "on"
                        | "or"
                        | "so"
                        | "to"
                        | "up"
                        | "us"
                        | "we"
                )
        })
        .filter(|word| {
            !matches!(
                word.as_str(),
                "the"
                    | "and"
                    | "what"
                    | "who"
                    | "where"
                    | "when"
                    | "how"
                    | "does"
                    | "with"
                    | "that"
                    | "this"
                    | "was"
                    | "are"
            )
        })
        .collect()
}

fn score(query: &BTreeSet<String>, text: &str) -> usize {
    let words = terms(text);
    query.intersection(&words).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_application::{DocumentSnapshot, DocumentVisibility};
    use parchmint_domain::{NodeId, Project, ProjectId};
    use parchmint_editor_api::EditorRevision;

    fn fixture() -> AuthoredProjectSnapshot {
        let mut project = Project::new(ProjectId::from_bytes([1; 16]));
        let entries = [
            (
                2,
                ProjectSection::Manuscript,
                "Chapter",
                "<p>Marin reaches the harbor.</p>",
            ),
            (
                3,
                ProjectSection::Lore,
                "Marin",
                "<p>Marin has a silver key.</p>",
            ),
            (
                4,
                ProjectSection::Research,
                "Harbor",
                "<p>A harbor protects ships.</p>",
            ),
        ];
        let mut documents = Vec::new();
        for (id, section, title, body) in entries {
            let document_id = DocumentId::from_bytes([id; 16]);
            project
                .nodes
                .try_insert_document(
                    NodeId::from_bytes([id; 16]),
                    document_id,
                    section.root_id(),
                    0,
                    title,
                )
                .unwrap();
            documents.push(DocumentSnapshot {
                document_id,
                body: body.into(),
                comments: Vec::new(),
                revision: EditorRevision::from(u64::from(id)),
                visibility: DocumentVisibility::Open,
            });
        }
        AuthoredProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents,
        }
    }

    #[test]
    fn adjacent_prose_is_bounded_nonrecursive_and_has_exact_distinct_anchors() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = "<p>  同じだ 🍃  </p><p>The bridge fell.</p><p>  同じだ 🍃  </p><p>A distant unrelated event.</p>".into();
        let mut context =
            retrieve(&snapshot, "bridge", &BTreeSet::new(), &BTreeSet::new()).unwrap();
        assert_eq!(context.passages.len(), 1);
        let original = context.passages.clone();
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &BTreeSet::new(), &mut context).unwrap(),
            2
        );
        assert_eq!(context.passages[0], original[0]);
        assert_eq!(context.passages[1].text, "同じだ 🍃");
        assert_eq!(context.passages[2].text, "同じだ 🍃");
        assert_ne!(context.passages[1].anchor, context.passages[2].anchor);
        assert!(
            context
                .passages
                .iter()
                .all(|passage| !passage.text.contains("distant unrelated"))
        );
        let mut load = CanonicalDocumentLoad::new(
            snapshot.documents[0].document_id,
            &snapshot.documents[0].body,
        );
        load.revision = snapshot.documents[0].revision;
        let session = EditorCoreSession::open(load).unwrap();
        let text = session
            .canonical_projection()
            .semantic()
            .plain_text()
            .chars()
            .collect::<Vec<_>>();
        for passage in &context.passages {
            let parts = passage
                .anchor
                .as_ref()
                .unwrap()
                .split(':')
                .collect::<Vec<_>>();
            let start: usize = parts[1].parse().unwrap();
            let end: usize = parts[2].parse().unwrap();
            assert_eq!(text[start..end].iter().collect::<String>(), passage.text);
        }
    }

    #[test]
    fn adjacent_prose_respects_exclusions_breaks_and_direct_text_matches() {
        let mut snapshot = fixture();
        let document = snapshot.documents[0].document_id;
        snapshot.documents[0].body = "<p>Earlier context.</p><hr data-kind=\"scene-break\"><p>The bridge fell.</p><hr data-kind=\"page-break\"><p>Later context.</p>".into();
        let mut context =
            retrieve(&snapshot, "bridge", &BTreeSet::new(), &BTreeSet::new()).unwrap();
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &BTreeSet::new(), &mut context).unwrap(),
            0
        );
        let excluded = BTreeSet::from([document]);
        let mut context = retrieve(&snapshot, "bridge", &BTreeSet::new(), &excluded).unwrap();
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &excluded, &mut context).unwrap(),
            0
        );
        assert!(context.passages.is_empty());
        snapshot
            .project
            .nodes
            .get_mut(NodeId::from_bytes([2; 16]))
            .unwrap()
            .title = "Bridge".into();
        snapshot.documents[0].body =
            "<p>First unrelated paragraph.</p><p>Second unrelated paragraph.</p>".into();
        let mut title_only =
            retrieve(&snapshot, "bridge", &BTreeSet::new(), &BTreeSet::new()).unwrap();
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &BTreeSet::new(), &mut title_only)
                .unwrap(),
            0
        );
        snapshot
            .project
            .nodes
            .get_mut(NodeId::from_bytes([2; 16]))
            .unwrap()
            .title = "Chapter".into();
        snapshot.documents[0].body =
            "<p>The bridge was old.</p><p>The bridge fell.</p><p>A witness hesitated.</p>".into();
        let mut existing =
            retrieve(&snapshot, "bridge", &BTreeSet::new(), &BTreeSet::new()).unwrap();
        assert_eq!(existing.passages.len(), 2);
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &BTreeSet::new(), &mut existing).unwrap(),
            1
        );
        assert_eq!(
            existing
                .passages
                .iter()
                .filter(|passage| passage.text == "The bridge was old.")
                .count(),
            1
        );
    }

    #[test]
    fn distant_end_of_a_long_paragraph_does_not_create_false_adjacency() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = format!(
            "<p>Earlier context.</p><p>The bridge fell. {}</p><p>A later paragraph.</p>",
            "filler ".repeat(300)
        );
        let mut context =
            retrieve(&snapshot, "bridge", &BTreeSet::new(), &BTreeSet::new()).unwrap();
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &BTreeSet::new(), &mut context).unwrap(),
            1
        );
        assert!(
            context
                .passages
                .iter()
                .any(|passage| passage.text == "Earlier context.")
        );
        assert!(
            !context
                .passages
                .iter()
                .any(|passage| passage.text == "A later paragraph.")
        );
    }

    #[test]
    fn adjacent_prose_does_not_displace_existing_evidence_and_uses_remaining_bytes() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = "<p>A predecessor longer than the available space.</p><p>The bridge fell.</p><p>Short</p>".into();
        let mut context =
            retrieve(&snapshot, "bridge", &BTreeSet::new(), &BTreeSet::new()).unwrap();
        let seed = context.passages[0].clone();
        let mut filler = seed.clone();
        filler.document_id = "not-an-authored-document".into();
        filler.anchor = None;
        filler.source_id = "S2".into();
        filler.text = "x".repeat(MAX_TOTAL_BYTES - seed.text.len() - 5);
        context.passages.push(filler.clone());
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &BTreeSet::new(), &mut context).unwrap(),
            1
        );
        assert_eq!(context.passages[0], seed);
        assert_eq!(context.passages[1], filler);
        assert_eq!(context.passages[2].text, "Short");
        assert!(context.truncated);
        let mut full = RetrievedContext {
            passages: vec![seed; MAX_PASSAGES],
            truncated: false,
        };
        let original = full.passages.clone();
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &BTreeSet::new(), &mut full).unwrap(),
            0
        );
        assert_eq!(full.passages, original);
        assert!(full.truncated);
    }

    #[test]
    fn uninspected_or_duplicate_seeds_do_not_claim_context_was_omitted() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = "<p>The bridge fell.</p>".into();
        let mut context =
            retrieve(&snapshot, "bridge", &BTreeSet::new(), &BTreeSet::new()).unwrap();
        let seed = context.passages[0].clone();
        context.passages.extend(std::iter::repeat_n(seed, 20));
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &BTreeSet::new(), &mut context).unwrap(),
            0
        );
        assert!(!context.truncated);
    }

    #[test]
    fn shared_neighbor_keeps_the_best_seed_rank() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = "<p>Bridge left.</p><p>Shared qualifier.</p><p>Bridge center.</p><p>Center qualifier.</p><p>Bridge right.</p><p>Right qualifier.</p>".into();
        let mut context =
            retrieve(&snapshot, "bridge", &BTreeSet::new(), &BTreeSet::new()).unwrap();
        context.passages.sort_by_key(|passage| {
            if passage.text.contains("center") {
                0
            } else if passage.text.contains("right") {
                1
            } else {
                2
            }
        });
        assert_eq!(
            append_adjacent_context(&snapshot, "bridge", &BTreeSet::new(), &mut context).unwrap(),
            2
        );
        assert!(
            context
                .passages
                .iter()
                .any(|passage| passage.text == "Shared qualifier.")
        );
        assert!(
            context
                .passages
                .iter()
                .any(|passage| passage.text == "Center qualifier.")
        );
        assert!(
            !context
                .passages
                .iter()
                .any(|passage| passage.text == "Right qualifier.")
        );
    }

    #[test]
    fn continuity_match_reuses_only_unique_current_evidence() {
        use parchmint_assistant_api::{CheckCategory, CheckFinding, CheckReport, CheckScope};
        use parchmint_domain::{CheckDecisionId, ContinuityDecision};

        let mut snapshot = fixture();
        let chapter = DocumentId::from_bytes([2; 16]);
        let context = retrieve(
            &snapshot,
            "Marin",
            &BTreeSet::from([chapter]),
            &BTreeSet::new(),
        )
        .unwrap()
        .passages;
        let first = context
            .iter()
            .find(|p| p.document_id == encode_stable_id(chapter.as_bytes()))
            .unwrap()
            .clone();
        let second = context
            .iter()
            .find(|p| p.section == ContextSection::Lore)
            .unwrap()
            .clone();
        let scope = CheckScope {
            target: encode_stable_id(NodeId::from_bytes([2; 16]).as_bytes()),
            evidence_scope: None,
            excluded: vec![],
            selection_only: false,
        };
        let finding = CheckFinding {
            category: CheckCategory::Other,
            explanation: "Possible conflict".into(),
            uncertainty: None,
            evidence: [first, second],
            matched_decision_id: None,
        };
        let mut record =
            continuity_match::capture(&finding, &scope, &context, ContinuityDecision::Intentional)
                .unwrap();
        record.id = CheckDecisionId::from_bytes([42; 16]);
        snapshot
            .project
            .continuity_decisions
            .insert(record.id, record.clone());
        let mut report = CheckReport {
            summary: String::new(),
            findings: vec![finding],
            scope: Some(scope),
        };

        continuity_match::attach_matches(&mut report, &context, &snapshot, &[]);
        assert_eq!(
            report.findings[0].matched_decision_id,
            Some(encode_stable_id(record.id.as_bytes()))
        );

        // A revision change elsewhere does not affect either source.
        snapshot.documents[2].revision = EditorRevision::from(99);
        continuity_match::attach_matches(&mut report, &context, &snapshot, &[]);
        assert!(report.findings[0].matched_decision_id.is_some());

        // The same captured excerpt at a changed revision must not be trusted.
        snapshot.documents[0].revision = EditorRevision::from(99);
        continuity_match::attach_matches(&mut report, &context, &snapshot, &[]);
        assert!(report.findings[0].matched_decision_id.is_none());
        snapshot.documents[0].revision = EditorRevision::from(2);

        // Repetition elsewhere in the complete document makes the evidence ambiguous.
        snapshot.documents[0].body =
            "<p>Marin reaches the harbor.</p><p>Marin reaches the harbor.</p>".into();
        continuity_match::attach_matches(&mut report, &context, &snapshot, &[]);
        assert!(report.findings[0].matched_decision_id.is_none());
        snapshot.documents[0].body = "<p>Marin reaches the harbor.</p>".into();

        // Ambiguous prior decisions and excluded evidence never inherit a status.
        let mut duplicate = record.clone();
        duplicate.id = CheckDecisionId::from_bytes([43; 16]);
        snapshot
            .project
            .continuity_decisions
            .insert(duplicate.id, duplicate);
        continuity_match::attach_matches(&mut report, &context, &snapshot, &[]);
        assert!(report.findings[0].matched_decision_id.is_none());
        snapshot
            .project
            .continuity_decisions
            .remove(&CheckDecisionId::from_bytes([43; 16]));
        report
            .scope
            .as_mut()
            .unwrap()
            .excluded
            .push(encode_stable_id(NodeId::from_bytes([3; 16]).as_bytes()));
        continuity_match::attach_matches(&mut report, &context, &snapshot, &[]);
        assert!(report.findings[0].matched_decision_id.is_none());
    }

    #[test]
    fn continuity_match_survives_unrelated_edits_in_the_same_document() {
        use parchmint_assistant_api::{CheckCategory, CheckFinding, CheckReport, CheckScope};
        use parchmint_domain::{CheckDecisionId, ContinuityDecision};

        let mut snapshot = fixture();
        let chapter = DocumentId::from_bytes([2; 16]);
        let scope = CheckScope {
            target: encode_stable_id(NodeId::from_bytes([2; 16]).as_bytes()),
            evidence_scope: None,
            excluded: vec![],
            selection_only: false,
        };
        let make_report = |context: &[ContextPassage]| CheckReport {
            summary: String::new(),
            findings: vec![CheckFinding {
                category: CheckCategory::Other,
                explanation: "Possible conflict".into(),
                uncertainty: None,
                evidence: [
                    context
                        .iter()
                        .find(|p| {
                            p.document_id == encode_stable_id(chapter.as_bytes())
                                && p.text.contains("Marin reaches")
                        })
                        .unwrap()
                        .clone(),
                    context
                        .iter()
                        .find(|p| p.section == ContextSection::Lore)
                        .unwrap()
                        .clone(),
                ],
                matched_decision_id: None,
            }],
            scope: Some(scope.clone()),
        };
        let context = retrieve(
            &snapshot,
            "Marin",
            &BTreeSet::from([chapter]),
            &BTreeSet::new(),
        )
        .unwrap()
        .passages;
        let report = make_report(&context);
        let mut record = continuity_match::capture(
            &report.findings[0],
            &scope,
            &context,
            ContinuityDecision::Intentional,
        )
        .unwrap();
        record.id = CheckDecisionId::from_bytes([44; 16]);
        snapshot
            .project
            .continuity_decisions
            .insert(record.id, record.clone());
        snapshot.documents[0].body =
            "<p>A separate scene.</p><p>Marin reaches the harbor.</p>".into();
        snapshot.documents[0].revision = EditorRevision::from(20);
        let context = retrieve(
            &snapshot,
            "Marin",
            &BTreeSet::from([chapter]),
            &BTreeSet::new(),
        )
        .unwrap()
        .passages;
        let mut report = make_report(&context);
        continuity_match::attach_matches(&mut report, &context, &snapshot, &[]);
        assert_eq!(
            report.findings[0].matched_decision_id,
            Some(encode_stable_id(record.id.as_bytes()))
        );
    }

    #[test]
    fn edit_proposal_target_requires_current_unformatted_target_paragraph() {
        use parchmint_assistant_api::{CheckCategory, CheckFinding, CheckScope};
        let mut snapshot = fixture();
        let chapter = DocumentId::from_bytes([2; 16]);
        let context = retrieve(
            &snapshot,
            "Marin",
            &BTreeSet::from([chapter]),
            &BTreeSet::new(),
        )
        .unwrap()
        .passages;
        let finding = CheckFinding {
            category: CheckCategory::Other,
            explanation: "Possible conflict".into(),
            uncertainty: None,
            evidence: [
                context
                    .iter()
                    .find(|p| p.section == ContextSection::Manuscript)
                    .unwrap()
                    .clone(),
                context
                    .iter()
                    .find(|p| p.section == ContextSection::Lore)
                    .unwrap()
                    .clone(),
            ],
            matched_decision_id: None,
        };
        let scope = CheckScope {
            target: encode_stable_id(NodeId::from_bytes([2; 16]).as_bytes()),
            evidence_scope: None,
            excluded: vec![],
            selection_only: false,
        };
        let target = edit_proposal::target(&finding, &scope, &context, 0, &snapshot, &[]).unwrap();
        assert_eq!(target.document, chapter);
        assert_eq!(
            (target.start, target.end),
            (0, "Marin reaches the harbor.".chars().count() as u64)
        );
        assert!(edit_proposal::target(&finding, &scope, &context, 1, &snapshot, &[]).is_none());
        snapshot.documents[0].revision = EditorRevision::from(20);
        assert!(edit_proposal::target(&finding, &scope, &context, 0, &snapshot, &[]).is_none());
        snapshot.documents[0].revision = EditorRevision::from(2);
        snapshot.documents[0].body = "<p><strong>Marin reaches the harbor.</strong></p>".into();
        assert!(edit_proposal::target(&finding, &scope, &context, 0, &snapshot, &[]).is_none());
        snapshot.documents[0].body = "<p>Marin reaches the harbor.</p>".into();
        snapshot.documents[1].revision = EditorRevision::from(30);
        assert!(edit_proposal::target(&finding, &scope, &context, 0, &snapshot, &[]).is_none());
        snapshot.documents[1].revision = EditorRevision::from(3);
        let mut excluded = scope.clone();
        excluded
            .excluded
            .push(encode_stable_id(NodeId::from_bytes([3; 16]).as_bytes()));
        assert!(edit_proposal::target(&finding, &excluded, &context, 0, &snapshot, &[]).is_none());
        snapshot.documents[0].body = "<p>🌊 Marin reaches the harbor.</p>".into();
        let unicode_context = retrieve(
            &snapshot,
            "Marin",
            &BTreeSet::from([chapter]),
            &BTreeSet::new(),
        )
        .unwrap()
        .passages;
        let mut unicode = finding.clone();
        unicode.evidence[0] = unicode_context
            .iter()
            .find(|p| p.section == ContextSection::Manuscript)
            .unwrap()
            .clone();
        unicode.evidence[0].text = "Marin".into();
        unicode.evidence[1] = unicode_context
            .iter()
            .find(|p| p.section == ContextSection::Lore)
            .unwrap()
            .clone();
        let target =
            edit_proposal::target(&unicode, &scope, &unicode_context, 0, &snapshot, &[]).unwrap();
        assert_eq!((target.start, target.end), (2, 7));
        snapshot.documents[0].body =
            "<p>🌊 Marin reaches the harbor.</p><p>🌊 Marin reaches the harbor.</p>".into();
        assert!(
            edit_proposal::target(&unicode, &scope, &unicode_context, 0, &snapshot, &[]).is_none()
        );
    }

    #[test]
    fn edit_proposal_can_use_an_original_as_noneditable_supporting_evidence() {
        use parchmint_assistant_api::{CheckCategory, CheckFinding, CheckScope};
        let mut snapshot = fixture();
        let chapter = DocumentId::from_bytes([2; 16]);
        let research = DocumentId::from_bytes([4; 16]);
        let source_id = ResearchSourceId::from_bytes([9; 16]);
        snapshot.project.research_originals.insert(
            source_id,
            parchmint_domain::ResearchOriginal {
                document: research,
                filename: "archive.txt".into(),
                media_type: "text/plain".into(),
                source_url: None,
                retrieved_at_unix_seconds: 42,
                byte_length: 25,
                sha256: [7; 32],
            },
        );
        let originals = vec![ExtractedOriginal {
            source_id,
            text: "Saffron trade began here.".into(),
        }];
        let context = retrieve_with_originals(
            &snapshot,
            "saffron",
            &BTreeSet::from([chapter]),
            &BTreeSet::new(),
            &originals,
        )
        .unwrap()
        .passages;
        let finding = CheckFinding {
            category: CheckCategory::Other,
            explanation: "Review".into(),
            uncertainty: None,
            evidence: [
                context
                    .iter()
                    .find(|p| p.section == ContextSection::Manuscript)
                    .unwrap()
                    .clone(),
                context
                    .iter()
                    .find(|p| p.original.is_some())
                    .unwrap()
                    .clone(),
            ],
            matched_decision_id: None,
        };
        let scope = CheckScope {
            target: encode_stable_id(NodeId::from_bytes([2; 16]).as_bytes()),
            evidence_scope: None,
            excluded: vec![],
            selection_only: false,
        };
        assert!(
            edit_proposal::target(&finding, &scope, &context, 0, &snapshot, &originals).is_some()
        );
        assert!(
            edit_proposal::target(&finding, &scope, &context, 1, &snapshot, &originals).is_none()
        );
        snapshot
            .project
            .research_originals
            .get_mut(&source_id)
            .unwrap()
            .sha256[0] ^= 1;
        assert!(
            edit_proposal::target(&finding, &scope, &context, 0, &snapshot, &originals).is_none()
        );
    }

    #[test]
    fn retrieves_live_relevant_passages_and_obeys_exclusions() {
        let snapshot = fixture();
        let chapter = DocumentId::from_bytes([2; 16]);
        let research = DocumentId::from_bytes([4; 16]);
        let result = retrieve(
            &snapshot,
            "What does Marin carry?",
            &BTreeSet::from([chapter]),
            &BTreeSet::from([research]),
        )
        .unwrap();
        assert_eq!(result.passages.len(), 2);
        assert_eq!(result.passages[0].section, ContextSection::Manuscript);
        assert_eq!(result.passages[1].section, ContextSection::Lore);
        assert_eq!(result.passages[1].document_revision, 3);
        assert!(result.passages[1].text.contains("silver key"));
        assert!(result.passages.iter().all(|p| p.anchor.is_some()));
    }

    #[test]
    fn retrieves_short_names_without_matching_common_lowercase_words() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = "<p>Li reaches the harbor.</p>".into();
        snapshot.documents[1].body = "<p>Li has a silver key.</p>".into();
        snapshot.documents[2].body = "<p>A ship is in port.</p>".into();
        let result = retrieve(
            &snapshot,
            "Where is Li?",
            &BTreeSet::new(),
            &BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(result.passages.len(), 2);
        assert!(
            result
                .passages
                .iter()
                .all(|passage| passage.text.contains("Li"))
        );

        for query in ["where is li?", "IN WHICH ROOM IS LI?"] {
            let result = retrieve(&snapshot, query, &BTreeSet::new(), &BTreeSet::new()).unwrap();
            assert_eq!(result.passages.len(), 2);
        }
        snapshot.documents[2].body = (0..60)
            .map(|index| format!("<p>Is the gate locked? Record {index}.</p>"))
            .collect();
        let result =
            retrieve(&snapshot, "Is Li safe?", &BTreeSet::new(), &BTreeSet::new()).unwrap();
        assert_eq!(result.passages.len(), 2);
        assert!(
            result
                .passages
                .iter()
                .all(|passage| passage.text.contains("Li"))
        );
        assert!(
            retrieve(&snapshot, "Is in", &BTreeSet::new(), &BTreeSet::new())
                .unwrap()
                .passages
                .is_empty()
        );
    }

    #[test]
    fn repeated_passages_do_not_crowd_out_another_documents_qualifier() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = "<p>The keeper gave Neri the silver token.</p>".into();
        snapshot.documents[1].body = (0..250)
            .map(|index| format!("<p>The keeper recorded silver token details {index}.</p>"))
            .collect();
        snapshot.documents[2].body = "<p>No silver token transfer is recorded.</p>".into();
        let result = retrieve(
            &snapshot,
            "Which keeper has the silver token?",
            &BTreeSet::from([snapshot.documents[0].document_id]),
            &BTreeSet::new(),
        )
        .unwrap();
        assert!(result.truncated);
        assert!(
            result
                .passages
                .iter()
                .any(|passage| passage.text.contains("No silver token transfer"))
        );
        assert!(
            result
                .passages
                .iter()
                .filter(|passage| passage.text.contains("keeper recorded"))
                .count()
                > 1
        );
        assert!(result.passages.len() <= MAX_PASSAGES);
        assert!(
            result
                .passages
                .iter()
                .map(|passage| passage.text.len())
                .sum::<usize>()
                <= MAX_TOTAL_BYTES
        );

        let excluded = BTreeSet::from([snapshot.documents[2].document_id]);
        let scoped = retrieve(
            &snapshot,
            "keeper silver token",
            &BTreeSet::new(),
            &excluded,
        )
        .unwrap();
        assert!(
            scoped
                .passages
                .iter()
                .all(|passage| !passage.text.contains("No silver token transfer"))
        );
    }

    #[test]
    fn diversity_does_not_promote_expansion_only_over_direct_matches() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = "<p>An unrelated scene.</p>".into();
        snapshot.documents[1].body = (0..100)
            .map(|index| format!("<p>Silver evidence {index}.</p>"))
            .collect();
        snapshot.documents[2].body = "<p>Saffron evidence.</p>".into();
        let result = retrieve_with_originals_and_expansion(
            &snapshot,
            "silver",
            "saffron",
            &BTreeSet::new(),
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
        assert_eq!(result.passages.len(), MAX_PASSAGES);
        assert!(
            result
                .passages
                .iter()
                .all(|passage| passage.text.contains("Silver"))
        );
    }

    #[test]
    fn refuses_a_snapshot_missing_a_live_body() {
        let mut snapshot = fixture();
        snapshot.documents.pop();
        assert_eq!(
            retrieve(&snapshot, "harbor", &BTreeSet::new(), &BTreeSet::new()),
            Err(RetrievalError::IncompleteSnapshot)
        );
    }

    #[test]
    fn authored_passage_anchors_point_to_exact_unicode_text_at_the_captured_revision() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = format!(
            "<p>  Première ligne  </p><p>{}fin  </p>",
            "🌊".repeat(MAX_SCALARS_PER_PASSAGE + 3)
        );
        let document = &snapshot.documents[0];
        let mut load = CanonicalDocumentLoad::new(document.document_id, &document.body);
        load.revision = document.revision;
        let session = EditorCoreSession::open(load).unwrap();
        let plain = session.canonical_projection().semantic().plain_text();
        let result = retrieve(
            &snapshot,
            "irrelevant",
            &BTreeSet::from([document.document_id]),
            &BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(result.passages.len(), 3);
        for passage in result.passages {
            let anchor = passage.anchor.unwrap();
            let parts = anchor.split(':').collect::<Vec<_>>();
            assert_eq!(parts[0], "text");
            let start = parts[1].parse::<usize>().unwrap();
            let end = parts[2].parse::<usize>().unwrap();
            assert_eq!(
                plain
                    .chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>(),
                passage.text
            );
            assert_eq!(passage.document_revision, document.revision.value());
        }
    }

    #[test]
    fn large_target_keeps_room_for_related_lore() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = (0..240)
            .map(|index| format!("<p>Marin reaches harbor scene {index}.</p>"))
            .collect();
        let result = retrieve(
            &snapshot,
            "Marin",
            &BTreeSet::from([DocumentId::from_bytes([2; 16])]),
            &BTreeSet::new(),
        )
        .unwrap();
        assert!(result.truncated);
        assert!(
            result
                .passages
                .iter()
                .any(|p| p.section == ContextSection::Manuscript)
        );
        assert!(
            result
                .passages
                .iter()
                .any(|p| p.section == ContextSection::Lore)
        );
    }

    #[test]
    fn target_expansion_cannot_crowd_out_a_passage_matching_the_question() {
        let mut snapshot = fixture();
        snapshot.documents[0].body = format!(
            "{}<p>The silver bell belongs to Mara.</p>",
            "<p>Filler paragraph with ordinary details.</p>".repeat(80)
        );
        let result = retrieve_with_originals_and_expansion(
            &snapshot,
            "Who owns the silver bell?",
            "filler paragraph ordinary details",
            &BTreeSet::from([DocumentId::from_bytes([2; 16])]),
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
        assert!(result.truncated);
        assert_eq!(result.passages[0].text, "The silver bell belongs to Mara.");
    }

    #[test]
    fn identity_reserve_is_bounded_preserves_direct_matches_and_respects_exclusions() {
        let mut snapshot = fixture();
        let chapter = DocumentId::from_bytes([2; 16]);
        snapshot.documents[0].body = "<p>The keeper waited.</p>".into();
        snapshot.documents[1].body = (0..80)
            .map(|index| format!("<p>Keeper ledger entry {index}.</p>"))
            .collect();
        for id in 60..63 {
            let document = DocumentId::from_bytes([id; 16]);
            snapshot
                .project
                .nodes
                .try_insert_document(
                    NodeId::from_bytes([id; 16]),
                    document,
                    ProjectSection::Lore.root_id(),
                    0,
                    format!("Source {id}"),
                )
                .unwrap();
            snapshot.documents.push(DocumentSnapshot {
                document_id: document,
                body: format!("<p>Zara carries the sealed parcel {id}.</p>"),
                comments: Vec::new(),
                revision: EditorRevision::from(1),
                visibility: DocumentVisibility::Closed,
            });
        }
        let pinned = BTreeSet::from([chapter]);
        let result = retrieve_with_identity_expansion(
            &snapshot,
            "keeper",
            "",
            "Zara",
            &pinned,
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
        assert!(result.truncated);
        assert_eq!(
            result
                .passages
                .iter()
                .filter(|p| p.text.contains("Zara"))
                .count(),
            2
        );
        assert!(
            result
                .passages
                .iter()
                .any(|p| p.text.contains("Keeper ledger entry 0"))
        );
        assert!(
            result
                .passages
                .iter()
                .any(|p| p.text.contains("Keeper ledger entry 1"))
        );
        let excluded = BTreeSet::from([DocumentId::from_bytes([60; 16])]);
        let scoped = retrieve_with_identity_expansion(
            &snapshot,
            "keeper",
            "",
            "Zara",
            &pinned,
            &excluded,
            &[],
        )
        .unwrap();
        assert!(scoped.passages.iter().all(
            |p| p.document_id != encode_stable_id(DocumentId::from_bytes([60; 16]).as_bytes())
        ));
        assert!(
            scoped
                .passages
                .iter()
                .filter(|p| p.text.contains("Zara"))
                .count()
                <= 2
        );
        let baseline = retrieve_with_originals_and_expansion(
            &snapshot,
            "keeper",
            "",
            &pinned,
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
        let no_candidates = retrieve_with_identity_expansion(
            &snapshot,
            "keeper",
            "",
            "UnmatchedName",
            &pinned,
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
        assert_eq!(baseline, no_candidates);
    }

    #[test]
    fn identity_candidates_fill_unused_capacity_after_pruning() {
        let mut snapshot = fixture();
        let chapter = DocumentId::from_bytes([2; 16]);
        snapshot.documents[0].body = "<p>The keeper waited.</p>".into();
        snapshot.documents[1].body = (0..220)
            .map(|index| format!("<p>Zara archive item {index}.</p>"))
            .collect();
        let result = retrieve_with_identity_expansion(
            &snapshot,
            "keeper",
            "",
            "Zara",
            &BTreeSet::from([chapter]),
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
        assert!(result.truncated);
        assert!(
            result
                .passages
                .iter()
                .filter(|p| p.text.contains("Zara"))
                .count()
                >= 40
        );
    }

    #[test]
    fn original_text_obeys_scope_and_carries_original_provenance() {
        let mut snapshot = fixture();
        let document = DocumentId::from_bytes([4; 16]);
        let source_id = ResearchSourceId::from_bytes([9; 16]);
        snapshot.project.research_originals.insert(
            source_id,
            parchmint_domain::ResearchOriginal {
                document,
                filename: "archive.txt".into(),
                media_type: "text/plain".into(),
                source_url: Some("https://example.org/archive.txt".into()),
                retrieved_at_unix_seconds: 42,
                byte_length: 22,
                sha256: [7; 32],
            },
        );
        let originals = vec![ExtractedOriginal {
            source_id,
            text: "Saffron trade began here.\nShips arrived in spring.".into(),
        }];
        let selected = retrieve_with_originals(
            &snapshot,
            "saffron",
            &BTreeSet::new(),
            &BTreeSet::new(),
            &originals,
        )
        .unwrap();
        let excerpt = selected
            .passages
            .iter()
            .find(|p| p.original.is_some())
            .unwrap();
        assert_eq!(excerpt.text, "Saffron trade began here.");
        let origin = excerpt.original.as_ref().unwrap();
        assert_eq!(origin.source_id, encode_stable_id(source_id.as_bytes()));
        assert_eq!(origin.sha256, [7; 32]);
        assert_eq!(origin.segment, 1);
        assert_eq!(origin.retrieved_at_unix_seconds, 42);
        let excluded = retrieve_with_originals(
            &snapshot,
            "saffron",
            &BTreeSet::from([document]),
            &BTreeSet::from([document]),
            &originals,
        )
        .unwrap();
        assert!(excluded.passages.iter().all(|p| p.original.is_none()));
        let pinned = retrieve_with_originals(
            &snapshot,
            "unrelated",
            &BTreeSet::from([document]),
            &BTreeSet::new(),
            &originals,
        )
        .unwrap();
        assert!(pinned.passages.iter().any(|p| p.original.is_some()));
        snapshot.project.research_originals.clear();
        assert_eq!(
            retrieve_with_originals(
                &snapshot,
                "saffron",
                &BTreeSet::new(),
                &BTreeSet::new(),
                &originals
            ),
            Err(RetrievalError::InvalidOriginal)
        );
    }

    #[test]
    fn original_segment_work_limit_reports_omitted_content() {
        let mut snapshot = fixture();
        let source_id = ResearchSourceId::from_bytes([10; 16]);
        snapshot.project.research_originals.insert(
            source_id,
            parchmint_domain::ResearchOriginal {
                document: DocumentId::from_bytes([4; 16]),
                filename: "long.txt".into(),
                media_type: "text/plain".into(),
                source_url: None,
                retrieved_at_unix_seconds: 1,
                byte_length: 1,
                sha256: [2; 32],
            },
        );
        let mut text = "ordinary line\n".repeat(MAX_ORIGINAL_SEGMENTS);
        text.push_str("rare final clue");
        let result = retrieve_with_originals(
            &snapshot,
            "rare",
            &BTreeSet::new(),
            &BTreeSet::new(),
            &[ExtractedOriginal { source_id, text }],
        )
        .unwrap();
        assert!(result.truncated);
        assert!(
            result
                .passages
                .iter()
                .all(|passage| !passage.text.contains("rare final clue"))
        );
    }
}
