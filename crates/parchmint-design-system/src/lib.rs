//! Deterministic, framework-neutral data generated from ParchMint's UI source.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use serde_json::Value;
use sha2::{Digest, Sha256};

const REQUIRED_SEMANTIC_ROLES: [&str; 2] = ["color.surface.application", "color.text.primary"];

/// The source material used to generate a design-system snapshot.
#[derive(Clone, Debug)]
pub struct DesignSource {
    token_json: String,
    token_checksum: String,
    icons: Vec<SourceIcon>,
}

#[derive(Clone, Debug)]
struct SourceIcon {
    name: String,
    svg: String,
    checksum: String,
}

impl DesignSource {
    /// Builds a source from a DTCG-style token document and product SVG vectors.
    pub fn from_token_json_and_icons(
        token_json: impl Into<String>,
        icons: Vec<(String, String)>,
    ) -> Self {
        let token_json = token_json.into();
        Self {
            token_checksum: sha256(token_json.as_bytes()),
            token_json,
            icons: icons
                .into_iter()
                .map(|(name, svg)| SourceIcon {
                    checksum: sha256(svg.as_bytes()),
                    name,
                    svg,
                })
                .collect(),
        }
    }

    /// Replaces the recorded token-source checksum. This is useful when reading
    /// an externally indexed design export.
    #[must_use]
    pub fn with_token_checksum(mut self, checksum: impl Into<String>) -> Self {
        self.token_checksum = checksum.into();
        self
    }

    /// Replaces an icon's recorded source checksum.
    #[must_use]
    pub fn with_icon_checksum(mut self, name: &str, checksum: impl Into<String>) -> Self {
        if let Some(icon) = self.icons.iter_mut().find(|icon| icon.name == name) {
            icon.checksum = checksum.into();
        }
        self
    }
}

/// Errors found while compiling the maintained design source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenerationError {
    InvalidTokenSource {
        message: String,
    },
    InvalidToken {
        name: String,
        message: String,
    },
    TokenChecksumMismatch {
        expected: String,
        actual: String,
    },
    DuplicateToken {
        name: String,
    },
    MissingAlias {
        token: String,
        alias: String,
    },
    AliasCycle {
        token: String,
    },
    MissingThemeRole {
        role: String,
        appearance: String,
    },
    MissingSemanticRole {
        role: String,
    },
    MissingIcon {
        icon: String,
    },
    DuplicateIcon {
        icon: String,
    },
    VectorChecksumMismatch {
        icon: String,
        expected: String,
        actual: String,
    },
    InvalidSvg {
        icon: String,
        message: String,
    },
}

impl fmt::Display for GenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTokenSource { message } => {
                write!(formatter, "invalid token source: {message}")
            }
            Self::InvalidToken { name, message } => {
                write!(formatter, "invalid token {name}: {message}")
            }
            Self::TokenChecksumMismatch { expected, actual } => write!(
                formatter,
                "token source checksum mismatch: expected {expected}, got {actual}"
            ),
            Self::DuplicateToken { name } => write!(formatter, "duplicate token: {name}"),
            Self::MissingAlias { token, alias } => {
                write!(formatter, "token {token} aliases missing token {alias}")
            }
            Self::AliasCycle { token } => write!(formatter, "token alias cycle includes {token}"),
            Self::MissingThemeRole { role, appearance } => {
                write!(formatter, "token {role} has no {appearance} value")
            }
            Self::MissingSemanticRole { role } => {
                write!(formatter, "missing semantic role: {role}")
            }
            Self::MissingIcon { icon } => write!(formatter, "missing product icon: {icon}"),
            Self::DuplicateIcon { icon } => write!(formatter, "duplicate icon: {icon}"),
            Self::VectorChecksumMismatch {
                icon,
                expected,
                actual,
            } => write!(
                formatter,
                "vector checksum mismatch for {icon}: expected {expected}, got {actual}"
            ),
            Self::InvalidSvg { icon, message } => {
                write!(formatter, "invalid SVG {icon}: {message}")
            }
        }
    }
}

impl std::error::Error for GenerationError {}

