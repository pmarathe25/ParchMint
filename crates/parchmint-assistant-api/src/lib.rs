//! Framework-neutral assistant requests and provider results.
//!
//! Callers own context selection, authorization, source validation, and mutations.

use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

/// A credential deliberately excluded from debug output and provider errors.
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistantMode {
    Ask,
    Check,
    Research,
}

/// Conservative intent gate; ambiguous or negated requests do not grant note access.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoteIntent {
    pub read: bool,
    pub create: bool,
}

fn contains_quoted_or_code_text(text: &str) -> bool {
    let characters = text.chars().collect::<Vec<_>>();
    characters.iter().enumerate().any(|(index, character)| {
        if *character == '\'' {
            let inside_word = index > 0
                && index + 1 < characters.len()
                && characters[index - 1].is_alphanumeric()
                && characters[index + 1].is_alphanumeric();
            !inside_word
        } else {
            ['"', '`', '“', '”', '‘', '’', '«', '»'].contains(character)
        }
    })
}

// A colon can introduce a scope restriction as well as a question. Only
// accept forms whose scope is unambiguous; leave other drafts for review.
fn supported_note_read_suffix(prompt: &str) -> bool {
    let Some((_, suffix)) = prompt.split_once(':') else {
        return true;
    };
    let words = suffix
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let words = words.iter().map(String::as_str).collect::<Vec<_>>();
    matches!(
        words.as_slice(),
        ["across", "the" | "my", "project"]
            | ["whole" | "entire", "project"]
            | ["all", "project", "notes"]
    ) || (suffix.trim_end().ends_with('?')
        && matches!(
            words.as_slice(),
            ["what", "do", "they", "say"]
                | ["what", "do", "they", "say", "about", _]
                | ["what", "do", "they", "say", "about", "the", _]
        ))
}

fn supported_note_read_command(words: &[String], start: usize) -> bool {
    let words = words[start..]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    if matches!(
        words.as_slice(),
        [
            "what" | "which" | "who",
            "do",
            "my" | "our" | "the",
            "notes",
            "say"
        ] | [
            "what" | "which" | "who",
            "do",
            "my" | "our" | "the",
            "notes",
            "say",
            "about",
            _
        ] | [
            "what" | "which" | "who",
            "do",
            "my" | "our" | "the",
            "notes",
            "say",
            "about",
            "the",
            _
        ] | ["make", "this", "scene", "better", "using", "my", "notes"]
            | ["use", "my", "notes"]
            | ["answer", "using", "my", "notes"]
    ) {
        return true;
    }
    let Some((verb, tail)) = words.split_first() else {
        return false;
    };
    if ![
        "read",
        "show",
        "find",
        "list",
        "summarize",
        "review",
        "search",
        "consider",
    ]
    .contains(verb)
    {
        return false;
    }
    let tail = match tail {
        ["me", rest @ ..] => rest,
        rest => rest,
    };
    let tail = match tail {
        ["my" | "our" | "the", rest @ ..] => rest,
        rest => rest,
    };
    matches!(
        tail,
        ["note" | "notes"]
            | ["note" | "notes", "across", "the" | "my", "project"]
            | ["all", "project", "notes"]
            | ["note" | "notes", "here"]
            | ["note" | "notes", "about", _]
            | ["note" | "notes", "about", "the", _]
            | [
                "note" | "notes",
                "on" | "in" | "from" | "for" | "to" | "at" | "under" | "of" | "near",
                "this" | "current",
                "document" | "chapter" | "scene" | "passage" | "selection"
            ]
            | [
                "note" | "notes",
                "on" | "in" | "from" | "for" | "to" | "at" | "under" | "of" | "near",
                "the",
                "current",
                "document" | "chapter" | "scene" | "passage" | "selection"
            ]
    )
}

