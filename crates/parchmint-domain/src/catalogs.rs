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

/// Shared schema for persisted style properties and their editing controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StylePropertyInput {
    Text,
    Number,
    Boolean,
    Alignment,
    Decoration,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextDecoration {
    None,
    Underline,
    Strikethrough,
    UnderlineAndStrikethrough,
}
impl TextDecoration {
    pub const fn underline(self) -> bool {
        matches!(self, Self::Underline | Self::UnderlineAndStrikethrough)
    }
    pub const fn strikethrough(self) -> bool {
        matches!(self, Self::Strikethrough | Self::UnderlineAndStrikethrough)
    }
}
impl StylePropertyInput {
    pub const fn choices(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Boolean => &[("false", "Off"), ("true", "On")],
            Self::Alignment => &[
                ("Start", "Left"),
                ("Center", "Center"),
                ("End", "Right"),
                ("Justify", "Justified"),
            ],
            Self::Decoration => &[
                ("none", "None"),
                ("underline", "Underline"),
                ("line-through", "Strikethrough"),
                ("underline line-through", "Underline and strikethrough"),
            ],
            Self::Text | Self::Number => &[],
        }
    }
}
macro_rules! style_enum_value {
    ($ty:ty, $($variant:ident => $value:literal),+ $(,)?) => {
        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(match self { $(Self::$variant => $value),+ }) }
        }
        impl std::str::FromStr for $ty {
            type Err = &'static str;
            fn from_str(s: &str) -> Result<Self, Self::Err> { match s { $($value => Ok(Self::$variant)),+, _ => Err("Unsupported style value") } }
        }
    };
}
style_enum_value!(TextAlignment, Start => "Start", Center => "Center", End => "End", Justify => "Justify");
style_enum_value!(TextDecoration, None => "none", Underline => "underline", Strikethrough => "line-through", UnderlineAndStrikethrough => "underline line-through");
macro_rules! style_properties {
    ($($variant:ident => $field:ident: $ty:ty, $label:literal, $group:literal, $input:ident;)+) => {
        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct StyleProperties { $(pub $field: Option<$ty>,)+ }
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum StyleProperty { $($variant,)+ }
        impl StyleProperty {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];
            pub const fn label(self) -> &'static str { match self { $(Self::$variant => $label,)+ } }
            pub const fn group(self) -> &'static str { match self { $(Self::$variant => $group,)+ } }
            pub const fn input(self) -> StylePropertyInput { match self { $(Self::$variant => StylePropertyInput::$input,)+ } }
            pub fn value(self, properties: &StyleProperties) -> String { match self { $(Self::$variant => properties.$field.as_ref().map(ToString::to_string).unwrap_or_default(),)+ } }
            pub fn set(self, properties: &mut StyleProperties, value: &str) -> bool {
                let value = value.trim();
                match self { $(Self::$variant => {
                    match (!value.is_empty()).then_some(value).map(str::parse).transpose() {
                        Ok(value) => properties.$field = value,
                        Err(_) => return false,
                    }
                },)+ }
                true
            }
        }
        impl StyleProperties {
            pub fn overlay(&mut self, source: &Self) { $(if source.$field.is_some() { self.$field.clone_from(&source.$field); })+ }
        }
    };
}
style_properties! {
    FontFamily => font_family: String, "Font family", "Typography", Text;
    FontSizePoints => font_size_points: f32, "Font size (pt)", "Typography", Number;
    Weight => weight: u16, "Weight", "Typography", Number;
    Italic => italic: bool, "Italic", "Typography", Boolean;
    TextDecoration => text_decoration: TextDecoration, "Text decoration", "Typography", Decoration;
    Alignment => alignment: TextAlignment, "Alignment", "Typography", Alignment;
    FirstLineIndentPoints => first_line_indent_points: f32, "First-line indent (pt)", "Spacing", Number;
    LeftIndentPoints => left_indent_points: f32, "Left indent (pt)", "Spacing", Number;
    RightIndentPoints => right_indent_points: f32, "Right indent (pt)", "Spacing", Number;
    LineSpacing => line_spacing: f32, "Line spacing", "Spacing", Number;
    SpaceBeforePoints => space_before_points: f32, "Space before (pt)", "Spacing", Number;
    SpaceAfterPoints => space_after_points: f32, "Space after (pt)", "Spacing", Number;
    KeepWithNext => keep_with_next: bool, "Keep with next", "Pagination", Boolean;
    PageBreakBefore => page_break_before: bool, "Page break before", "Pagination", Boolean;
}

