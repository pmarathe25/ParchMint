//! ParchMint-owned Iced appearance boundary.
//!
//! Screens select semantic roles through this module. They never branch on
//! Light/Dark or read the Penpot archive.

use iced::{Color, Theme, theme::Palette};
use parchmint_design_system::{
    generated_penpot_tokens::{PENPOT_TOKEN_SOURCE_SHA256, TOKENS},
    production_token,
};
use parchmint_preferences::ResolvedAppearance;

/// Provenance for the generated production token set.
pub const TOKEN_SOURCE_SHA256: &str = PENPOT_TOKEN_SOURCE_SHA256;

/// Fixed desktop metrics from the native design source.
pub const RIBBON_HEIGHT: u16 = 52;
pub const STATUS_HEIGHT: u16 = 32;
pub const COMPACT_CONTROL_HEIGHT: u16 = 28;
pub const CONTROL_HEIGHT: u16 = 36;
pub const CORE_ICON_SIZE: u16 = 20;
pub const DEFAULT_RADIUS: f32 = 4.0;
pub const FOCUS_BORDER_WIDTH: f32 = 2.0;

/// Exact launcher composition metrics from the Penpot launcher frame.
///
/// These constants are intentionally reusable: the launcher is a full-window
/// application surface, so future first-run and project-picker surfaces use
/// the same reading-column geometry instead of independent magic numbers.
pub const LAUNCHER_INSET: u16 = 72;
pub const LAUNCHER_RHYTHM: u16 = 28;
pub const LAUNCHER_ACTION_ROW_HEIGHT: u16 = 52;
pub const LAUNCHER_PROJECT_CARD_WIDTH: u16 = 520;
pub const LAUNCHER_PROJECT_CARD_HEIGHT: u16 = 96;
pub const LAUNCHER_PROJECT_CARD_GAP: u16 = 22;
pub const LAUNCHER_PROJECT_CARD_HORIZONTAL_PADDING: u16 = 16;
pub const LAUNCHER_PROJECT_CARD_VERTICAL_PADDING: u16 = 10;
pub const LAUNCHER_PROJECT_ICON_SIZE: u16 = 20;
pub const LAUNCHER_LAST_OPENED_ICON_SIZE: u16 = 14;
pub const LAUNCHER_PROJECT_TITLE_WIDTH: u16 = 124;
pub const LAUNCHER_PROJECT_HEADER_GAP: u16 = 14;
pub const LAUNCHER_PROJECT_METADATA_GAP: u16 = 12;
pub const LAUNCHER_PROJECT_NAME_MAX_CHARS: usize = 24;
pub const LAUNCHER_PROJECT_PATH_MAX_CHARS: usize = 38;

/// Exact launcher type sizes from the Penpot source.
pub const LAUNCHER_WORDMARK_SIZE: u16 = 24;
pub const LAUNCHER_TITLE_SIZE: u16 = 24;
pub const LAUNCHER_SUBTITLE_SIZE: u16 = 14;
pub const LAUNCHER_PROJECT_NAME_SIZE: u16 = 15;
pub const LAUNCHER_PROJECT_PATH_SIZE: u16 = 12;
pub const LAUNCHER_PROJECT_LAST_OPENED_SIZE: u16 = 11;

/// Application typography contract. The matching Source Sans 3 assets are
/// bundled by the native and deterministic headless renderers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    pub family: &'static str,
    pub size: u16,
    pub weight: u16,
    pub line_height: f32,
}

