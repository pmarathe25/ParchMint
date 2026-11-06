//! Author decisions about continuity findings. Provider output is never an
//! authority for identifiers, decisions, or the evidence identities here.

use crate::{CheckDecisionId, DocumentId, DomainError, NodeId, ResearchSourceId};

pub const MAX_CONTINUITY_DECISION_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuityCategory {
    IdentityAppearance,
    TimeTravel,
    ObjectsPossession,
    Rules,
    Relationships,
    Knowledge,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuityDecision {
    Intentional,
    Dismissed,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuityOriginal {
    pub source_id: ResearchSourceId,
    pub sha256: [u8; 32],
}

/// Captured identity and surrounding text make repeated quotations distinguishable.
/// Revision and anchor are audit data; neither is used as the issue identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuityEvidence {
    pub document: DocumentId,
    pub original: Option<ContinuityOriginal>,
    pub quote: String,
    pub before: String,
    pub after: String,
    pub revision: u64,
    pub anchor: Option<String>,
}

impl ContinuityEvidence {
    /// The quote and its captured surroundings must identify one occurrence,
    /// including overlapping occurrences, in the complete current source.
    pub fn occurs_uniquely(&self, text: &str) -> bool {
        let needle = format!("{}{}{}", self.before, self.quote, self.after);
        let Some(start) = text.find(&needle) else {
            return false;
        };
        !text[start..]
            .char_indices()
            .skip(1)
            .any(|(offset, _)| text[start + offset..].starts_with(&needle))
    }

    /// Checks an authored scalar anchor against the exact current source.
    pub fn anchor_matches(&self, text: &str) -> bool {
        let Some(anchor) = &self.anchor else {
            return true;
        };
        let mut parts = anchor.split(':');
        if parts.next() != Some("text") {
            return false;
        }
        let Some(start) = parts.next().and_then(|value| value.parse::<usize>().ok()) else {
            return false;
        };
        let Some(end) = parts.next().and_then(|value| value.parse::<usize>().ok()) else {
            return false;
        };
        parts.next().is_none()
            && end >= start
            && text
                .chars()
                .skip(start)
                .take(end - start)
                .eq(self.quote.chars())
    }

    /// Both the anchor and the unique context must identify the same occurrence.
    pub fn matches_source(&self, text: &str) -> bool {
        if !self.occurs_uniquely(text) || !self.anchor_matches(text) {
            return false;
        }
        let Some(anchor) = &self.anchor else {
            return true;
        };
        let needle = format!("{}{}{}", self.before, self.quote, self.after);
        let Some(byte_start) = text.find(&needle) else {
            return false;
        };
        let quote_start = text[..byte_start].chars().count() + self.before.chars().count();
        anchor
            .split(':')
            .nth(1)
            .and_then(|value| value.parse::<usize>().ok())
            == Some(quote_start)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuityDecisionRecord {
    pub id: CheckDecisionId,
    pub category: ContinuityCategory,
    pub explanation: String,
    pub decision: ContinuityDecision,
    pub rationale: Option<String>,
    pub evidence: [ContinuityEvidence; 2],
    pub target: NodeId,
    pub evidence_scope: Option<NodeId>,
    pub excluded: Vec<NodeId>,
    pub selection_only: bool,
}

impl ContinuityDecisionRecord {
    /// Conservative pre-encoding budget, including identity and table overhead.
    pub fn estimated_bytes(&self) -> usize {
        let mut bytes = 512_usize
            .saturating_add(self.explanation.len().saturating_mul(2))
            .saturating_add(
                self.rationale
                    .as_ref()
                    .map_or(0, String::len)
                    .saturating_mul(2),
            )
            .saturating_add(self.excluded.len().saturating_mul(40));
        for evidence in &self.evidence {
            bytes = bytes
                .saturating_add(evidence.quote.len().saturating_mul(2))
                .saturating_add(evidence.before.len().saturating_mul(2))
                .saturating_add(evidence.after.len().saturating_mul(2))
                .saturating_add(
                    evidence
                        .anchor
                        .as_ref()
                        .map_or(0, String::len)
                        .saturating_mul(2),
                );
        }
        bytes
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        let valid = |text: &str, max: usize, empty: bool| {
            (empty || !text.trim().is_empty())
                && text.chars().count() <= max
                && !text
                    .chars()
                    .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
        };
        if !valid(&self.explanation, 600, false)
            || self
                .rationale
                .as_ref()
                .is_some_and(|text| !valid(text, 1_000, true))
            || self.excluded.len() > 1_024
            || (self.selection_only && self.evidence_scope.is_some())
            || self
                .excluded
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.excluded.len()
            || self.evidence.iter().any(|evidence| {
                evidence.revision > i64::MAX as u64
                    || !valid(&evidence.quote, 400, false)
                    || !valid(&evidence.before, 200, true)
                    || !valid(&evidence.after, 200, true)
                    || evidence.anchor.as_ref().is_some_and(|anchor| {
                        if evidence.original.is_some() || !valid(anchor, 128, false) {
                            return true;
                        }
                        let mut parts = anchor.split(':');
                        let Some((start, end)) = (|| {
                            if parts.next()? != "text" {
                                return None;
                            }
                            let start = parts.next()?.parse::<u64>().ok()?;
                            let end = parts.next()?.parse::<u64>().ok()?;
                            if parts.next().is_some() {
                                return None;
                            }
                            Some((start, end))
                        })() else {
                            return true;
                        };
                        end.checked_sub(start) != u64::try_from(evidence.quote.chars().count()).ok()
                    })
            })
            || (self.evidence[0].document == self.evidence[1].document
                && self.evidence[0].original == self.evidence[1].original
                && self.evidence[0].quote == self.evidence[1].quote
                && self.evidence[0].before == self.evidence[1].before
                && self.evidence[0].after == self.evidence[1].after)
        {
            return Err(DomainError::InvalidInput {
                field: "continuity decision",
                reason: "invalid or oversized decision evidence",
            });
        }
        Ok(())
    }

    /// Candidate matching only. Each occurrence must also resolve uniquely in
    /// current source content before any old author decision can be reused.
    /// Changed text, context, source hash, category, or explanation never match.
    pub fn same_issue_as(&self, other: &Self) -> bool {
        fn same_evidence(a: &ContinuityEvidence, b: &ContinuityEvidence) -> bool {
            a.document == b.document
                && a.original == b.original
                && a.quote == b.quote
                && a.before == b.before
                && a.after == b.after
        }
        self.category == other.category
            && self.explanation.trim() == other.explanation.trim()
            && ((same_evidence(&self.evidence[0], &other.evidence[0])
                && same_evidence(&self.evidence[1], &other.evidence[1]))
                || (same_evidence(&self.evidence[0], &other.evidence[1])
                    && same_evidence(&self.evidence[1], &other.evidence[0])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> ContinuityDecisionRecord {
        let evidence = ContinuityEvidence {
            document: DocumentId::from_bytes([1; 16]),
            original: None,
            quote: "The key".into(),
            before: "holds ".into(),
            after: " at dusk".into(),
            revision: 1,
            anchor: Some("text:1:8".into()),
        };
        let mut other = evidence.clone();
        other.document = DocumentId::from_bytes([3; 16]);
        ContinuityDecisionRecord {
            id: CheckDecisionId::from_bytes([2; 16]),
            category: ContinuityCategory::ObjectsPossession,
            explanation: "The owner differs.".into(),
            decision: ContinuityDecision::Intentional,
            rationale: None,
            evidence: [evidence, other],
            target: NodeId::manuscript_root(),
            evidence_scope: None,
            excluded: Vec::new(),
            selection_only: false,
        }
    }

    #[test]
    fn validates_bounds_and_matches_only_the_same_issue_evidence() {
        let first = record();
        assert!(first.validate().is_ok());
        let mut later = first.clone();
        later.evidence[0].revision += 1;
        later.evidence[0].anchor = Some("text:99:106".into());
        assert!(first.same_issue_as(&later));
        later.evidence.swap(0, 1);
        assert!(first.same_issue_as(&later));
        later.evidence[0].quote = "Other key".into();
        assert!(!first.same_issue_as(&later));
        later = first.clone();
        later.explanation = "A different issue.".into();
        assert!(!first.same_issue_as(&later));
        later.evidence[0].quote = "x".repeat(401);
        assert!(later.validate().is_err());
    }

    #[test]
    fn rejects_invalid_anchors_and_conflicting_scopes() {
        let mut item = record();
        item.evidence[0].anchor = Some("text:18446744073709551615:1".into());
        assert!(item.validate().is_err());
        item.evidence[0].anchor = Some("text:1:7".into());
        assert!(item.validate().is_err());
        item.evidence[0].anchor = Some("text:1:8".into());
        item.selection_only = true;
        item.evidence_scope = Some(NodeId::lore_root());
        assert!(item.validate().is_err());
        item.selection_only = false;
        item.excluded = vec![NodeId::lore_root(), NodeId::lore_root()];
        assert!(item.validate().is_err());
        item.excluded.clear();
        item.evidence[0].original = Some(ContinuityOriginal {
            source_id: ResearchSourceId::from_bytes([4; 16]),
            sha256: [5; 32],
        });
        assert!(item.validate().is_err());
        item.evidence[0].anchor = None;
        assert!(item.validate().is_ok());
    }

    #[test]
    fn decisions_use_project_undo_and_keep_the_new_format_sticky() {
        use crate::{Project, ProjectCommand, ProjectId, Resource, apply_project_command};
        let mut project = Project::new(ProjectId::from_bytes([9; 16]));
        for (node, document, title) in [
            (
                NodeId::from_bytes([11; 16]),
                DocumentId::from_bytes([1; 16]),
                "First",
            ),
            (
                NodeId::from_bytes([12; 16]),
                DocumentId::from_bytes([3; 16]),
                "Second",
            ),
        ] {
            project = apply_project_command(
                &project,
                project.revision,
                ProjectCommand::create_document(
                    node,
                    document,
                    NodeId::manuscript_root(),
                    0,
                    title,
                ),
            )
            .unwrap()
            .project;
        }
        let item = record();
        let applied = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::SetContinuityDecision {
                record: Box::new(item.clone()),
                expected: None,
            },
        )
        .unwrap();
        assert!(applied.project.continuity_enabled);
        assert!(applied.changed_resources.contains(Resource::Manifest));
        let mut dismissed = item.clone();
        dismissed.decision = ContinuityDecision::Dismissed;
        let changed = apply_project_command(
            &applied.project,
            applied.project.revision,
            ProjectCommand::SetContinuityDecision {
                record: Box::new(dismissed.clone()),
                expected: Some(Box::new(item.clone())),
            },
        )
        .unwrap();
        let mut resolved = item.clone();
        resolved.decision = ContinuityDecision::Resolved;
        assert!(
            apply_project_command(
                &changed.project,
                changed.project.revision,
                ProjectCommand::SetContinuityDecision {
                    record: Box::new(resolved),
                    expected: Some(Box::new(item.clone()))
                }
            )
            .is_err()
        );
        assert!(
            apply_project_command(
                &changed.project,
                changed.project.revision,
                ProjectCommand::SetContinuityDecision {
                    record: Box::new(dismissed),
                    expected: Some(Box::new(item.clone()))
                }
            )
            .is_ok()
        );
        let undone =
            apply_project_command(&applied.project, applied.project.revision, applied.inverse)
                .unwrap();
        assert!(undone.project.continuity_enabled);
        assert!(undone.project.continuity_decisions.is_empty());
        let redone =
            apply_project_command(&undone.project, undone.project.revision, undone.inverse)
                .unwrap();
        assert_eq!(
            redone.project.continuity_decisions.get(&item.id),
            Some(&item)
        );
        let deleted = apply_project_command(
            &redone.project,
            redone.project.revision,
            ProjectCommand::DeleteNode {
                id: NodeId::from_bytes([11; 16]),
                deleted_at_unix_millis: 1,
                restoring_checkpoint: None,
            },
        )
        .unwrap();
        assert!(deleted.project.continuity_decisions.contains_key(&item.id));
        assert!(deleted.project.validate().is_ok());
    }
}
