use std::collections::BTreeMap;

use crate::{
    AtomicBlockKind, BlockId, DocumentPosition, EditorSelection, SemanticBlock, SemanticBlockKind,
    SemanticDocument, SemanticInlineMark, SemanticMarkRange,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EngineMark {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) mark: SemanticInlineMark,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SemanticBlockSnapshot {
    pub(super) id: BlockId,
    pub(super) kind: SemanticBlockKind,
    pub(super) attributes: BTreeMap<String, String>,
    pub(super) text: String,
    pub(super) marks: Vec<EngineMark>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SemanticDocumentSnapshot {
    pub(super) blocks: Vec<SemanticBlockSnapshot>,
    pub(super) canonical_html: bool,
}

impl SemanticDocumentSnapshot {
    pub(super) fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .map(|block| match block.kind {
                SemanticBlockKind::SceneBreak | SemanticBlockKind::PageBreak => "\u{fffc}",
                _ => block.text.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(super) fn word_count(&self) -> usize {
        self.blocks
            .iter()
            .filter(|block| !is_atomic(block.kind))
            .map(|block| block.text.split_whitespace().count())
            .sum()
    }

    pub(super) fn semantic_projection(&self) -> SemanticDocument {
        let blocks = self
            .blocks
            .iter()
            .map(|block| {
                let marks = block
                    .marks
                    .iter()
                    .map(|mark| {
                        SemanticMarkRange::new(
                            EditorSelection::new(
                                DocumentPosition::from(mark.start as u64),
                                DocumentPosition::from(mark.end as u64),
                            ),
                            mark.mark.clone(),
                        )
                    })
                    .collect();
                SemanticBlock::new(
                    block.id,
                    block.kind,
                    block.attributes.get("data-style-id").cloned(),
                    block.text.clone(),
                    marks,
                )
            })
            .collect();
        SemanticDocument::new(blocks)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EngineEdit {
    at: usize,
    removed: usize,
    inserted: String,
}

impl EngineEdit {
    pub(super) fn new(at: usize, removed: usize, inserted: String) -> Self {
        Self {
            at,
            removed,
            inserted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PositionMapping {
    at: usize,
    removed: usize,
    inserted: usize,
}

impl PositionMapping {
    pub(super) const fn identity() -> Self {
        Self {
            at: 0,
            removed: 0,
            inserted: 0,
        }
    }

    pub(super) const fn inverse(self) -> Self {
        Self {
            at: self.at,
            removed: self.inserted,
            inserted: self.removed,
        }
    }

    pub(super) fn map(self, position: usize) -> Result<usize, EngineError> {
        let removed_end = self
            .at
            .checked_add(self.removed)
            .ok_or(EngineError::InvalidEdit)?;
        if position <= self.at {
            Ok(position)
        } else if position <= removed_end {
            self.at
                .checked_add(self.inserted)
                .ok_or(EngineError::InvalidEdit)
        } else {
            position
                .checked_sub(self.removed)
                .and_then(|value| value.checked_add(self.inserted))
                .ok_or(EngineError::InvalidEdit)
        }
    }

    pub(super) fn overlaps(self, range: (usize, usize)) -> bool {
        if self.removed == 0 {
            return false;
        }
        let (start, length) = range;
        let end = start.saturating_add(length);
        let removed_end = self.at.saturating_add(self.removed);
        if length == 0 {
            self.at < start && start < removed_end
        } else {
            start < removed_end && self.at < end
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EngineChange {
    mapping: PositionMapping,
    changed_blocks: Vec<BlockId>,
}

impl EngineChange {
    pub(super) const fn mapping(&self) -> PositionMapping {
        self.mapping
    }
    pub(super) fn changed_blocks(&self) -> &[BlockId] {
        &self.changed_blocks
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EngineError {
    InvalidEdit,
    InvalidSnapshot,
}

pub(super) trait DocumentEngine {
    fn load(&mut self, document: SemanticDocumentSnapshot) -> Result<(), EngineError>;
    fn apply(&mut self, edit: EngineEdit) -> Result<EngineChange, EngineError>;
    fn toggle_inline_mark(
        &mut self,
        start: usize,
        end: usize,
        mark: SemanticInlineMark,
    ) -> Result<EngineChange, EngineError>;
    fn set_link(
        &mut self,
        start: usize,
        end: usize,
        target: Option<String>,
    ) -> Result<EngineChange, EngineError>;
    fn toggle_block_format(
        &mut self,
        start: usize,
        end: usize,
        target: SemanticBlockKind,
    ) -> Result<EngineChange, EngineError>;
    fn insert_atomic_block(
        &mut self,
        at: usize,
        kind: AtomicBlockKind,
        atomic_id: BlockId,
        after_id: BlockId,
    ) -> Result<EngineChange, EngineError>;
    fn apply_paragraph_style(
        &mut self,
        start: usize,
        end: usize,
        style: String,
    ) -> Result<EngineChange, EngineError>;
    fn snapshot(&self) -> SemanticDocumentSnapshot;
    fn text(&self) -> &str;
    fn scalar_len(&self) -> usize;
}

#[derive(Debug, Default)]
pub(super) struct PrivateTextEngine {
    document: Option<SemanticDocumentSnapshot>,
    text: String,
}

impl DocumentEngine for PrivateTextEngine {
    fn load(&mut self, document: SemanticDocumentSnapshot) -> Result<(), EngineError> {
        if document.blocks.is_empty() {
            return Err(EngineError::InvalidSnapshot);
        }
        self.text = document.plain_text();
        self.document = Some(document);
        Ok(())
    }

    fn apply(&mut self, edit: EngineEdit) -> Result<EngineChange, EngineError> {
        let document = self.document.as_mut().ok_or(EngineError::InvalidSnapshot)?;
        let (block_index, local_start) = locate_position(&document.blocks, edit.at, true)?;
        let removed_end = edit
            .at
            .checked_add(edit.removed)
            .ok_or(EngineError::InvalidEdit)?;
        let (end_block, local_end) = locate_position(&document.blocks, removed_end, false)?;
        if block_index != end_block {
            return Err(EngineError::InvalidEdit);
        }
        let block = &mut document.blocks[block_index];
        if is_atomic(block.kind) {
            return Err(EngineError::InvalidEdit);
        }
        let byte_start =
            scalar_to_byte(&block.text, local_start).ok_or(EngineError::InvalidEdit)?;
        let byte_end = scalar_to_byte(&block.text, local_end).ok_or(EngineError::InvalidEdit)?;
        block
            .text
            .replace_range(byte_start..byte_end, &edit.inserted);
        update_marks_for_edit(
            &mut block.marks,
            local_start,
            local_end,
            edit.inserted.chars().count(),
        );
        let changed_block = block.id;
        self.text = document.plain_text();
        Ok(EngineChange {
            mapping: PositionMapping {
                at: edit.at,
                removed: edit.removed,
                inserted: edit.inserted.chars().count(),
            },
            changed_blocks: vec![changed_block],
        })
    }

    fn toggle_inline_mark(
        &mut self,
        start: usize,
        end: usize,
        mark: SemanticInlineMark,
    ) -> Result<EngineChange, EngineError> {
        if start >= end {
            return Err(EngineError::InvalidEdit);
        }
        let document = self.document.as_mut().ok_or(EngineError::InvalidSnapshot)?;
        let before = document.clone();
        let mut changed = Vec::new();
        let mut offset = 0usize;
        let fully_marked = document.blocks.iter().all(|block| {
            let block_end = offset.saturating_add(block.text.chars().count());
            let local_start = start.saturating_sub(offset).min(block.text.chars().count());
            let local_end = end.saturating_sub(offset).min(block.text.chars().count());
            offset = block_end.saturating_add(1);
            local_start >= local_end
                || range_fully_marked(&block.marks, local_start, local_end, &mark)
        });
        offset = 0;
        for block in &mut document.blocks {
            let len = block.text.chars().count();
            let local_start = start.saturating_sub(offset).min(len);
            let local_end = end.saturating_sub(offset).min(len);
            if local_start < local_end {
                if fully_marked {
                    remove_mark(&mut block.marks, local_start, local_end, &mark);
                } else {
                    add_mark(&mut block.marks, local_start, local_end, mark.clone());
                }
                changed.push(block.id);
            }
            offset = offset.saturating_add(len).saturating_add(1);
        }
        if changed.is_empty() || *document == before {
            return Err(EngineError::InvalidEdit);
        }
        Ok(EngineChange {
            mapping: PositionMapping::identity(),
            changed_blocks: changed,
        })
    }

    fn set_link(
        &mut self,
        start: usize,
        end: usize,
        target: Option<String>,
    ) -> Result<EngineChange, EngineError> {
        if start >= end {
            return Err(EngineError::InvalidEdit);
        }
        let document = self.document.as_mut().ok_or(EngineError::InvalidSnapshot)?;
        let before = document.clone();
        let mut changed = Vec::new();
        let mut offset = 0usize;
        for block in &mut document.blocks {
            let len = block.text.chars().count();
            let local_start = start.saturating_sub(offset).min(len);
            let local_end = end.saturating_sub(offset).min(len);
            if local_start < local_end {
                remove_links(&mut block.marks, local_start, local_end);
                if let Some(target) = &target {
                    add_mark(
                        &mut block.marks,
                        local_start,
                        local_end,
                        SemanticInlineMark::Link(target.clone()),
                    );
                }
                changed.push(block.id);
            }
            offset = offset.saturating_add(len).saturating_add(1);
        }
        if changed.is_empty() || *document == before {
            return Err(EngineError::InvalidEdit);
        }
        Ok(EngineChange {
            mapping: PositionMapping::identity(),
            changed_blocks: changed,
        })
    }

    fn toggle_block_format(
        &mut self,
        start: usize,
        end: usize,
        target: SemanticBlockKind,
    ) -> Result<EngineChange, EngineError> {
        let document = self.document.as_mut().ok_or(EngineError::InvalidSnapshot)?;
        let mut selected = Vec::new();
        let mut offset = 0usize;
        for (index, block) in document.blocks.iter().enumerate() {
            let len = block_scalar_len(block);
            let block_end = offset.saturating_add(len);
            let intersects = if start == end {
                start >= offset && start <= block_end
            } else {
                start < block_end && offset < end
            };
            if intersects {
                if is_atomic(block.kind) {
                    return Err(EngineError::InvalidEdit);
                }
                selected.push(index);
            }
            offset = block_end.saturating_add(1);
        }
        if selected.is_empty() {
            return Err(EngineError::InvalidEdit);
        }
        let remove = selected
            .iter()
            .all(|index| document.blocks[*index].kind == target);
        let replacement = if remove {
            SemanticBlockKind::Paragraph
        } else {
            target
        };
        let mut changed = Vec::new();
        for index in selected {
            let block = &mut document.blocks[index];
            block.kind = replacement;
            changed.push(block.id);
        }
        self.text = document.plain_text();
        Ok(EngineChange {
            mapping: PositionMapping::identity(),
            changed_blocks: changed,
        })
    }

    fn insert_atomic_block(
        &mut self,
        at: usize,
        kind: AtomicBlockKind,
        atomic_id: BlockId,
        after_id: BlockId,
    ) -> Result<EngineChange, EngineError> {
        let document = self.document.as_mut().ok_or(EngineError::InvalidSnapshot)?;
        let before_len = document.plain_text().chars().count();
        let (index, local) = locate_position(&document.blocks, at, true)?;
        let block = document.blocks[index].clone();
        let len = block_scalar_len(&block);
        if local > len {
            return Err(EngineError::InvalidEdit);
        }
        let atomic = SemanticBlockSnapshot {
            id: atomic_id,
            kind: match kind {
                AtomicBlockKind::SceneBreak => SemanticBlockKind::SceneBreak,
                AtomicBlockKind::PageBreak => SemanticBlockKind::PageBreak,
            },
            attributes: BTreeMap::from([(
                "data-kind".into(),
                match kind {
                    AtomicBlockKind::SceneBreak => "scene-break".into(),
                    AtomicBlockKind::PageBreak => "page-break".into(),
                },
            )]),
            text: String::new(),
            marks: Vec::new(),
        };
        let mut changed = vec![atomic_id];
        if is_atomic(block.kind) {
            let insertion = if local == 0 { index } else { index + 1 };
            document.blocks.insert(insertion, atomic);
        } else if local == 0 {
            document.blocks.insert(index, atomic);
        } else if local == len {
            document.blocks.insert(index + 1, atomic);
        } else {
            let byte = scalar_to_byte(&block.text, local).ok_or(EngineError::InvalidEdit)?;
            let mut before = block.clone();
            before.text = block.text[..byte].to_owned();
            before.marks = clipped_marks(&block.marks, 0, local);
            let mut after = block;
            after.id = after_id;
            after.text = after.text[byte..].to_owned();
            after.marks = clipped_marks(&after.marks, local, len);
            changed.extend([before.id, after.id]);
            document
                .blocks
                .splice(index..=index, [before, atomic, after]);
        }
        self.text = document.plain_text();
        let after_len = self.text.chars().count();
        let inserted = after_len
            .checked_sub(before_len)
            .ok_or(EngineError::InvalidEdit)?;
        Ok(EngineChange {
            mapping: PositionMapping {
                at,
                removed: 0,
                inserted,
            },
            changed_blocks: changed,
        })
    }

    fn apply_paragraph_style(
        &mut self,
        start: usize,
        end: usize,
        style: String,
    ) -> Result<EngineChange, EngineError> {
        let document = self.document.as_mut().ok_or(EngineError::InvalidSnapshot)?;
        let mut changed = Vec::new();
        let mut offset = 0usize;
        for block in &mut document.blocks {
            let len = block.text.chars().count();
            let block_end = offset.saturating_add(len);
            let selected = if start == end {
                start >= offset && start <= block_end
            } else {
                start < block_end && offset < end
            };
            if selected {
                block
                    .attributes
                    .insert("data-style-id".into(), style.clone());
                changed.push(block.id);
            }
            offset = block_end.saturating_add(1);
        }
        if changed.is_empty() {
            return Err(EngineError::InvalidEdit);
        }
        Ok(EngineChange {
            mapping: PositionMapping::identity(),
            changed_blocks: changed,
        })
    }

    fn snapshot(&self) -> SemanticDocumentSnapshot {
        self.document.clone().expect("loaded engine")
    }
    fn text(&self) -> &str {
        &self.text
    }
    fn scalar_len(&self) -> usize {
        self.text.chars().count()
    }
}

fn locate_position(
    blocks: &[SemanticBlockSnapshot],
    position: usize,
    prefer_previous: bool,
) -> Result<(usize, usize), EngineError> {
    let mut offset = 0usize;
    for (index, block) in blocks.iter().enumerate() {
        let len = block_scalar_len(block);
        let end = offset.checked_add(len).ok_or(EngineError::InvalidEdit)?;
        if position < end || position == end && (prefer_previous || index + 1 == blocks.len()) {
            return Ok((index, position - offset));
        }
        if position == end + 1 && !prefer_previous && index + 1 < blocks.len() {
            return Ok((index + 1, 0));
        }
        offset = end.checked_add(1).ok_or(EngineError::InvalidEdit)?;
    }
    Err(EngineError::InvalidEdit)
}

fn block_scalar_len(block: &SemanticBlockSnapshot) -> usize {
    if is_atomic(block.kind) {
        1
    } else {
        block.text.chars().count()
    }
}

fn is_atomic(kind: SemanticBlockKind) -> bool {
    matches!(
        kind,
        SemanticBlockKind::SceneBreak | SemanticBlockKind::PageBreak
    )
}

fn clipped_marks(marks: &[EngineMark], start: usize, end: usize) -> Vec<EngineMark> {
    marks
        .iter()
        .filter_map(|mark| {
            let clipped_start = mark.start.max(start);
            let clipped_end = mark.end.min(end);
            (clipped_start < clipped_end).then(|| EngineMark {
                start: clipped_start - start,
                end: clipped_end - start,
                mark: mark.mark.clone(),
            })
        })
        .collect()
}

fn update_marks_for_edit(marks: &mut Vec<EngineMark>, start: usize, end: usize, inserted: usize) {
    let removed = end - start;
    let inherited: Vec<_> = marks
        .iter()
        .filter(|mark| mark.start < start && start < mark.end)
        .map(|mark| mark.mark.clone())
        .collect();
    for mark in marks.iter_mut() {
        if mark.end <= start {
            continue;
        }
        if mark.start >= end {
            mark.start = mark.start - removed + inserted;
            mark.end = mark.end - removed + inserted;
        } else {
            mark.start = mark.start.min(start);
            mark.end = if mark.end <= end {
                start
            } else {
                mark.end - removed + inserted
            };
        }
    }
    marks.retain(|mark| mark.start < mark.end);
    for mark in inherited {
        add_mark(marks, start, start + inserted, mark);
    }
    normalize_marks(marks);
}

fn range_fully_marked(
    marks: &[EngineMark],
    start: usize,
    end: usize,
    kind: &SemanticInlineMark,
) -> bool {
    let mut covered = start;
    let mut spans: Vec<_> = marks
        .iter()
        .filter(|mark| &mark.mark == kind && mark.end > start && mark.start < end)
        .collect();
    spans.sort_by_key(|mark| mark.start);
    for mark in spans {
        if mark.start > covered {
            return false;
        }
        covered = covered.max(mark.end);
        if covered >= end {
            return true;
        }
    }
    false
}

fn add_mark(marks: &mut Vec<EngineMark>, start: usize, end: usize, mark: SemanticInlineMark) {
    if start < end {
        marks.push(EngineMark { start, end, mark });
        normalize_marks(marks);
    }
}

fn remove_mark(marks: &mut Vec<EngineMark>, start: usize, end: usize, kind: &SemanticInlineMark) {
    let mut replacements = Vec::new();
    marks.retain(|mark| {
        if &mark.mark != kind || mark.end <= start || mark.start >= end {
            return true;
        }
        if mark.start < start {
            replacements.push(EngineMark {
                start: mark.start,
                end: start,
                mark: mark.mark.clone(),
            });
        }
        if mark.end > end {
            replacements.push(EngineMark {
                start: end,
                end: mark.end,
                mark: mark.mark.clone(),
            });
        }
        false
    });
    marks.extend(replacements);
    normalize_marks(marks);
}

fn remove_links(marks: &mut Vec<EngineMark>, start: usize, end: usize) {
    let mut replacements = Vec::new();
    marks.retain(|mark| {
        if !matches!(mark.mark, SemanticInlineMark::Link(_))
            || mark.end <= start
            || mark.start >= end
        {
            return true;
        }
        if mark.start < start {
            replacements.push(EngineMark {
                start: mark.start,
                end: start,
                mark: mark.mark.clone(),
            });
        }
        if mark.end > end {
            replacements.push(EngineMark {
                start: end,
                end: mark.end,
                mark: mark.mark.clone(),
            });
        }
        false
    });
    marks.extend(replacements);
    normalize_marks(marks);
}

fn normalize_marks(marks: &mut Vec<EngineMark>) {
    marks.sort_by(|a, b| (&a.mark, a.start, a.end).cmp(&(&b.mark, b.start, b.end)));
    let mut normalized: Vec<EngineMark> = Vec::new();
    for mark in marks.drain(..) {
        if let Some(previous) = normalized.last_mut()
            && previous.mark == mark.mark
            && mark.start <= previous.end
        {
            previous.end = previous.end.max(mark.end);
        } else {
            normalized.push(mark);
        }
    }
    *marks = normalized;
}

fn scalar_to_byte(text: &str, scalar: usize) -> Option<usize> {
    if scalar == text.chars().count() {
        Some(text.len())
    } else {
        text.char_indices().nth(scalar).map(|(index, _)| index)
    }
}
