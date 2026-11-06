//! Locally capture and conservatively match reviewed continuity evidence.

use std::collections::BTreeMap;

use parchmint_application::AuthoredProjectSnapshot;
use parchmint_assistant_api::{
    CheckCategory, CheckFinding, CheckReport, CheckScope, ContextPassage, ProjectSection,
};
use parchmint_domain::{
    CheckDecisionId, ContinuityCategory, ContinuityDecision, ContinuityDecisionRecord,
    ContinuityEvidence, ContinuityOriginal, DocumentId, NodeId, NodeKind, ResearchSourceId,
    decode_stable_id, encode_stable_id,
};
use parchmint_editor_api::CanonicalDocumentLoad;
use parchmint_editor_core::EditorCoreSession;

use crate::ExtractedOriginal;

/// Reconstructs evidence from the captured excerpts, not from model-supplied
/// document IDs, offsets, or surrounding text.
pub fn capture(
    finding: &CheckFinding,
    scope: &CheckScope,
    context: &[ContextPassage],
    decision: ContinuityDecision,
) -> Option<ContinuityDecisionRecord> {
    let target = NodeId::from_bytes(decode_stable_id(&scope.target)?);
    let evidence_scope = match scope.evidence_scope.as_deref() {
        Some(id) => Some(NodeId::from_bytes(decode_stable_id(id)?)),
        None => None,
    };
    let excluded = scope
        .excluded
        .iter()
        .map(|id| decode_stable_id(id).map(NodeId::from_bytes))
        .collect::<Option<Vec<_>>>()?;
    let capture_evidence = |item: &ContextPassage| -> Option<ContinuityEvidence> {
        if item.section == ProjectSection::Notes || item.text.trim().is_empty() {
            return None;
        }
        let mut sources = context
            .iter()
            .filter(|source| source.source_id == item.source_id);
        let source = sources.next()?;
        if sources.next().is_some()
            || source.document_id != item.document_id
            || source.document_revision != item.document_revision
            || source.section != item.section
            || source.original != item.original
        {
            return None;
        }
        let start = source.text.find(&item.text)?;
        if source.text[start..]
            .char_indices()
            .skip(1)
            .any(|(index, _)| source.text[start + index..].starts_with(&item.text))
        {
            return None;
        }
        let before = source.text[..start]
            .chars()
            .rev()
            .take(100)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let after = source.text[start + item.text.len()..]
            .chars()
            .take(100)
            .collect();
        let anchor = source.anchor.as_deref().and_then(|anchor| {
            let mut parts = anchor.split(':');
            if parts.next()? != "text" {
                return None;
            }
            let old_start = parts.next()?.parse::<u64>().ok()?;
            let old_end = parts.next()?.parse::<u64>().ok()?;
            if parts.next().is_some()
                || old_end.checked_sub(old_start)?
                    != u64::try_from(source.text.chars().count()).ok()?
            {
                return None;
            }
            let begin =
                old_start.checked_add(u64::try_from(source.text[..start].chars().count()).ok()?)?;
            let end = begin.checked_add(u64::try_from(item.text.chars().count()).ok()?)?;
            Some(format!("text:{begin}:{end}"))
        });
        let original = match &source.original {
            Some(original) => Some(ContinuityOriginal {
                source_id: ResearchSourceId::from_bytes(decode_stable_id(&original.source_id)?),
                sha256: original.sha256,
            }),
            None => None,
        };
        Some(ContinuityEvidence {
            document: DocumentId::from_bytes(decode_stable_id(&item.document_id)?),
            original,
            quote: item.text.clone(),
            before,
            after,
            revision: item.document_revision,
            anchor,
        })
    };
    let category = match finding.category {
        CheckCategory::IdentityAppearance => ContinuityCategory::IdentityAppearance,
        CheckCategory::TimeTravel => ContinuityCategory::TimeTravel,
        CheckCategory::ObjectsPossession => ContinuityCategory::ObjectsPossession,
        CheckCategory::Rules => ContinuityCategory::Rules,
        CheckCategory::Relationships => ContinuityCategory::Relationships,
        CheckCategory::Knowledge => ContinuityCategory::Knowledge,
        CheckCategory::Other => ContinuityCategory::Other,
    };
    let record = ContinuityDecisionRecord {
        id: CheckDecisionId::default(),
        category,
        explanation: finding.explanation.clone(),
        decision,
        rationale: None,
        evidence: [
            capture_evidence(&finding.evidence[0])?,
            capture_evidence(&finding.evidence[1])?,
        ],
        target,
        evidence_scope,
        excluded,
        selection_only: scope.selection_only,
    };
    record.validate().ok()?;
    Some(record)
}

