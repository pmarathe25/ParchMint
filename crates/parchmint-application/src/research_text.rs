//! Local extraction from saved originals. This never changes source bytes or
//! fetches external resources. PDF needs a dedicated safe extractor.

/// Limit extracted source input independently of the storage limit.
pub const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
pub const EXTRACTOR_VERSION: &str = "utf8-text-v1";
pub const HTML_EXTRACTOR_VERSION: &str = "html5ever-0.40.1-v2";
pub const XHTML_EXTRACTOR_VERSION: &str = "roxmltree-0.20.0-v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceEncoding {
    Utf8,
    Ascii,
}

#[derive(Default)]
pub(super) struct EncodingDeclarations(Option<SourceEncoding>);

impl EncodingDeclarations {
    pub(super) fn add(&mut self, encoding: SourceEncoding) -> Result<(), ExtractionError> {
        if self.0.is_some_and(|previous| previous != encoding) {
            return Err(ExtractionError::UnsupportedEncoding);
        }
        self.0 = Some(encoding);
        Ok(())
    }

    pub(super) fn merge(&mut self, other: Self) -> Result<(), ExtractionError> {
        if let Some(encoding) = other.0 {
            self.add(encoding)?;
        }
        Ok(())
    }

    pub(super) fn validate(
        &self,
        text: &str,
        require_explicit_non_ascii: bool,
    ) -> Result<(), ExtractionError> {
        if !text.is_ascii()
            && (self.0 == Some(SourceEncoding::Ascii)
                || require_explicit_non_ascii && self.0.is_none())
        {
            return Err(ExtractionError::UnsupportedEncoding);
        }
        Ok(())
    }
}

pub(super) fn encoding_label(label: &str) -> Result<SourceEncoding, ExtractionError> {
    let label = label.trim();
    if label.eq_ignore_ascii_case("utf-8") || label.eq_ignore_ascii_case("utf8") {
        Ok(SourceEncoding::Utf8)
    } else if label.eq_ignore_ascii_case("us-ascii") || label.eq_ignore_ascii_case("ascii") {
        Ok(SourceEncoding::Ascii)
    } else {
        Err(ExtractionError::UnsupportedEncoding)
    }
}

pub(super) fn mime_encoding(value: &str) -> Result<Option<SourceEncoding>, ExtractionError> {
    let parsed =
        parchmint_content_type::parse(value).ok_or(ExtractionError::UnsupportedEncoding)?;
    if parsed.charsets.len() > 1 {
        return Err(ExtractionError::UnsupportedEncoding);
    }
    parsed
        .charsets
        .first()
        .map(|label| encoding_label(label))
        .transpose()
}

/// The extractor identifier is attached to derived evidence, not source bytes.
pub fn extractor_version(media_type: &str) -> Option<&'static str> {
    match media_type
        .split(';')
        .next()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "text/plain" | "text/markdown" => Some(EXTRACTOR_VERSION),
        "text/html" => Some(HTML_EXTRACTOR_VERSION),
        "application/xhtml+xml" => Some(XHTML_EXTRACTOR_VERSION),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionError {
    UnsupportedFormat,
    UnsupportedEncoding,
    InvalidMarkup,
    TooLarge,
    NoText,
}

