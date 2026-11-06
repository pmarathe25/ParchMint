//! Strict versioned encoding of author-owned continuity decisions.

use parchmint_domain::{
    CheckDecisionId, ContinuityCategory, ContinuityDecision, ContinuityDecisionRecord,
    ContinuityEvidence, ContinuityOriginal, DocumentId, NodeId, Project, ResearchSourceId,
};
use serde_json::{Value, json};

use super::{FormatError, hex_text, parse_content_hash, parse_stable_id, stable_id_text};

fn invalid(reason: &str) -> FormatError {
    FormatError::InvalidManifest(format!("invalid continuity decisions: {reason}"))
}

fn validate_serialized_size(value: &toml::Value) -> Result<(), FormatError> {
    let bytes = toml::to_string(value).map_err(|_| invalid("could not encode extension"))?;
    if bytes.len() > parchmint_domain::MAX_CONTINUITY_DECISION_BYTES {
        return Err(invalid("serialized extension is too large"));
    }
    Ok(())
}

fn category(value: ContinuityCategory) -> &'static str {
    match value {
        ContinuityCategory::IdentityAppearance => "identity-appearance",
        ContinuityCategory::TimeTravel => "time-travel",
        ContinuityCategory::ObjectsPossession => "objects-possession",
        ContinuityCategory::Rules => "rules",
        ContinuityCategory::Relationships => "relationships",
        ContinuityCategory::Knowledge => "knowledge",
        ContinuityCategory::Other => "other",
    }
}

fn evidence_value(evidence: &ContinuityEvidence) -> Value {
    let mut value = json!({
        "document": stable_id_text(evidence.document.as_bytes()),
        "quote": evidence.quote, "before": evidence.before, "after": evidence.after,
        "revision": evidence.revision,
    });
    let object = value.as_object_mut().expect("object literal");
    if let Some(anchor) = &evidence.anchor {
        object.insert("anchor".into(), json!(anchor));
    }
    if let Some(original) = &evidence.original {
        object.insert(
            "original".into(),
            json!({
                "source": stable_id_text(original.source_id.as_bytes()),
                "sha256": hex_text(&original.sha256),
            }),
        );
    }
    value
}

pub(super) fn encode(project: &Project) -> Result<Option<toml::Value>, FormatError> {
    if !project.continuity_enabled {
        return Ok(None);
    }
    let records = project.continuity_decisions.values().map(|record| {
        let mut value = json!({
            "id": stable_id_text(record.id.as_bytes()), "category": category(record.category),
            "explanation": record.explanation,
            "decision": match record.decision {
                ContinuityDecision::Intentional => "intentional",
                ContinuityDecision::Dismissed => "dismissed",
                ContinuityDecision::Resolved => "resolved",
            },
            "evidence": record.evidence.iter().map(evidence_value).collect::<Vec<_>>(),
            "target": stable_id_text(record.target.as_bytes()),
            "excluded": record.excluded.iter().map(|id| stable_id_text(id.as_bytes())).collect::<Vec<_>>(),
            "selection-only": record.selection_only,
        });
        let object = value.as_object_mut().expect("object literal");
        if let Some(rationale) = &record.rationale {
            object.insert("rationale".into(), json!(rationale));
        }
        if let Some(scope) = record.evidence_scope {
            object.insert("evidence-scope".into(), json!(stable_id_text(scope.as_bytes())));
        }
        value
    }).collect::<Vec<_>>();
    let value = toml::Value::try_from(json!({"version": 1, "records": records}))
        .map_err(|_| invalid("could not encode records"))?;
    validate_serialized_size(&value)?;
    Ok(Some(value))
}

fn object<'a>(
    value: &'a Value,
    allowed: &[&str],
) -> Result<&'a serde_json::Map<String, Value>, FormatError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("expected a table"))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("unsupported field"));
    }
    Ok(object)
}

fn text<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Result<&'a str, FormatError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("expected text"))
}

fn optional_text(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, FormatError> {
    object
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("expected text"))
        })
        .transpose()
}

