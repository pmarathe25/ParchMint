#![allow(missing_docs)] // Public lifecycle surface is described in the Stage 03 handoff.
//! Safe lifecycle for one open editor document.

use parchmint_domain::{DocumentId, ProjectGeneration, Revision, WorkStamp};
use parchmint_markdown::{BlockNode, Diagnostic, Document, MarkdownError, ParseOptions};
use parchmint_storage::{
    DocumentSavePlan, OpenProject, PreparedAtomicWrite, ProjectStorage, StorageError, atomic_write,
    prepare_atomic_write, read_document_body_at, read_document_bytes_bounded,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::search::text_statistics;

pub const RECOVERY_FORMAT_VERSION: u32 = 1;

fn utf16_offset_to_byte(value: &str, target: usize) -> Option<usize> {
    if target == 0 {
        return Some(0);
    }
    let mut utf16 = 0usize;
    for (byte, character) in value.char_indices() {
        if utf16 == target {
            return Some(byte);
        }
        utf16 = utf16.checked_add(character.len_utf16())?;
        if utf16 > target {
            return None;
        }
    }
    (utf16 == target).then_some(value.len())
}

fn previous_char_boundary(value: &str, mut byte: usize, count: usize) -> usize {
    for _ in 0..count {
        let Some((index, _)) = value[..byte].char_indices().next_back() else {
            return 0;
        };
        byte = index;
    }
    byte
}

fn next_char_boundary(value: &str, mut byte: usize, count: usize) -> usize {
    for _ in 0..count {
        let Some(character) = value[byte..].chars().next() else {
            return value.len();
        };
        byte = byte.saturating_add(character.len_utf8());
    }
    byte
}

fn signed_difference(after: u64, before: u64) -> i64 {
    if after >= before {
        i64::try_from(after - before).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(before - after).unwrap_or(i64::MAX)
    }
}

#[derive(Clone, Debug)]
pub struct DocumentLifecycleConfig {
    pub journal_debounce: Duration,
    pub rotating_backups: usize,
}

impl Default for DocumentLifecycleConfig {
    fn default() -> Self {
        Self {
            journal_debounce: Duration::from_millis(750),
            rotating_backups: 10,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorMode {
    Wysiwyg,
    Source,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SaveState {
    Saved,
    Dirty,
    Journaling,
    Saving,
    Error(String),
}

/// Aggregate adjustment produced from a bounded editor text replacement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextCountDelta {
    pub words: i64,
    pub characters: i64,
}

/// Result of applying one Qt `contentsChange` payload to the Rust session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppliedTextDelta {
    pub revision: Revision,
    pub counts: TextCountDelta,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContentFingerprint {
    pub bytes: u64,
    pub hash: i64,
}

impl ContentFingerprint {
    pub fn of(source: &str) -> Self {
        // Fixed FNV-1a is deterministic across processes/platforms. This is a
        // change detector, not an authenticity primitive.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in source.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Self {
            bytes: u64::try_from(source.len()).unwrap_or(u64::MAX),
            hash: hash.cast_signed(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DirtyBlocks {
    ranges: Vec<Range<usize>>,
}

impl DirtyBlocks {
    pub fn ranges(&self) -> &[Range<usize>] {
        &self.ranges
    }

    fn insert(&mut self, mut incoming: Range<usize>) {
        if incoming.end < incoming.start {
            std::mem::swap(&mut incoming.start, &mut incoming.end);
        }
        incoming.end = incoming.end.max(incoming.start.saturating_add(1));
        let mut merged = Vec::with_capacity(self.ranges.len() + 1);
        for range in self.ranges.drain(..) {
            if range.end < incoming.start {
                merged.push(range);
            } else if incoming.end < range.start {
                merged.push(incoming.clone());
                incoming = range;
            } else {
                incoming.start = incoming.start.min(range.start);
                incoming.end = incoming.end.max(range.end);
            }
        }
        merged.push(incoming);
        self.ranges = merged;
    }

    fn clear(&mut self) {
        self.ranges.clear();
    }
}

pub struct DocumentSession {
    document_id: DocumentId,
    project_root: PathBuf,
    generation: ProjectGeneration,
    revision: Revision,
    saved_revision: Revision,
    journaled_revision: Revision,
    semantic: Document,
    semantic_dirty: bool,
    body: String,
    parse_options: ParseOptions,
    mode: EditorMode,
    raw_buffer: Option<String>,
    raw_status: SourceParseStatus,
    dirty_blocks: DirtyBlocks,
    last_edit: Option<Instant>,
    save_state: SaveState,
    disk_fingerprint: ContentFingerprint,
    undo_epoch: u64,
    config: DocumentLifecycleConfig,
}

impl DocumentSession {
    pub fn open(
        opened: &OpenProject,
        document_id: DocumentId,
        generation: ProjectGeneration,
        config: DocumentLifecycleConfig,
    ) -> Result<Self, DocumentLifecycleError> {
        let body = opened.body(document_id)?;
        let known_style_ids = opened
            .project
            .styles
            .keys()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        let parse_options = ParseOptions {
            known_style_ids,
            ..ParseOptions::default()
        };
        let semantic = Document::parse_body(body, &parse_options)?;
        Ok(Self {
            document_id,
            project_root: opened.root().to_owned(),
            generation,
            revision: Revision::INITIAL,
            saved_revision: Revision::INITIAL,
            journaled_revision: Revision::INITIAL,
            semantic,
            semantic_dirty: false,
            body: body.to_owned(),
            parse_options,
            mode: EditorMode::Wysiwyg,
            raw_buffer: None,
            raw_status: SourceParseStatus::NotInSourceMode,
            dirty_blocks: DirtyBlocks::default(),
            last_edit: None,
            save_state: SaveState::Saved,
            disk_fingerprint: ContentFingerprint::of(body),
            undo_epoch: 0,
            config,
        })
    }

    pub const fn document_id(&self) -> DocumentId {
        self.document_id
    }

    pub const fn revision(&self) -> Revision {
        self.revision
    }

    pub const fn saved_revision(&self) -> Revision {
        self.saved_revision
    }

    pub const fn journaled_revision(&self) -> Revision {
        self.journaled_revision
    }

    pub const fn mode(&self) -> EditorMode {
        self.mode
    }

    pub const fn undo_epoch(&self) -> u64 {
        self.undo_epoch
    }

    pub fn semantic(&self) -> &Document {
        &self.semantic
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.semantic.diagnostics()
    }

    pub fn raw_buffer(&self) -> Option<&str> {
        self.raw_buffer.as_deref()
    }

    pub fn raw_status(&self) -> &SourceParseStatus {
        &self.raw_status
    }

    pub fn dirty_blocks(&self) -> &DirtyBlocks {
        &self.dirty_blocks
    }

    pub fn save_state(&self) -> &SaveState {
        &self.save_state
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Authoritative live Markdown body projected into any pane referencing
    /// this session. QML editor strings are never the persistence authority.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Replaces the live projection after a Qt text delta. Parsing uses the
    /// same project style and resource options as the original open.
    pub fn replace_body(
        &mut self,
        body: String,
        first_block: usize,
        last_block_exclusive: usize,
        now: Instant,
    ) -> Result<Revision, DocumentLifecycleError> {
        self.require_wysiwyg()?;
        if body == self.body {
            return Ok(self.revision);
        }
        match Document::parse_body(&body, &self.parse_options) {
            Ok(document) => {
                let status = source_parse_status(&document);
                self.semantic = document;
                self.semantic_dirty = false;
                self.raw_status = match status {
                    SourceParseStatus::Invalid { message } => {
                        SourceParseStatus::Invalid { message }
                    }
                    _ => SourceParseStatus::NotInSourceMode,
                };
            }
            Err(error) => {
                // The live string is still authoritative even while it is not
                // valid canonical Markdown. Keep journaling it so a temporary
                // or user-authored syntax error cannot discard acknowledged
                // typing; canonical save remains vetoed until it parses.
                self.raw_status = SourceParseStatus::Invalid {
                    message: error.to_string(),
                };
                self.semantic_dirty = true;
            }
        }
        self.body = body;
        self.record_edit(first_block..last_block_exclusive.max(first_block + 1), now)
    }

    /// Applies a bounded UTF-16 Qt text delta without transporting or parsing
    /// the complete document. Semantic parsing is deferred to the journal/save
    /// boundary; the full live string remains owned by this session.
    pub fn apply_text_delta(
        &mut self,
        position_utf16: usize,
        removed_utf16: usize,
        inserted: &str,
        first_block: usize,
        last_block_exclusive: usize,
        now: Instant,
    ) -> Result<AppliedTextDelta, DocumentLifecycleError> {
        self.require_wysiwyg()?;
        let start = utf16_offset_to_byte(&self.body, position_utf16)
            .ok_or(DocumentLifecycleError::InvalidTextDelta)?;
        let end_utf16 = position_utf16
            .checked_add(removed_utf16)
            .ok_or(DocumentLifecycleError::InvalidTextDelta)?;
        let end = utf16_offset_to_byte(&self.body, end_utf16)
            .ok_or(DocumentLifecycleError::InvalidTextDelta)?;

        let context_start = previous_char_boundary(&self.body, start, 2);
        let context_end = next_char_boundary(&self.body, end, 2);
        let old_words = text_statistics(&self.body[context_start..context_end]).words;
        let removed_characters = self.body[start..end].chars().count();
        let suffix_bytes = context_end.saturating_sub(end);
        self.body.replace_range(start..end, inserted);
        let inserted_end = start.saturating_add(inserted.len());
        let new_context_end = inserted_end
            .saturating_add(suffix_bytes)
            .min(self.body.len());
        let new_words = text_statistics(&self.body[context_start..new_context_end]).words;
        self.semantic_dirty = true;
        self.raw_status = SourceParseStatus::NotInSourceMode;
        let revision = self.record_edit(
            first_block..last_block_exclusive.max(first_block.saturating_add(1)),
            now,
        )?;
        Ok(AppliedTextDelta {
            revision,
            counts: TextCountDelta {
                words: signed_difference(new_words, old_words),
                characters: signed_difference(
                    u64::try_from(inserted.chars().count()).unwrap_or(u64::MAX),
                    u64::try_from(removed_characters).unwrap_or(u64::MAX),
                ),
            },
        })
    }

    pub fn replace_block(
        &mut self,
        index: usize,
        node: BlockNode,
        now: Instant,
    ) -> Result<Revision, DocumentLifecycleError> {
        self.require_current_semantic()?;
        self.semantic.replace_block(index, node)?;
        self.body = self.semantic.serialize_body();
        self.semantic_dirty = false;
        self.record_edit(index..index.saturating_add(1), now)
    }

    pub fn insert_block(
        &mut self,
        index: usize,
        node: BlockNode,
        now: Instant,
    ) -> Result<Revision, DocumentLifecycleError> {
        self.require_current_semantic()?;
        self.semantic.insert_block(index, node)?;
        self.body = self.semantic.serialize_body();
        self.semantic_dirty = false;
        self.record_edit(index..index.saturating_add(1), now)
    }

    pub fn remove_block(
        &mut self,
        index: usize,
        now: Instant,
    ) -> Result<Revision, DocumentLifecycleError> {
        self.require_current_semantic()?;
        self.semantic.remove_block(index)?;
        self.body = self.semantic.serialize_body();
        self.semantic_dirty = false;
        self.record_edit(index..index.saturating_add(1), now)
    }

    /// Records a revisioned incremental range emitted by the Qt adapter.
    pub fn note_editor_delta(
        &mut self,
        first_block: usize,
        last_block_exclusive: usize,
        now: Instant,
    ) -> Result<Revision, DocumentLifecycleError> {
        self.require_wysiwyg()?;
        self.record_edit(first_block..last_block_exclusive, now)
    }

    fn record_edit(
        &mut self,
        dirty: Range<usize>,
        now: Instant,
    ) -> Result<Revision, DocumentLifecycleError> {
        self.revision = self.revision.next()?;
        self.dirty_blocks.insert(dirty);
        self.last_edit = Some(now);
        self.save_state = SaveState::Dirty;
        Ok(self.revision)
    }

    pub fn stamp(&self) -> WorkStamp {
        WorkStamp {
            generation: self.generation,
            revision: self.revision,
        }
    }

    pub fn journal_due(&self, now: Instant) -> bool {
        self.is_dirty()
            && self.journaled_revision < self.revision
            && self.last_edit.is_some_and(|edited| {
                now.saturating_duration_since(edited) >= self.config.journal_debounce
            })
    }

    /// Creates immutable journal work for a worker thread. `force` is used on focus loss.
    pub fn prepare_journal(
        &mut self,
        now: Instant,
        force: bool,
    ) -> Result<Option<JournalRequest>, DocumentLifecycleError> {
        if !self.is_dirty() || self.journaled_revision >= self.revision {
            return Ok(None);
        }
        if !force && !self.journal_due(now) {
            return Ok(None);
        }
        let body = self.current_body_owned();
        let request = JournalRequest::new(
            self.recovery_path(),
            self.stamp(),
            self.document_id,
            self.disk_fingerprint,
            body,
        )?;
        self.save_state = SaveState::Journaling;
        Ok(Some(request))
    }

    pub fn acknowledge_journal(
        &mut self,
        stamp: WorkStamp,
        outcome: Result<(), String>,
    ) -> CompletionDisposition {
        if !stamp.is_current(self.generation, self.revision) {
            return CompletionDisposition::Stale;
        }
        match outcome {
            Ok(()) => {
                self.journaled_revision = stamp.revision;
                self.save_state = SaveState::Dirty;
                CompletionDisposition::Applied
            }
            Err(error) => {
                self.save_state = SaveState::Error(error);
                CompletionDisposition::Applied
            }
        }
    }

    pub fn prepare_canonical_save(
        &mut self,
    ) -> Result<Option<CanonicalSaveRequest>, DocumentLifecycleError> {
        if !self.is_dirty() {
            return Ok(None);
        }
        self.refresh_semantic()?;
        if let SourceParseStatus::Invalid { message } = &self.raw_status {
            let message = message.clone();
            self.save_state = SaveState::Error(format!(
                "The current text is safely journaled but cannot replace the document yet: {message}"
            ));
            return Err(DocumentLifecycleError::InvalidRawSource(message));
        }
        if self.journaled_revision < self.revision {
            return Err(DocumentLifecycleError::JournalRequired(self.revision));
        }
        let request = CanonicalSaveRequest {
            stamp: self.stamp(),
            document_id: self.document_id,
            body: self.current_body_owned(),
            expected_disk_fingerprint: self.disk_fingerprint,
            rotating_backups: self.config.rotating_backups,
        };
        self.save_state = SaveState::Saving;
        Ok(Some(request))
    }

    pub fn acknowledge_canonical_save(
        &mut self,
        stamp: WorkStamp,
        outcome: Result<ContentFingerprint, String>,
    ) -> CompletionDisposition {
        if !stamp.is_current(self.generation, self.revision) {
            return CompletionDisposition::Stale;
        }
        match outcome {
            Ok(fingerprint) => {
                self.saved_revision = stamp.revision;
                self.journaled_revision = stamp.revision;
                self.disk_fingerprint = fingerprint;
                self.dirty_blocks.clear();
                self.last_edit = None;
                self.save_state = SaveState::Saved;
                let _ = self.compact_fulfilled_recovery(stamp.revision);
                CompletionDisposition::Applied
            }
            Err(error) => {
                self.save_state = SaveState::Error(error);
                CompletionDisposition::Applied
            }
        }
    }

    /// Focus-loss and clean-shutdown helper. The caller executes returned work on its worker.
    pub fn prepare_flush(
        &mut self,
        now: Instant,
    ) -> Result<Option<(JournalRequest, Option<CanonicalSaveRequest>)>, DocumentLifecycleError>
    {
        let Some(journal) = self.prepare_journal(now, true)? else {
            return Ok(None);
        };
        let semantic_valid = self.refresh_semantic().is_ok();
        // The caller must execute and acknowledge the journal before executing
        // the save. Capturing the save payload here avoids touching Qt later.
        let save = semantic_valid.then(|| CanonicalSaveRequest {
            stamp: journal.stamp,
            document_id: self.document_id,
            body: self.current_body_owned(),
            expected_disk_fingerprint: self.disk_fingerprint,
            rotating_backups: self.config.rotating_backups,
        });
        Ok(Some((journal, save)))
    }

    pub fn enter_source_mode(&mut self) -> Result<&str, DocumentLifecycleError> {
        self.require_current_semantic()?;
        self.raw_buffer = Some(self.semantic.serialize_body());
        self.raw_status = SourceParseStatus::Valid(self.semantic.diagnostics().to_vec());
        self.mode = EditorMode::Source;
        self.undo_epoch = self.undo_epoch.saturating_add(1);
        Ok(self.raw_buffer.as_deref().unwrap_or_default())
    }

    pub fn update_raw_source(
        &mut self,
        raw: String,
        now: Instant,
    ) -> Result<&SourceParseStatus, DocumentLifecycleError> {
        if self.mode != EditorMode::Source {
            return Err(DocumentLifecycleError::WrongMode);
        }
        self.raw_status = match Document::parse_body(&raw, &self.parse_options) {
            Ok(document) => source_parse_status(&document),
            Err(error) => SourceParseStatus::Invalid {
                message: error.to_string(),
            },
        };
        self.body.clone_from(&raw);
        self.semantic_dirty = true;
        self.raw_buffer = Some(raw);
        self.revision = self.revision.next()?;
        self.dirty_blocks
            .insert(0..self.semantic.blocks().len().max(1));
        self.last_edit = Some(now);
        self.save_state = SaveState::Dirty;
        Ok(&self.raw_status)
    }

    pub fn exit_source_mode(&mut self) -> Result<(), DocumentLifecycleError> {
        if self.mode != EditorMode::Source {
            return Err(DocumentLifecycleError::WrongMode);
        }
        let raw = self.raw_buffer.as_deref().unwrap_or_default();
        let document = Document::parse_body(raw, &self.parse_options).map_err(|error| {
            self.raw_status = SourceParseStatus::Invalid {
                message: error.to_string(),
            };
            DocumentLifecycleError::InvalidRawSource(error.to_string())
        })?;
        if let SourceParseStatus::Invalid { message } = source_parse_status(&document) {
            self.raw_status = SourceParseStatus::Invalid {
                message: message.clone(),
            };
            return Err(DocumentLifecycleError::InvalidRawSource(message));
        }
        self.semantic = document;
        self.semantic_dirty = false;
        self.body = raw.to_owned();
        self.raw_buffer = None;
        self.raw_status = SourceParseStatus::NotInSourceMode;
        self.mode = EditorMode::Wysiwyg;
        self.body = self.semantic.serialize_body();
        self.semantic_dirty = false;
        self.undo_epoch = self.undo_epoch.saturating_add(1);
        Ok(())
    }

    pub fn discard_raw_changes(&mut self) -> Result<(), DocumentLifecycleError> {
        if self.mode != EditorMode::Source {
            return Err(DocumentLifecycleError::WrongMode);
        }
        self.raw_buffer = None;
        self.raw_status = SourceParseStatus::NotInSourceMode;
        self.mode = EditorMode::Wysiwyg;
        self.body = self.semantic.serialize_body();
        self.semantic_dirty = false;
        self.undo_epoch = self.undo_epoch.saturating_add(1);
        Ok(())
    }

    pub fn poll_external_change(
        &mut self,
        opened: &OpenProject,
    ) -> Result<ExternalChange, DocumentLifecycleError> {
        let external = opened.canonical_body_on_disk(self.document_id)?;
        self.observe_external_body(external)
    }

    /// Applies a canonical body read by the project worker. Clean sessions
    /// reload; dirty sessions retain both sides for explicit resolution.
    pub fn observe_external_body(
        &mut self,
        external: String,
    ) -> Result<ExternalChange, DocumentLifecycleError> {
        let fingerprint = ContentFingerprint::of(&external);
        if fingerprint == self.disk_fingerprint {
            return Ok(ExternalChange::Unchanged);
        }
        if self.is_dirty() {
            return Ok(ExternalChange::Conflict(ExternalConflict {
                document_id: self.document_id,
                base_fingerprint: self.disk_fingerprint,
                external_fingerprint: fingerprint,
                local_body: self.current_body_owned(),
                external_body: external,
            }));
        }
        self.semantic = Document::parse_body(&external, &self.parse_options)?;
        self.semantic_dirty = false;
        self.body = external;
        self.revision = self.revision.next()?;
        self.saved_revision = self.revision;
        self.journaled_revision = self.revision;
        self.disk_fingerprint = fingerprint;
        self.dirty_blocks.clear();
        self.undo_epoch = self.undo_epoch.saturating_add(1);
        self.save_state = SaveState::Saved;
        Ok(ExternalChange::AutoReloaded(self.revision))
    }

    pub fn resolve_external_reload(
        &mut self,
        conflict: &ExternalConflict,
    ) -> Result<(), DocumentLifecycleError> {
        self.check_conflict(conflict)?;
        self.semantic = Document::parse_body(&conflict.external_body, &self.parse_options)?;
        self.semantic_dirty = false;
        self.body.clone_from(&conflict.external_body);
        self.revision = self.revision.next()?;
        self.saved_revision = self.revision;
        self.journaled_revision = self.revision;
        self.disk_fingerprint = conflict.external_fingerprint;
        self.dirty_blocks.clear();
        self.save_state = SaveState::Saved;
        self.undo_epoch = self.undo_epoch.saturating_add(1);
        Ok(())
    }

    /// Keeps local content dirty. A later save is the explicit overwrite action.
    pub fn resolve_external_overwrite(
        &mut self,
        conflict: &ExternalConflict,
    ) -> Result<(), DocumentLifecycleError> {
        self.check_conflict(conflict)?;
        self.disk_fingerprint = conflict.external_fingerprint;
        self.save_state = SaveState::Dirty;
        self.undo_epoch = self.undo_epoch.saturating_add(1);
        Ok(())
    }

    pub fn save_conflict_copy(
        &self,
        conflict: &ExternalConflict,
        destination: &Path,
    ) -> Result<(), DocumentLifecycleError> {
        self.check_conflict(conflict)?;
        atomic_write(destination, conflict.local_body.as_bytes())?;
        Ok(())
    }

    fn check_conflict(&self, conflict: &ExternalConflict) -> Result<(), DocumentLifecycleError> {
        if conflict.document_id != self.document_id {
            return Err(DocumentLifecycleError::ConflictDocument);
        }
        Ok(())
    }

    fn current_body_owned(&self) -> String {
        self.body.clone()
    }

    fn require_wysiwyg(&self) -> Result<(), DocumentLifecycleError> {
        if self.mode == EditorMode::Wysiwyg {
            Ok(())
        } else {
            Err(DocumentLifecycleError::WrongMode)
        }
    }

    fn require_current_semantic(&mut self) -> Result<(), DocumentLifecycleError> {
        self.require_wysiwyg()?;
        self.refresh_semantic()
    }

    fn refresh_semantic(&mut self) -> Result<(), DocumentLifecycleError> {
        if !self.semantic_dirty {
            return Ok(());
        }
        let document = Document::parse_body(&self.body, &self.parse_options).map_err(|error| {
            let message = error.to_string();
            self.raw_status = SourceParseStatus::Invalid {
                message: message.clone(),
            };
            self.save_state = SaveState::Error(format!(
                "The current text is safely journaled but cannot replace the document yet: {message}"
            ));
            DocumentLifecycleError::InvalidRawSource(message)
        })?;
        self.raw_status = source_parse_status(&document);
        self.semantic = document;
        self.semantic_dirty = false;
        Ok(())
    }

    fn recovery_path(&self) -> PathBuf {
        self.project_root
            .join(".parchmint/recovery")
            .join(format!("{}.toml", self.document_id))
    }

    fn compact_fulfilled_recovery(&self, saved: Revision) -> Result<(), DocumentLifecycleError> {
        let path = self.recovery_path();
        if !path.is_file() {
            return Ok(());
        }
        let record = RecoveryRecord::read(&path)?;
        if record.revision <= saved.get() {
            fs::remove_file(path).map_err(DocumentLifecycleError::CompactRecovery)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceParseStatus {
    NotInSourceMode,
    Valid(Vec<Diagnostic>),
    Invalid { message: String },
}

fn source_parse_status(document: &Document) -> SourceParseStatus {
    if let Some(diagnostic) = document
        .diagnostics()
        .iter()
        .find(|item| item.severity == parchmint_markdown::DiagnosticSeverity::Error)
    {
        SourceParseStatus::Invalid {
            message: diagnostic.message.clone(),
        }
    } else {
        SourceParseStatus::Valid(document.diagnostics().to_vec())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionDisposition {
    Applied,
    Stale,
}

/// Deterministic persistence boundaries used by crash/full-disk/permission tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceFault {
    JournalBeforeReplacement,
    JournalAfterReplacement,
    CanonicalBeforeBackup,
    CanonicalBeforeWrite,
    CanonicalAfterWrite,
    FullDisk,
    PermissionDenied,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryRecord {
    pub format_version: u32,
    pub project_generation: u64,
    pub document_id: DocumentId,
    pub revision: u64,
    pub base_fingerprint: ContentFingerprint,
    pub body_fingerprint: ContentFingerprint,
    pub created_unix_ms: u64,
    pub body: String,
}

impl RecoveryRecord {
    fn read(path: &Path) -> Result<Self, DocumentLifecycleError> {
        let bytes = fs::read(path).map_err(DocumentLifecycleError::ReadRecovery)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX)
            > parchmint_storage::MAX_DOCUMENT_BYTES + 1024 * 1024
        {
            return Err(DocumentLifecycleError::RecoveryTooLarge);
        }
        let text = std::str::from_utf8(&bytes).map_err(DocumentLifecycleError::RecoveryUtf8)?;
        let record: Self = toml::from_str(text).map_err(DocumentLifecycleError::RecoveryToml)?;
        if record.format_version != RECOVERY_FORMAT_VERSION {
            return Err(DocumentLifecycleError::RecoveryVersion(
                record.format_version,
            ));
        }
        if ContentFingerprint::of(&record.body) != record.body_fingerprint {
            return Err(DocumentLifecycleError::RecoveryFingerprint);
        }
        Ok(record)
    }
}

#[derive(Clone, Debug)]
pub struct JournalRequest {
    pub stamp: WorkStamp,
    pub path: PathBuf,
    bytes: Vec<u8>,
}

impl JournalRequest {
    fn new(
        path: PathBuf,
        stamp: WorkStamp,
        document_id: DocumentId,
        base_fingerprint: ContentFingerprint,
        body: String,
    ) -> Result<Self, DocumentLifecycleError> {
        let created_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        let record = RecoveryRecord {
            format_version: RECOVERY_FORMAT_VERSION,
            project_generation: stamp.generation.get(),
            document_id,
            revision: stamp.revision.get(),
            base_fingerprint,
            body_fingerprint: ContentFingerprint::of(&body),
            created_unix_ms,
            body,
        };
        let bytes = toml::to_string(&record)
            .map_err(DocumentLifecycleError::SerializeRecovery)?
            .into_bytes();
        Ok(Self { stamp, path, bytes })
    }

    pub fn execute(&self) -> Result<(), DocumentLifecycleError> {
        self.execute_with_fault(None)
    }

    pub fn execute_with_fault(
        &self,
        fault: Option<PersistenceFault>,
    ) -> Result<(), DocumentLifecycleError> {
        if matches!(
            fault,
            Some(
                PersistenceFault::JournalBeforeReplacement
                    | PersistenceFault::FullDisk
                    | PersistenceFault::PermissionDenied
            )
        ) {
            return Err(DocumentLifecycleError::InjectedFault(fault.unwrap()));
        }
        atomic_write(&self.path, &self.bytes)?;
        if fault == Some(PersistenceFault::JournalAfterReplacement) {
            return Err(DocumentLifecycleError::InjectedFault(
                PersistenceFault::JournalAfterReplacement,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CanonicalSaveRequest {
    pub stamp: WorkStamp,
    pub document_id: DocumentId,
    pub body: String,
    pub expected_disk_fingerprint: ContentFingerprint,
    pub rotating_backups: usize,
}

struct PreparedCanonicalDiskCommit {
    canonical: PreparedAtomicWrite,
    backup: Option<(PreparedAtomicWrite, PathBuf)>,
}

impl CanonicalSaveRequest {
    /// Freezes storage-owned front matter and the resolved canonical path on
    /// the project owner before this request crosses to its worker.
    pub fn prepare_disk_plan(
        &self,
        opened: &OpenProject,
    ) -> Result<DocumentSavePlan, DocumentLifecycleError> {
        ProjectStorage::prepare_document_save(opened, self.document_id, self.body.clone())
            .map_err(DocumentLifecycleError::Storage)
    }

    /// Executes only bounded filesystem work from an immutable plan. The
    /// caller supplies the latest worker-visible stamp immediately before the
    /// external fingerprint check and mutation.
    pub fn execute_disk(
        &self,
        plan: &DocumentSavePlan,
        current: WorkStamp,
    ) -> Result<ContentFingerprint, DocumentLifecycleError> {
        self.execute_disk_with_fault(plan, current, None)
    }

    pub fn execute_disk_with_fault(
        &self,
        plan: &DocumentSavePlan,
        current: WorkStamp,
        fault: Option<PersistenceFault>,
    ) -> Result<ContentFingerprint, DocumentLifecycleError> {
        let prepared = self.prepare_disk_commit(plan, current, fault)?;
        self.commit_disk(prepared, fault)
    }

    fn prepare_disk_commit(
        &self,
        plan: &DocumentSavePlan,
        current: WorkStamp,
        fault: Option<PersistenceFault>,
    ) -> Result<PreparedCanonicalDiskCommit, DocumentLifecycleError> {
        if self.stamp != current {
            return Err(DocumentLifecycleError::StaleWork(self.stamp));
        }
        let observed = ContentFingerprint::of(&read_document_body_at(
            &plan.canonical_path,
            self.document_id,
        )?);
        if observed != self.expected_disk_fingerprint {
            return Err(DocumentLifecycleError::ExternalChangedDuringSave {
                expected: self.expected_disk_fingerprint,
                observed,
            });
        }
        if matches!(
            fault,
            Some(
                PersistenceFault::CanonicalBeforeBackup
                    | PersistenceFault::FullDisk
                    | PersistenceFault::PermissionDenied
            )
        ) {
            return Err(DocumentLifecycleError::InjectedFault(fault.unwrap()));
        }
        let backup = self.prepare_backup_from_path(&plan.canonical_path)?;
        if fault == Some(PersistenceFault::CanonicalBeforeWrite) {
            return Err(DocumentLifecycleError::InjectedFault(
                PersistenceFault::CanonicalBeforeWrite,
            ));
        }
        let canonical = prepare_atomic_write(&plan.canonical_path, &plan.canonical_bytes)?;
        Ok(PreparedCanonicalDiskCommit { canonical, backup })
    }

    fn commit_disk(
        &self,
        prepared: PreparedCanonicalDiskCommit,
        fault: Option<PersistenceFault>,
    ) -> Result<ContentFingerprint, DocumentLifecycleError> {
        if let Some((backup, directory)) = prepared.backup {
            backup.commit()?;
            rotate(&directory, self.rotating_backups)?;
        }
        prepared.canonical.commit()?;
        if fault == Some(PersistenceFault::CanonicalAfterWrite) {
            return Err(DocumentLifecycleError::InjectedFault(
                PersistenceFault::CanonicalAfterWrite,
            ));
        }
        Ok(ContentFingerprint::of(&self.body))
    }

    fn commit_disk_if_current(
        &self,
        prepared: PreparedCanonicalDiskCommit,
        current: &Mutex<BTreeMap<DocumentId, WorkStamp>>,
    ) -> Result<ContentFingerprint, DocumentLifecycleError> {
        let latest = current
            .lock()
            .map_err(|_| DocumentLifecycleError::WorkerStatePoisoned)?;
        if latest.get(&self.document_id) != Some(&self.stamp) {
            return Err(DocumentLifecycleError::StaleWork(self.stamp));
        }
        self.commit_disk(prepared, None)
    }

    /// Runs on the project worker. `current` is checked immediately before mutation;
    /// one serial worker per project guarantees request ordering after that point.
    pub fn execute(
        &self,
        opened: &mut OpenProject,
        current: WorkStamp,
    ) -> Result<ContentFingerprint, DocumentLifecycleError> {
        self.execute_with_fault(opened, current, None)
    }

    pub fn execute_with_fault(
        &self,
        opened: &mut OpenProject,
        current: WorkStamp,
        fault: Option<PersistenceFault>,
    ) -> Result<ContentFingerprint, DocumentLifecycleError> {
        if self.stamp != current {
            return Err(DocumentLifecycleError::StaleWork(self.stamp));
        }
        let observed = ContentFingerprint::of(&opened.canonical_body_on_disk(self.document_id)?);
        if observed != self.expected_disk_fingerprint {
            return Err(DocumentLifecycleError::ExternalChangedDuringSave {
                expected: self.expected_disk_fingerprint,
                observed,
            });
        }
        if matches!(
            fault,
            Some(
                PersistenceFault::CanonicalBeforeBackup
                    | PersistenceFault::FullDisk
                    | PersistenceFault::PermissionDenied
            )
        ) {
            return Err(DocumentLifecycleError::InjectedFault(fault.unwrap()));
        }
        self.create_backup(opened)?;
        if fault == Some(PersistenceFault::CanonicalBeforeWrite) {
            return Err(DocumentLifecycleError::InjectedFault(
                PersistenceFault::CanonicalBeforeWrite,
            ));
        }
        opened.set_body(self.document_id, self.body.clone())?;
        ProjectStorage::save_document(opened, self.document_id)?;
        if fault == Some(PersistenceFault::CanonicalAfterWrite) {
            return Err(DocumentLifecycleError::InjectedFault(
                PersistenceFault::CanonicalAfterWrite,
            ));
        }
        Ok(ContentFingerprint::of(&self.body))
    }

    fn create_backup(&self, opened: &OpenProject) -> Result<(), DocumentLifecycleError> {
        if self.rotating_backups == 0 {
            return Ok(());
        }
        let record = opened
            .project
            .documents
            .get(&self.document_id)
            .ok_or(DocumentLifecycleError::MissingDocument(self.document_id))?;
        let canonical = parchmint_storage::resolve_project_path(opened.root(), &record.path)?;
        if canonical.is_file() {
            self.create_backup_from_path(&canonical)?;
        }
        Ok(())
    }

    fn create_backup_from_path(&self, canonical: &Path) -> Result<(), DocumentLifecycleError> {
        if let Some((backup, directory)) = self.prepare_backup_from_path(canonical)? {
            backup.commit()?;
            rotate(&directory, self.rotating_backups)?;
        }
        Ok(())
    }

    fn prepare_backup_from_path(
        &self,
        canonical: &Path,
    ) -> Result<Option<(PreparedAtomicWrite, PathBuf)>, DocumentLifecycleError> {
        if self.rotating_backups == 0 || !canonical.is_file() {
            return Ok(None);
        }
        let project_root = canonical
            .ancestors()
            .find(|ancestor| ancestor.join(".parchmint").is_dir())
            .ok_or(DocumentLifecycleError::BackupProjectRoot)?;
        let backup_dir = project_root
            .join(".parchmint/backups")
            .join(self.document_id.to_string());
        let backup = backup_dir.join(format!("{:020}.md", self.stamp.revision.get()));
        let bytes =
            read_document_bytes_bounded(canonical).map_err(DocumentLifecycleError::Storage)?;
        let prepared = prepare_atomic_write(&backup, &bytes)?;
        Ok(Some((prepared, backup_dir)))
    }
}

fn rotate(directory: &Path, retain: usize) -> Result<(), DocumentLifecycleError> {
    let mut entries = fs::read_dir(directory)
        .map_err(DocumentLifecycleError::ReadBackupDirectory)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .collect::<Vec<_>>();
    entries.sort_by_key(fs::DirEntry::file_name);
    let remove = entries.len().saturating_sub(retain);
    for entry in entries.into_iter().take(remove) {
        fs::remove_file(entry.path()).map_err(DocumentLifecycleError::RotateBackup)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentWorkKind {
    Journal,
    Canonical,
    ExternalPoll,
}

#[derive(Debug)]
pub enum DocumentWorkPayload {
    Journaled,
    Saved {
        fingerprint: ContentFingerprint,
        plan: DocumentSavePlan,
    },
    ExternalBody(String),
}

#[derive(Debug)]
pub struct DocumentWorkCompletion {
    pub document_id: DocumentId,
    pub stamp: WorkStamp,
    pub kind: DocumentWorkKind,
    pub outcome: Result<DocumentWorkPayload, String>,
}

enum DocumentWorkJob {
    Journal {
        document_id: DocumentId,
        request: JournalRequest,
    },
    Canonical {
        request: CanonicalSaveRequest,
        plan: DocumentSavePlan,
    },
    ExternalPoll {
        document_id: DocumentId,
        stamp: WorkStamp,
        canonical_path: PathBuf,
    },
}

/// One serial persistence worker per open project. The current-stamp registry
/// is published on every editor delta so delayed requests are rejected before
/// they inspect or replace canonical files.
pub struct DocumentLifecycleWorker {
    jobs: Option<mpsc::Sender<DocumentWorkJob>>,
    results: mpsc::Receiver<DocumentWorkCompletion>,
    current: Arc<Mutex<BTreeMap<DocumentId, WorkStamp>>>,
    worker: Option<JoinHandle<()>>,
}

impl DocumentLifecycleWorker {
    pub fn start(name: &str) -> Result<Self, std::io::Error> {
        let (job_sender, job_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        let current = Arc::new(Mutex::new(BTreeMap::new()));
        let worker_current = Arc::clone(&current);
        let worker = thread::Builder::new()
            .name(name.into())
            .spawn(move || document_worker_loop(&job_receiver, &result_sender, &worker_current))?;
        Ok(Self {
            jobs: Some(job_sender),
            results: result_receiver,
            current,
            worker: Some(worker),
        })
    }

    pub fn publish_current(
        &self,
        document_id: DocumentId,
        stamp: WorkStamp,
    ) -> Result<(), DocumentWorkerError> {
        self.current
            .lock()
            .map_err(|_| DocumentWorkerError::Poisoned)?
            .insert(document_id, stamp);
        Ok(())
    }

    pub fn clear_current(&self) -> Result<(), DocumentWorkerError> {
        self.current
            .lock()
            .map_err(|_| DocumentWorkerError::Poisoned)?
            .clear();
        Ok(())
    }

    pub fn submit_journal(
        &self,
        document_id: DocumentId,
        request: JournalRequest,
    ) -> Result<(), DocumentWorkerError> {
        self.submit(DocumentWorkJob::Journal {
            document_id,
            request,
        })
    }

    pub fn submit_canonical(
        &self,
        request: CanonicalSaveRequest,
        plan: DocumentSavePlan,
    ) -> Result<(), DocumentWorkerError> {
        self.submit(DocumentWorkJob::Canonical { request, plan })
    }

    pub fn submit_external_poll(
        &self,
        document_id: DocumentId,
        stamp: WorkStamp,
        canonical_path: PathBuf,
    ) -> Result<(), DocumentWorkerError> {
        self.submit(DocumentWorkJob::ExternalPoll {
            document_id,
            stamp,
            canonical_path,
        })
    }

    fn submit(&self, job: DocumentWorkJob) -> Result<(), DocumentWorkerError> {
        self.jobs
            .as_ref()
            .ok_or(DocumentWorkerError::Closed)?
            .send(job)
            .map_err(|_| DocumentWorkerError::Closed)
    }

    pub fn try_result(&self) -> Result<Option<DocumentWorkCompletion>, DocumentWorkerError> {
        match self.results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(DocumentWorkerError::Closed),
        }
    }
}

impl Drop for DocumentLifecycleWorker {
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            // Joining an I/O-stalled worker here would defeat the bounded
            // shutdown handshake. Completed workers are reaped; an active
            // worker is detached and process teardown releases it after the
            // current recovery-safe shutdown decision.
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

fn document_worker_loop(
    jobs: &mpsc::Receiver<DocumentWorkJob>,
    results: &mpsc::Sender<DocumentWorkCompletion>,
    current: &Mutex<BTreeMap<DocumentId, WorkStamp>>,
) {
    while let Ok(job) = jobs.recv() {
        let (document_id, stamp, kind, outcome) = match job {
            DocumentWorkJob::Journal {
                document_id,
                request,
            } => {
                let current_stamp = current
                    .lock()
                    .ok()
                    .and_then(|values| values.get(&document_id).copied());
                let outcome = if current_stamp == Some(request.stamp) {
                    request
                        .execute()
                        .map(|()| DocumentWorkPayload::Journaled)
                        .map_err(|error| error.to_string())
                } else {
                    Err(DocumentLifecycleError::StaleWork(request.stamp).to_string())
                };
                (
                    document_id,
                    request.stamp,
                    DocumentWorkKind::Journal,
                    outcome,
                )
            }
            DocumentWorkJob::Canonical { request, plan } => {
                let initial_stamp = current
                    .lock()
                    .ok()
                    .and_then(|values| values.get(&request.document_id).copied())
                    .unwrap_or(WorkStamp {
                        generation: request.stamp.generation,
                        revision: Revision::INITIAL,
                    });
                let outcome = request
                    .prepare_disk_commit(&plan, initial_stamp, None)
                    .and_then(|prepared| {
                        // The expensive reads, serialization, and temporary
                        // writes happened above. Hold the stamp mutex only for
                        // the short authoritative replacements so a newly
                        // published editor revision linearizes either wholly
                        // before or wholly after this commit.
                        request.commit_disk_if_current(prepared, current)
                    })
                    .map(|fingerprint| DocumentWorkPayload::Saved { fingerprint, plan })
                    .map_err(|error| error.to_string());
                (
                    request.document_id,
                    request.stamp,
                    DocumentWorkKind::Canonical,
                    outcome,
                )
            }
            DocumentWorkJob::ExternalPoll {
                document_id,
                stamp,
                canonical_path,
            } => {
                let current_stamp = current
                    .lock()
                    .ok()
                    .and_then(|values| values.get(&document_id).copied());
                let outcome = if current_stamp == Some(stamp) {
                    read_document_body_at(&canonical_path, document_id)
                        .map(DocumentWorkPayload::ExternalBody)
                        .map_err(|error| error.to_string())
                } else {
                    Err(DocumentLifecycleError::StaleWork(stamp).to_string())
                };
                (document_id, stamp, DocumentWorkKind::ExternalPoll, outcome)
            }
        };
        if results
            .send(DocumentWorkCompletion {
                document_id,
                stamp,
                kind,
                outcome,
            })
            .is_err()
        {
            break;
        }
    }
}

#[derive(Debug, Error)]
pub enum DocumentWorkerError {
    #[error("document lifecycle worker is closed")]
    Closed,
    #[error("document lifecycle worker stamp registry is unavailable")]
    Poisoned,
}

#[derive(Clone, Debug)]
pub struct RecoveryCandidate {
    pub path: PathBuf,
    pub record: RecoveryRecord,
    pub canonical_fingerprint: Option<ContentFingerprint>,
}

impl RecoveryCandidate {
    pub fn preview(&self) -> &str {
        &self.record.body
    }

    pub fn is_newer_than_canonical(&self) -> bool {
        self.canonical_fingerprint != Some(self.record.body_fingerprint)
    }

    pub fn discard(self) -> Result<(), DocumentLifecycleError> {
        fs::remove_file(self.path).map_err(DocumentLifecycleError::DiscardRecovery)
    }

    pub fn save_copy(&self, destination: &Path) -> Result<(), DocumentLifecycleError> {
        atomic_write(destination, self.record.body.as_bytes())?;
        Ok(())
    }
}

pub struct RecoveryStore;

#[derive(Clone, Debug)]
pub struct RecoveryIssue {
    pub path: PathBuf,
    pub message: String,
}

impl RecoveryIssue {
    pub fn discard(self) -> Result<(), DocumentLifecycleError> {
        fs::remove_file(self.path).map_err(DocumentLifecycleError::DiscardRecovery)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RecoveryScan {
    pub candidates: Vec<RecoveryCandidate>,
    pub issues: Vec<RecoveryIssue>,
}

impl RecoveryStore {
    pub fn scan(opened: &OpenProject) -> Result<Vec<RecoveryCandidate>, DocumentLifecycleError> {
        Ok(Self::scan_isolated(opened)?.candidates)
    }

    /// Scans every record independently. A malformed or unsupported record is
    /// reported with its path and can never hide another recoverable document.
    pub fn scan_isolated(opened: &OpenProject) -> Result<RecoveryScan, DocumentLifecycleError> {
        let directory = opened.root().join(".parchmint/recovery");
        if !directory.is_dir() {
            return Ok(RecoveryScan::default());
        }
        let mut candidates = Vec::new();
        let mut issues = Vec::new();
        for entry in
            fs::read_dir(directory).map_err(DocumentLifecycleError::ReadRecoveryDirectory)?
        {
            let entry = entry.map_err(DocumentLifecycleError::ReadRecoveryDirectory)?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }
            let record = match RecoveryRecord::read(&path) {
                Ok(record) => record,
                Err(error) => {
                    issues.push(RecoveryIssue {
                        path,
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            let canonical_fingerprint = opened
                .canonical_body_on_disk(record.document_id)
                .ok()
                .map(|body| ContentFingerprint::of(&body));
            if canonical_fingerprint != Some(record.body_fingerprint) {
                candidates.push(RecoveryCandidate {
                    path,
                    record,
                    canonical_fingerprint,
                });
            }
        }
        candidates
            .sort_by_key(|candidate| (candidate.record.document_id, candidate.record.revision));
        issues.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(RecoveryScan { candidates, issues })
    }

    pub fn restore(
        session: &mut DocumentSession,
        candidate: &RecoveryCandidate,
        now: Instant,
    ) -> Result<Revision, DocumentLifecycleError> {
        if session.document_id != candidate.record.document_id {
            return Err(DocumentLifecycleError::ConflictDocument);
        }
        session.semantic = Document::parse_body(&candidate.record.body, &session.parse_options)?;
        session.semantic_dirty = false;
        session.body.clone_from(&candidate.record.body);
        session.mode = EditorMode::Wysiwyg;
        session.raw_buffer = None;
        session.raw_status = SourceParseStatus::NotInSourceMode;
        session.revision = session.revision.next()?;
        session
            .dirty_blocks
            .insert(0..session.semantic.blocks().len().max(1));
        session.last_edit = Some(now);
        session.save_state = SaveState::Dirty;
        session.undo_epoch = session.undo_epoch.saturating_add(1);
        Ok(session.revision)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalChange {
    Unchanged,
    AutoReloaded(Revision),
    Conflict(ExternalConflict),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalConflict {
    pub document_id: DocumentId,
    pub base_fingerprint: ContentFingerprint,
    pub external_fingerprint: ContentFingerprint,
    pub local_body: String,
    pub external_body: String,
}

#[derive(Debug, Error)]
pub enum DocumentLifecycleError {
    #[error(transparent)]
    Markdown(#[from] MarkdownError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Atomic(#[from] parchmint_storage::AtomicWriteError),
    #[error(transparent)]
    Revision(#[from] parchmint_domain::RevisionError),
    #[error("operation is not valid in the current editor mode")]
    WrongMode,
    #[error("editor text delta does not align with the current UTF-16 document")]
    InvalidTextDelta,
    #[error("raw source is invalid and remains open: {0}")]
    InvalidRawSource(String),
    #[error("revision {0:?} must be journaled before canonical save")]
    JournalRequired(Revision),
    #[error("background work is stale: {0}")]
    StaleWork(WorkStamp),
    #[error("document worker revision state is unavailable")]
    WorkerStatePoisoned,
    #[error(
        "canonical document changed again while saving (expected {expected:?}, observed {observed:?})"
    )]
    ExternalChangedDuringSave {
        expected: ContentFingerprint,
        observed: ContentFingerprint,
    },
    #[error("injected persistence fault at {0:?}")]
    InjectedFault(PersistenceFault),
    #[error("conflict belongs to a different document")]
    ConflictDocument,
    #[error("document is absent from the project: {0}")]
    MissingDocument(DocumentId),
    #[error("could not serialize recovery record: {0}")]
    SerializeRecovery(toml::ser::Error),
    #[error("could not read recovery record: {0}")]
    ReadRecovery(std::io::Error),
    #[error("recovery record is not UTF-8: {0}")]
    RecoveryUtf8(std::str::Utf8Error),
    #[error("invalid recovery record: {0}")]
    RecoveryToml(toml::de::Error),
    #[error("unsupported recovery format {0}")]
    RecoveryVersion(u32),
    #[error("recovery record is too large")]
    RecoveryTooLarge,
    #[error("recovery body fingerprint does not match its payload")]
    RecoveryFingerprint,
    #[error("could not list recovery records: {0}")]
    ReadRecoveryDirectory(std::io::Error),
    #[error("could not compact fulfilled recovery record: {0}")]
    CompactRecovery(std::io::Error),
    #[error("could not discard recovery record: {0}")]
    DiscardRecovery(std::io::Error),
    #[error("could not read canonical backup source: {0}")]
    ReadBackupSource(std::io::Error),
    #[error("could not identify the project root for a canonical backup")]
    BackupProjectRoot,
    #[error("could not list rotating backups: {0}")]
    ReadBackupDirectory(std::io::Error),
    #[error("could not rotate old backup: {0}")]
    RotateBackup(std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_domain::{
        DocumentMetadata, DocumentRecord, Node, NodeId, NodeKind, ProjectCommand,
        RelativeProjectPath,
    };
    use parchmint_markdown::{Attributes, Inline};
    use parchmint_storage::ProjectStorage;
    use tempfile::tempdir;

    fn project_with_document() -> (tempfile::TempDir, OpenProject, DocumentId) {
        let directory = tempdir().unwrap();
        let mut opened = ProjectStorage::create(directory.path(), "Lifecycle").unwrap();
        let node_id = NodeId::new();
        let document_id = DocumentId::new();
        opened
            .execute(ProjectCommand::Create {
                parent: opened.project.manuscript_root(),
                node: Node {
                    id: node_id,
                    kind: NodeKind::Document { document_id },
                    parent: Some(opened.project.manuscript_root()),
                    children: Vec::new(),
                },
                document: DocumentRecord {
                    id: document_id,
                    node_id,
                    path: RelativeProjectPath::new(format!("manuscript/{node_id}.md")).unwrap(),
                    metadata: DocumentMetadata {
                        title: "Chapter".into(),
                        ..DocumentMetadata::default()
                    },
                },
                index: 0,
            })
            .unwrap();
        opened.set_body(document_id, "Original.\n".into()).unwrap();
        ProjectStorage::save(&mut opened).unwrap();
        (directory, opened, document_id)
    }

    fn replacement(text: &str) -> BlockNode {
        BlockNode::Paragraph {
            content: vec![Inline::Text(text.into())],
            attributes: Attributes::default(),
        }
    }

    fn wait_for_worker(worker: &DocumentLifecycleWorker) -> DocumentWorkCompletion {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(completion) = worker.try_result().unwrap() {
                return completion;
            }
            assert!(Instant::now() < deadline, "document worker timed out");
            std::thread::yield_now();
        }
    }

    #[test]
    fn journal_precedes_save_and_stale_completions_never_acknowledge() {
        let (_directory, mut opened, id) = project_with_document();
        let generation = ProjectGeneration::new(1).unwrap();
        let config = DocumentLifecycleConfig {
            journal_debounce: Duration::ZERO,
            rotating_backups: 2,
        };
        let mut session = DocumentSession::open(&opened, id, generation, config).unwrap();
        let now = Instant::now();
        session.replace_block(0, replacement("First"), now).unwrap();
        assert!(matches!(
            session.prepare_canonical_save(),
            Err(DocumentLifecycleError::JournalRequired(_))
        ));
        let journal = session.prepare_journal(now, false).unwrap().unwrap();
        journal.execute().unwrap();
        assert_eq!(
            session.acknowledge_journal(journal.stamp, Ok(())),
            CompletionDisposition::Applied
        );
        let save = session.prepare_canonical_save().unwrap().unwrap();
        session.replace_block(0, replacement("Newer"), now).unwrap();
        assert_eq!(
            session.acknowledge_canonical_save(save.stamp, Ok(ContentFingerprint::of(&save.body))),
            CompletionDisposition::Stale
        );
        assert!(session.is_dirty());
        assert!(matches!(
            save.execute(&mut opened, session.stamp()),
            Err(DocumentLifecycleError::StaleWork(_))
        ));
    }

    #[test]
    fn recovery_survives_restart_and_can_restore_preview_or_copy() {
        let (directory, opened, id) = project_with_document();
        let generation = ProjectGeneration::new(2).unwrap();
        let mut session =
            DocumentSession::open(&opened, id, generation, DocumentLifecycleConfig::default())
                .unwrap();
        let now = Instant::now();
        session
            .replace_block(0, replacement("Recovered"), now)
            .unwrap();
        let journal = session.prepare_journal(now, true).unwrap().unwrap();
        journal.execute().unwrap();
        let candidates = RecoveryStore::scan(&opened).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].preview().contains("Recovered"));
        let copy = directory.path().join("recovered-copy.md");
        candidates[0].save_copy(&copy).unwrap();
        assert!(fs::read_to_string(copy).unwrap().contains("Recovered"));
        RecoveryStore::restore(&mut session, &candidates[0], now).unwrap();
        assert!(session.is_dirty());
    }

    #[test]
    fn corrupt_recovery_is_isolated_from_valid_candidates() {
        let (directory, opened, id) = project_with_document();
        let mut session = DocumentSession::open(
            &opened,
            id,
            ProjectGeneration::new(20).unwrap(),
            DocumentLifecycleConfig::default(),
        )
        .unwrap();
        session
            .replace_block(0, replacement("Recover me"), Instant::now())
            .unwrap();
        session
            .prepare_journal(Instant::now(), true)
            .unwrap()
            .unwrap()
            .execute()
            .unwrap();
        let corrupt = directory.path().join(".parchmint/recovery/corrupt.toml");
        fs::write(&corrupt, b"this is not a recovery record").unwrap();
        let scan = RecoveryStore::scan_isolated(&opened).unwrap();
        assert_eq!(scan.candidates.len(), 1);
        assert_eq!(scan.candidates[0].record.document_id, id);
        assert_eq!(scan.issues.len(), 1);
        assert_eq!(scan.issues[0].path, corrupt);
    }

    #[test]
    fn serial_worker_rejects_delayed_stale_journal_and_saves_latest_revision() {
        let (_directory, mut opened, id) = project_with_document();
        let generation = ProjectGeneration::new(21).unwrap();
        let mut session = DocumentSession::open(
            &opened,
            id,
            generation,
            DocumentLifecycleConfig {
                journal_debounce: Duration::ZERO,
                rotating_backups: 1,
            },
        )
        .unwrap();
        let worker = DocumentLifecycleWorker::start("document-worker-test").unwrap();
        session
            .replace_block(0, replacement("Old revision"), Instant::now())
            .unwrap();
        let stale = session
            .prepare_journal(Instant::now(), true)
            .unwrap()
            .unwrap();
        session
            .replace_block(0, replacement("Latest revision"), Instant::now())
            .unwrap();
        worker.publish_current(id, session.stamp()).unwrap();
        worker.submit_journal(id, stale).unwrap();
        let completion = wait_for_worker(&worker);
        assert_eq!(completion.kind, DocumentWorkKind::Journal);
        assert!(completion.outcome.is_err());
        assert!(session.is_dirty());

        let journal = session
            .prepare_journal(Instant::now(), true)
            .unwrap()
            .unwrap();
        worker.publish_current(id, journal.stamp).unwrap();
        worker.submit_journal(id, journal).unwrap();
        let completion = wait_for_worker(&worker);
        assert!(completion.outcome.is_ok());
        session.acknowledge_journal(completion.stamp, Ok(()));
        let save = session.prepare_canonical_save().unwrap().unwrap();
        let plan = save.prepare_disk_plan(&opened).unwrap();
        worker.publish_current(id, save.stamp).unwrap();
        worker.submit_canonical(save, plan).unwrap();
        let completion = wait_for_worker(&worker);
        let DocumentWorkPayload::Saved { fingerprint, plan } = completion.outcome.unwrap() else {
            panic!("canonical worker returned unexpected payload")
        };
        ProjectStorage::acknowledge_document_save(&mut opened, &plan).unwrap();
        session.acknowledge_canonical_save(completion.stamp, Ok(fingerprint));
        assert!(!session.is_dirty());
        assert!(
            opened
                .canonical_body_on_disk(id)
                .unwrap()
                .contains("Latest revision")
        );
    }

    #[test]
    fn canonical_commit_rechecks_revision_after_temporary_write() {
        let (_directory, opened, id) = project_with_document();
        let mut session = DocumentSession::open(
            &opened,
            id,
            ProjectGeneration::new(23).unwrap(),
            DocumentLifecycleConfig {
                journal_debounce: Duration::ZERO,
                rotating_backups: 1,
            },
        )
        .unwrap();
        session
            .replace_block(0, replacement("Prepared old revision"), Instant::now())
            .unwrap();
        let journal = session
            .prepare_journal(Instant::now(), true)
            .unwrap()
            .unwrap();
        journal.execute().unwrap();
        session.acknowledge_journal(journal.stamp, Ok(()));
        let save = session.prepare_canonical_save().unwrap().unwrap();
        let plan = save.prepare_disk_plan(&opened).unwrap();
        let prepared = save.prepare_disk_commit(&plan, save.stamp, None).unwrap();

        session
            .replace_block(0, replacement("Newer live revision"), Instant::now())
            .unwrap();
        let current = Mutex::new(BTreeMap::from([(id, session.stamp())]));
        assert!(matches!(
            save.commit_disk_if_current(prepared, &current),
            Err(DocumentLifecycleError::StaleWork(_))
        ));
        assert!(
            opened
                .canonical_body_on_disk(id)
                .unwrap()
                .contains("Original."),
            "a stale prepared artifact must never replace canonical content"
        );
    }

    #[test]
    fn backup_source_is_bounded_before_copy() {
        let (directory, _opened, id) = project_with_document();
        let oversized = directory.path().join("oversized.md");
        let file = fs::File::create(&oversized).unwrap();
        file.set_len(parchmint_storage::MAX_DOCUMENT_BYTES + 1)
            .unwrap();
        let request = CanonicalSaveRequest {
            stamp: WorkStamp {
                generation: ProjectGeneration::new(22).unwrap(),
                revision: Revision::new(1),
            },
            document_id: id,
            body: "body".into(),
            expected_disk_fingerprint: ContentFingerprint::of(""),
            rotating_backups: 1,
        };
        assert!(matches!(
            request.create_backup_from_path(&oversized),
            Err(DocumentLifecycleError::Storage(StorageError::SizeLimit(
                "document",
                parchmint_storage::MAX_DOCUMENT_BYTES
            )))
        ));
    }

    #[test]
    fn invalid_raw_buffer_is_retained_until_explicit_resolution() {
        let (_directory, opened, id) = project_with_document();
        let mut session = DocumentSession::open(
            &opened,
            id,
            ProjectGeneration::new(3).unwrap(),
            DocumentLifecycleConfig::default(),
        )
        .unwrap();
        session.enter_source_mode().unwrap();
        session
            .update_raw_source("```\nunclosed".into(), Instant::now())
            .unwrap();
        assert!(matches!(
            session.raw_status(),
            SourceParseStatus::Invalid { .. }
        ));
        assert!(session.exit_source_mode().is_err());
        assert_eq!(session.mode(), EditorMode::Source);
        assert_eq!(session.raw_buffer(), Some("```\nunclosed"));
        let journal = session
            .prepare_journal(Instant::now(), true)
            .unwrap()
            .unwrap();
        journal.execute().unwrap();
        session.acknowledge_journal(journal.stamp, Ok(()));
        assert!(matches!(
            session.prepare_canonical_save(),
            Err(DocumentLifecycleError::InvalidRawSource(_))
        ));
        session.discard_raw_changes().unwrap();
        assert_eq!(session.mode(), EditorMode::Wysiwyg);
    }

    #[test]
    fn utf16_text_delta_updates_only_the_changed_range_and_exact_counts() {
        let (_directory, opened, id) = project_with_document();
        let mut session = DocumentSession::open(
            &opened,
            id,
            ProjectGeneration::new(3).unwrap(),
            DocumentLifecycleConfig::default(),
        )
        .unwrap();
        let applied = session
            .apply_text_delta(0, 8, "雪 and river", 0, 1, Instant::now())
            .unwrap();
        assert_eq!(session.body(), "雪 and river.\n");
        assert_eq!(applied.counts.words, 2);
        assert_eq!(applied.counts.characters, 3);
        assert!(session.is_dirty());
    }

    #[test]
    fn invalid_live_wysiwyg_text_is_journaled_and_vetoes_canonical_save() {
        let (_directory, opened, id) = project_with_document();
        let mut session = DocumentSession::open(
            &opened,
            id,
            ProjectGeneration::new(31).unwrap(),
            DocumentLifecycleConfig::default(),
        )
        .unwrap();
        let invalid = "```\nunclosed";
        session
            .replace_body(invalid.into(), 0, 1, Instant::now())
            .unwrap();
        assert_eq!(session.body(), invalid);
        assert!(matches!(
            session.raw_status(),
            SourceParseStatus::Invalid { .. }
        ));

        let journal = session
            .prepare_journal(Instant::now(), true)
            .unwrap()
            .unwrap();
        journal.execute().unwrap();
        let recovered = RecoveryRecord::read(&journal.path).unwrap();
        assert_eq!(recovered.body, invalid);
        session.acknowledge_journal(journal.stamp, Ok(()));
        assert!(matches!(
            session.prepare_canonical_save(),
            Err(DocumentLifecycleError::InvalidRawSource(_))
        ));
        assert!(matches!(session.save_state(), SaveState::Error(_)));
    }

    #[test]
    fn external_change_auto_reloads_clean_but_conflicts_with_dirty() {
        let (directory, mut opened, id) = project_with_document();
        let mut session = DocumentSession::open(
            &opened,
            id,
            ProjectGeneration::new(4).unwrap(),
            DocumentLifecycleConfig::default(),
        )
        .unwrap();
        let record = &opened.project.documents[&id];
        let path = parchmint_storage::resolve_project_path(opened.root(), &record.path).unwrap();
        let source = fs::read_to_string(&path)
            .unwrap()
            .replace("Original.", "External.");
        atomic_write(&path, source.as_bytes()).unwrap();
        assert!(matches!(
            session.poll_external_change(&opened).unwrap(),
            ExternalChange::AutoReloaded(_)
        ));
        session
            .replace_block(0, replacement("Local"), Instant::now())
            .unwrap();
        let source = fs::read_to_string(&path)
            .unwrap()
            .replace("External.", "Again.");
        atomic_write(&path, source.as_bytes()).unwrap();
        let ExternalChange::Conflict(conflict) = session.poll_external_change(&opened).unwrap()
        else {
            panic!("dirty external edit must conflict")
        };
        assert!(conflict.local_body.contains("Local"));
        assert!(conflict.external_body.contains("Again"));
        let copy = directory.path().join("local-conflict-copy.md");
        session.save_conflict_copy(&conflict, &copy).unwrap();
        assert!(fs::read_to_string(copy).unwrap().contains("Local"));
        session.resolve_external_reload(&conflict).unwrap();
        assert!(session.body().contains("Again"));
        assert!(!session.is_dirty());

        session
            .replace_block(0, replacement("Local overwrite"), Instant::now())
            .unwrap();
        let source = fs::read_to_string(&path)
            .unwrap()
            .replace("Again.", "Second.");
        atomic_write(&path, source.as_bytes()).unwrap();
        let ExternalChange::Conflict(conflict) = session.poll_external_change(&opened).unwrap()
        else {
            panic!("second dirty external edit must conflict")
        };
        session.resolve_external_overwrite(&conflict).unwrap();
        let journal = session
            .prepare_journal(Instant::now(), true)
            .unwrap()
            .unwrap();
        journal.execute().unwrap();
        session.acknowledge_journal(journal.stamp, Ok(()));
        let save = session.prepare_canonical_save().unwrap().unwrap();
        let source = fs::read_to_string(&path)
            .unwrap()
            .replace("Second.", "Third.");
        atomic_write(&path, source.as_bytes()).unwrap();
        assert!(matches!(
            save.execute(&mut opened, save.stamp),
            Err(DocumentLifecycleError::ExternalChangedDuringSave { .. })
        ));
    }

    #[test]
    fn injected_journal_and_canonical_boundaries_preserve_recoverability() {
        let (_directory, mut opened, id) = project_with_document();
        let mut session = DocumentSession::open(
            &opened,
            id,
            ProjectGeneration::new(5).unwrap(),
            DocumentLifecycleConfig {
                journal_debounce: Duration::ZERO,
                rotating_backups: 2,
            },
        )
        .unwrap();
        let now = Instant::now();
        session
            .replace_block(0, replacement("Durable"), now)
            .unwrap();
        let journal = session.prepare_journal(now, true).unwrap().unwrap();
        for fault in [
            PersistenceFault::JournalBeforeReplacement,
            PersistenceFault::FullDisk,
            PersistenceFault::PermissionDenied,
        ] {
            assert!(journal.execute_with_fault(Some(fault)).is_err());
        }
        assert!(
            journal
                .execute_with_fault(Some(PersistenceFault::JournalAfterReplacement))
                .is_err()
        );
        assert_eq!(RecoveryStore::scan(&opened).unwrap().len(), 1);
        session.acknowledge_journal(journal.stamp, Ok(()));
        let save = session.prepare_canonical_save().unwrap().unwrap();
        let original = opened.body(id).unwrap().to_owned();
        for fault in [
            PersistenceFault::CanonicalBeforeBackup,
            PersistenceFault::CanonicalBeforeWrite,
            PersistenceFault::FullDisk,
            PersistenceFault::PermissionDenied,
        ] {
            assert!(
                save.execute_with_fault(&mut opened, save.stamp, Some(fault))
                    .is_err()
            );
            assert_eq!(opened.body(id).unwrap(), original);
        }
        assert!(
            save.execute_with_fault(
                &mut opened,
                save.stamp,
                Some(PersistenceFault::CanonicalAfterWrite),
            )
            .is_err()
        );
        assert!(opened.body(id).unwrap().contains("Durable"));
        assert!(
            session.is_dirty(),
            "failed completion must not acknowledge save"
        );
    }

    #[test]
    fn acknowledged_save_is_canonical_compacts_recovery_and_keeps_backup() {
        let (directory, mut opened, id) = project_with_document();
        let other_node = NodeId::new();
        let other_document = DocumentId::new();
        let other_path = RelativeProjectPath::new(format!("manuscript/{other_node}.md")).unwrap();
        opened
            .execute(ProjectCommand::Create {
                parent: opened.project.manuscript_root(),
                node: Node {
                    id: other_node,
                    kind: NodeKind::Document {
                        document_id: other_document,
                    },
                    parent: Some(opened.project.manuscript_root()),
                    children: Vec::new(),
                },
                document: DocumentRecord {
                    id: other_document,
                    node_id: other_node,
                    path: other_path.clone(),
                    metadata: DocumentMetadata {
                        title: "Other".into(),
                        ..DocumentMetadata::default()
                    },
                },
                index: 1,
            })
            .unwrap();
        opened
            .set_body(other_document, "Unrelated original.\n".into())
            .unwrap();
        ProjectStorage::save(&mut opened).unwrap();
        let other_canonical = directory.path().join(other_path.as_str());
        let externally_changed = fs::read_to_string(&other_canonical)
            .unwrap()
            .replace("Unrelated original.", "Unrelated external.");
        atomic_write(&other_canonical, externally_changed.as_bytes()).unwrap();
        let mut session = DocumentSession::open(
            &opened,
            id,
            ProjectGeneration::new(6).unwrap(),
            DocumentLifecycleConfig {
                journal_debounce: Duration::ZERO,
                rotating_backups: 1,
            },
        )
        .unwrap();
        let now = Instant::now();
        session
            .replace_block(0, replacement("Acknowledged"), now)
            .unwrap();
        let journal = session.prepare_journal(now, true).unwrap().unwrap();
        journal.execute().unwrap();
        session.acknowledge_journal(journal.stamp, Ok(()));
        let save = session.prepare_canonical_save().unwrap().unwrap();
        let fingerprint = save.execute(&mut opened, save.stamp).unwrap();
        session.acknowledge_canonical_save(save.stamp, Ok(fingerprint));
        assert!(!session.is_dirty());
        assert!(
            opened
                .canonical_body_on_disk(id)
                .unwrap()
                .contains("Acknowledged")
        );
        assert!(
            opened
                .canonical_body_on_disk(other_document)
                .unwrap()
                .contains("Unrelated external."),
            "saving one editor document must not rewrite an external edit in another"
        );
        assert!(!journal.path.exists());
        let backup = directory
            .path()
            .join(".parchmint/backups")
            .join(id.to_string());
        assert_eq!(fs::read_dir(backup).unwrap().count(), 1);
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore = "release-mode Stage 14 performance gate")]
    fn records_large_document_journal_and_save_latency() {
        let (_directory, mut opened, id) = project_with_document();
        let mut large = String::with_capacity(1_500_000);
        for word in 0..250_000 {
            large.push_str("orchard ");
            if word % 40 == 39 {
                large.push('\n');
                large.push('\n');
            }
        }
        opened.set_body(id, large).unwrap();
        ProjectStorage::save(&mut opened).unwrap();
        let load_start = Instant::now();
        let mut session = DocumentSession::open(
            &opened,
            id,
            ProjectGeneration::new(7).unwrap(),
            DocumentLifecycleConfig {
                journal_debounce: Duration::ZERO,
                rotating_backups: 2,
            },
        )
        .unwrap();
        let load = load_start.elapsed();
        let ui_start = Instant::now();
        session
            .note_editor_delta(3_000, 3_001, Instant::now())
            .unwrap();
        let ui_dirty = ui_start.elapsed();
        let journal_start = Instant::now();
        let journal = session
            .prepare_journal(Instant::now(), true)
            .unwrap()
            .unwrap();
        journal.execute().unwrap();
        let journal_time = journal_start.elapsed();
        session.acknowledge_journal(journal.stamp, Ok(()));
        let save = session.prepare_canonical_save().unwrap().unwrap();
        let save_start = Instant::now();
        let fingerprint = save.execute(&mut opened, save.stamp).unwrap();
        let save_time = save_start.elapsed();
        session.acknowledge_canonical_save(save.stamp, Ok(fingerprint));
        eprintln!(
            "lifecycle words=250000 load={load:?} ui_dirty={ui_dirty:?} journal={journal_time:?} canonical_save={save_time:?}"
        );
        assert!(load < Duration::from_secs(1), "document load took {load:?}");
        assert!(ui_dirty < Duration::from_millis(8));
        assert!(
            journal_time < Duration::from_secs(1),
            "journal write took {journal_time:?}"
        );
        assert!(
            save_time < Duration::from_secs(1),
            "canonical save took {save_time:?}"
        );
        assert!(!session.is_dirty());
    }
}