/// Recognize a likely read request so ambiguous instructions can be retained
/// instead of being answered without the notes the author requested.
pub fn note_read_needs_clarification(prompt: &str) -> bool {
    let command = prompt
        .split_once(':')
        .map_or(prompt, |(command, _)| command);
    if contains_quoted_or_code_text(command) {
        return false;
    }
    let words = command
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    if !words.iter().any(|word| word == "note" || word == "notes") {
        return false;
    }
    if words
        .iter()
        .any(|word| ["panel", "sidebar", "tab", "width", "settings"].contains(&word.as_str()))
    {
        return false;
    }
    let words = match words.as_slice() {
        [first, rest @ ..] if first == "please" => rest,
        [first, second, rest @ ..]
            if ["can", "could", "would"].contains(&first.as_str()) && second == "you" =>
        {
            rest
        }
        rest => rest,
    };
    words.first().is_some_and(|word| {
        [
            "read",
            "show",
            "find",
            "list",
            "summarize",
            "review",
            "search",
            "consider",
        ]
        .contains(&word.as_str())
            || (["what", "which", "who"].contains(&word.as_str())
                && words
                    .iter()
                    .any(|word| ["say", "contain", "contents"].contains(&word.as_str())))
            || (["use", "make", "revise", "edit", "check", "write", "answer"]
                .contains(&word.as_str())
                && words
                    .iter()
                    .any(|word| ["using", "use", "consult"].contains(&word.as_str())))
    })
}

pub fn note_intent(prompt: &str) -> NoteIntent {
    let instruction = prompt
        .split_once(':')
        .map_or(prompt, |(command, _)| command);
    if !prompt
        .trim_start()
        .chars()
        .next()
        .is_some_and(char::is_alphabetic)
        || contains_quoted_or_code_text(instruction)
    {
        return NoteIntent::default();
    }
    let words = instruction
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let contains = |word: &str| words.iter().any(|candidate| candidate == word);
    if !(contains("note") || contains("notes"))
        || [
            "not", "never", "without", "dont", "don't", "ignore", "exclude",
        ]
        .iter()
        .any(|word| contains(word))
        || (contains("don") && contains("t"))
    {
        return NoteIntent::default();
    }
    let start = match words
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["please", ..] => 1,
        ["can" | "could" | "would", "you", ..] => 2,
        _ => 0,
    };
    let create = words.get(start).is_some_and(|word| {
        ["add", "create", "make", "write", "save"].contains(&word.as_str())
            && words[start + 1..]
                .iter()
                .take(4)
                .take_while(|word| {
                    [
                        "a", "an", "new", "some", "inline", "document", "anchored", "note", "notes",
                    ]
                    .contains(&word.as_str())
                })
                .any(|word| word == "note" || word == "notes")
    });
    let names_project_notes = words.windows(2).any(|pair| {
        ["my", "our", "project", "these", "the"].contains(&pair[0].as_str())
            && ["note", "notes"].contains(&pair[1].as_str())
    });
    let asks_to_read = words
        .get(start + 1..)
        .unwrap_or_default()
        .iter()
        .take(3)
        .take_while(|word| {
            ["all", "me", "my", "our", "the", "project", "note", "notes"].contains(&word.as_str())
        })
        .any(|word| word == "note" || word == "notes");
    let describes_ui = ["panel", "sidebar", "tab", "width", "settings"]
        .iter()
        .any(|word| contains(word));
    let instructional = words
        .first()
        .is_some_and(|word| ["explain", "describe", "define", "how"].contains(&word.as_str()));
    let first_action = words.get(start).map(String::as_str).unwrap_or("");
    let explicit_read = [
        "read",
        "show",
        "find",
        "list",
        "summarize",
        "review",
        "search",
        "consider",
    ]
    .contains(&first_action)
        && asks_to_read;
    let content_question = ["what", "which", "who"].contains(&first_action)
        && names_project_notes
        && ["say", "contain", "contents"]
            .iter()
            .any(|word| contains(word));
    let use_as_context = ["use", "make", "revise", "edit", "check", "write", "answer"]
        .contains(&first_action)
        && names_project_notes
        && ["using", "use", "consult"]
            .iter()
            .any(|word| contains(word));
    // A colon may introduce the author's question, not just a note body.
    // Check the whole request for negation before granting read access.
    let full_words = prompt
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let full_contains = |word: &str| full_words.iter().any(|candidate| candidate == word);
    let read_negated = [
        "not",
        "never",
        "without",
        "dont",
        "ignore",
        "exclude",
        "excluding",
        "except",
        "excepting",
        "skip",
        "omit",
        "omitting",
    ]
    .iter()
    .any(|word| full_contains(word))
        || (full_contains("don") && full_contains("t"))
        || full_words.windows(2).any(|pair| {
            matches!(
                (pair[0].as_str(), pair[1].as_str()),
                ("other", "than") | ("leave", "out")
            )
        });
    let read = !read_negated
        && !prompt.contains(';')
        && supported_note_read_suffix(prompt)
        && supported_note_read_command(&words, start)
        && !contains_quoted_or_code_text(prompt)
        && !describes_ui
        && !instructional
        && (explicit_read || content_question || use_as_context);
    NoteIntent { read, create }
}

