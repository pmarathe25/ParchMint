//! Validation of untrusted provider findings against captured project evidence.

use std::collections::BTreeSet;

use parchmint_assistant_api::{CheckCategory, CheckFinding, CheckReport, ContextPassage};
use serde::Deserialize;

const MAX_RESPONSE_BYTES: usize = 128 * 1024;
const MAX_FINDINGS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidCheckReport {
    Malformed,
    InvalidEvidence,
}

#[derive(Deserialize)]
struct WireReport {
    summary: String,
    findings: Vec<WireFinding>,
}

#[derive(Deserialize)]
struct WireFinding {
    category: CheckCategory,
    explanation: String,
    #[serde(default)]
    uncertainty: Option<String>,
    evidence: [WireEvidence; 2],
}

#[derive(Deserialize)]
struct WireEvidence {
    source_id: String,
    quote: String,
}

/// Turns provider JSON into findings only when each quote uniquely matches
/// the exact context sent with this request. Provider offsets and document
/// identities are never trusted.
pub fn validate(
    response: &str,
    context: &[ContextPassage],
    targets: &BTreeSet<String>,
) -> Result<CheckReport, InvalidCheckReport> {
    if response.len() > MAX_RESPONSE_BYTES || targets.is_empty() {
        return Err(InvalidCheckReport::Malformed);
    }
    let response = response.trim();
    let json = response
        .strip_prefix("```json\n")
        .or_else(|| response.strip_prefix("```json\r\n"))
        .or_else(|| response.strip_prefix("```\n"))
        .or_else(|| response.strip_prefix("```\r\n"))
        .and_then(|body| body.strip_suffix("```"))
        .unwrap_or(response)
        .trim();
    let wire: WireReport = serde_json::from_str(json).map_err(|_| InvalidCheckReport::Malformed)?;
    if wire.summary.trim().is_empty()
        || wire.summary.chars().count() > 2_000
        || wire.findings.len() > MAX_FINDINGS
    {
        return Err(InvalidCheckReport::Malformed);
    }
    let mut findings = Vec::with_capacity(wire.findings.len());
    for finding in wire.findings {
        if finding.explanation.trim().is_empty()
            || finding.explanation.chars().count() > 600
            || finding
                .uncertainty
                .as_ref()
                .is_some_and(|text| text.chars().count() > 400)
        {
            return Err(InvalidCheckReport::Malformed);
        }
        let [first, second] = finding.evidence;
        let first = resolve(first, context)?;
        let second = resolve(second, context)?;
        if first.source_id == second.source_id && first.text == second.text {
            return Err(InvalidCheckReport::InvalidEvidence);
        }
        let is_target = |source: &ContextPassage| {
            source.section != parchmint_assistant_api::ProjectSection::Notes
                && targets.contains(&source.document_id)
        };
        if !is_target(&first) && !is_target(&second) {
            return Err(InvalidCheckReport::InvalidEvidence);
        }
        findings.push(CheckFinding {
            category: finding.category,
            explanation: finding.explanation,
            uncertainty: finding.uncertainty.filter(|text| !text.trim().is_empty()),
            evidence: [first, second],
            matched_decision_id: None,
        });
    }
    Ok(CheckReport {
        summary: wire.summary,
        findings,
        scope: None,
    })
}