/// A generated semantic token. Values are strings because the design-token
/// source can represent colors, dimensions, and font names without choosing a
/// UI toolkit type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticToken {
    name: String,
    token_type: String,
    value: String,
    light: Option<String>,
    dark: Option<String>,
}

impl SemanticToken {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn token_type(&self) -> &str {
        &self.token_type
    }

    /// Returns the shared value, or the Light value for a themed token.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// A source SVG kept as vector geometry and colored by semantic roles at render time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorIcon {
    name: String,
    view_box: String,
    checksum: String,
}

impl VectorIcon {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn view_box(&self) -> &str {
        &self.view_box
    }

    pub fn checksum(&self) -> &str {
        &self.checksum
    }

    pub fn is_monochrome(&self) -> bool {
        true
    }
}

/// The shared icon catalog for both appearances.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IconCatalog {
    icons: BTreeMap<String, VectorIcon>,
}

impl IconCatalog {
    pub fn len(&self) -> usize {
        self.icons.len()
    }

    pub fn is_empty(&self) -> bool {
        self.icons.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&VectorIcon> {
        self.icons.get(name)
    }
}

/// Framework-neutral result of compiling a design source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedDesignSystem {
    tokens: BTreeMap<String, SemanticToken>,
    icons: IconCatalog,
    source_digest: String,
    generated_rust: String,
}

impl GeneratedDesignSystem {
    pub fn token(&self, name: &str) -> &SemanticToken {
        &self.tokens[name]
    }

    pub fn icon(&self, name: &str) -> Option<&VectorIcon> {
        self.icons.get(name)
    }

    pub fn icon_catalog(&self) -> &IconCatalog {
        &self.icons
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    /// Generated Rust data with a stable ordering and no framework references.
    pub fn generated_rust(&self) -> &str {
        &self.generated_rust
    }

    pub fn theme_snapshot(&self, appearance: &str, generation: u64) -> ThemeSnapshot {
        let use_dark = appearance.eq_ignore_ascii_case("dark");
        let tokens = self
            .tokens
            .iter()
            .map(|(name, token)| {
                let value = if use_dark {
                    token.dark.as_ref().unwrap_or(&token.value)
                } else {
                    token.light.as_ref().unwrap_or(&token.value)
                };
                (name.clone(), value.clone())
            })
            .collect();

        ThemeSnapshot {
            appearance: if use_dark { "Dark" } else { "Light" }.to_owned(),
            generation,
            tokens,
            icons: self.icons.clone(),
        }
    }
}

/// The selected values and shared vectors for one appearance generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeSnapshot {
    appearance: String,
    generation: u64,
    tokens: BTreeMap<String, String>,
    icons: IconCatalog,
}