/// Only an explicit current-selection command authorizes an immediate note.
/// Everything after the colon is note content, not a destination clause.
pub fn note_creation_targets_current_selection(prompt: &str) -> bool {
    let Some((command, body)) = prompt.split_once(':') else {
        return false;
    };
    if body.trim().is_empty()
        || !command
            .trim_start()
            .chars()
            .next()
            .is_some_and(char::is_alphabetic)
        || contains_quoted_or_code_text(command)
    {
        return false;
    }
    let words = command
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let words = words.iter().map(String::as_str).collect::<Vec<_>>();
    let words = match words.as_slice() {
        ["please", rest @ ..]
        | ["can", "you", rest @ ..]
        | ["could", "you", rest @ ..]
        | ["would", "you", rest @ ..] => rest,
        rest => rest,
    };
    let Some((verb, rest)) = words.split_first() else {
        return false;
    };
    if !["add", "create", "make", "write", "save"].contains(verb) {
        return false;
    }
    let rest = match rest {
        ["a", tail @ ..] | ["an", tail @ ..] | ["new", tail @ ..] => tail,
        tail => tail,
    };
    matches!(
        rest,
        ["note", "here"]
            | ["note", "at", "the", "cursor"]
            | ["note", "at", "this", "selection"]
            | ["note", "to", "this", "selection"]
            | ["note", "on", "this", "selection"]
            | ["note", "on", "this", "passage"]
            | ["note", "in", "this", "document"]
            | ["note", "on", "this", "scene"]
    )
}

/// Refuse an explicit noncurrent note scope rather than reading the active
/// document's annotations in response to a request for a different document.
pub fn note_read_scope_is_current_or_project(prompt: &str) -> bool {
    let words = prompt
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    // An unqualified named target or an "only" restriction may narrow the
    // requested notes even without a preposition (for example, "Chapter 2
    // only"). Do not substitute the active document or the whole project.
    if words.iter().any(|word| word == "only")
        || words.iter().enumerate().any(|(index, word)| {
            [
                "document",
                "chapter",
                "scene",
                "passage",
                "selection",
                "group",
                "section",
            ]
            .contains(&word.as_str())
                && !index
                    .checked_sub(1)
                    .is_some_and(|previous| ["this", "current"].contains(&words[previous].as_str()))
        })
    {
        return false;
    }
    for (index, word) in words.iter().enumerate() {
        if !["in", "on", "from", "for", "to", "at", "under", "of", "near"].contains(&word.as_str())
        {
            continue;
        }
        let tail = &words[index + 1..];
        let local = matches!(
            tail.iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            [
                "this" | "current",
                "document" | "chapter" | "scene" | "passage" | "selection",
                ..,
            ] | [
                "the",
                "current",
                "document" | "chapter" | "scene" | "passage" | "selection",
                ..,
            ] | ["here", ..]
        );
        if !local {
            return false;
        }
    }
    true
}