/// Extracts bounded UTF-8 text. HTML is a derived text view, not original bytes.
pub fn extract(media_type: &str, bytes: &[u8]) -> Result<String, ExtractionError> {
    if extractor_version(media_type).is_none() {
        return Err(ExtractionError::UnsupportedFormat);
    }
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(ExtractionError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ExtractionError::UnsupportedEncoding)?;
    let mut encoding = EncodingDeclarations::default();
    if let Some(header) = mime_encoding(media_type)? {
        encoding.add(header)?;
    }
    let has_bom = text.starts_with('\u{feff}');
    if has_bom {
        encoding.add(SourceEncoding::Utf8)?;
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let kind = media_type.split(';').next().unwrap_or(media_type).trim();
    if text.chars().any(|ch| {
        ch.is_control()
            && !matches!(ch, '\n' | '\r' | '\t')
            && !(kind.eq_ignore_ascii_case("text/html") && ch == '\u{000c}')
    }) {
        return Err(ExtractionError::UnsupportedEncoding);
    }
    let extracted = if kind.eq_ignore_ascii_case("text/html") {
        crate::research_html::extract(text, encoding, has_bom)?
    } else if kind.eq_ignore_ascii_case("application/xhtml+xml") {
        crate::research_html::extract_xhtml(text, encoding)?
    } else {
        encoding.validate(text, false)?;
        text.to_owned()
    };
    if extracted.trim().is_empty() {
        return Err(ExtractionError::NoText);
    }
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_unicode_text_without_mutating_original_bytes() {
        let bytes = b"\xef\xbb\xbfA caf\xc3\xa9\r\nsecond line".to_vec();
        let before = bytes.clone();
        assert_eq!(
            extract("text/plain", &bytes).unwrap(),
            "A café\r\nsecond line"
        );
        assert_eq!(bytes, before);
        assert_eq!(
            extract("text/markdown", b"# Place\nA harbor").unwrap(),
            "# Place\nA harbor"
        );
        let xhtml = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Archive<br/>entry</p></body></html>"#;
        assert!(
            extract("application/xhtml+xml", xhtml)
                .unwrap()
                .contains("Archive\nentry")
        );
        assert_eq!(
            extractor_version("application/xhtml+xml"),
            Some(XHTML_EXTRACTOR_VERSION)
        );
    }

    #[test]
    fn reports_unsupported_empty_invalid_and_oversized_sources() {
        assert_eq!(
            extract("application/pdf", b"%PDF"),
            Err(ExtractionError::UnsupportedFormat)
        );
        assert_eq!(extract("text/html", b"<p>text</p>").unwrap().trim(), "text");
        assert!(extract("text/html", b"<p>first\x0csecond</p>").is_ok());
        assert_eq!(
            extract("text/plain", &[0xff]),
            Err(ExtractionError::UnsupportedEncoding)
        );
        assert_eq!(
            extract("text/plain", b"a\0b"),
            Err(ExtractionError::UnsupportedEncoding)
        );
        assert_eq!(extract("text/plain", b" \n"), Err(ExtractionError::NoText));
        assert_eq!(
            extract("text/plain", &vec![b'x'; MAX_TEXT_BYTES + 1]),
            Err(ExtractionError::TooLarge)
        );
    }

    #[test]
    fn html_requires_consistent_supported_encoding_evidence() {
        let body = "<p>café</p>".as_bytes();
        assert_eq!(
            extract("text/html", body),
            Err(ExtractionError::UnsupportedEncoding)
        );
        assert!(
            extract("text/html; charset=\"UTF-8\"", body)
                .unwrap()
                .contains("café")
        );
        assert_eq!(
            extract("text/html; charset=windows-1252", body),
            Err(ExtractionError::UnsupportedEncoding)
        );
        assert_eq!(
            extract("text/html; charset=utf-8; charset=utf-8", body),
            Err(ExtractionError::UnsupportedEncoding)
        );
        assert_eq!(
            extract("text/html; charset=us-ascii", body),
            Err(ExtractionError::UnsupportedEncoding)
        );
        assert!(extract("text/html; charset=us-ascii", b"<p>plain</p>").is_ok());
        let with_bom = b"\xef\xbb\xbf<p>plain</p>";
        assert_eq!(
            extract("text/html; charset=us-ascii", with_bom),
            Err(ExtractionError::UnsupportedEncoding)
        );
        assert!(extract("text/html", "<meta charset='UTF-8'><p>café</p>".as_bytes()).is_ok());
        assert!(
            extract(
                "text/html",
                "<meta http-equiv='Content-Type' content='text/html; charset=utf-8'><p>café</p>"
                    .as_bytes()
            )
            .is_ok()
        );
        assert_eq!(
            extract(
                "text/html; charset=utf-8",
                "<meta charset='windows-1252'><p>café</p>".as_bytes()
            ),
            Err(ExtractionError::UnsupportedEncoding)
        );
        for fake in [
            "<!-- <meta charset=utf-8> --><p>café</p>",
            "<script>const fake = '<meta charset=utf-8>';</script><p>café</p>",
            "<template><meta charset=utf-8></template><p>café</p>",
        ] {
            assert_eq!(
                extract("text/html", fake.as_bytes()),
                Err(ExtractionError::UnsupportedEncoding)
            );
        }
        let late = format!("{}<meta charset='utf-8'><p>café</p>", " ".repeat(1024));
        assert_eq!(
            extract("text/html", late.as_bytes()),
            Err(ExtractionError::UnsupportedEncoding)
        );
    }

    #[test]
    fn xhtml_obeys_xml_encoding_declaration_and_does_not_use_html_meta() {
        let prefix = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><meta charset="utf-8"/></head><body>café</body></html>"#;
        // XML defaults to UTF-8 even without a declaration.
        assert!(extract("application/xhtml+xml", prefix.as_bytes()).is_ok());
        let xml = format!("<?xml version=\"1.0\" encoding=\"windows-1252\"?>{prefix}");
        assert_eq!(
            extract("application/xhtml+xml", xml.as_bytes()),
            Err(ExtractionError::UnsupportedEncoding)
        );
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>{prefix}");
        assert!(extract("application/xhtml+xml; charset=utf-8", xml.as_bytes()).is_ok());
        let xml = format!("<?xml version=\"1.0\" encoding=\"US-ASCII\"?>{prefix}");
        assert_eq!(
            extract("application/xhtml+xml", xml.as_bytes()),
            Err(ExtractionError::UnsupportedEncoding)
        );
    }
}
