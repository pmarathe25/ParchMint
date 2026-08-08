//! ParchMint-owned contracts for desktop operating-system features.
//!
//! This crate intentionally contains values and service traits only.  Native
//! windows, event loops, shell objects, and operating-system callbacks remain
//! private to the UI and native-platform adapters.  Every window-scoped call
//! receives a [`WindowCapability`]; native implementations must validate its
//! exact generation immediately before dispatching work.

use std::{
    error::Error,
    fmt,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};

/// A `Send` future returned by a platform operation that may wait on native
/// work.  Native adapters use [`PlatformError::StaleCapability`] when the
/// supplied window no longer belongs to their live-window registry.
pub type AsyncResult<T> = Pin<Box<dyn Future<Output = Result<T, PlatformError>> + Send + 'static>>;

/// One ParchMint-owned capability for a live native window.
///
/// `window_id` identifies the logical window while `generation` changes when
/// that window is closed or re-created.  It is deliberately a value rather
/// than a native-window handle.  The native adapter owns authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowCapability {
    window_id: u64,
    generation: u64,
}

impl WindowCapability {
    /// Creates a capability issued by the private UI/native registration
    /// boundary.
    pub const fn new(window_id: u64, generation: u64) -> Self {
        Self {
            window_id,
            generation,
        }
    }

    /// The ParchMint logical window identifier.
    pub const fn window_id(self) -> u64 {
        self.window_id
    }

    /// The generation that native adapters must authorize exactly.
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// A result or callback associated with the window that started the work.
///
/// Receiving code must compare [`Self::window`] with its live capability
/// before using [`Self::value`]. This keeps an exact generation with every
/// window-scoped result without exposing a native handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowResult<T> {
    window: WindowCapability,
    value: T,
}

impl<T> WindowResult<T> {
    pub const fn new(window: WindowCapability, value: T) -> Self {
        Self { window, value }
    }

    pub const fn window(&self) -> WindowCapability {
        self.window
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

/// A platform operation could not be completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// The capability was unknown, closed, or superseded before dispatch.
    StaleCapability { window_id: u64, generation: u64 },
    /// The requested feature is not available on the current platform.
    Unavailable { operation: &'static str },
    /// The platform rejected or could not complete an operation.
    Failed {
        operation: &'static str,
        reason: String,
    },
}

impl PlatformError {
    /// Builds the required stale-capability failure for a rejected window.
    pub const fn stale_capability(capability: WindowCapability) -> Self {
        Self::StaleCapability {
            window_id: capability.window_id,
            generation: capability.generation,
        }
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleCapability {
                window_id,
                generation,
            } => write!(
                formatter,
                "stale window capability for window {window_id}, generation {generation}"
            ),
            Self::Unavailable { operation } => {
                write!(formatter, "platform operation is unavailable: {operation}")
            }
            Self::Failed { operation, reason } => {
                write!(
                    formatter,
                    "platform operation failed ({operation}): {reason}"
                )
            }
        }
    }
}

impl Error for PlatformError {}

/// A menu expressed in ParchMint semantics rather than native menu objects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticMenu {
    entries: Vec<SemanticMenuEntry>,
}

impl SemanticMenu {
    pub fn new(entries: Vec<SemanticMenuEntry>) -> Self {
        Self { entries }
    }

    pub fn entries(&self) -> &[SemanticMenuEntry] {
        &self.entries
    }
}

/// An entry in a semantic menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticMenuEntry {
    Command(MenuCommand),
    Separator,
    Submenu { label: String, entries: Vec<Self> },
}

/// One ParchMint command made available through a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuCommand {
    id: String,
    label: String,
    enabled: bool,
}

impl MenuCommand {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
        }
    }

    pub fn disabled(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: false,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

/// A ParchMint-owned binding for one installed native menu.
///
/// Native menu callbacks retain this value so the receiving UI can reject an
/// activation whose window generation no longer matches a live window. Its
/// value is an adapter-issued ID, not a native handle.
pub type MenuBinding = WindowResult<u64>;

/// One native menu activation represented without a native callback object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuActivation {
    pub binding: MenuBinding,
    pub command_id: String,
}

