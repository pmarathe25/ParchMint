//! Bounded MIME parsing without retaining unrelated response parameters.

const MAX_HEADER_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentType {
    pub essence: String,
    pub charsets: Vec<String>,
}

/// Parses the MIME essence and every charset parameter. Each parameter is
/// parsed separately to avoid a `mime` crate indexing bug involving a quoted
/// UTF-8 charset followed by another parameter. Duplicate charsets are retained
/// so the caller can reject ambiguity.
pub fn parse(value: &str) -> Option<ContentType> {
    if value.len() > MAX_HEADER_BYTES || !value.is_ascii() {
        return None;
    }
    let mut segments = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (index, byte) in value.bytes().enumerate() {
        if quoted && escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        } else if byte == b';' && !quoted {
            segments.push(&value[start..index]);
            start = index + 1;
        }
    }
    if quoted || escaped {
        return None;
    }
    segments.push(&value[start..]);
    let first = segments.first()?.trim();
    let essence = first.parse::<mime::Mime>().ok()?;
    if essence.params().next().is_some() {
        return None;
    }
    let essence = essence.essence_str().to_ascii_lowercase();
    let mut charsets = Vec::new();
    for segment in segments.iter().skip(1) {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let (name, parameter_value) = segment.split_once('=')?;
        let individual = format!("{essence}; {}={}", name.trim(), parameter_value.trim());
        let parsed = individual.parse::<mime::Mime>().ok()?;
        let mut params = parsed.params();
        let (name, parameter_value) = params.next()?;
        if params.next().is_some() {
            return None;
        }
        if name.as_str().eq_ignore_ascii_case("charset") {
            charsets.push(parameter_value.as_str().to_ascii_lowercase());
        }
    }
    Some(ContentType { essence, charsets })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_parameters_and_preserves_duplicate_charsets() {
        let parsed = parse("Text/HTML; Charset=\"UTF-8\"; other=\"x;y\"").unwrap();
        assert_eq!(parsed.essence, "text/html");
        assert_eq!(parsed.charsets, ["utf-8"]);
        let parsed = parse("text/html; charset=utf-8; charset=windows-1252").unwrap();
        assert_eq!(parsed.charsets, ["utf-8", "windows-1252"]);
        assert!(parse("text/html; charset=\"unfinished").is_none());
        assert!(parse("text/html; charset=utf-8; invalid").is_none());
        assert!(parse(&format!("text/html; a={}", "x".repeat(1024))).is_none());
        assert_eq!(
            parse(r#"text/html; other="x;y"; charset="UTF-8""#)
                .unwrap()
                .charsets,
            ["utf-8"]
        );
        assert_eq!(
            parse(r#"text/html; other="x\\"; charset=windows-1252"#)
                .unwrap()
                .charsets,
            ["windows-1252"]
        );
        assert!(parse(r#"text/html; other="x\";charset=windows-1252"; charset=utf-8"#).is_none());
    }
}
