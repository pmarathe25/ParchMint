//! Reusable semantic Iced widget styles for all ParchMint screens.

use iced::{
    Background, Color, Shadow, Vector, border,
    widget::{button, container, text_input},
};

use crate::design_tokens::{DEFAULT_RADIUS, FOCUS_BORDER_WIDTH, ParchMintTheme, SemanticPalette};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Surface {
    Application,
    Sidebar,
    Panel,
    Manuscript,
    Elevated,
    Dialog,
    Status,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Quiet,
    Destructive,
    Tab,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Interaction {
    Rest,
    Hovered,
    Pressed,
    Disabled,
    Focused,
    Selected,
    Error,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusKind {
    Success,
    Saving,
    Warning,
    Error,
}

pub fn surface(
    theme: ParchMintTheme,
    surface: Surface,
    interaction: Interaction,
) -> container::Style {
    let palette = theme.palette();
    let background = match surface {
        Surface::Application => palette.application,
        Surface::Sidebar => palette.sidebar,
        Surface::Panel | Surface::Status => palette.panel,
        Surface::Manuscript => palette.manuscript,
        Surface::Elevated | Surface::Dialog => palette.elevated,
    };
    let mut style = container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(palette.primary_text),
        border: outlined(palette.border, 1.0),
        shadow: Shadow::default(),
        snap: true,
    };
    if matches!(surface, Surface::Dialog | Surface::Elevated) {
        style.shadow = shadow(palette, surface);
    }
    match interaction {
        Interaction::Focused => style.border = outlined(palette.focus_ring, FOCUS_BORDER_WIDTH),
        Interaction::Selected => {
            style.background = Some(Background::Color(palette.accent_subtle));
            style.border = outlined(palette.selection_border, 1.0);
        }
        Interaction::Error => style.border = outlined(palette.error, FOCUS_BORDER_WIDTH),
        _ => {}
    }
    style
}

pub fn button_style(
    theme: ParchMintTheme,
    kind: ButtonKind,
    interaction: Interaction,
) -> button::Style {
    let palette = theme.palette();
    let (base_background, base_text, base_border) = match kind {
        ButtonKind::Primary => (palette.accent, palette.on_accent_text, palette.accent),
        ButtonKind::Destructive => (
            palette.destructive,
            palette.on_accent_text,
            palette.destructive,
        ),
        ButtonKind::Tab => (palette.panel, palette.primary_text, palette.border),
        ButtonKind::Secondary => (palette.panel, palette.primary_text, palette.border),
        ButtonKind::Quiet => (
            Color::TRANSPARENT,
            palette.secondary_text,
            Color::TRANSPARENT,
        ),
    };
    let (background, text_color) = match interaction {
        Interaction::Hovered => (
            if matches!(kind, ButtonKind::Primary) {
                palette.accent_hover
            } else {
                palette.control_hover
            },
            base_text,
        ),
        Interaction::Pressed => (
            if matches!(kind, ButtonKind::Primary) {
                palette.accent_pressed
            } else {
                palette.control_pressed
            },
            base_text,
        ),
        Interaction::Disabled => (palette.control_disabled, palette.disabled_text),
        Interaction::Selected => (palette.accent_subtle, palette.primary_text),
        _ => (base_background, base_text),
    };
    let border_color = match interaction {
        Interaction::Focused => palette.focus_ring,
        Interaction::Selected => palette.selection_border,
        Interaction::Error => palette.error,
        Interaction::Disabled => palette.border,
        _ => base_border,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: outlined(
            border_color,
            if matches!(interaction, Interaction::Focused | Interaction::Error) {
                FOCUS_BORDER_WIDTH
            } else {
                1.0
            },
        ),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn field_style(theme: ParchMintTheme, interaction: Interaction) -> text_input::Style {
    let palette = theme.palette();
    let border_color = match interaction {
        Interaction::Focused => palette.focus_ring,
        Interaction::Error => palette.error,
        Interaction::Disabled => palette.border,
        _ => palette.border,
    };
    text_input::Style {
        background: Background::Color(if matches!(interaction, Interaction::Disabled) {
            palette.control_disabled
        } else {
            palette.panel
        }),
        border: outlined(
            border_color,
            if matches!(interaction, Interaction::Focused | Interaction::Error) {
                FOCUS_BORDER_WIDTH
            } else {
                1.0
            },
        ),
        icon: palette.secondary_text,
        placeholder: palette.placeholder_text,
        value: if matches!(interaction, Interaction::Disabled) {
            palette.disabled_text
        } else {
            palette.primary_text
        },
        selection: palette.selection,
    }
}

pub fn status_style(theme: ParchMintTheme, kind: StatusKind) -> container::Style {
    let palette = theme.palette();
    let (background, text) = match kind {
        StatusKind::Success => (palette.success_subtle, palette.success),
        StatusKind::Saving => (palette.saving_subtle, palette.saving),
        StatusKind::Warning => (palette.warning_subtle, palette.warning),
        StatusKind::Error => (palette.error_subtle, palette.error),
    };
    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(text),
        border: outlined(text, 1.0),
        shadow: Shadow::default(),
        snap: true,
    }
}

fn outlined(color: Color, width: f32) -> iced::Border {
    border::color(color).width(width).rounded(DEFAULT_RADIUS)
}
fn shadow(palette: SemanticPalette, surface: Surface) -> Shadow {
    Shadow {
        color: palette.scrim,
        offset: Vector::new(
            0.0,
            if matches!(surface, Surface::Dialog) {
                12.0
            } else {
                4.0
            },
        ),
        blur_radius: if matches!(surface, Surface::Dialog) {
            32.0
        } else {
            16.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_preferences::ResolvedAppearance;

    #[test]
    fn shared_components_use_semantic_focus_selection_and_error_styles_in_both_appearances() {
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            assert_eq!(
                surface(theme, Surface::Application, Interaction::Rest)
                    .border
                    .radius
                    .top_left,
                DEFAULT_RADIUS
            );
            assert_eq!(
                button_style(theme, ButtonKind::Primary, Interaction::Focused)
                    .border
                    .width,
                FOCUS_BORDER_WIDTH
            );
            assert_eq!(
                field_style(theme, Interaction::Error).border.width,
                FOCUS_BORDER_WIDTH
            );
            assert_ne!(
                button_style(theme, ButtonKind::Tab, Interaction::Selected).background,
                button_style(theme, ButtonKind::Tab, Interaction::Rest).background
            );
        }
    }
}
