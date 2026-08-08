use std::collections::{BTreeMap, BTreeSet};

use crate::{DomainError, MetadataFieldId, NodeKind, StyleId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyleRole {
    Body,
    DocumentTitle,
    Heading1,
    Heading2,
    Heading3,
    BlockQuote,
    Verse,
    Custom,
}

impl StyleRole {
    pub const fn is_reserved(self) -> bool {
        !matches!(self, Self::Custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    Start,
    Center,
    End,
    Justify,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleProperties {
    pub font_family: Option<String>,
    pub font_size_points: Option<f32>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub alignment: Option<TextAlignment>,
    pub first_line_indent_points: Option<f32>,
    pub left_indent_points: Option<f32>,
    pub right_indent_points: Option<f32>,
    pub line_spacing: Option<f32>,
    pub space_before_points: Option<f32>,
    pub space_after_points: Option<f32>,
    pub keep_with_next: Option<bool>,
    pub page_break_before: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyleDefinition {
    pub id: StyleId,
    pub display_name: String,
    pub role: StyleRole,
    pub inherits: Option<StyleId>,
    pub properties: StyleProperties,
}

impl StyleDefinition {
    pub fn custom(id: StyleId, display_name: impl Into<String>) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            role: StyleRole::Custom,
            inherits: Some(StyleCatalog::body_id()),
            properties: StyleProperties::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyleCatalog {
    definitions: BTreeMap<StyleId, StyleDefinition>,
    order: Vec<StyleId>,
}

impl Default for StyleCatalog {
    fn default() -> Self {
        let mut catalog = Self {
            definitions: BTreeMap::new(),
            order: Vec::new(),
        };
        for (id, name, role) in [
            (Self::body_id(), "Body", StyleRole::Body),
            (
                Self::document_title_id(),
                "Document Title",
                StyleRole::DocumentTitle,
            ),
            (Self::heading_1_id(), "Heading 1", StyleRole::Heading1),
            (Self::heading_2_id(), "Heading 2", StyleRole::Heading2),
            (Self::heading_3_id(), "Heading 3", StyleRole::Heading3),
            (Self::block_quote_id(), "Block Quote", StyleRole::BlockQuote),
            (Self::verse_id(), "Verse", StyleRole::Verse),
        ] {
            catalog.order.push(id);
            catalog.definitions.insert(
                id,
                StyleDefinition {
                    id,
                    display_name: name.into(),
                    role,
                    inherits: None,
                    properties: StyleProperties::default(),
                },
            );
        }
        catalog
    }
}

impl StyleCatalog {
    const fn reserved_id(last_byte: u8) -> StyleId {
        StyleId::from_bytes([
            0x50, 0x41, 0x52, 0x43, 0x48, 0x4d, 0x49, 0x4e, 0x54, 0x53, 0x54, 0x59, 0x4c, 0, 0,
            last_byte,
        ])
    }

    pub const fn body_id() -> StyleId {
        Self::reserved_id(1)
    }

    pub const fn document_title_id() -> StyleId {
        Self::reserved_id(2)
    }

    pub const fn heading_1_id() -> StyleId {
        Self::reserved_id(3)
    }

    pub const fn heading_2_id() -> StyleId {
        Self::reserved_id(4)
    }

    pub const fn heading_3_id() -> StyleId {
        Self::reserved_id(5)
    }

    pub const fn block_quote_id() -> StyleId {
        Self::reserved_id(6)
    }

    pub const fn verse_id() -> StyleId {
        Self::reserved_id(7)
    }

    pub fn get(&self, id: StyleId) -> Option<&StyleDefinition> {
        self.definitions.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &StyleDefinition> {
        self.order.iter().filter_map(|id| self.definitions.get(id))
    }

    pub fn upsert(&mut self, definition: StyleDefinition) -> Result<(), DomainError> {
        if definition.display_name.trim().is_empty() {
            return Err(DomainError::InvalidInput {
                field: "style display name",
                reason: "must not be empty",
            });
        }
        if let Some(existing) = self.definitions.get(&definition.id)
            && existing.role.is_reserved()
            && existing.role != definition.role
        {
            return Err(DomainError::InvalidInput {
                field: "style role",
                reason: "a reserved style role cannot change",
            });
        }
        if definition.role.is_reserved()
            && self
                .definitions
                .values()
                .any(|style| style.id != definition.id && style.role == definition.role)
        {
            return Err(DomainError::DuplicateId {
                field: "reserved style role",
            });
        }
        if let Some(parent) = definition.inherits {
            if parent == definition.id || !self.definitions.contains_key(&parent) {
                return Err(DomainError::InvalidInput {
                    field: "style inheritance",
                    reason: "parent style must exist and differ from the child",
                });
            }
            let mut cursor = Some(parent);
            while let Some(id) = cursor {
                if id == definition.id {
                    return Err(DomainError::InvalidInput {
                        field: "style inheritance",
                        reason: "inheritance cycles are not allowed",
                    });
                }
                cursor = self.definitions.get(&id).and_then(|style| style.inherits);
            }
        }
        if !self.definitions.contains_key(&definition.id) {
            self.order.push(definition.id);
        }
        self.definitions.insert(definition.id, definition);
        Ok(())
    }

    pub fn remove(&mut self, id: StyleId) -> Result<StyleDefinition, DomainError> {
        let definition = self
            .definitions
            .get(&id)
            .ok_or(DomainError::MissingItem { kind: "style" })?;
        if definition.role.is_reserved() {
            return Err(DomainError::InvalidInput {
                field: "style",
                reason: "reserved styles cannot be deleted",
            });
        }
        if self
            .definitions
            .values()
            .any(|candidate| candidate.inherits == Some(id))
        {
            return Err(DomainError::InvalidInput {
                field: "style",
                reason: "a style in use as an inheritance parent cannot be deleted",
            });
        }
        self.order.retain(|candidate| *candidate != id);
        Ok(self.definitions.remove(&id).expect("style was checked"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataApplicability {
    Groups,
    Documents,
    GroupsAndDocuments,
}

impl MetadataApplicability {
    pub const fn applies_to_group(self) -> bool {
        matches!(self, Self::Groups | Self::GroupsAndDocuments)
    }

    pub const fn applies_to_document(self) -> bool {
        matches!(self, Self::Documents | Self::GroupsAndDocuments)
    }

    pub const fn applies_to(self, node_kind: NodeKind) -> bool {
        match node_kind {
            NodeKind::Group => self.applies_to_group(),
            NodeKind::Document(_) => self.applies_to_document(),
            NodeKind::Root(_) => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataTextKind {
    SingleLine,
    Multiline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataFieldDefinition {
    pub id: MetadataFieldId,
    pub label: String,
    pub description: Option<String>,
    pub applicability: MetadataApplicability,
    pub text_kind: MetadataTextKind,
    pub default_value: Option<String>,
    pub visible_on_cards: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataCatalog {
    definitions: BTreeMap<MetadataFieldId, MetadataFieldDefinition>,
    order: Vec<MetadataFieldId>,
}

impl MetadataCatalog {
    pub fn get(&self, id: MetadataFieldId) -> Option<&MetadataFieldDefinition> {
        self.definitions.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &MetadataFieldDefinition> {
        self.order.iter().filter_map(|id| self.definitions.get(id))
    }

    pub fn upsert(&mut self, definition: MetadataFieldDefinition) -> Result<(), DomainError> {
        if definition.label.trim().is_empty() {
            return Err(DomainError::InvalidInput {
                field: "metadata field label",
                reason: "must not be empty",
            });
        }
        if definition.text_kind == MetadataTextKind::SingleLine
            && definition
                .default_value
                .as_deref()
                .is_some_and(|value| value.contains(['\r', '\n']))
        {
            return Err(DomainError::InvalidInput {
                field: "metadata default value",
                reason: "single-line fields cannot contain line breaks",
            });
        }
        if !self.definitions.contains_key(&definition.id) {
            self.order.push(definition.id);
        }
        self.definitions.insert(definition.id, definition);
        Ok(())
    }

    pub fn remove(&mut self, id: MetadataFieldId) -> Result<MetadataFieldDefinition, DomainError> {
        self.order.retain(|candidate| *candidate != id);
        self.definitions
            .remove(&id)
            .ok_or(DomainError::MissingItem {
                kind: "metadata field",
            })
    }

    pub fn move_to(&mut self, id: MetadataFieldId, index: usize) -> Result<(), DomainError> {
        let old_index = self
            .order
            .iter()
            .position(|candidate| *candidate == id)
            .ok_or(DomainError::MissingItem {
                kind: "metadata field",
            })?;
        self.order.remove(old_index);
        let destination = index.min(self.order.len());
        self.order.insert(destination, id);
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectDictionary {
    words: BTreeSet<String>,
}

impl ProjectDictionary {
    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(word)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.words.iter().map(String::as_str)
    }

    pub fn insert(&mut self, word: impl Into<String>) -> Result<bool, DomainError> {
        let word = word.into();
        if word.trim().is_empty() || word.contains(char::is_whitespace) {
            return Err(DomainError::InvalidInput {
                field: "dictionary word",
                reason: "must be one non-empty word",
            });
        }
        Ok(self.words.insert(word))
    }

    pub fn remove(&mut self, word: &str) -> bool {
        self.words.remove(word)
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectDictionary;

    #[test]
    fn project_dictionary_add_and_remove_are_deterministic() {
        let mut dictionary = ProjectDictionary::default();

        assert!(dictionary.insert("zebra").unwrap());
        assert!(dictionary.insert("apple").unwrap());
        assert!(!dictionary.insert("apple").unwrap());
        assert_eq!(dictionary.iter().collect::<Vec<_>>(), ["apple", "zebra"]);

        assert!(dictionary.remove("apple"));
        assert!(!dictionary.remove("apple"));
        assert!(!dictionary.contains("apple"));
        assert_eq!(dictionary.iter().collect::<Vec<_>>(), ["zebra"]);
    }
}
