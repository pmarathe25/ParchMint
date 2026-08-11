//! Generated from `docs/ui-design/parchmint-ui.penpot`.
//!
//! Source entry: `files/2be68822-842f-8175-8008-65eef13b0227/tokens.json`
//! Source SHA-256: `ad30015644a1d9c17c3bd8357d5e72c1e2e772a5adbd9288e8ff2b00a431ee70`.
//! Regenerate this file from the checked-in export; do not parse the archive at runtime.

/// Checksum of the exact Penpot `tokens.json` entry from which this file was generated.
pub const PENPOT_TOKEN_SOURCE_SHA256: &str =
    "ad30015644a1d9c17c3bd8357d5e72c1e2e772a5adbd9288e8ff2b00a431ee70";

/// Immutable value for the same semantic role in both appearances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedToken {
    pub name: &'static str,
    pub token_type: &'static str,
    pub light: &'static str,
    pub dark: &'static str,
}

/// Resolved, framework-neutral Light and Dark values used by production UI.
///
/// This is deliberately a semantic subset of the source export: it contains
/// every appearance-dependent semantic role plus shared layout and type roles.
pub const TOKENS: &[GeneratedToken] = &[
    GeneratedToken {
        name: "color.canvas.application",
        token_type: "color",
        light: "#F6F8F7",
        dark: "#151713",
    },
    GeneratedToken {
        name: "color.divider",
        token_type: "color",
        light: "#D7DED9",
        dark: "#374039",
    },
    GeneratedToken {
        name: "color.border.default",
        token_type: "color",
        light: "#D7DED9",
        dark: "#465048",
    },
    GeneratedToken {
        name: "color.border.strong",
        token_type: "color",
        light: "#818D85",
        dark: "#6E7A71",
    },
    GeneratedToken {
        name: "color.focus.ring",
        token_type: "color",
        light: "#3578B8",
        dark: "#73B7F0",
    },
    GeneratedToken {
        name: "color.surface.sidebar",
        token_type: "color",
        light: "#FAFBFA",
        dark: "#1A1D19",
    },
    GeneratedToken {
        name: "color.surface.panel",
        token_type: "color",
        light: "#FFFFFF",
        dark: "#20231F",
    },
    GeneratedToken {
        name: "color.surface.manuscript",
        token_type: "color",
        light: "#FFFFFF",
        dark: "#252924",
    },
    GeneratedToken {
        name: "color.surface.elevated",
        token_type: "color",
        light: "#FFFFFF",
        dark: "#2A2F29",
    },
    GeneratedToken {
        name: "color.surface.sunken",
        token_type: "color",
        light: "#EDF1EE",
        dark: "#111310",
    },
    GeneratedToken {
        name: "color.surface.inverse",
        token_type: "color",
        light: "#1A1C1A",
        dark: "#101310",
    },
    GeneratedToken {
        name: "color.text.primary",
        token_type: "color",
        light: "#1A1C1A",
        dark: "#F1F4EF",
    },
    GeneratedToken {
        name: "color.text.secondary",
        token_type: "color",
        light: "#39423C",
        dark: "#C4CCC5",
    },
    GeneratedToken {
        name: "color.text.muted",
        token_type: "color",
        light: "#626D65",
        dark: "#94A097",
    },
    GeneratedToken {
        name: "color.text.disabled",
        token_type: "color",
        light: "#657068",
        dark: "#737D76",
    },
    GeneratedToken {
        name: "color.text.on-accent",
        token_type: "color",
        light: "#FFFFFF",
        dark: "#102019",
    },
    GeneratedToken {
        name: "color.text.placeholder",
        token_type: "color",
        light: "#626D65",
        dark: "#94A097",
    },
    GeneratedToken {
        name: "color.text.inverse",
        token_type: "color",
        light: "#FFFFFF",
        dark: "#F1F4EF",
    },
    GeneratedToken {
        name: "color.accent.default",
        token_type: "color",
        light: "#216E52",
        dark: "#77C3A0",
    },
    GeneratedToken {
        name: "color.accent.hover",
        token_type: "color",
        light: "#185A45",
        dark: "#8FD3B4",
    },
    GeneratedToken {
        name: "color.accent.pressed",
        token_type: "color",
        light: "#185A45",
        dark: "#5CAF89",
    },
    GeneratedToken {
        name: "color.accent.subtle",
        token_type: "color",
        light: "#D7F1E5",
        dark: "#183A2D",
    },
    GeneratedToken {
        name: "color.control.hover",
        token_type: "color",
        light: "#EDF1EE",
        dark: "#2A2F29",
    },
    GeneratedToken {
        name: "color.control.pressed",
        token_type: "color",
        light: "#D7DED9",
        dark: "#37443B",
    },
    GeneratedToken {
        name: "color.control.disabled.background",
        token_type: "color",
        light: "#EDF1EE",
        dark: "#1A1D19",
    },
    GeneratedToken {
        name: "color.control.disabled.border",
        token_type: "color",
        light: "#D7DED9",
        dark: "#374039",
    },
    GeneratedToken {
        name: "color.selection.background",
        token_type: "color",
        light: "#D7F1E5",
        dark: "#183A2D",
    },
    GeneratedToken {
        name: "color.selection.border",
        token_type: "color",
        light: "#216E52",
        dark: "#77C3A0",
    },
    GeneratedToken {
        name: "color.search.match",
        token_type: "color",
        light: "#FFF0CC",
        dark: "#5A4316",
    },
    GeneratedToken {
        name: "color.search.match.active",
        token_type: "color",
        light: "#F1C470",
        dark: "#6A4C16",
    },
    GeneratedToken {
        name: "color.comment.highlight",
        token_type: "color",
        light: "#C7B5FF",
        dark: "#A88DFF",
    },
    GeneratedToken {
        name: "color.comment.active",
        token_type: "color",
        light: "#6D4AA2",
        dark: "#C9A8FF",
    },
    GeneratedToken {
        name: "color.comment.resolved",
        token_type: "color",
        light: "#2D7646",
        dark: "#83D9A1",
    },
    GeneratedToken {
        name: "color.comment.orphaned",
        token_type: "color",
        light: "#9A5B00",
        dark: "#E7B46E",
    },
    GeneratedToken {
        name: "color.status.success",
        token_type: "color",
        light: "#2D7646",
        dark: "#83D9A1",
    },
    GeneratedToken {
        name: "color.status.saving",
        token_type: "color",
        light: "#3578B8",
        dark: "#73B7F0",
    },
    GeneratedToken {
        name: "color.status.warning",
        token_type: "color",
        light: "#9A5B00",
        dark: "#F1C470",
    },
    GeneratedToken {
        name: "color.status.error",
        token_type: "color",
        light: "#A63D32",
        dark: "#FF9D8E",
    },
    GeneratedToken {
        name: "color.status.success.subtle",
        token_type: "color",
        light: "#DFF2E5",
        dark: "#173B29",
    },
    GeneratedToken {
        name: "color.status.saving.subtle",
        token_type: "color",
        light: "#DDEBFA",
        dark: "#1C3447",
    },
    GeneratedToken {
        name: "color.status.warning.subtle",
        token_type: "color",
        light: "#FFF0CC",
        dark: "#4D3A1B",
    },
    GeneratedToken {
        name: "color.status.error.subtle",
        token_type: "color",
        light: "#FBE2DF",
        dark: "#4A211E",
    },
    GeneratedToken {
        name: "color.destructive.default",
        token_type: "color",
        light: "#A63D32",
        dark: "#FF9D8E",
    },
    GeneratedToken {
        name: "color.destructive.subtle",
        token_type: "color",
        light: "#FBE2DF",
        dark: "#4A211E",
    },
    GeneratedToken {
        name: "color.overlay.scrim",
        token_type: "color",
        light: "#1A1C1A66",
        dark: "#00000099",
    },
    GeneratedToken {
        name: "size.ribbon.height",
        token_type: "sizing",
        light: "52",
        dark: "52",
    },
    GeneratedToken {
        name: "size.status.height",
        token_type: "sizing",
        light: "32",
        dark: "32",
    },
    GeneratedToken {
        name: "size.control.compact",
        token_type: "sizing",
        light: "28",
        dark: "28",
    },
    GeneratedToken {
        name: "size.control.default",
        token_type: "sizing",
        light: "36",
        dark: "36",
    },
    GeneratedToken {
        name: "size.icon.core",
        token_type: "sizing",
        light: "20",
        dark: "20",
    },
    GeneratedToken {
        name: "radius.default",
        token_type: "borderRadius",
        light: "4",
        dark: "4",
    },
    GeneratedToken {
        name: "border.default",
        token_type: "borderWidth",
        light: "1",
        dark: "1",
    },
    GeneratedToken {
        name: "border.focus",
        token_type: "borderWidth",
        light: "2",
        dark: "2",
    },
    GeneratedToken {
        name: "font.family.ui",
        token_type: "fontFamily",
        light: "Source Sans 3",
        dark: "Source Sans 3",
    },
    GeneratedToken {
        name: "font.family.prose.sample",
        token_type: "fontFamily",
        light: "Source Serif 4",
        dark: "Source Serif 4",
    },
    GeneratedToken {
        name: "font.family.code",
        token_type: "fontFamily",
        light: "ui-monospace, Menlo, Consolas, Liberation Mono",
        dark: "ui-monospace, Menlo, Consolas, Liberation Mono",
    },
];

/// Exact semantic roles that every appearance must provide.
pub const REQUIRED_SEMANTIC_ROLES: &[&str] = &[
    "color.canvas.application",
    "color.surface.sidebar",
    "color.surface.panel",
    "color.surface.manuscript",
    "color.surface.elevated",
    "color.text.primary",
    "color.text.secondary",
    "color.text.disabled",
    "color.border.default",
    "color.focus.ring",
    "color.accent.default",
    "color.selection.background",
    "color.status.error",
    "color.destructive.default",
    "color.overlay.scrim",
];
