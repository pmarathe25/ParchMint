use parchmint_editor_api::{BlockId, EditorRevision, EditorSelection};

/// A bounded text range near one view's caret, captured at an exact revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorTextContext {
    pub revision: EditorRevision,
    pub block_id: BlockId,
    pub range: EditorSelection,
    pub text: String,
}

pub(crate) fn bounded_text_context(
    document: &parchmint_editor_api::SemanticDocument,
    revision: EditorRevision,
    caret: u64,
    max_chars: usize,
) -> Option<EditorTextContext> {
    let mut offset = 0_u64;
    for (index, block) in document.blocks().iter().enumerate() {
        let text = match block.kind() {
            parchmint_editor_api::SemanticBlockKind::SceneBreak
            | parchmint_editor_api::SemanticBlockKind::PageBreak => "\u{fffc}",
            _ => block.text(),
        };
        let length = text.chars().count();
        if caret <= offset + length as u64 || index + 1 == document.blocks().len() {
            let local_caret = caret.saturating_sub(offset).min(length as u64) as usize;
            let mut start = local_caret
                .saturating_sub(max_chars / 2)
                .min(length.saturating_sub(max_chars));
            let mut context: String = text.chars().skip(start).take(max_chars).collect();
            let reaches_end = start + context.chars().count() == length;
            if start > 0 {
                let boundary = context.find(char::is_whitespace)?;
                start += context[..boundary].chars().count() + 1;
                let boundary_end = boundary + context[boundary..].chars().next()?.len_utf8();
                context.drain(..boundary_end);
            }
            if !reaches_end {
                let boundary = context.rfind(char::is_whitespace)?;
                context.truncate(boundary);
            }
            let start = offset + start as u64;
            let end = start + context.chars().count() as u64;
            return Some(EditorTextContext {
                revision,
                block_id: block.id(),
                range: EditorSelection::new(start.into(), end.into()),
                text: context,
            });
        }
        offset += length as u64 + 1;
    }
    None
}

#[cfg(test)]
mod text_context_tests {
    use super::*;
    use parchmint_editor_api::{SemanticBlock, SemanticBlockKind, SemanticDocument};

    #[test]
    fn bounded_context_keeps_unicode_positions_and_complete_words_in_long_paragraphs() {
        let text = format!(
            "{}teh marker {}",
            "élan ".repeat(20_000),
            "word ".repeat(20_000)
        );
        let document = SemanticDocument::new(vec![
            SemanticBlock::new(
                BlockId::from_bytes([1; 16]),
                SemanticBlockKind::Paragraph,
                None,
                "Preface",
                vec![],
            ),
            SemanticBlock::new(
                BlockId::from_bytes([2; 16]),
                SemanticBlockKind::Paragraph,
                None,
                &text,
                vec![],
            ),
        ]);
        let context = bounded_text_context(&document, 7.into(), 100_008, 4096).unwrap();
        assert!(context.text.contains("teh marker"));
        assert!(context.text.chars().count() <= 4096);
        let start = context.range.start().value() as usize - 8;
        let end = context.range.end().value() as usize - 8;
        assert_eq!(
            context.text,
            text.chars()
                .skip(start)
                .take(end - start)
                .collect::<String>()
        );
        assert!(text.chars().nth(start - 1).unwrap().is_whitespace());
        assert!(text.chars().nth(end).unwrap().is_whitespace());
        assert_eq!(context.block_id, BlockId::from_bytes([2; 16]));
        assert_eq!(context.revision, EditorRevision::from(7));
    }
}