/// A request for a native path dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathDialog {
    pub kind: PathDialogKind,
    pub title: Option<String>,
}

/// The intended selection in a [`PathDialog`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PathDialogKind {
    #[default]
    OpenFile,
    OpenDirectory,
    SaveFile,
}

/// A path returned from a native dialog before a receiving boundary checks it.
///
/// This wrapper does not grant filesystem authority.  A project/filesystem
/// boundary must validate the path and acquire its own capability before I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrustedPathSelection {
    path: PathBuf,
}

impl UntrustedPathSelection {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns data for validation only; it is not an authorized project path.
    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

/// Clipboard formats a caller is prepared to receive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClipboardFormats {
    #[default]
    PlainText,
    PlainTextAndHtml,
}

impl ClipboardFormats {
    pub const fn plain_text() -> Self {
        Self::PlainText
    }

    pub const fn plain_text_and_html() -> Self {
        Self::PlainTextAndHtml
    }

    pub const fn accepts_plain_text(self) -> bool {
        true
    }

    pub const fn accepts_html(self) -> bool {
        matches!(self, Self::PlainTextAndHtml)
    }
}

/// Clipboard content ParchMint is prepared to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardContent {
    plain_text: String,
}

impl ClipboardContent {
    pub fn plain_text(text: impl Into<String>) -> Self {
        Self {
            plain_text: text.into(),
        }
    }

    pub fn as_plain_text(&self) -> &str {
        &self.plain_text
    }
}

/// Data received from the system clipboard before ParchMint validates it.
///
/// Both text and HTML can originate outside ParchMint.  Consumers must apply
/// format, size, and sanitization policy before treating either value as
/// editor or project content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrustedClipboardContent {
    plain_text: Option<String>,
    html: Option<String>,
}

impl UntrustedClipboardContent {
    /// Builds an empty clipboard result.
    pub const fn empty() -> Self {
        Self {
            plain_text: None,
            html: None,
        }
    }

    pub fn with_plain_text(mut self, text: impl Into<String>) -> Self {
        self.plain_text = Some(text.into());
        self
    }

    pub fn with_html(mut self, html: impl Into<String>) -> Self {
        self.html = Some(html.into());
        self
    }

    pub fn plain_text(&self) -> Option<&str> {
        self.plain_text.as_deref()
    }

    pub fn html(&self) -> Option<&str> {
        self.html.as_deref()
    }
}

/// A checked external URL that may be passed to [`ExternalOpenService`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValidatedExternalIntent {
    url: String,
    action: ExternalOpenAction,
}

impl ValidatedExternalIntent {
    /// Validates a user-facing HTTPS link for the platform external-open
    /// boundary.  This deliberately excludes arbitrary schemes, paths, and
    /// shell commands; v1 only opens HTTPS URLs through the user's browser.
    pub fn https_url(url: impl AsRef<str>) -> Result<Self, ExternalIntentError> {
        let url = url.as_ref();
        validate_https_url(url)?;
        Ok(Self {
            url: url.to_owned(),
            action: ExternalOpenAction::OpenInBrowser,
        })
    }

    pub const fn scheme(&self) -> &'static str {
        "https"
    }

    pub fn as_url(&self) -> &str {
        &self.url
    }

    /// The only v1 external action.  The typed intent never contains a shell
    /// command, file action, or arbitrary application target.
    pub const fn action(&self) -> ExternalOpenAction {
        self.action
    }
}

/// The ParchMint action permitted for a validated external intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExternalOpenAction {
    OpenInBrowser,
}

/// Why an external URL did not meet the external-open policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalIntentError {
    Scheme,
    MissingAuthority,
    InvalidAuthority,
    UnsafeCharacter,
    InvalidEscape,
}

impl fmt::Display for ExternalIntentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::Scheme => "only HTTPS URLs are allowed",
            Self::MissingAuthority => "an HTTPS URL must include a host",
            Self::InvalidAuthority => "the URL host or port is invalid",
            Self::UnsafeCharacter => "the URL contains an unsafe character",
            Self::InvalidEscape => "the URL contains an invalid percent escape",
        };
        formatter.write_str(reason)
    }
}