/// Revalidates captured evidence and scope against a complete project snapshot.
/// The caller supplies verified extracted originals, never provider assertions.
pub fn evidence_is_current(
    candidate: &ContinuityDecisionRecord,
    snapshot: &AuthoredProjectSnapshot,
    originals: &[ExtractedOriginal],
) -> bool {
    evidence_is_current_with_cache(candidate, snapshot, originals, &mut BTreeMap::new())
}

fn evidence_is_current_with_cache(
    candidate: &ContinuityDecisionRecord,
    snapshot: &AuthoredProjectSnapshot,
    originals: &[ExtractedOriginal],
    documents: &mut BTreeMap<DocumentId, Option<String>>,
) -> bool {
    let project = &snapshot.project;
    if project.nodes.get(candidate.target).is_none()
        || candidate
            .evidence_scope
            .is_some_and(|id| project.nodes.get(id).is_none())
        || candidate
            .excluded
            .iter()
            .any(|id| project.nodes.get(*id).is_none())
    {
        return false;
    }
    let node_for = |document| {
        project
            .nodes
            .iter()
            .find_map(|(id, node)| (node.kind == NodeKind::Document(document)).then_some(*id))
    };
    let within = |mut node, root| loop {
        if node == root {
            break true;
        }
        let Some(parent) = project.nodes.parent(node) else {
            break false;
        };
        node = parent;
    };
    let Some(first) = node_for(candidate.evidence[0].document) else {
        return false;
    };
    let Some(second) = node_for(candidate.evidence[1].document) else {
        return false;
    };
    let nodes = [first, second];
    if !nodes.iter().any(|node| within(*node, candidate.target))
        || nodes
            .iter()
            .any(|node| candidate.excluded.iter().any(|root| within(*node, *root)))
        || candidate.evidence_scope.is_some_and(|root| {
            nodes
                .iter()
                .any(|node| !within(*node, candidate.target) && !within(*node, root))
        })
    {
        return false;
    }
    candidate.evidence.iter().all(|evidence| {
        if let Some(original) = &evidence.original {
            let Some(source) = project.research_originals.get(&original.source_id) else {
                return false;
            };
            if source.document != evidence.document || source.sha256 != original.sha256 {
                return false;
            }
            return originals
                .iter()
                .find(|item| item.source_id == original.source_id)
                .is_some_and(|item| evidence.occurs_uniquely(&item.text));
        }
        let Some(doc) = snapshot
            .documents
            .iter()
            .find(|document| document.document_id == evidence.document)
        else {
            return false;
        };
        if doc.revision.value() != evidence.revision {
            return false;
        }
        let text = documents.entry(evidence.document).or_insert_with(|| {
            let mut load = CanonicalDocumentLoad::new(doc.document_id, &doc.body);
            load.revision = doc.revision;
            Some(
                EditorCoreSession::open(load)
                    .ok()?
                    .canonical_projection()
                    .semantic()
                    .plain_text(),
            )
        });
        text.as_ref()
            .is_some_and(|text| evidence.matches_source(text))
    })
}

/// Adds only unambiguous local decision matches after the provider has
/// responded. Stored author decisions are never sent to the provider.
pub fn attach_matches(
    report: &mut CheckReport,
    context: &[ContextPassage],
    snapshot: &AuthoredProjectSnapshot,
    originals: &[ExtractedOriginal],
) {
    let Some(scope) = report.scope.as_ref() else {
        return;
    };
    let mut documents: BTreeMap<DocumentId, Option<String>> = BTreeMap::new();
    for finding in &mut report.findings {
        finding.matched_decision_id = None;
        let Some(candidate) = capture(finding, scope, context, ContinuityDecision::Intentional)
        else {
            continue;
        };
        if !evidence_is_current_with_cache(&candidate, snapshot, originals, &mut documents) {
            continue;
        }
        let project = &snapshot.project;
        let mut matches = project
            .continuity_decisions
            .values()
            .filter(|prior| prior.same_issue_as(&candidate));
        let Some(prior) = matches.next() else {
            continue;
        };
        if matches.next().is_none() {
            finding.matched_decision_id = Some(encode_stable_id(prior.id.as_bytes()));
        }
    }
}