impl StyleProperties {
    pub fn for_role(role: StyleRole) -> Self {
        let mut value = Self {
            font_family: Some("Source Serif 4".into()),
            font_size_points: Some(15.0),
            weight: Some(400),
            italic: Some(false),
            text_decoration: Some(TextDecoration::None),
            first_line_indent_points: Some(0.0),
            left_indent_points: Some(0.0),
            right_indent_points: Some(0.0),
            space_before_points: Some(0.0),
            space_after_points: Some(0.0),
            keep_with_next: Some(false),
            page_break_before: Some(false),
            alignment: Some(TextAlignment::Start),
            line_spacing: Some(1.0),
        };
        match role {
            StyleRole::DocumentTitle => {
                value.font_size_points = Some(28.0);
                value.weight = Some(700);
                value.space_after_points = Some(18.0);
            }
            StyleRole::Heading1 | StyleRole::Heading2 | StyleRole::Heading3 => {
                value.font_size_points = Some(match role {
                    StyleRole::Heading1 => 24.0,
                    StyleRole::Heading2 => 20.0,
                    _ => 17.0,
                });
                value.weight = Some(700);
                value.line_spacing = Some(1.15);
                value.space_before_points = Some(12.0);
                value.space_after_points = Some(6.0);
                value.keep_with_next = Some(true);
            }
            StyleRole::BlockQuote => {
                value.italic = Some(true);
                value.left_indent_points = Some(24.0);
                value.right_indent_points = Some(24.0);
            }
            StyleRole::Verse => value.line_spacing = Some(1.0),
            _ => {}
        }
        value
    }
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

    /// Effective paragraph formatting, including built-in defaults and inheritance.
    pub fn resolved_properties(&self, id: StyleId) -> StyleProperties {
        let mut chain = Vec::new();
        let mut current = Some(id);
        while let Some(id) = current {
            let Some(style) = self.get(id) else { break };
            if chain.len() >= self.order.len() {
                break;
            }
            chain.push(style);
            current = style.inherits;
        }
        let mut resolved = StyleProperties::default();
        for style in chain.into_iter().rev() {
            if style.inherits.is_none() {
                resolved.overlay(&StyleProperties::for_role(style.role));
            }
            resolved.overlay(&style.properties);
        }
        resolved
    }

    /// Builds an explicitly ordered catalog, as stored by the canonical
    /// project manifest. Explicit catalogs must contain every reserved style
    /// exactly once and must use the fixed ID assigned to that role.
    pub fn from_definitions(
        definitions: impl IntoIterator<Item = StyleDefinition>,
    ) -> Result<Self, DomainError> {
        let definitions = definitions.into_iter().collect::<Vec<_>>();
        let mut catalog = Self {
            definitions: BTreeMap::new(),
            order: Vec::new(),
        };
        for definition in definitions {
            validate_style_definition(&definition)?;
            if catalog
                .definitions
                .insert(definition.id, definition.clone())
                .is_some()
            {
                return Err(DomainError::DuplicateId { field: "style ID" });
            }
            if catalog.definitions.values().any(|candidate| {
                candidate.id != definition.id
                    && candidate.role == definition.role
                    && definition.role.is_reserved()
            }) {
                return Err(DomainError::DuplicateId {
                    field: "reserved style role",
                });
            }
            catalog.order.push(definition.id);
        }
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if self.order.len() != self.definitions.len()
            || self.order.iter().copied().collect::<BTreeSet<_>>().len() != self.order.len()
            || self
                .order
                .iter()
                .any(|id| !self.definitions.contains_key(id))
        {
            return Err(DomainError::InvalidInput {
                field: "style order",
                reason: "must contain every style exactly once",
            });
        }
        for (id, role) in [
            (Self::body_id(), StyleRole::Body),
            (Self::document_title_id(), StyleRole::DocumentTitle),
            (Self::heading_1_id(), StyleRole::Heading1),
            (Self::heading_2_id(), StyleRole::Heading2),
            (Self::heading_3_id(), StyleRole::Heading3),
            (Self::block_quote_id(), StyleRole::BlockQuote),
            (Self::verse_id(), StyleRole::Verse),
        ] {
            if self.definitions.get(&id).map(|style| style.role) != Some(role) {
                return Err(DomainError::InvalidInput {
                    field: "reserved style",
                    reason: "reserved role and ID do not match",
                });
            }
        }
        for definition in self.definitions.values() {
            validate_style_definition(definition)?;
            if let Some(parent) = definition.inherits {
                if parent == definition.id || !self.definitions.contains_key(&parent) {
                    return Err(DomainError::InvalidInput {
                        field: "style inheritance",
                        reason: "parent style must exist and differ from the child",
                    });
                }
                let mut cursor = Some(parent);
                let mut seen = BTreeSet::new();
                while let Some(id) = cursor {
                    if !seen.insert(id) || id == definition.id {
                        return Err(DomainError::InvalidInput {
                            field: "style inheritance",
                            reason: "inheritance cycles are not allowed",
                        });
                    }
                    cursor = self.definitions.get(&id).and_then(|style| style.inherits);
                }
            }
        }
        Ok(())
    }