fn node(value: &str) -> Result<NodeId, FormatError> {
    Ok(NodeId::from_bytes(parse_stable_id(value)?))
}

fn decode_evidence(value: &Value) -> Result<ContinuityEvidence, FormatError> {
    let value = object(
        value,
        &[
            "document", "quote", "before", "after", "revision", "anchor", "original",
        ],
    )?;
    let original = value
        .get("original")
        .map(|value| {
            let original = object(value, &["source", "sha256"])?;
            Ok(ContinuityOriginal {
                source_id: ResearchSourceId::from_bytes(parse_stable_id(text(
                    original, "source",
                )?)?),
                sha256: *parse_content_hash(text(original, "sha256")?)?.as_bytes(),
            })
        })
        .transpose()?;
    Ok(ContinuityEvidence {
        document: DocumentId::from_bytes(parse_stable_id(text(value, "document")?)?),
        original,
        quote: text(value, "quote")?.to_owned(),
        before: text(value, "before")?.to_owned(),
        after: text(value, "after")?.to_owned(),
        revision: value
            .get("revision")
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid("invalid revision"))?,
        anchor: optional_text(value, "anchor")?,
    })
}

pub(super) fn decode(
    value: Option<&toml::Value>,
    project: &mut Project,
) -> Result<(), FormatError> {
    let Some(value) = value else {
        return Ok(());
    };
    validate_serialized_size(value)?;
    let value = serde_json::to_value(value).map_err(|_| invalid("could not read extension"))?;
    let value = object(&value, &["version", "records"])?;
    if value.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(invalid("unsupported version"));
    }
    let records = value
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("expected records"))?;
    if records.len() > 1_024 {
        return Err(invalid("too many records"));
    }
    project.continuity_enabled = true;
    for record in records {
        let record = object(
            record,
            &[
                "id",
                "category",
                "explanation",
                "decision",
                "rationale",
                "evidence",
                "target",
                "evidence-scope",
                "excluded",
                "selection-only",
            ],
        )?;
        let category = match text(record, "category")? {
            "identity-appearance" => ContinuityCategory::IdentityAppearance,
            "time-travel" => ContinuityCategory::TimeTravel,
            "objects-possession" => ContinuityCategory::ObjectsPossession,
            "rules" => ContinuityCategory::Rules,
            "relationships" => ContinuityCategory::Relationships,
            "knowledge" => ContinuityCategory::Knowledge,
            "other" => ContinuityCategory::Other,
            _ => return Err(invalid("unknown category")),
        };
        let decision = match text(record, "decision")? {
            "intentional" => ContinuityDecision::Intentional,
            "dismissed" => ContinuityDecision::Dismissed,
            "resolved" => ContinuityDecision::Resolved,
            _ => return Err(invalid("unknown decision")),
        };
        let evidence = record
            .get("evidence")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("expected evidence"))?;
        if evidence.len() != 2 {
            return Err(invalid("expected two evidence items"));
        }
        let excluded = record
            .get("excluded")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("expected exclusions"))?;
        if excluded.len() > 1_024 {
            return Err(invalid("too many exclusions"));
        }
        let id = CheckDecisionId::from_bytes(parse_stable_id(text(record, "id")?)?);
        let record = ContinuityDecisionRecord {
            id,
            category,
            explanation: text(record, "explanation")?.to_owned(),
            decision,
            rationale: optional_text(record, "rationale")?,
            evidence: [
                decode_evidence(&evidence[0])?,
                decode_evidence(&evidence[1])?,
            ],
            target: node(text(record, "target")?)?,
            evidence_scope: optional_text(record, "evidence-scope")?
                .map(|value| node(&value))
                .transpose()?,
            excluded: excluded
                .iter()
                .map(|item| {
                    item.as_str()
                        .ok_or_else(|| invalid("invalid exclusion"))
                        .and_then(node)
                })
                .collect::<Result<_, _>>()?,
            selection_only: record
                .get("selection-only")
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid("invalid selection scope"))?,
        };
        record
            .validate()
            .map_err(|error| invalid(&error.to_string()))?;
        if project.continuity_decisions.insert(id, record).is_some() {
            return Err(invalid("duplicate decision ID"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CanonicalCodec, CanonicalInputSet, CanonicalProjectPathMap, FormatVersion,
        ProjectFormatCodec,
    };
    use parchmint_domain::ProjectId;
    use std::collections::BTreeMap;

    fn project_with_decision() -> Project {
        let mut project = Project::new(ProjectId::from_bytes([1; 16]));
        let first = ContinuityEvidence {
            document: DocumentId::from_bytes([2; 16]),
            original: None,
            quote: "The key".into(),
            before: "Mara has ".into(),
            after: " today".into(),
            revision: 7,
            anchor: Some("text:10:17".into()),
        };
        let mut second = first.clone();
        second.document = DocumentId::from_bytes([3; 16]);
        let id = CheckDecisionId::from_bytes([4; 16]);
        project.continuity_enabled = true;
        project.continuity_decisions.insert(
            id,
            ContinuityDecisionRecord {
                id,
                category: ContinuityCategory::ObjectsPossession,
                explanation: "The owner differs.".into(),
                decision: ContinuityDecision::Intentional,
                rationale: Some("A later transfer is planned.".into()),
                evidence: [first, second],
                target: NodeId::manuscript_root(),
                evidence_scope: None,
                excluded: vec![NodeId::lore_root()],
                selection_only: false,
            },
        );
        project
    }

    #[test]
    fn versioned_decisions_round_trip_and_reject_mismatched_marker() {
        let project = project_with_decision();
        let codec = ProjectFormatCodec::default();
        let encoded = codec
            .encode_domain_project(
                &project,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &CanonicalProjectPathMap::default(),
            )
            .unwrap();
        let control = encoded
            .resources
            .values()
            .find(|resource| matches!(resource.resource, crate::ResourceId::FormatControl))
            .unwrap();
        assert_eq!(control.bytes, FormatVersion::V2.control_bytes());
        let manifest = encoded
            .resources
            .values()
            .find(|resource| matches!(resource.resource, crate::ResourceId::Manifest))
            .unwrap();
        let manifest = codec.decode_manifest(&manifest.bytes).unwrap();
        let (decoded, _) = codec
            .decode_domain_project(&manifest, project.id)
            .unwrap()
            .unwrap();
        assert_eq!(decoded.continuity_decisions, project.continuity_decisions);
        assert!(decoded.continuity_enabled);
        assert!(
            codec
                .validate_manifest_version(FormatVersion::V1, &manifest)
                .is_err()
        );
        let legacy = Project::new(project.id);
        let legacy_encoded = codec
            .encode_domain_project(
                &legacy,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &CanonicalProjectPathMap::default(),
            )
            .unwrap();
        let legacy_manifest = legacy_encoded
            .resources
            .values()
            .find(|resource| matches!(resource.resource, crate::ResourceId::Manifest))
            .unwrap();
        let legacy_manifest = codec.decode_manifest(&legacy_manifest.bytes).unwrap();
        assert!(
            codec
                .validate_manifest_version(FormatVersion::V2, &legacy_manifest)
                .is_err()
        );
        assert!(
            codec
                .validate_manifest_version(FormatVersion::V2, &manifest)
                .is_ok()
        );
        let resources = encoded
            .resources
            .into_iter()
            .filter(|(path, _)| path.as_str() != ".parchmint/format-version")
            .map(|(path, resource)| (path, resource.bytes))
            .collect();
        assert!(
            codec
                .decode_project(CanonicalInputSet {
                    format_control: Some(FormatVersion::V1.control_bytes().to_vec()),
                    resources
                })
                .is_err()
        );
    }

    #[test]
    fn unknown_extension_fields_and_versions_are_rejected() {
        let project = project_with_decision();
        let mut value = encode(&project).unwrap().unwrap();
        value
            .as_table_mut()
            .unwrap()
            .insert("future".into(), toml::Value::Boolean(true));
        let mut decoded = Project::new(project.id);
        assert!(decode(Some(&value), &mut decoded).is_err());
        let mut value = encode(&project).unwrap().unwrap();
        value
            .as_table_mut()
            .unwrap()
            .insert("version".into(), toml::Value::Integer(2));
        assert!(decode(Some(&value), &mut Project::new(project.id)).is_err());
    }

    #[test]
    fn incremental_upgrade_writes_marker_and_manifest_together_and_stays_v2() {
        use crate::{
            CanonicalDomainUpdate, CanonicalPersistenceFrontierTransition, CanonicalRelativePath,
            CanonicalResourceMetadata,
        };
        let codec = ProjectFormatCodec::default();
        let legacy = Project::new(ProjectId::from_bytes([1; 16]));
        let baseline = codec
            .encode_domain_project(
                &legacy,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &CanonicalProjectPathMap::default(),
            )
            .unwrap();
        let control = CanonicalRelativePath::parse(".parchmint/format-version").unwrap();
        let manifest = CanonicalRelativePath::parse("project.toml").unwrap();
        assert_eq!(
            baseline.resources[&control].bytes,
            FormatVersion::V1.control_bytes()
        );
        let existing = baseline
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect();
        let metadata = baseline
            .resources
            .iter()
            .map(|(path, resource)| {
                (
                    path.clone(),
                    CanonicalResourceMetadata {
                        resource: resource.resource.clone(),
                        hash: resource.hash,
                    },
                )
            })
            .collect();
        let project = project_with_decision();
        let update = CanonicalDomainUpdate {
            manifest: true,
            ..Default::default()
        };
        let transition = || CanonicalPersistenceFrontierTransition {
            previous: &baseline.persistence_frontier,
            next: &baseline.persistence_frontier,
        };
        let added = codec
            .encode_domain_project_patch(
                &project,
                &update,
                &existing,
                &metadata,
                &baseline.paths,
                transition(),
            )
            .unwrap();
        assert_eq!(
            added.resources[&control].bytes,
            FormatVersion::V2.control_bytes()
        );
        assert!(added.resources.contains_key(&manifest));
        let mut no_decisions = project;
        no_decisions.continuity_decisions.clear();
        let retained = codec
            .encode_domain_project_patch(
                &no_decisions,
                &update,
                &BTreeMap::new(),
                &added.complete_resources,
                &added.paths,
                transition(),
            )
            .unwrap();
        assert!(!retained.resources.contains_key(&control));
        let decoded = codec
            .decode_manifest(&retained.resources[&manifest].bytes)
            .unwrap();
        assert!(
            codec
                .validate_manifest_version(FormatVersion::V2, &decoded)
                .is_ok()
        );
    }

    #[test]
    fn escaping_heavy_records_stay_within_the_serialized_budget() {
        let mut project = project_with_decision();
        let mut record = project
            .continuity_decisions
            .values()
            .next()
            .unwrap()
            .clone();
        record.explanation = "\\".repeat(600);
        record.rationale = Some("\\".repeat(1_000));
        record.evidence[0].quote = "\\".repeat(400);
        record.evidence[1].quote = "\\".repeat(400);
        record.evidence[0].anchor = None;
        record.evidence[1].anchor = None;
        let count =
            (parchmint_domain::MAX_CONTINUITY_DECISION_BYTES / record.estimated_bytes()).min(1_024);
        project.continuity_decisions.clear();
        for index in 0..count {
            let mut bytes = [0; 16];
            bytes[..8].copy_from_slice(&(index as u64).to_be_bytes());
            record.id = CheckDecisionId::from_bytes(bytes);
            project
                .continuity_decisions
                .insert(record.id, record.clone());
        }
        assert!(project.validate().is_ok());
        let encoded = encode(&project).unwrap().unwrap();
        assert!(
            toml::to_string(&encoded).unwrap().len()
                <= parchmint_domain::MAX_CONTINUITY_DECISION_BYTES
        );
        let oversized = toml::Value::try_from(json!({"version":1,"records":[],"padding":"\\".repeat(parchmint_domain::MAX_CONTINUITY_DECISION_BYTES)})).unwrap();
        assert!(decode(Some(&oversized), &mut Project::new(project.id)).is_err());
    }
}
