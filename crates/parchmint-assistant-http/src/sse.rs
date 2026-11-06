//! Bounded framing for provider server-sent events. Event contents are still
//! untrusted and must be interpreted and validated by the provider adapter.

use parchmint_assistant_api::AssistantError;

pub(crate) struct Decoder {
    maximum: usize,
    received: usize,
    events: usize,
    line: Vec<u8>,
    data: Vec<u8>,
    has_data: bool,
    first_line: bool,
    after_cr: bool,
}

impl Decoder {
    pub(crate) fn new(maximum: usize) -> Self {
        Self {
            maximum,
            received: 0,
            events: 0,
            line: Vec::new(),
            data: Vec::new(),
            has_data: false,
            first_line: true,
            after_cr: false,
        }
    }

    pub(crate) fn push(
        &mut self,
        bytes: &[u8],
        mut event: impl FnMut(&str) -> Result<bool, AssistantError>,
    ) -> Result<bool, AssistantError> {
        self.received = self
            .received
            .checked_add(bytes.len())
            .filter(|size| *size <= self.maximum)
            .ok_or(AssistantError::ResponseTooLarge)?;
        for &byte in bytes {
            if self.after_cr {
                self.after_cr = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => {
                    if self.end_line(&mut event)? {
                        return Ok(true);
                    }
                    self.after_cr = true;
                }
                b'\n' => {
                    if self.end_line(&mut event)? {
                        return Ok(true);
                    }
                }
                _ => self.line.push(byte),
            }
        }
        Ok(false)
    }

    fn end_line(
        &mut self,
        event: &mut impl FnMut(&str) -> Result<bool, AssistantError>,
    ) -> Result<bool, AssistantError> {
        let mut line = std::mem::take(&mut self.line);
        if self.first_line {
            self.first_line = false;
            if line.starts_with(&[0xef, 0xbb, 0xbf]) {
                line.drain(..3);
            }
        }
        if line.is_empty() {
            if self.has_data {
                self.events += 1;
                if self.events > 8192 {
                    return Err(AssistantError::ResponseTooLarge);
                }
                self.data.pop(); // Each data field appends a newline.
                let data =
                    std::str::from_utf8(&self.data).map_err(|_| AssistantError::InvalidResponse)?;
                let finished = event(data)?;
                self.data.clear();
                self.has_data = false;
                if finished {
                    return Ok(true);
                }
            }
        } else if line[0] != b':' {
            let separator = line.iter().position(|byte| *byte == b':');
            let (field, value) = match separator {
                Some(index) => (&line[..index], &line[index + 1..]),
                None => (line.as_slice(), &[][..]),
            };
            if field == b"data" {
                self.has_data = true;
                self.data
                    .extend_from_slice(value.strip_prefix(b" ").unwrap_or(value));
                self.data.push(b'\n');
            }
        }
        Ok(false)
    }

    /// A truncated final event is never accepted as a successful response.
    pub(crate) fn finish(&self) -> Result<(), AssistantError> {
        if self.line.is_empty() && !self.has_data {
            Ok(())
        } else {
            Err(AssistantError::InvalidResponse)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_split_utf8_crlf_comments_and_multiline_data() {
        let input =
            "\u{feff}: ping\r\nevent: ignored\r\ndata: café\r\ndata: next\r\n\r\ndata: [DONE]\n\n";
        let mut decoder = Decoder::new(1024);
        let mut events = Vec::new();
        for byte in input.as_bytes() {
            decoder
                .push(std::slice::from_ref(byte), |data| {
                    events.push(data.to_owned());
                    Ok(false)
                })
                .unwrap();
        }
        decoder.finish().unwrap();
        assert_eq!(events, ["café\nnext", "[DONE]"]);
    }

    #[test]
    fn rejects_oversized_invalid_and_truncated_events() {
        let mut decoder = Decoder::new(4);
        assert!(matches!(
            decoder.push(b"12345", |_| Ok(false)),
            Err(AssistantError::ResponseTooLarge)
        ));
        let mut decoder = Decoder::new(100);
        assert!(matches!(
            decoder.push(b"data: \xff\n\n", |_| Ok(false)),
            Err(AssistantError::InvalidResponse)
        ));
        let mut decoder = Decoder::new(100);
        decoder.push(b"data: partial\n", |_| Ok(false)).unwrap();
        assert!(matches!(
            decoder.finish(),
            Err(AssistantError::InvalidResponse)
        ));
    }
}