impl ThemeSnapshot {
    pub fn appearance(&self) -> &str {
        &self.appearance
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn token(&self, role: &str) -> Option<&str> {
        self.tokens.get(role).map(String::as_str)
    }

    pub fn role_names(&self) -> Vec<&str> {
        self.tokens.keys().map(String::as_str).collect()
    }

    pub fn icon_catalog(&self) -> &IconCatalog {
        &self.icons
    }
}

#[derive(Clone, Debug)]
struct RawToken {
    token_type: String,
    shared: Option<String>,
    light: Option<String>,
    dark: Option<String>,
}

impl RawToken {
    fn is_themed(&self) -> bool {
        self.light.is_some() || self.dark.is_some()
    }
}

/// Parses, validates, resolves, and deterministically renders the design source.
pub fn generate(source: DesignSource) -> Result<GeneratedDesignSystem, GenerationError> {
    verify_checksum(&source.token_checksum, source.token_json.as_bytes()).map_err(
        |(expected, actual)| GenerationError::TokenChecksumMismatch { expected, actual },
    )?;

    let raw_tokens = parse_tokens(&source.token_json)?;
    require_semantic_roles(&raw_tokens)?;
    let tokens = resolve_tokens(&raw_tokens)?;
    let icons = parse_icons(source.icons)?;
    let source_digest = source_digest(&source.token_json, &icons);
    let generated_rust = render_rust(&tokens, &icons, &source_digest);

    Ok(GeneratedDesignSystem {
        tokens,
        icons,
        source_digest,
        generated_rust,
    })
}

fn parse_tokens(token_json: &str) -> Result<BTreeMap<String, RawToken>, GenerationError> {
    let document: Value =
        serde_json::from_str(token_json).map_err(|error| GenerationError::InvalidTokenSource {
            message: error.to_string(),
        })?;
    let tokens = document
        .get("tokens")
        .ok_or_else(|| GenerationError::InvalidTokenSource {
            message: "missing tokens property".to_owned(),
        })?;

    let mut result = BTreeMap::new();
    match tokens {
        Value::Object(entries) => {
            for (name, definition) in entries {
                insert_token(&mut result, name, definition)?;
            }
        }
        Value::Array(entries) => {
            for definition in entries {
                let object =
                    definition
                        .as_object()
                        .ok_or_else(|| GenerationError::InvalidTokenSource {
                            message: "token array entries must be objects".to_owned(),
                        })?;
                let name = object.get("name").and_then(Value::as_str).ok_or_else(|| {
                    GenerationError::InvalidTokenSource {
                        message: "token array entry is missing a string name".to_owned(),
                    }
                })?;
                insert_token(&mut result, name, definition)?;
            }
        }
        _ => {
            return Err(GenerationError::InvalidTokenSource {
                message: "tokens must be an object or array".to_owned(),
            });
        }
    }
    Ok(result)
}

fn insert_token(
    result: &mut BTreeMap<String, RawToken>,
    name: &str,
    definition: &Value,
) -> Result<(), GenerationError> {
    if name.is_empty() {
        return Err(GenerationError::InvalidToken {
            name: name.to_owned(),
            message: "name cannot be empty".to_owned(),
        });
    }
    if result.contains_key(name) {
        return Err(GenerationError::DuplicateToken {
            name: name.to_owned(),
        });
    }
    let object = definition
        .as_object()
        .ok_or_else(|| GenerationError::InvalidToken {
            name: name.to_owned(),
            message: "definition must be an object".to_owned(),
        })?;
    let token_type = object
        .get("type")
        .or_else(|| object.get("$type"))
        .and_then(Value::as_str)
        .ok_or_else(|| GenerationError::InvalidToken {
            name: name.to_owned(),
            message: "missing string type".to_owned(),
        })?
        .to_owned();
    let has_light = object.contains_key("light");
    let has_dark = object.contains_key("dark");
    let light = scalar_value(object.get("light"));
    let dark = scalar_value(object.get("dark"));
    if has_light || has_dark {
        if light.is_none() {
            return Err(GenerationError::MissingThemeRole {
                role: name.to_owned(),
                appearance: "Light".to_owned(),
            });
        }
        if dark.is_none() {
            return Err(GenerationError::MissingThemeRole {
                role: name.to_owned(),
                appearance: "Dark".to_owned(),
            });
        }
    }
    let shared = scalar_value(object.get("value").or_else(|| object.get("$value")));
    if shared.is_none() && light.is_none() {
        return Err(GenerationError::InvalidToken {
            name: name.to_owned(),
            message: "missing value or Light/Dark values".to_owned(),
        });
    }
    result.insert(
        name.to_owned(),
        RawToken {
            token_type,
            shared,
            light,
            dark,
        },
    );
    Ok(())
}

fn scalar_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn require_semantic_roles(tokens: &BTreeMap<String, RawToken>) -> Result<(), GenerationError> {
    for role in REQUIRED_SEMANTIC_ROLES {
        if !tokens.contains_key(role) {
            return Err(GenerationError::MissingSemanticRole {
                role: role.to_owned(),
            });
        }
    }
    Ok(())
}

fn resolve_tokens(
    raw_tokens: &BTreeMap<String, RawToken>,
) -> Result<BTreeMap<String, SemanticToken>, GenerationError> {
    let mut resolved = BTreeMap::new();
    for (name, raw) in raw_tokens {
        let light = raw
            .is_themed()
            .then(|| resolve_value(raw_tokens, name, Some("Light"), &mut Vec::new()))
            .transpose()?;
        let dark = raw
            .is_themed()
            .then(|| resolve_value(raw_tokens, name, Some("Dark"), &mut Vec::new()))
            .transpose()?;
        let shared = if raw.is_themed() {
            None
        } else {
            Some(resolve_value(raw_tokens, name, None, &mut Vec::new())?)
        };
        let value = light.clone().or_else(|| shared.clone()).ok_or_else(|| {
            GenerationError::InvalidToken {
                name: name.clone(),
                message: "could not resolve a value".to_owned(),
            }
        })?;
        resolved.insert(
            name.clone(),
            SemanticToken {
                name: name.clone(),
                token_type: raw.token_type.clone(),
                value,
                light,
                dark,
            },
        );
    }
    Ok(resolved)
}

fn resolve_value(
    tokens: &BTreeMap<String, RawToken>,
    name: &str,
    appearance: Option<&str>,
    visiting: &mut Vec<String>,
) -> Result<String, GenerationError> {
    if visiting.iter().any(|token| token == name) {
        return Err(GenerationError::AliasCycle {
            token: name.to_owned(),
        });
    }
    visiting.push(name.to_owned());
    let result = (|| {
        let token = tokens
            .get(name)
            .ok_or_else(|| GenerationError::MissingAlias {
                token: name.to_owned(),
                alias: name.to_owned(),
            })?;
        let value = match appearance {
            Some("Light") => token.light.as_ref().or(token.shared.as_ref()),
            Some("Dark") => token.dark.as_ref().or(token.shared.as_ref()),
            _ => token.shared.as_ref(),
        }
        .ok_or_else(|| GenerationError::MissingThemeRole {
            role: name.to_owned(),
            appearance: appearance.unwrap_or("shared").to_owned(),
        })?;
        if let Some(alias) = alias_target(value) {
            if !tokens.contains_key(alias) {
                return Err(GenerationError::MissingAlias {
                    token: name.to_owned(),
                    alias: alias.to_owned(),
                });
            }
            resolve_value(tokens, alias, appearance, visiting)
        } else {
            Ok(normalize_value(&token.token_type, value))
        }
    })();
    visiting.pop();
    result
}

fn alias_target(value: &str) -> Option<&str> {
    value
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .filter(|value| !value.is_empty())
}

fn normalize_value(token_type: &str, value: &str) -> String {
    if token_type == "fontFamily"
        && (value.eq_ignore_ascii_case("sourcesanspro") || value.eq_ignore_ascii_case("inter"))
    {
        "Source Sans 3".to_owned()
    } else {
        value.to_owned()
    }
}

fn parse_icons(source_icons: Vec<SourceIcon>) -> Result<IconCatalog, GenerationError> {
    if source_icons.is_empty() {
        return Err(GenerationError::MissingIcon {
            icon: "product vector catalog".to_owned(),
        });
    }
    let mut icons = BTreeMap::new();
    for source in source_icons {
        if source.name.is_empty() {
            return Err(GenerationError::MissingIcon {
                icon: "unnamed vector".to_owned(),
            });
        }
        if icons.contains_key(&source.name) {
            return Err(GenerationError::DuplicateIcon { icon: source.name });
        }
        let actual = sha256(source.svg.as_bytes());
        if source.checksum != actual {
            return Err(GenerationError::VectorChecksumMismatch {
                icon: source.name,
                expected: source.checksum,
                actual,
            });
        }
        let view_box = parse_svg_view_box(&source.name, &source.svg)?;
        icons.insert(
            source.name.clone(),
            VectorIcon {
                name: source.name,
                view_box,
                checksum: actual,
            },
        );
    }
    Ok(IconCatalog { icons })
}

fn parse_svg_view_box(name: &str, svg: &str) -> Result<String, GenerationError> {
    let svg = svg.trim();
    let root = svg
        .strip_prefix("<svg")
        .ok_or_else(|| invalid_svg(name, "root element must be svg"))?;
    if !root
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '>' | '/') || character.is_ascii_whitespace())
    {
        return Err(invalid_svg(name, "root element must be svg"));
    }
    if svg.contains("<image") {
        return Err(invalid_svg(name, "raster image elements are not allowed"));
    }
    let end = svg
        .find('>')
        .ok_or_else(|| invalid_svg(name, "unterminated svg tag"))?;
    let opening_tag = &svg[..=end];
    let view_box = svg_attribute(opening_tag, "viewBox")
        .ok_or_else(|| invalid_svg(name, "missing viewBox"))?;
    if view_box.split_ascii_whitespace().count() != 4
        || view_box
            .split_ascii_whitespace()
            .any(|dimension| dimension.parse::<f32>().is_err())
    {
        return Err(invalid_svg(name, "viewBox must contain four numbers"));
    }
    if ![
        "path", "circle", "rect", "line", "polyline", "polygon", "ellipse", "g",
    ]
    .iter()
    .any(|element| contains_svg_element(svg, element))
    {
        return Err(invalid_svg(name, "missing vector geometry"));
    }
    Ok(view_box.to_owned())
}