    pub fn upsert(&mut self, definition: StyleDefinition) -> Result<(), DomainError> {
        validate_style_definition(&definition)?;
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
        self.validate()
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

fn validate_style_definition(definition: &StyleDefinition) -> Result<(), DomainError> {
    if definition.id.as_bytes().iter().all(|byte| *byte == 0) {
        return Err(DomainError::InvalidInput {
            field: "style ID",
            reason: "must not be the nil ID",
        });
    }
    if definition.display_name.trim().is_empty()
        || definition.display_name.chars().any(char::is_control)
    {
        return Err(DomainError::InvalidInput {
            field: "style display name",
            reason: "must be non-empty text without control characters",
        });
    }
    if let Some(family) = &definition.properties.font_family
        && (family.trim().is_empty() || family.chars().any(char::is_control))
    {
        return Err(DomainError::InvalidInput {
            field: "style font family",
            reason: "must be non-empty text without control characters",
        });
    }
    if definition
        .properties
        .font_size_points
        .is_some_and(|value| !value.is_finite() || value <= 0.0)
    {
        return Err(DomainError::InvalidInput {
            field: "style font size",
            reason: "must be a positive finite point value",
        });
    }
    if definition
        .properties
        .weight
        .is_some_and(|value| !(1..=1000).contains(&value))
    {
        return Err(DomainError::InvalidInput {
            field: "style font weight",
            reason: "must be between 1 and 1000",
        });
    }
    if definition
        .properties
        .line_spacing
        .is_some_and(|value| !value.is_finite() || value <= 0.0)
    {
        return Err(DomainError::InvalidInput {
            field: "style line spacing",
            reason: "must be a positive finite multiplier",
        });
    }
    for value in [
        definition.properties.first_line_indent_points,
        definition.properties.left_indent_points,
        definition.properties.right_indent_points,
        definition.properties.space_before_points,
        definition.properties.space_after_points,
    ]
    .into_iter()
    .flatten()
    {
        if !value.is_finite() {
            return Err(DomainError::InvalidInput {
                field: "style point value",
                reason: "must be finite",
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataApplicability {
    Groups,
    Documents,
    GroupsAndDocuments,
}

impl MetadataApplicability {
    pub const ALL: &'static [Self] = &[Self::Groups, Self::Documents, Self::GroupsAndDocuments];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Groups => "Groups",
            Self::Documents => "Documents",
            Self::GroupsAndDocuments => "Groups and documents",
        }
    }

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

impl MetadataTextKind {
    pub const ALL: &'static [Self] = &[Self::SingleLine, Self::Multiline];
    pub const fn label(self) -> &'static str {
        match self {
            Self::SingleLine => "Single line",
            Self::Multiline => "Multiline",
        }
    }
}
impl std::fmt::Display for MetadataApplicability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}
impl std::fmt::Display for MetadataTextKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
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
        if definition.id.as_bytes().iter().all(|byte| *byte == 0) {
            return Err(DomainError::InvalidInput {
                field: "metadata field ID",
                reason: "must not be the nil ID",
            });
        }
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
    use super::*;

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

    #[test]
    fn metadata_catalog_preserves_stable_identity_through_edit_reorder_and_delete() {
        let first = MetadataFieldId::from_bytes([1; 16]);
        let second = MetadataFieldId::from_bytes([2; 16]);
        let mut catalog = MetadataCatalog::default();
        for (id, label) in [(first, "Status"), (second, "Location")] {
            catalog
                .upsert(MetadataFieldDefinition {
                    id,
                    label: label.into(),
                    description: None,
                    applicability: MetadataApplicability::GroupsAndDocuments,
                    text_kind: MetadataTextKind::SingleLine,
                    default_value: None,
                    visible_on_cards: false,
                })
                .unwrap();
        }
        let mut renamed = catalog.get(first).unwrap().clone();
        renamed.label = "Draft status".into();
        catalog.upsert(renamed).unwrap();
        catalog.move_to(second, 0).unwrap();
        assert_eq!(
            catalog.iter().map(|field| field.id).collect::<Vec<_>>(),
            [second, first]
        );
        assert_eq!(catalog.remove(first).unwrap().id, first);
        assert_eq!(
            catalog.iter().map(|field| field.id).collect::<Vec<_>>(),
            [second]
        );
    }

    #[test]
    fn style_catalog_protects_reserved_styles_and_rejects_invalid_properties() {
        let mut catalog = StyleCatalog::default();
        assert!(catalog.remove(StyleCatalog::body_id()).is_err());

        let mut body = catalog.get(StyleCatalog::body_id()).unwrap().clone();
        body.properties.font_size_points = Some(f32::NAN);
        assert!(catalog.upsert(body).is_err());

        let custom_id = StyleId::from_bytes([8; 16]);
        catalog
            .upsert(StyleDefinition::custom(custom_id, "Custom"))
            .unwrap();
        let mut child = StyleDefinition::custom(StyleId::from_bytes([9; 16]), "Child");
        child.inherits = Some(custom_id);
        catalog.upsert(child).unwrap();
        assert!(catalog.remove(custom_id).is_err());
    }
}
