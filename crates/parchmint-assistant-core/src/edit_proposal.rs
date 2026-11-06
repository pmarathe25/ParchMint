//! Validation of a model-suggested single-paragraph replacement.
//!
//! The application owns the destination, source revision and range. Provider
//! output is never allowed to select a document or issue an editor command.

use parchmint_application::AuthoredProjectSnapshot;
use parchmint_assistant_api::{CheckFinding, CheckScope, ContextPassage};
use parchmint_domain::{ContinuityDecision, DocumentId, NodeKind};
use parchmint_editor_api::{CanonicalDocumentLoad, SemanticBlockKind};
use parchmint_editor_core::EditorCoreSession;
use serde::Deserialize;

use crate::{ExtractedOriginal, continuity_match};

const MAX_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_REPLACEMENT_SCALARS: usize = 1_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidEditProposal {
    Malformed,
    InvalidReplacement,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireProposal {
    replacement: String,
}

/// Application-owned target; no field is accepted from the provider response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditProposalTarget {
    pub document: DocumentId,
    pub revision: u64,
    pub start: u64,
    pub end: u64,
    pub original: String,
}

/// Captures one exact authored paragraph passage only after both evidence
/// sources and the original Check scope have been revalidated locally.
pub fn target(
    finding: &CheckFinding,
    scope: &CheckScope,
    context: &[ContextPassage],
    selected: usize,
    snapshot: &AuthoredProjectSnapshot,
    originals: &[ExtractedOriginal],
) -> Option<EditProposalTarget> {
    let candidate =
        continuity_match::capture(finding, scope, context, ContinuityDecision::Intentional)?;
    if !continuity_match::evidence_is_current(&candidate, snapshot, originals) {
        return None;
    }
    let evidence = candidate.evidence.get(selected)?;
    if evidence.original.is_some()
        || evidence
            .quote
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}'))
    {
        return None;
    }
    let mut node = snapshot.project.nodes.iter().find_map(|(id, node)| {
        (node.kind == NodeKind::Document(evidence.document)).then_some(*id)
    })?;
    loop {
        if node == candidate.target {
            break;
        }
        node = snapshot.project.nodes.parent(node)?;
    }
    let mut parts = evidence.anchor.as_deref()?.split(':');
    if parts.next()? != "text" {
        return None;
    }
    let start = parts.next()?.parse::<u64>().ok()?;
    let end = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() || end <= start {
        return None;
    }
    let doc = snapshot
        .documents
        .iter()
        .find(|doc| doc.document_id == evidence.document)?;
    let mut load = CanonicalDocumentLoad::new(doc.document_id, &doc.body);
    load.revision = doc.revision;
    let session = EditorCoreSession::open(load).ok()?;
    let projection = session.canonical_projection();
    let mut block_start = 0_u64;
    let mut eligible = false;
    for block in projection.semantic().blocks() {
        let block_end = block_start.checked_add(u64::try_from(block.scalar_len()).ok()?)?;
        if start >= block_start && end <= block_end {
            eligible = block.kind() == SemanticBlockKind::Paragraph && block.marks().is_empty();
            break;
        }
        block_start = block_end.checked_add(1)?;
    }
    eligible.then(|| EditProposalTarget {
        document: evidence.document,
        revision: evidence.revision,
        start,
        end,
        original: evidence.quote.clone(),
    })
}

/// Accept only a bounded, nonempty single-paragraph replacement which differs
/// from the captured source. No unstructured response is actionable.
pub fn validate(response: &str, original: &str) -> Result<String, InvalidEditProposal> {
    if response.len() > MAX_RESPONSE_BYTES {
        return Err(InvalidEditProposal::Malformed);
    }
    let wire: WireProposal =
        serde_json::from_str(response).map_err(|_| InvalidEditProposal::Malformed)?;
    let replacement = wire.replacement;
    validate_replacement(&replacement, original)?;
    Ok(replacement)
}

pub fn validate_replacement(replacement: &str, original: &str) -> Result<(), InvalidEditProposal> {
    if replacement.trim().is_empty()
        || replacement == original
        || replacement.chars().count() > MAX_REPLACEMENT_SCALARS
        || replacement
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}'))
    {
        return Err(InvalidEditProposal::InvalidReplacement);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_unicode_replacement_without_accepting_provider_targets() {
        assert_eq!(
            validate(r#"{"replacement":"Mara tient la clé."}"#, "Mara waits.").unwrap(),
            "Mara tient la clé."
        );
        assert_eq!(
            validate(r#"{"replacement":"A","document_id":"another"}"#, "B"),
            Err(InvalidEditProposal::Malformed)
        );
        assert_eq!(
            validate(r#"{"replacement":"A","replacement":"B"}"#, "C"),
            Err(InvalidEditProposal::Malformed)
        );
        assert_eq!(
            validate("not JSON", "B"),
            Err(InvalidEditProposal::Malformed)
        );
    }

    #[test]
    fn rejects_empty_unchanged_structural_and_oversized_replacements() {
        for value in [
            "",
            "  ",
            "original",
            "line\nnext",
            "line\u{2028}next",
            "tab\tvalue",
        ] {
            let response = serde_json::json!({"replacement":value}).to_string();
            assert_eq!(
                validate(&response, "original"),
                Err(InvalidEditProposal::InvalidReplacement)
            );
        }
        let response =
            serde_json::json!({"replacement":"é".repeat(MAX_REPLACEMENT_SCALARS + 1)}).to_string();
        assert_eq!(
            validate(&response, "original"),
            Err(InvalidEditProposal::InvalidReplacement)
        );
        assert_eq!(
            validate(&"x".repeat(MAX_RESPONSE_BYTES + 1), "original"),
            Err(InvalidEditProposal::Malformed)
        );
    }
}