fn resolve(
    wire: WireEvidence,
    context: &[ContextPassage],
) -> Result<ContextPassage, InvalidCheckReport> {
    let source_id = wire
        .source_id
        .strip_prefix('[')
        .and_then(|id| id.strip_suffix(']'))
        .unwrap_or(&wire.source_id);
    if source_id.is_empty()
        || source_id.len() > 32
        || wire.quote.trim().is_empty()
        || wire.quote.chars().count() > 400
    {
        return Err(InvalidCheckReport::InvalidEvidence);
    }
    let mut matches = context
        .iter()
        .filter(|source| source.source_id == source_id);
    let source = matches.next().ok_or(InvalidCheckReport::InvalidEvidence)?;
    if matches.next().is_some() {
        return Err(InvalidCheckReport::InvalidEvidence);
    }
    let start = source
        .text
        .find(&wire.quote)
        .ok_or(InvalidCheckReport::InvalidEvidence)?;
    if source.text[start..]
        .char_indices()
        .skip(1)
        .any(|(index, _)| source.text[start + index..].starts_with(&wire.quote))
    {
        return Err(InvalidCheckReport::InvalidEvidence);
    }
    let mut evidence = source.clone();
    evidence.anchor = source.anchor.as_deref().and_then(|anchor| {
        let mut parts = anchor.split(':');
        if parts.next()? != "text" {
            return None;
        }
        let old_start = parts.next()?.parse::<u64>().ok()?;
        let old_end = parts.next()?.parse::<u64>().ok()?;
        if parts.next().is_some()
            || old_end.checked_sub(old_start)? != u64::try_from(source.text.chars().count()).ok()?
        {
            return None;
        }
        let quote_start =
            old_start.checked_add(u64::try_from(source.text[..start].chars().count()).ok()?)?;
        let quote_end = quote_start.checked_add(u64::try_from(wire.quote.chars().count()).ok()?)?;
        Some(format!("text:{quote_start}:{quote_end}"))
    });
    evidence.text = wire.quote;
    Ok(evidence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_assistant_api::ProjectSection;

    fn source(id: &str, document: &str, text: &str, anchor: &str) -> ContextPassage {
        ContextPassage {
            source_id: id.into(),
            document_id: document.into(),
            document_revision: 3,
            section: ProjectSection::Lore,
            title: id.into(),
            anchor: Some(anchor.into()),
            text: text.into(),
            original: None,
        }
    }

    fn report(first: &str, second: &str) -> String {
        serde_json::json!({"summary":"Possible conflict", "findings":[{
            "category":"objects_possession", "explanation":"The owner differs.",
            "uncertainty":"A transfer may explain it.",
            "evidence":[{"source_id":"S1","quote":first},{"source_id":"S2","quote":second}]
        }]})
        .to_string()
    }

    #[test]
    fn validates_quotes_and_derives_unicode_scalar_anchors() {
        let context = vec![
            source("S1", "target", "Éva has it.", "text:10:21"),
            source("S2", "other", "Mara has it.", "text:0:12"),
        ];
        let result = validate(
            &report("Éva", "Mara"),
            &context,
            &BTreeSet::from(["target".into()]),
        )
        .unwrap();
        assert_eq!(
            result.findings[0].evidence[0].anchor.as_deref(),
            Some("text:10:13")
        );
        assert_eq!(
            result.findings[0].evidence[1].anchor.as_deref(),
            Some("text:0:4")
        );
        assert_eq!(result.findings[0].evidence[0].text, "Éva");
        assert_eq!(context[0].text, "Éva has it.");
        let bracketed = report("Éva", "Mara").replace("\"S1\"", "\"[S1]\"");
        assert!(validate(&bracketed, &context, &BTreeSet::from(["target".into()])).is_ok());
    }

    #[test]
    fn rejects_invented_ambiguous_and_out_of_scope_evidence() {
        let context = vec![
            source("S1", "target", "aaa", "text:0:3"),
            source("S2", "other", "Mara", "text:0:4"),
        ];
        let targets = BTreeSet::from(["target".into()]);
        assert_eq!(
            validate(&report("aa", "Mara"), &context, &targets),
            Err(InvalidCheckReport::InvalidEvidence)
        );
        assert_eq!(
            validate(&report("invented", "Mara"), &context, &targets),
            Err(InvalidCheckReport::InvalidEvidence)
        );
        assert_eq!(
            validate(
                &report("aaa", "Mara"),
                &context,
                &BTreeSet::from(["elsewhere".into()])
            ),
            Err(InvalidCheckReport::InvalidEvidence)
        );
        assert_eq!(
            validate("not json", &context, &targets),
            Err(InvalidCheckReport::Malformed)
        );
        let duplicated = serde_json::json!({"summary":"Issue", "findings":[{
            "category":"other", "explanation":"Issue", "evidence":[
                {"source_id":"S1","quote":"aaa"}, {"source_id":"S1","quote":"aaa"}
            ]
        }]})
        .to_string();
        assert_eq!(
            validate(&duplicated, &context, &targets),
            Err(InvalidCheckReport::InvalidEvidence)
        );
        let mut notes_only_target = context.clone();
        notes_only_target[0].section = ProjectSection::Notes;
        assert_eq!(
            validate(&report("aaa", "Mara"), &notes_only_target, &targets),
            Err(InvalidCheckReport::InvalidEvidence)
        );
    }
}
