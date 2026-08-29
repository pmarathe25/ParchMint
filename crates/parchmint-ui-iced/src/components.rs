//! Reusable semantic Iced widget styles for all ParchMint screens.

use iced::{
    Background, Color, Font, Shadow, Vector, border, font,
    widget::{button, container, text, text_input},
};

use crate::design_tokens::{
    DEFAULT_RADIUS, FOCUS_BORDER_WIDTH, ParchMintTheme, SemanticPalette, UI_LABEL,
};

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

/// Text content for controls uses the Penpot label token instead of the
/// ambient body font. Buttons are compact actions, not authored prose.
pub fn button_label<'a>(value: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(value)
        .size(u32::from(UI_LABEL.size))
        .line_height(UI_LABEL.line_height)
        .font(Font {
            weight: font::Weight::Semibold,
            ..Font::with_name(UI_LABEL.family)
        })
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
        border: if matches!(surface, Surface::Elevated | Surface::Dialog) {
            outlined(palette.border, 1.0)
        } else {
            borderless()
        },
        shadow: Shadow::default(),
        snap: true,
    };
    // Penpot panels and menus use borders and contrast for separation. Keep
    // elevation reserved for modal dialogs so pointer hover cannot trigger a
    // broad shadow repaint behind ordinary controls or context menus.
    if matches!(surface, Surface::Dialog) {
        style.shadow = shadow(palette, surface);
    }
    match interaction {
        Interaction::Focused => style.border = outlined(palette.focus_ring, FOCUS_BORDER_WIDTH),
        Interaction::Selected => {
            style.background = Some(Background::Color(palette.accent_subtle));
            style.border = borderless();
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
    let (border_color, border_width) = match interaction {
        Interaction::Focused => (palette.focus_ring, FOCUS_BORDER_WIDTH),
        Interaction::Selected => (Color::TRANSPARENT, 0.0),
        Interaction::Error => (palette.error, FOCUS_BORDER_WIDTH),
        Interaction::Disabled => (palette.border, 1.0),
        _ => (base_border, 1.0),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: outlined(border_color, border_width),
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
fn borderless() -> iced::Border {
    outlined(Color::TRANSPARENT, 0.0)
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
    fn structural_surfaces_are_borderless_but_elevated_surfaces_remain_framed() {
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            for structural_surface in [
                Surface::Application,
                Surface::Sidebar,
                Surface::Panel,
                Surface::Manuscript,
                Surface::Status,
            ] {
                let resting = surface(theme, structural_surface, Interaction::Rest);
                assert_eq!(resting.border.width, 0.0);
                assert_eq!(resting.border.color, Color::TRANSPARENT);
            }
            for elevated_surface in [Surface::Elevated, Surface::Dialog] {
                let resting = surface(theme, elevated_surface, Interaction::Rest);
                assert_eq!(resting.border.width, 1.0);
                assert_eq!(resting.border.color, theme.palette().border);
            }
            assert_eq!(
                surface(theme, Surface::Dialog, Interaction::Rest)
                    .shadow
                    .blur_radius,
                32.0
            );
        }
    }

    #[test]
    fn selection_is_a_fill_and_keyboard_focus_is_a_ring_in_both_appearances() {
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            let selected_surface = surface(theme, Surface::Panel, Interaction::Selected);
            assert_eq!(
                selected_surface.background,
                Some(Background::Color(theme.palette().accent_subtle))
            );
            assert_eq!(selected_surface.border.width, 0.0);
            assert_eq!(selected_surface.border.color, Color::TRANSPARENT);

            for kind in [
                ButtonKind::Primary,
                ButtonKind::Secondary,
                ButtonKind::Quiet,
                ButtonKind::Destructive,
                ButtonKind::Tab,
            ] {
                let selected = button_style(theme, kind, Interaction::Selected);
                assert_eq!(
                    selected.background,
                    Some(Background::Color(theme.palette().accent_subtle))
                );
                assert_eq!(selected.border.width, 0.0);
                assert_eq!(selected.border.color, Color::TRANSPARENT);
            }
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
            assert_eq!(field_style(theme, Interaction::Rest).border.width, 1.0);
        }
    }
}