pub const UI_BODY: Typography = Typography {
    family: "Source Sans 3",
    size: 14,
    weight: 400,
    line_height: 1.4,
};
pub const UI_COMPACT: Typography = Typography {
    family: "Source Sans 3",
    size: 12,
    weight: 400,
    line_height: 1.35,
};
pub const UI_LABEL: Typography = Typography {
    family: "Source Sans 3",
    size: 12,
    weight: 600,
    line_height: 1.25,
};
pub const UI_HEADING: Typography = Typography {
    family: "Source Sans 3",
    size: 16,
    weight: 600,
    line_height: 1.25,
};
pub const UI_TAB: Typography = Typography {
    family: "Source Sans 3",
    size: 13,
    weight: 500,
    line_height: 1.2,
};
pub const UI_MENU: Typography = Typography {
    family: "Source Sans 3",
    size: 13,
    weight: 400,
    line_height: 1.35,
};
pub const UI_STATUS: Typography = Typography {
    family: "Source Sans 3",
    size: 12,
    weight: 400,
    line_height: 1.2,
};
pub const UI_CODE_PATH: Typography = Typography {
    family: "ui-monospace, Menlo, Consolas, Liberation Mono",
    size: 12,
    weight: 400,
    line_height: 1.4,
};

/// Bundled families used by the native and deterministic headless renderers.
pub const BUNDLED_FONT_FAMILIES: &[&str] = &["Source Sans 3", "Source Serif 4"];

/// All colors used by shared widgets. Field names are roles, never hue names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticPalette {
    pub application: Color,
    pub sidebar: Color,
    pub panel: Color,
    pub manuscript: Color,
    pub elevated: Color,
    pub sunken: Color,
    pub primary_text: Color,
    pub secondary_text: Color,
    pub muted_text: Color,
    pub disabled_text: Color,
    pub on_accent_text: Color,
    pub placeholder_text: Color,
    pub border: Color,
    pub strong_border: Color,
    pub divider: Color,
    pub focus_ring: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_pressed: Color,
    pub accent_subtle: Color,
    pub control_hover: Color,
    pub control_pressed: Color,
    pub control_disabled: Color,
    pub selection: Color,
    pub selection_border: Color,
    pub search_match: Color,
    pub search_match_active: Color,
    pub comment_highlight: Color,
    pub comment_active: Color,
    pub comment_resolved: Color,
    pub comment_orphaned: Color,
    pub success: Color,
    pub saving: Color,
    pub warning: Color,
    pub error: Color,
    pub success_subtle: Color,
    pub saving_subtle: Color,
    pub warning_subtle: Color,
    pub error_subtle: Color,
    pub destructive: Color,
    pub destructive_subtle: Color,
    pub scrim: Color,
}

/// A theme selected once per window from the preferences appearance snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParchMintTheme {
    appearance: ResolvedAppearance,
    palette: SemanticPalette,
}

impl ParchMintTheme {
    pub fn new(appearance: ResolvedAppearance) -> Self {
        let dark = appearance == ResolvedAppearance::Dark;
        let color = |role| semantic_color(role, dark);
        Self {
            appearance,
            palette: SemanticPalette {
                application: color("color.canvas.application"),
                sidebar: color("color.surface.sidebar"),
                panel: color("color.surface.panel"),
                manuscript: color("color.surface.manuscript"),
                elevated: color("color.surface.elevated"),
                sunken: color("color.surface.sunken"),
                primary_text: color("color.text.primary"),
                secondary_text: color("color.text.secondary"),
                muted_text: color("color.text.muted"),
                disabled_text: color("color.text.disabled"),
                on_accent_text: color("color.text.on-accent"),
                placeholder_text: color("color.text.placeholder"),
                border: color("color.border.default"),
                strong_border: color("color.border.strong"),
                divider: color("color.divider"),
                focus_ring: color("color.focus.ring"),
                accent: color("color.accent.default"),
                accent_hover: color("color.accent.hover"),
                accent_pressed: color("color.accent.pressed"),
                accent_subtle: color("color.accent.subtle"),
                control_hover: color("color.control.hover"),
                control_pressed: color("color.control.pressed"),
                control_disabled: color("color.control.disabled.background"),
                selection: color("color.selection.background"),
                selection_border: color("color.selection.border"),
                search_match: color("color.search.match"),
                search_match_active: color("color.search.match.active"),
                comment_highlight: color("color.comment.highlight"),
                comment_active: color("color.comment.active"),
                comment_resolved: color("color.comment.resolved"),
                comment_orphaned: color("color.comment.orphaned"),
                success: color("color.status.success"),
                saving: color("color.status.saving"),
                warning: color("color.status.warning"),
                error: color("color.status.error"),
                success_subtle: color("color.status.success.subtle"),
                saving_subtle: color("color.status.saving.subtle"),
                warning_subtle: color("color.status.warning.subtle"),
                error_subtle: color("color.status.error.subtle"),
                destructive: color("color.destructive.default"),
                destructive_subtle: color("color.destructive.subtle"),
                scrim: color("color.overlay.scrim"),
            },
        }
    }