/// Project-wide access must be requested in the note command itself, or as a
/// standalone scope after a colon. A later question about the project does not
/// authorize sending notes from other documents.
pub fn note_reads_across_project(prompt: &str) -> bool {
    let (command, suffix) = prompt.split_once(':').unwrap_or((prompt, ""));
    let tokenize = |text: &str| {
        text.split(|character: char| !character.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    };
    let all = tokenize(prompt);
    if all.iter().any(|word| word == "here")
        || all.windows(2).any(|pair| {
            ["this", "current"].contains(&pair[0].as_str())
                && ["document", "chapter", "scene", "passage", "selection"]
                    .contains(&pair[1].as_str())
        })
    {
        return false;
    }
    let is_scope = |words: &[String]| {
        matches!(
            words
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            ["across", "the" | "my", "project"]
                | ["whole" | "entire", "project"]
                | ["all", "project", "notes"]
        )
    };
    let command = tokenize(command);
    let directly_scoped = command
        .windows(4)
        .any(|words| ["note", "notes"].contains(&words[0].as_str()) && is_scope(&words[1..]))
        || command.windows(3).any(|words| {
            (["note", "notes"].contains(&words[0].as_str()) && is_scope(&words[1..]))
                || words
                    .iter()
                    .map(String::as_str)
                    .eq(["all", "project", "notes"])
        });
    directly_scoped || is_scope(&tokenize(suffix))
}

/// Explicit controls grant the whole chosen scope. Refuse note-related
/// wording that appears to request a different or narrower scope.
pub fn explicit_note_scope_conflicts(prompt: &str, project: bool) -> bool {
    let words = prompt
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    if words.iter().any(|word| {
        [
            "only",
            "solely",
            "exclusively",
            "nothing",
            "except",
            "excepting",
            "exclude",
            "excluding",
            "skip",
            "omit",
            "omitting",
            "ignore",
        ]
        .contains(&word.as_str())
    }) || words.windows(2).any(|pair| {
        matches!(
            (pair[0].as_str(), pair[1].as_str()),
            ("other", "than") | ("leave", "out") | ("apart", "from")
        )
    }) {
        return true;
    }
    let mentions_notes = words.iter().any(|word| {
        [
            "note",
            "notes",
            "annotation",
            "annotations",
            "comment",
            "comments",
        ]
        .contains(&word.as_str())
    });
    let references_context = mentions_notes
        || words.iter().any(|word| {
            [
                "these",
                "those",
                "them",
                "context",
                "material",
                "entries",
                "information",
                "private",
                "confidential",
                "consult",
                "consulting",
                "include",
                "including",
                "use",
                "using",
                "read",
                "reading",
                "access",
                "accessing",
                "reveal",
                "revealing",
            ]
            .contains(&word.as_str())
        });
    if references_context
        && words
            .iter()
            .any(|word| ["without", "avoid", "avoiding"].contains(&word.as_str()))
    {
        return true;
    }
    if references_context
        && (words
            .iter()
            .any(|word| ["not", "never", "dont", "no"].contains(&word.as_str()))
            || (words.iter().any(|word| word == "don") && words.iter().any(|word| word == "t")))
    {
        return true;
    }
    if !mentions_notes {
        return false;
    }
    if !note_read_scope_is_current_or_project(prompt) {
        return true;
    }
    let local = words.iter().any(|word| word == "here")
        || words.windows(2).any(|pair| {
            ["this", "current"].contains(&pair[0].as_str())
                && ["document", "chapter", "scene", "passage", "selection"]
                    .contains(&pair[1].as_str())
        });
    let project_mentioned = words.windows(3).any(|part| {
        matches!(
            (part[0].as_str(), part[1].as_str(), part[2].as_str()),
            ("across", "the" | "my", "project") | ("all", "project", "notes")
        )
    }) || words.windows(2).any(|part| {
        matches!(
            (part[0].as_str(), part[1].as_str()),
            ("whole" | "entire", "project")
        )
    });
    (project && local) || (!project && project_mentioned)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectSection {
    Manuscript,
    Lore,
    Research,
    Notes,
}

/// Provenance of an excerpt derived locally from an unchanged Research source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginalSourceEvidence {
    pub source_id: String,
    pub sha256: [u8; 32],
    pub filename: String,
    pub source_url: Option<String>,
    pub retrieved_at_unix_seconds: u64,
    pub extractor: String,
    /// One-based segment number within the extracted original.
    pub segment: u64,
}

/// Immutable, bounded evidence selected by the application for this request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextPassage {
    /// Unique identifier within the request, for example `S1`.
    pub source_id: String,
    pub document_id: String,
    pub document_revision: u64,
    pub section: ProjectSection,
    pub title: String,
    /// An application-owned stable anchor, revalidated before navigation.
    pub anchor: Option<String>,
    pub text: String,
    /// Absent for ordinary authored document passages and notes.
    #[serde(default)]
    pub original: Option<OriginalSourceEvidence>,
}