fn contains_svg_element(svg: &str, element: &str) -> bool {
    let marker = format!("<{element}");
    let mut offset = 0;
    while let Some(start) = svg[offset..].find(&marker) {
        let end = offset + start + marker.len();
        if svg[end..].chars().next().is_some_and(|character| {
            matches!(character, '>' | '/') || character.is_ascii_whitespace()
        }) {
            return true;
        }
        offset = end;
    }
    false
}

fn svg_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=");
    let mut offset = 0;
    let start = loop {
        let position = tag[offset..].find(&marker)? + offset;
        if tag[..position]
            .chars()
            .next_back()
            .is_some_and(|character| character == '<' || character.is_ascii_whitespace())
        {
            break position + marker.len();
        }
        offset = position + marker.len();
    };
    let quote = tag.as_bytes().get(start).copied()?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let value_start = start + 1;
    let value_end = tag[value_start..].find(quote as char)? + value_start;
    Some(&tag[value_start..value_end])
}

fn invalid_svg(name: &str, message: &str) -> GenerationError {
    GenerationError::InvalidSvg {
        icon: name.to_owned(),
        message: message.to_owned(),
    }
}

fn verify_checksum(expected: &str, bytes: &[u8]) -> Result<(), (String, String)> {
    let actual = sha256(bytes);
    if expected == actual {
        Ok(())
    } else {
        Err((expected.to_owned(), actual))
    }
}

