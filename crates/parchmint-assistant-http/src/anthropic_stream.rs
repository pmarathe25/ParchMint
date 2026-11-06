//! Assemble a tool-free Anthropic message without exposing thinking blocks.

use std::collections::BTreeMap;

use parchmint_assistant_api::AssistantError;
use serde_json::{Value, json};

enum Block {
    Text { text: String, closed: bool },
    Hidden { closed: bool },
}

impl Block {
    fn closed(&self) -> bool {
        match self {
            Self::Text { closed, .. } | Self::Hidden { closed } => *closed,
        }
    }
    fn close(&mut self) {
        match self {
            Self::Text { closed, .. } | Self::Hidden { closed } => *closed = true,
        }
    }
}

#[derive(Default)]
pub(crate) struct Message {
    started: bool,
    stopped: bool,
    fallback: bool,
    blocks: BTreeMap<u64, Block>,
    stop_reason: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

impl Message {
    pub(crate) fn event(
        &mut self,
        data: &str,
        progress: &mut dyn FnMut(&str),
    ) -> Result<bool, AssistantError> {
        let value: Value =
            serde_json::from_str(data).map_err(|_| AssistantError::InvalidResponse)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or(AssistantError::InvalidResponse)?;
        match kind {
            "message_start" if !self.started => {
                self.started = true;
                self.input_tokens = value
                    .pointer("/message/usage/input_tokens")
                    .and_then(Value::as_u64);
            }
            "content_block_start" if self.started && !self.stopped => {
                let index = index(&value)?;
                if index != self.blocks.len() as u64 {
                    return Err(AssistantError::InvalidResponse);
                }
                let source = value
                    .get("content_block")
                    .ok_or(AssistantError::InvalidResponse)?;
                let block = match source.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let text = source
                            .get("text")
                            .and_then(Value::as_str)
                            .ok_or(AssistantError::InvalidResponse)?
                            .to_owned();
                        if !text.is_empty() {
                            progress(&text);
                        }
                        Block::Text {
                            text,
                            closed: false,
                        }
                    }
                    Some("fallback") => {
                        self.fallback = true;
                        Block::Hidden { closed: false }
                    }
                    Some("thinking" | "redacted_thinking") => Block::Hidden { closed: false },
                    _ => return Err(AssistantError::InvalidResponse),
                };
                self.blocks.insert(index, block);
            }
            "content_block_delta" if self.started && !self.stopped => {
                let block = self
                    .blocks
                    .get_mut(&index(&value)?)
                    .ok_or(AssistantError::InvalidResponse)?;
                if block.closed() {
                    return Err(AssistantError::InvalidResponse);
                }
                let delta = value.get("delta").ok_or(AssistantError::InvalidResponse)?;
                match (block, delta.get("type").and_then(Value::as_str)) {
                    (Block::Text { text, .. }, Some("text_delta")) => {
                        let part = delta
                            .get("text")
                            .and_then(Value::as_str)
                            .ok_or(AssistantError::InvalidResponse)?;
                        text.push_str(part);
                        progress(part);
                    }
                    (Block::Hidden { .. }, Some("thinking_delta" | "signature_delta")) => {}
                    _ => return Err(AssistantError::InvalidResponse),
                }
            }
            "content_block_stop" if self.started && !self.stopped => {
                let block = self
                    .blocks
                    .get_mut(&index(&value)?)
                    .ok_or(AssistantError::InvalidResponse)?;
                if block.closed() {
                    return Err(AssistantError::InvalidResponse);
                }
                block.close();
            }
            "message_delta" if self.started && !self.stopped => {
                if let Some(reason) = value.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(reason.to_owned());
                }
                if let Some(input) = value.pointer("/usage/input_tokens").and_then(Value::as_u64) {
                    if !self.fallback && self.input_tokens.is_some_and(|previous| input < previous)
                    {
                        return Err(AssistantError::InvalidResponse);
                    }
                    self.input_tokens = Some(input);
                }
                if let Some(output) = value
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    if !self.fallback
                        && self.output_tokens.is_some_and(|previous| output < previous)
                    {
                        return Err(AssistantError::InvalidResponse);
                    }
                    self.output_tokens = Some(output);
                }
            }
            "message_stop" if self.started && !self.stopped => {
                if self.blocks.values().any(|block| !block.closed())
                    || self.stop_reason.as_deref() != Some("end_turn")
                {
                    return Err(AssistantError::InvalidResponse);
                }
                self.stopped = true;
                return Ok(true);
            }
            "error" => return Err(AssistantError::InvalidResponse),
            "ping" => {}
            "message_start"
            | "content_block_start"
            | "content_block_delta"
            | "content_block_stop"
            | "message_delta"
            | "message_stop" => return Err(AssistantError::InvalidResponse),
            _ => {} // Ignore future non-content events.
        }
        Ok(false)
    }

    pub(crate) fn finish(self) -> Result<Value, AssistantError> {
        if !self.stopped {
            return Err(AssistantError::InvalidResponse);
        }
        let content: Vec<Value> = self
            .blocks
            .into_values()
            .filter_map(|block| match block {
                Block::Text { text, .. } => Some(json!({"type":"text", "text":text})),
                Block::Hidden { .. } => None,
            })
            .collect();
        let usage = if self.fallback {
            None
        } else {
            self.input_tokens
                .zip(self.output_tokens)
                .map(|(input, output)| json!({"input_tokens":input,"output_tokens":output}))
        };
        Ok(json!({"stop_reason":"end_turn", "content":content, "usage":usage}))
    }
}

fn index(value: &Value) -> Result<u64, AssistantError> {
    value
        .get("index")
        .and_then(Value::as_u64)
        .filter(|index| *index < 128)
        .ok_or(AssistantError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_continues_text_without_claiming_complete_usage() {
        let events = [
            json!({"type":"message_start","message":{"usage":{"input_tokens":20}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Before "}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"fallback"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"text","text":"after"}}),
            json!({"type":"content_block_stop","index":2}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":2,"output_tokens":1}}),
            json!({"type":"message_stop"}),
        ];
        let mut message = Message::default();
        let mut preview = String::new();
        for event in events {
            message
                .event(&event.to_string(), &mut |part| preview.push_str(part))
                .unwrap();
        }
        assert_eq!(preview, "Before after");
        assert!(message.finish().unwrap()["usage"].is_null());
    }
}
