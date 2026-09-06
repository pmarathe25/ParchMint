use std::collections::VecDeque;

use crate::{SemanticDocumentSnapshot, UndoEntry};

const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(super) struct UndoHistory {
    entries: VecDeque<(UndoEntry, usize)>,
    bytes: usize,
}

impl UndoHistory {
    pub(super) fn push(&mut self, mut entry: UndoEntry) {
        entry.compact();
        let bytes = entry.byte_cost();
        self.bytes += bytes;
        self.entries.push_back((entry, bytes));
        // Keep the latest operation undoable even if that one operation alone
        // exceeds the budget. Evict whole operations, never partial inverses.
        while self.entries.len() > 1 && (self.entries.len() > MAX_ENTRIES || self.bytes > MAX_BYTES)
        {
            self.bytes -= self.entries.pop_front().expect("nonempty history").1;
        }
    }

    pub(super) fn pop(&mut self) -> Option<UndoEntry> {
        let (entry, bytes) = self.entries.pop_back()?;
        self.bytes -= bytes;
        Some(entry)
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

impl UndoEntry {
    fn compact(&mut self) {
        let prefix = self
            .before
            .blocks
            .iter()
            .zip(&self.after.blocks)
            .take_while(|(before, after)| before == after)
            .count();
        self.unchanged_prefix += prefix;
        self.before.blocks.drain(..prefix);
        self.after.blocks.drain(..prefix);
        let suffix = self
            .before
            .blocks
            .iter()
            .rev()
            .zip(self.after.blocks.iter().rev())
            .take_while(|(before, after)| before == after)
            .count();
        self.before
            .blocks
            .truncate(self.before.blocks.len() - suffix);
        self.after.blocks.truncate(self.after.blocks.len() - suffix);
        self.before.blocks.shrink_to_fit();
        self.after.blocks.shrink_to_fit();
    }

    pub(super) fn restore(
        &self,
        mut current: SemanticDocumentSnapshot,
        redo: bool,
    ) -> SemanticDocumentSnapshot {
        let (remove, insert) = if redo {
            (&self.before, &self.after)
        } else {
            (&self.after, &self.before)
        };
        current.blocks.splice(
            self.unchanged_prefix..self.unchanged_prefix + remove.blocks.len(),
            insert.blocks.clone(),
        );
        current.canonical_html = insert.canonical_html;
        current
    }

    fn byte_cost(&self) -> usize {
        let documents = [&self.before, &self.after]
            .into_iter()
            .flat_map(|snapshot| &snapshot.blocks)
            .map(|block| {
                std::mem::size_of_val(block)
                    + block.text.capacity()
                    + block
                        .attributes
                        .iter()
                        .map(|(key, value)| key.capacity() + value.capacity())
                        .sum::<usize>()
                    + block
                        .marks
                        .iter()
                        .map(|mark| {
                            std::mem::size_of_val(mark)
                                + match &mark.mark {
                                    crate::SemanticInlineMark::Link(target) => target.capacity(),
                                    _ => 0,
                                }
                        })
                        .sum::<usize>()
            })
            .sum::<usize>();
        let comments = [&self.before_comments, &self.after_comments]
            .into_iter()
            .flat_map(|comments| comments.values())
            .map(|stored| {
                let comment = &stored.canonical;
                let anchor = match &comment.anchor {
                    crate::CanonicalCommentAnchor::Document { unknown_fields } => {
                        fields_cost(unknown_fields)
                    }
                    crate::CanonicalCommentAnchor::Text {
                        quote,
                        context_before,
                        context_after,
                        unknown_fields,
                        ..
                    } => {
                        quote.capacity()
                            + context_before.capacity()
                            + context_after.capacity()
                            + fields_cost(unknown_fields)
                    }
                };
                std::mem::size_of_val(stored)
                    + anchor * 2
                    + fields_cost(&comment.unknown_fields)
                    + comment
                        .messages
                        .iter()
                        .map(|message| {
                            std::mem::size_of_val(message)
                                + message.body.capacity()
                                + fields_cost(&message.unknown_fields)
                        })
                        .sum::<usize>()
            })
            .sum::<usize>();
        std::mem::size_of::<Self>()
            + documents
            + comments
            + self.changed_blocks.capacity() * std::mem::size_of::<crate::BlockId>()
    }
}

fn fields_cost(fields: &std::collections::BTreeMap<String, crate::AnnotationValue>) -> usize {
    fields
        .iter()
        .map(|(key, value)| key.capacity() + value_cost(value))
        .sum()
}

fn value_cost(value: &crate::AnnotationValue) -> usize {
    use crate::AnnotationValue;
    std::mem::size_of_val(value)
        + match value {
            AnnotationValue::Null | AnnotationValue::Bool(_) => 0,
            AnnotationValue::Number(value) | AnnotationValue::String(value) => value.capacity(),
            AnnotationValue::Array(values) => values.iter().map(value_cost).sum(),
            AnnotationValue::Object(fields) => fields_cost(fields),
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn long_chapter_undo_retains_only_changed_paragraphs_and_evicts_whole_edits() {
        let body = format!(
            "<p>{}</p><p>target</p><p>{}</p>",
            "first ".repeat(5000),
            "last ".repeat(5000)
        );
        let mut session = EditorCoreSession::open(CanonicalDocumentLoad::new(
            DocumentId::from_bytes([1; 16]),
            &body,
        ))
        .unwrap();
        let view = ViewId::from_bytes([2; 16]);
        session.attach_view(view).unwrap();
        for _ in 0..MAX_ENTRIES + 10 {
            session
                .execute(
                    EditorCommandOrigin::new(view),
                    EditorCommand::new(
                        session.revision(),
                        EditorCommandKind::InsertText {
                            at: 30001.into(),
                            text: "x".into(),
                        },
                    ),
                )
                .unwrap();
        }
        assert_eq!(session.inner.undo.entries.len(), MAX_ENTRIES);
        assert!(session.inner.undo.bytes < 1024 * 1024);
        for _ in 0..MAX_ENTRIES {
            session
                .execute(
                    EditorCommandOrigin::new(view),
                    EditorCommand::new(session.revision(), EditorCommandKind::Undo),
                )
                .unwrap();
        }
        assert!(
            session
                .canonical_projection()
                .body()
                .contains("xxxxxxxxxxtarget")
        );
        assert!(
            session
                .execute(
                    EditorCommandOrigin::new(view),
                    EditorCommand::new(session.revision(), EditorCommandKind::Undo)
                )
                .is_err()
        );
        for _ in 0..MAX_ENTRIES {
            session
                .execute(
                    EditorCommandOrigin::new(view),
                    EditorCommand::new(session.revision(), EditorCommandKind::Redo),
                )
                .unwrap();
        }
        assert!(
            session
                .canonical_projection()
                .body()
                .contains(&format!("{}target", "x".repeat(MAX_ENTRIES + 10)))
        );
    }
}