    pub const fn appearance(self) -> ResolvedAppearance {
        self.appearance
    }
    pub const fn palette(self) -> SemanticPalette {
        self.palette
    }

    /// The underlying Iced theme only supplies generic widget defaults; shared
    /// components use `SemanticPalette` for roles Iced does not model.
    pub fn iced_theme(self) -> Theme {
        Theme::custom(
            match self.appearance {
                ResolvedAppearance::Light => "ParchMint Light",
                ResolvedAppearance::Dark => "ParchMint Dark",
            },
            Palette {
                background: self.palette.application,
                text: self.palette.primary_text,
                primary: self.palette.accent,
                success: self.palette.success,
                warning: self.palette.warning,
                danger: self.palette.error,
            },
        )
    }

    /// Recovers a ParchMint appearance from one of this module's generated
    /// Iced themes. This lets reusable widget styles remain semantic in both
    /// native and headless rendering without a parallel Light/Dark branch.
    pub fn from_iced_theme(theme: &Theme) -> Option<Self> {
        [ResolvedAppearance::Light, ResolvedAppearance::Dark]
            .into_iter()
            .map(Self::new)
            .find(|candidate| candidate.iced_theme().palette() == theme.palette())
    }
}

fn semantic_color(role: &str, dark: bool) -> Color {
    let token = production_token(role)
        .unwrap_or_else(|| panic!("missing generated semantic token: {role}"));
    color_from_hex(if dark { token.dark } else { token.light })
}

fn color_from_hex(value: &str) -> Color {
    let value = value
        .strip_prefix('#')
        .expect("generated colors use #RRGGBB[AA]");
    let component = |index| {
        u8::from_str_radix(&value[index..index + 2], 16).expect("generated color is valid hex")
    };
    let alpha = if value.len() == 8 {
        component(6) as f32 / 255.0
    } else {
        1.0
    };
    Color::from_rgba8(component(0), component(2), component(4), alpha)
}

/// Makes provenance and semantic colors part of the binary, without parsing
/// the source archive or a JSON token file at runtime.
pub fn generated_token_count() -> usize {
    TOKENS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_iced_themes_keep_light_and_dark_semantic_roles_distinct() {
        let light = ParchMintTheme::new(ResolvedAppearance::Light);
        let dark = ParchMintTheme::new(ResolvedAppearance::Dark);
        assert_ne!(light.palette().application, dark.palette().application);
        assert_ne!(light.palette().manuscript, dark.palette().manuscript);
        assert_eq!(light.iced_theme().palette().primary, light.palette().accent);
        assert_eq!(dark.iced_theme().palette().danger, dark.palette().error);
        assert!(generated_token_count() >= 50);
        assert_eq!(
            ParchMintTheme::from_iced_theme(&light.iced_theme()),
            Some(light)
        );
        assert_eq!(
            ParchMintTheme::from_iced_theme(&dark.iced_theme()),
            Some(dark)
        );
        assert_eq!(LAUNCHER_INSET, 72);
        assert_eq!(LAUNCHER_PROJECT_CARD_WIDTH, 520);
        assert_eq!(LAUNCHER_PROJECT_CARD_HEIGHT, 96);
    }
}