/// A locally validated possible inconsistency and its captured evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckFinding {
    pub category: CheckCategory,
    pub explanation: String,
    pub uncertainty: Option<String>,
    pub evidence: [ContextPassage; 2],
    /// Locally matched candidate, never supplied by the provider.
    #[serde(skip)]
    pub matched_decision_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckCategory {
    IdentityAppearance,
    TimeTravel,
    ObjectsPossession,
    Rules,
    Relationships,
    Knowledge,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckReport {
    pub summary: String,
    pub findings: Vec<CheckFinding>,
    /// Captured locally from the request; never supplied by the provider.
    #[serde(default)]
    pub scope: Option<CheckScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckScope {
    pub target: String,
    pub evidence_scope: Option<String>,
    pub excluded: Vec<String>,
    pub selection_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub text: String,
}

/// A tool exposed only after the application has decided it is authorized.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssistantRequest {
    pub mode: AssistantMode,
    pub prompt: String,
    pub context: Vec<ContextPassage>,
    pub history: Vec<ConversationMessage>,
    pub tools: Vec<ToolDefinition>,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub project_context: bool,
    pub function_tools: bool,
    pub web_search: bool,
    pub usage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCitation {
    pub source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebCitation {
    pub url: String,
    pub title: String,
    /// Source-associated character offsets in the corresponding answer. A
    /// provider may supply these directly, or associate a whole text block.
    pub start_index: Option<usize>,
    pub end_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Untrusted JSON arguments. The application must validate and authorize them.
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssistantResponse {
    pub text: String,
    pub project_citations: Vec<ProjectCitation>,
    pub unverified_project_citations: Vec<String>,
    pub web_citations: Vec<WebCitation>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantError {
    MissingCredential,
    InvalidConfiguration,
    InvalidRequest,
    ContextUnavailable,
    CheckTargetExcluded,
    NoteTargetExcluded,
    SelectionUnavailable,
    SelectionExcluded,
    SelectionNoteScope,
    UnsupportedCapability,
    Unauthorized,
    Forbidden,
    RateLimited,
    ServiceUnavailable(u16),
    Transport,
    ResponseTooLarge,
    InvalidResponse,
    ResearchNotSearched,
    ResearchContinuationLimit,
    Cancelled,
    BudgetLimitReached,
    BudgetUnavailable,
    ConversationUnavailable,
    ReservationHeld(Box<AssistantError>),
}

impl fmt::Display for AssistantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::ReservationHeld(error) = self {
            return write!(
                f,
                "{error}. The usage reservation remains held; the provider may have charged for this request"
            );
        }
        let description = match self {
            Self::MissingCredential => "No API key has been configured",
            Self::InvalidConfiguration => "The provider configuration is invalid",
            Self::InvalidRequest => "The assistant request is invalid or exceeds its limits",
            Self::ContextUnavailable => {
                "A context item is no longer available. Remove it from Project context before trying again"
            }
            Self::CheckTargetExcluded => {
                "The check target includes excluded material. Remove the exclusion or choose another target"
            }
            Self::NoteTargetExcluded => {
                "The requested note is outside the selected context or is excluded. Change Project context before trying again"
            }
            Self::SelectionUnavailable => {
                "Select up to 32 KiB of text in an open document before sending"
            }
            Self::SelectionExcluded => {
                "The editor selection is excluded. Change Project context before trying again"
            }
            Self::SelectionNoteScope => {
                "Notes cannot be read in selection-only mode. Choose a document or project scope to read notes"
            }
            Self::UnsupportedCapability => "The provider does not support this operation",
            Self::Unauthorized => "The provider rejected the API key",
            Self::Forbidden => "The provider denied this request",
            Self::RateLimited => "The provider rate limit was reached",
            Self::ServiceUnavailable(_) => "The provider could not complete the request",
            Self::Transport => "The provider could not be reached",
            Self::ResponseTooLarge => "The provider response exceeded its limit",
            Self::InvalidResponse => "The provider returned an invalid or incomplete response",
            Self::ResearchNotSearched => {
                "The provider did not complete a web search. Try a more specific research query"
            }
            Self::ResearchContinuationLimit => {
                "The provider paused research too many times; no further requests were sent"
            }
            Self::Cancelled => "The request was cancelled",
            Self::BudgetLimitReached => "The estimated local monthly limit would be exceeded",
            Self::BudgetUnavailable => "The local usage journal is unavailable",
            Self::ConversationUnavailable => {
                "Assistant history could not be saved; the request was not sent"
            }
            Self::ReservationHeld(_) => unreachable!(),
        };
        f.write_str(description)
    }
}
impl Error for AssistantError {}

/// Implementations are blocking and must be called from a worker.
pub trait AssistantProvider: Send + Sync + 'static {
    fn capabilities(&self) -> ProviderCapabilities;
    fn respond(
        &self,
        request: &AssistantRequest,
        credential: &SecretString,
        cancelled: &AtomicBool,
    ) -> Result<AssistantResponse, AssistantError>;
}

pub struct RequestHandle {
    cancelled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<Result<AssistantResponse, AssistantError>>,
}
impl RequestHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn try_recv(
        &self,
    ) -> Result<Result<AssistantResponse, AssistantError>, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

/// Start one bounded provider request on a worker without blocking the UI.
pub fn start_request(
    provider: Arc<dyn AssistantProvider>,
    request: AssistantRequest,
    credential: SecretString,
) -> Result<RequestHandle, AssistantError> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("assistant-request".into())
        .spawn(move || {
            let result = provider.respond(&request, &credential, &worker_cancelled);
            let result = if worker_cancelled.load(Ordering::Acquire) {
                Err(AssistantError::Cancelled)
            } else {
                result
            };
            let _ = sender.send(result);
        })
        .map_err(|_| AssistantError::Transport)?;
    Ok(RequestHandle {
        cancelled,
        receiver,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_permissions_require_an_explicit_non_negated_request() {
        assert_eq!(
            note_intent("What do my notes say about the harbor?"),
            NoteIntent {
                read: true,
                create: false
            }
        );
        assert_eq!(
            note_intent("Create a note about this passage"),
            NoteIntent {
                read: false,
                create: true
            }
        );
        assert_eq!(
            note_intent("Make this scene better using my notes"),
            NoteIntent {
                read: true,
                create: false
            }
        );
        assert_eq!(note_intent("Do not read my notes"), NoteIntent::default());
        assert_eq!(note_intent("Explain musical notes"), NoteIntent::default());
        assert_eq!(
            note_intent("How do I change the notes panel width?"),
            NoteIntent::default()
        );
        assert_eq!(
            note_intent("Explain what 'read my notes' means"),
            NoteIntent::default()
        );
        assert_eq!(
            note_intent("What does 'read my notes' mean?"),
            NoteIntent::default()
        );
        assert_eq!(
            note_intent("\"Read my notes\" is an example command; explain it."),
            NoteIntent::default()
        );
        assert_eq!(
            note_intent("\"Add a note here: check the tide\" — is this good wording?"),
            NoteIntent::default()
        );
        assert_eq!(
            note_intent("Please \"read my notes\" as an example"),
            NoteIntent::default()
        );
        assert_eq!(
            note_intent("Summarize instructions on how to read my notes"),
            NoteIntent::default()
        );
        assert!(note_intent("Read my notes: what do they say about Marin?").read);
        assert!(note_intent("Read my notes: what do they say about the key?").read);
        assert!(!note_intent("Read my notes: only from Chapter 2").read);
        assert!(
            !note_intent("Summarize the notes: use only the text below, not project annotations")
                .read
        );
        assert!(!note_intent("Read notes across the project: except private notes").read);
        assert!(!note_intent("Read notes across the project: skip confidential notes").read);
        assert!(!note_intent("Read notes across the project: avoid confidential notes").read);
        assert!(
            !note_intent(
                "Read notes across the project but avoid confidential notes: what do they say?"
            )
            .read
        );
        assert!(!note_intent("Read notes across the project; avoid confidential notes").read);
        let local_restriction = "Read notes on this scene but avoid confidential notes";
        assert!(!note_intent(local_restriction).read);
        assert!(note_read_needs_clarification(local_restriction));
        assert!(!note_read_needs_clarification("Show the notes tab"));
        assert_eq!(
            note_intent("Explain how to create a note"),
            NoteIntent::default()
        );
        assert_eq!(
            note_intent("What does 'add a note' mean?"),
            NoteIntent::default()
        );
        assert_eq!(
            note_intent("What happened at the harbor?"),
            NoteIntent::default()
        );
    }

    #[test]
    fn creating_a_note_requires_an_explicit_current_target_and_body() {
        assert!(note_creation_targets_current_selection(
            "Add a note here: compare this with Chapter 2"
        ));
        assert!(note_intent("Add a note here: compare this with \"Chapter 2\"").create);
        assert!(note_intent("Add a note here: Mara does not trust them").create);
        assert!(note_creation_targets_current_selection(
            "Please add a note to this selection: verify the date"
        ));
        assert!(!note_creation_targets_current_selection(
            "Add a note to Chapter 2: verify the date"
        ));
        assert!(!note_creation_targets_current_selection(
            "Explain how to create a note here: help"
        ));
        assert!(!note_creation_targets_current_selection(
            "Add a note here and in Chapter 2: check the date"
        ));
        assert!(!note_creation_targets_current_selection(
            "\"Add a note here: check the tide\" — is this good wording?"
        ));
        assert!(!note_creation_targets_current_selection("Add a note here:"));
    }

    #[test]
    fn note_reads_do_not_use_the_active_document_for_a_named_other_target() {
        assert!(note_read_scope_is_current_or_project(
            "Read my notes: what do they say about Marin?"
        ));
        assert!(note_read_scope_is_current_or_project(
            "Read notes on this scene"
        ));
        assert!(!note_read_scope_is_current_or_project(
            "Read notes on this scene: compare with events in Chapter 2"
        ));
        assert!(note_read_scope_is_current_or_project(
            "Summarize my notes across the project"
        ));
        assert!(!note_read_scope_is_current_or_project(
            "Read notes in Chapter 2"
        ));
        assert!(!note_read_scope_is_current_or_project(
            "Show notes from Harbor Ledger"
        ));
        assert!(!note_read_scope_is_current_or_project(
            "Read my notes: only from Chapter 2"
        ));
        assert!(!note_read_scope_is_current_or_project(
            "Read notes across the project: Chapter 2 only"
        ));
        assert!(!note_read_scope_is_current_or_project(
            "Read notes across the project: Chapter 2"
        ));
    }

    #[test]
    fn project_note_scope_requires_explicit_scope_and_respects_local_targets() {
        assert!(note_reads_across_project("Read notes across the project"));
        assert!(note_reads_across_project("Read notes: across the project"));
        assert!(!note_reads_across_project(
            "Read notes on this passage: summarize the rebellion across the project"
        ));
        assert!(!note_reads_across_project(
            "Read my notes here: summarize the entire project"
        ));
        assert!(!note_reads_across_project(
            "Read my notes: what happened across the project?"
        ));
        assert!(!note_reads_across_project(
            "Read my notes about what happened across the project"
        ));
        assert!(explicit_note_scope_conflicts("Do not read my notes", true));
        assert!(explicit_note_scope_conflicts(
            "Answer without consulting annotations",
            true
        ));
        assert!(explicit_note_scope_conflicts(
            "Do not read confidential material; answer from the manuscript",
            true
        ));
        assert!(explicit_note_scope_conflicts(
            "Never access these; answer the question from the story",
            true
        ));
        assert!(explicit_note_scope_conflicts(
            "Use manuscript text only",
            true
        ));
        assert!(explicit_note_scope_conflicts(
            "Summarize these, omitting confidential entries",
            true
        ));
        assert!(explicit_note_scope_conflicts(
            "Use the context but skip anything private",
            true
        ));
        assert!(explicit_note_scope_conflicts(
            "Summarize them without revealing personal details",
            true
        ));
        assert!(explicit_note_scope_conflicts(
            "Read notes on this scene only",
            true
        ));
        assert!(explicit_note_scope_conflicts(
            "Read notes across the project",
            false
        ));
        assert!(!explicit_note_scope_conflicts(
            "What do my notes say about Mara and Ivo?",
            false
        ));
        assert!(!explicit_note_scope_conflicts(
            "Why did Mara not trust Ivo?",
            true
        ));
        assert!(!explicit_note_scope_conflicts(
            "Why did Mara avoid Ivo?",
            true
        ));
    }
}