fn source_digest(token_json: &str, icons: &IconCatalog) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token_json.as_bytes());
    for icon in icons.icons.values() {
        hasher.update(icon.name.as_bytes());
        hasher.update(icon.checksum.as_bytes());
    }
    hex_digest(hasher.finalize().as_slice())
}

fn render_rust(
    tokens: &BTreeMap<String, SemanticToken>,
    icons: &IconCatalog,
    source_digest: &str,
) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "// Generated by parchmint-design-system. Do not edit."
    )
    .expect("write to string");
    writeln!(output, "pub const SOURCE_DIGEST: &str = {source_digest:?};")
        .expect("write to string");
    writeln!(
        output,
        "pub const TOKENS: &[(&str, &str, &str, Option<&str>, Option<&str>)] = &["
    )
    .expect("write to string");
    for token in tokens.values() {
        writeln!(
            output,
            "    ({:?}, {:?}, {:?}, {:?}, {:?}),",
            token.name, token.token_type, token.value, token.light, token.dark
        )
        .expect("write to string");
    }
    writeln!(output, "];\npub const ICONS: &[(&str, &str, &str)] = &[").expect("write to string");
    for icon in icons.icons.values() {
        writeln!(
            output,
            "    ({:?}, {:?}, {:?}),",
            icon.name, icon.view_box, icon.checksum
        )
        .expect("write to string");
    }
    writeln!(output, "];\n").expect("write to string");
    output
}

fn sha256(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut digest = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(digest, "{byte:02x}").expect("write to string");
    }
    digest
}
