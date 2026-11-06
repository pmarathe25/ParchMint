//! Bounded assembly of one tool-free Chat Completions stream.

use parchmint_assistant_api::AssistantError;
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct Message {
    id: Option<String>,
    text: String,
    finished: bool,
    done: bool,
    usage: Option<Value>,
}

impl Message {
    pub(crate) fn event(
        &mut self,
        data: &str,
        progress: &mut dyn FnMut(&str),
    ) -> Result<bool, AssistantError> {
        if data == "[DONE]" {
            if !self.finished || self.done {
                return Err(AssistantError::InvalidResponse);
            }
            self.done = true;
            return Ok(true);
        }
        if self.done {
            return Err(AssistantError::InvalidResponse);
        }
        let value: Value =
            serde_json::from_str(data).map_err(|_| AssistantError::InvalidResponse)?;
        if value.get("error").is_some_and(|error| !error.is_null()) {
            return Err(AssistantError::InvalidResponse);
        }
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            if self.id.as_deref().is_some_and(|previous| previous != id) {
                return Err(AssistantError::InvalidResponse);
            }
            self.id.get_or_insert_with(|| id.to_owned());
        }
        if let Some(usage) = value.get("usage").filter(|usage| !usage.is_null()) {
            self.usage = Some(usage.clone());
        }
        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .ok_or(AssistantError::InvalidResponse)?;
        if choices.is_empty() {
            return Ok(false);
        }
        if choices.len() != 1 || self.finished {
            return Err(AssistantError::InvalidResponse);
        }
        let choice = &choices[0];
        if choice
            .get("index")
            .is_some_and(|index| index.as_u64() != Some(0))
        {
            return Err(AssistantError::InvalidResponse);
        }
        let delta = choice
            .get("delta")
            .and_then(Value::as_object)
            .ok_or(AssistantError::InvalidResponse)?;
        if delta.get("tool_calls").is_some_and(|calls| {
            !calls.is_null() && calls.as_array().is_none_or(|items| !items.is_empty())
        }) || delta
            .get("function_call")
            .is_some_and(|call| !call.is_null())
            || delta
                .get("refusal")
                .is_some_and(|refusal| !refusal.is_null())
            || delta
                .get("role")
                .is_some_and(|role| !role.is_null() && role.as_str() != Some("assistant"))
        {
            return Err(AssistantError::InvalidResponse);
        }
        if let Some(content) = delta.get("content").filter(|content| !content.is_null()) {
            let part = content.as_str().ok_or(AssistantError::InvalidResponse)?;
            self.text.push_str(part);
            progress(part);
        }
        if let Some(reason) = choice
            .get("finish_reason")
            .filter(|reason| !reason.is_null())
        {
            if reason.as_str() != Some("stop") {
                return Err(AssistantError::InvalidResponse);
            }
            self.finished = true;
        }
        Ok(false)
    }

    pub(crate) fn finish(self) -> Result<Value, AssistantError> {
        if !self.done || !self.finished {
            return Err(AssistantError::InvalidResponse);
        }
        Ok(
            json!({"choices":[{"finish_reason":"stop","message":{"content":self.text}}],"usage":self.usage}),
        )
    }
}
