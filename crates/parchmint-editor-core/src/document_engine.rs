use crate::BlockId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SemanticBlockSnapshot {
    pub(super) id: BlockId,
    pub(super) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SemanticDocumentSnapshot {
    pub(super) blocks: Vec<SemanticBlockSnapshot>,
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

    pub(super) const fn at(&self) -> usize {
        self.at
    }

    pub(super) fn inserted(&self) -> &str {
        &self.inserted
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PositionMapping {
    at: usize,
    removed: usize,
    inserted: usize,
}

impl PositionMapping {
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
                .and_then(|position| position.checked_add(self.inserted))
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
    removed_text: String,
    changed_blocks: Vec<BlockId>,
}

impl EngineChange {
    pub(super) const fn mapping(&self) -> PositionMapping {
        self.mapping
    }

    pub(super) fn removed_text(&self) -> &str {
        &self.removed_text
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
    fn blocks(&self) -> Vec<SemanticBlockSnapshot>;
    fn text(&self) -> &str;
    fn scalar_len(&self) -> usize;
}

#[derive(Debug, Default)]
pub(super) struct PrivateTextEngine {
    blocks: Vec<SemanticBlockSnapshot>,
}

impl DocumentEngine for PrivateTextEngine {
    fn load(&mut self, document: SemanticDocumentSnapshot) -> Result<(), EngineError> {
        if document.blocks.len() != 1 {
            return Err(EngineError::InvalidSnapshot);
        }
        self.blocks = document.blocks;
        Ok(())
    }

    fn apply(&mut self, edit: EngineEdit) -> Result<EngineChange, EngineError> {
        let block = self
            .blocks
            .first_mut()
            .ok_or(EngineError::InvalidSnapshot)?;
        let scalar_len = block.text.chars().count();
        let removed_end = edit
            .at
            .checked_add(edit.removed)
            .ok_or(EngineError::InvalidEdit)?;
        if edit.at > scalar_len || removed_end > scalar_len {
            return Err(EngineError::InvalidEdit);
        }

        let byte_start = scalar_to_byte(&block.text, edit.at).ok_or(EngineError::InvalidEdit)?;
        let byte_end = scalar_to_byte(&block.text, removed_end).ok_or(EngineError::InvalidEdit)?;
        let removed_text = block.text[byte_start..byte_end].to_owned();
        let changed_block = block.id;
        block
            .text
            .replace_range(byte_start..byte_end, &edit.inserted);
        Ok(EngineChange {
            mapping: PositionMapping {
                at: edit.at,
                removed: edit.removed,
                inserted: edit.inserted.chars().count(),
            },
            removed_text,
            changed_blocks: vec![changed_block],
        })
    }

    fn blocks(&self) -> Vec<SemanticBlockSnapshot> {
        self.blocks.clone()
    }

    fn text(&self) -> &str {
        self.blocks.first().map_or("", |block| block.text.as_str())
    }

    fn scalar_len(&self) -> usize {
        self.blocks
            .first()
            .map_or(0, |block| block.text.chars().count())
    }
}

fn scalar_to_byte(text: &str, scalar: usize) -> Option<usize> {
    if scalar == text.chars().count() {
        return Some(text.len());
    }
    text.char_indices().nth(scalar).map(|(index, _)| index)
}