impl Error for ExternalIntentError {}

fn validate_https_url(url: &str) -> Result<(), ExternalIntentError> {
    let Some(remainder) = url.strip_prefix("https://") else {
        return Err(ExternalIntentError::Scheme);
    };
    if remainder.is_empty() {
        return Err(ExternalIntentError::MissingAuthority);
    }
    if !url.is_ascii()
        || url
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace() || byte == b'\\')
    {
        return Err(ExternalIntentError::UnsafeCharacter);
    }

    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    if authority.is_empty() {
        return Err(ExternalIntentError::MissingAuthority);
    }
    validate_authority(authority)?;

    let suffix = &remainder[authority_end..];
    validate_percent_escapes(suffix)
}

fn validate_authority(authority: &str) -> Result<(), ExternalIntentError> {
    if authority.contains('@') {
        return Err(ExternalIntentError::InvalidAuthority);
    }

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if host.is_empty() || host.contains(':') || !host.split('.').all(valid_host_label) {
        return Err(ExternalIntentError::InvalidAuthority);
    }
    if let Some(port) = port {
        let Ok(port) = port.parse::<u16>() else {
            return Err(ExternalIntentError::InvalidAuthority);
        };
        if port == 0 {
            return Err(ExternalIntentError::InvalidAuthority);
        }
    }
    Ok(())
}

fn valid_host_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn validate_percent_escapes(value: &str) -> Result<(), ExternalIntentError> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ExternalIntentError::InvalidEscape);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

/// ParchMint-owned application directories.  They are path values, not open
/// filesystem handles; callers retain responsibility for safe file handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationPaths {
    configuration: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

impl ApplicationPaths {
    pub fn new(
        configuration: impl Into<PathBuf>,
        data: impl Into<PathBuf>,
        cache: impl Into<PathBuf>,
    ) -> Self {
        Self {
            configuration: configuration.into(),
            data: data.into(),
            cache: cache.into(),
        }
    }

    pub fn configuration(&self) -> &Path {
        &self.configuration
    }

    pub fn data(&self) -> &Path {
        &self.data
    }

    pub fn cache(&self) -> &Path {
        &self.cache
    }
}

/// The current system appearance resolved by the operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAppearance {
    Light,
    Dark,
}

/// Installs semantic menus for a live window.
pub trait MenuService: Send + Sync {
    /// Implementations authorize `window` immediately before native dispatch.
    fn install(&self, window: WindowCapability, menu: SemanticMenu) -> AsyncResult<MenuBinding>;
}

/// Opens native file and directory dialogs for a live window.
pub trait DialogService: Send + Sync {
    /// Returned selections remain untrusted until the receiving boundary
    /// validates them for its intended filesystem operation.
    fn choose_path(
        &self,
        window: WindowCapability,
        request: PathDialog,
    ) -> AsyncResult<WindowResult<Option<UntrustedPathSelection>>>;
}

/// Reads and writes the system clipboard for a live window.
pub trait ClipboardService: Send + Sync {
    /// Returned clipboard data is untrusted external input.
    fn read(
        &self,
        window: WindowCapability,
        formats: ClipboardFormats,
    ) -> AsyncResult<WindowResult<UntrustedClipboardContent>>;

    fn write(
        &self,
        window: WindowCapability,
        content: ClipboardContent,
    ) -> AsyncResult<WindowResult<()>>;
}

/// Opens a prevalidated user-facing link through the platform's external
/// browser/application mechanism.
pub trait ExternalOpenService: Send + Sync {
    fn open(
        &self,
        window: WindowCapability,
        intent: ValidatedExternalIntent,
    ) -> AsyncResult<WindowResult<()>>;
}

/// Resolves application-only platform directories without exposing a native
/// filesystem handle.
pub trait ApplicationPathService: Send + Sync {
    fn application_paths(&self) -> AsyncResult<ApplicationPaths>;
}

/// Reads the current operating-system appearance without exposing platform
/// framework values.
pub trait SystemAppearanceService: Send + Sync {
    fn current_appearance(&self) -> AsyncResult<SystemAppearance>;
}
