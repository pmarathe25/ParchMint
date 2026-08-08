#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleChange {
    KeepDisplayTitle,
    SetDisplayTitle(String),
}

pub fn synchronize_content_title(
    display_title: &str,
    previous_content_title: Option<&str>,
    new_content_title: Option<&str>,
) -> TitleChange {
    let Some(new_title) = new_content_title.filter(|title| !title.trim().is_empty()) else {
        return TitleChange::KeepDisplayTitle;
    };
    if previous_content_title == Some(display_title) {
        TitleChange::SetDisplayTitle(new_title.to_owned())
    } else {
        TitleChange::KeepDisplayTitle
    }
}

pub fn synchronize_first_title_block<'a>(
    display_title: &str,
    previous_content_title: Option<&str>,
    mut blocks: impl Iterator<Item = SemanticBlockRef<'a>>,
) -> TitleChange {
    let first_title = blocks.find_map(|block| match block.kind {
        SemanticBlockKind::DocumentTitle => block.text,
        _ => None,
    });
    synchronize_content_title(display_title, previous_content_title, first_title)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticBlockKind {
    DocumentTitle,
    Heading,
    Prose,
    Comment,
    Synopsis,
    Metadata,
    SceneBreak,
    PageBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticBlockRef<'a> {
    pub kind: SemanticBlockKind,
    pub text: Option<&'a str>,
}

impl<'a> SemanticBlockRef<'a> {
    pub const fn document_title(text: &'a str) -> Self {
        Self::text(SemanticBlockKind::DocumentTitle, text)
    }

    pub const fn heading(text: &'a str) -> Self {
        Self::text(SemanticBlockKind::Heading, text)
    }

    pub const fn prose(text: &'a str) -> Self {
        Self::text(SemanticBlockKind::Prose, text)
    }

    pub const fn comment(text: &'a str) -> Self {
        Self::text(SemanticBlockKind::Comment, text)
    }

    pub const fn synopsis(text: &'a str) -> Self {
        Self::text(SemanticBlockKind::Synopsis, text)
    }

    pub const fn metadata(text: &'a str) -> Self {
        Self::text(SemanticBlockKind::Metadata, text)
    }

    pub const fn scene_break() -> Self {
        Self::structural(SemanticBlockKind::SceneBreak)
    }

    pub const fn page_break() -> Self {
        Self::structural(SemanticBlockKind::PageBreak)
    }

    const fn text(kind: SemanticBlockKind, text: &'a str) -> Self {
        Self {
            kind,
            text: Some(text),
        }
    }

    const fn structural(kind: SemanticBlockKind) -> Self {
        Self { kind, text: None }
    }

    pub const fn is_exportable_text(self) -> bool {
        matches!(
            self.kind,
            SemanticBlockKind::DocumentTitle
                | SemanticBlockKind::Heading
                | SemanticBlockKind::Prose
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_sync_follows_only_an_unchanged_display_title() {
        assert_eq!(
            synchronize_content_title("Chapter 1", Some("Chapter 1"), Some("Chapter One")),
            TitleChange::SetDisplayTitle("Chapter One".into())
        );
        assert_eq!(
            synchronize_content_title(
                "Pinned title",
                Some("Old body title"),
                Some("New body title")
            ),
            TitleChange::KeepDisplayTitle
        );
        assert_eq!(
            synchronize_content_title("Chapter 1", Some("Chapter 1"), None),
            TitleChange::KeepDisplayTitle
        );
    }

    #[test]
    fn title_sync_uses_only_the_first_title_block() {
        assert_eq!(
            synchronize_first_title_block(
                "First",
                Some("First"),
                [
                    SemanticBlockRef::document_title("First"),
                    SemanticBlockRef::document_title("Second"),
                ]
                .into_iter(),
            ),
            TitleChange::SetDisplayTitle("First".into())
        );
    }
}
