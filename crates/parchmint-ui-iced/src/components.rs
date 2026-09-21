//! Reusable semantic Iced widget styles for all ParchMint screens.

use iced::{
    Background, Color, Font, Shadow, Vector, border, font,
    widget::{button, container, text, text_input},
};

use crate::design_tokens::{
    DEFAULT_RADIUS, FOCUS_BORDER_WIDTH, ParchMintTheme, UI_LABEL, UI_PAGE_TITLE,
};

pub(crate) fn word_count_label(words: usize) -> String {
    format!("{words} {}", if words == 1 { "word" } else { "words" })
}

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

/// Secondary action button; override the style for primary or destructive actions.
pub fn semantic_button<'a, Message: Clone + 'a>(
    content: impl Into<iced::Element<'a, Message>>,
) -> iced::widget::Button<'a, Message> {
    button(content).padding([6, 10]).style(|theme, status| {
        button_style(
            presentation(theme),
            ButtonKind::Secondary,
            button_interaction(status, false),
        )
    })
}

pub fn semantic_text_input<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, value)
        .padding([7, 9])
        .style(|theme, status| field_style(presentation(theme), field_interaction(status)))
}

/// A select control with the same field, text, and focus tokens as text inputs.
pub fn semantic_pick_list<'a, T, L, V, Message>(
    options: L,
    selected: Option<V>,
    on_selected: impl Fn(T) -> Message + 'a,
) -> iced::widget::PickList<'a, T, L, V, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: std::borrow::Borrow<[T]> + 'a,
    V: std::borrow::Borrow<T> + 'a,
    Message: Clone + 'a,
{
    iced::widget::pick_list(options, selected, on_selected)
        .padding([6, 8])
        .text_size(14)
        .style(|theme, status| {
            let theme = presentation(theme);
            let interaction = match status {
                iced::widget::pick_list::Status::Opened { .. } => Interaction::Focused,
                iced::widget::pick_list::Status::Hovered => Interaction::Hovered,
                iced::widget::pick_list::Status::Active => Interaction::Rest,
            };
            let field = field_style(theme, interaction);
            iced::widget::pick_list::Style {
                text_color: field.value,
                placeholder_color: field.value,
                handle_color: field.icon,
                background: field.background,
                border: field.border,
            }
        })
        .menu_style(|theme| menu_style(presentation(theme)))
}

pub(crate) fn menu_style(theme: ParchMintTheme) -> iced::widget::overlay::menu::Style {
    let panel = surface(theme, Surface::Elevated, Interaction::Rest);
    iced::widget::overlay::menu::Style {
        background: theme.palette().elevated.into(),
        border: panel.border,
        text_color: theme.palette().primary_text,
        selected_text_color: theme.palette().primary_text,
        selected_background: theme.palette().accent_subtle.into(),
        shadow: panel.shadow,
    }
}

pub(crate) fn scrim(theme: ParchMintTheme) -> container::Style {
    container::Style {
        background: Some(theme.palette().scrim.into()),
        ..Default::default()
    }
}

pub(crate) fn presentation(theme: &iced::Theme) -> ParchMintTheme {
    ParchMintTheme::from_iced_theme(theme).unwrap_or_else(|| {
        ParchMintTheme::new(if theme.extended_palette().is_dark {
            parchmint_preferences::ResolvedAppearance::Dark
        } else {
            parchmint_preferences::ResolvedAppearance::Light
        })
    })
}

pub(crate) fn page_title<'a>(value: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(value)
        .size(u32::from(UI_PAGE_TITLE.size))
        .line_height(UI_PAGE_TITLE.line_height)
        .font(Font {
            weight: font::Weight::Semibold,
            ..Font::with_name(UI_PAGE_TITLE.family)
        })
}

/// Control text in the shared label font.
pub fn button_label<'a>(value: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(value)
        .size(u32::from(UI_LABEL.size))
        .line_height(UI_LABEL.line_height)
        .font(Font {
            weight: font::Weight::Semibold,
            ..Font::with_name(UI_LABEL.family)
        })
}

pub(crate) fn button_interaction(
    status: iced::widget::button::Status,
    selected: bool,
) -> Interaction {
    match status {
        status if selected && status != iced::widget::button::Status::Disabled => {
            Interaction::Selected
        }
        iced::widget::button::Status::Active => Interaction::Rest,
        iced::widget::button::Status::Hovered => Interaction::Hovered,
        iced::widget::button::Status::Pressed => Interaction::Pressed,
        iced::widget::button::Status::Disabled => Interaction::Disabled,
    }
}

pub(crate) fn field_interaction(status: iced::widget::text_input::Status) -> Interaction {
    match status {
        iced::widget::text_input::Status::Active => Interaction::Rest,
        iced::widget::text_input::Status::Hovered => Interaction::Hovered,
        iced::widget::text_input::Status::Focused { .. } => Interaction::Focused,
        iced::widget::text_input::Status::Disabled => Interaction::Disabled,
    }
}

pub(crate) fn multiline_field_style(
    theme: ParchMintTheme,
    status: iced::widget::text_editor::Status,
) -> iced::widget::text_editor::Style {
    let interaction = match status {
        iced::widget::text_editor::Status::Active => Interaction::Rest,
        iced::widget::text_editor::Status::Hovered => Interaction::Hovered,
        iced::widget::text_editor::Status::Focused { .. } => Interaction::Focused,
        iced::widget::text_editor::Status::Disabled => Interaction::Disabled,
    };
    let field = field_style(theme, interaction);
    iced::widget::text_editor::Style {
        background: field.background,
        border: field.border,
        placeholder: field.placeholder,
        value: field.value,
        selection: field.selection,
    }
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
    // Reserve shadows for dialogs to avoid broad repaints on pointer hover.
    if matches!(surface, Surface::Dialog) {
        style.shadow = Shadow {
            color: palette.scrim,
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        };
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
        Interaction::Hovered if matches!(kind, ButtonKind::Destructive) => {
            (palette.destructive_subtle, palette.destructive)
        }
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
            } else if matches!(kind, ButtonKind::Destructive) {
                palette.destructive
            } else {
                palette.control_pressed
            },
            base_text,
        ),
        Interaction::Disabled => (
            if matches!(kind, ButtonKind::Quiet | ButtonKind::Tab) {
                Color::TRANSPARENT
            } else {
                palette.control_disabled
            },
            palette.disabled_text,
        ),
        Interaction::Selected => (palette.accent_subtle, palette.primary_text),
        _ => (base_background, base_text),
    };
    let (border_color, border_width) = match interaction {
        Interaction::Focused => (palette.focus_ring, FOCUS_BORDER_WIDTH),
        Interaction::Selected => (Color::TRANSPARENT, 0.0),
        Interaction::Error => (palette.error, FOCUS_BORDER_WIDTH),
        Interaction::Disabled if matches!(kind, ButtonKind::Quiet | ButtonKind::Tab) => {
            (Color::TRANSPARENT, 0.0)
        }
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
        Interaction::Hovered => palette.strong_border,
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
#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_preferences::ResolvedAppearance;

    #[test]
    fn filled_actions_keep_readable_text_through_pointer_states() {
        fn luminance(color: Color) -> f32 {
            let linear = |value: f32| {
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
        }
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            for kind in [
                ButtonKind::Primary,
                ButtonKind::Destructive,
                ButtonKind::Secondary,
            ] {
                for interaction in [
                    Interaction::Rest,
                    Interaction::Hovered,
                    Interaction::Pressed,
                ] {
                    let style = button_style(theme, kind, interaction);
                    let Some(Background::Color(background)) = style.background else {
                        panic!("filled action")
                    };
                    let foreground = luminance(style.text_color);
                    let background = luminance(background);
                    let contrast =
                        (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
                    assert!(
                        contrast >= 4.5,
                        "{appearance:?} {kind:?} {interaction:?}: {contrast}"
                    );
                }
            }
        }
    }

    #[test]
    fn disabled_quiet_controls_stay_unboxed_and_menus_share_popup_surfaces() {
        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            let disabled = button_style(theme, ButtonKind::Quiet, Interaction::Disabled);
            assert_eq!(disabled.background, Some(Color::TRANSPARENT.into()));
            assert_eq!(disabled.border.width, 0.0);
            let popup = surface(theme, Surface::Elevated, Interaction::Rest);
            assert_eq!(popup.background, Some(menu_style(theme).background));
        }
    }

    #[test]
    fn pointer_states_preserve_selection_without_enabling_disabled_controls() {
        for status in [
            button::Status::Active,
            button::Status::Hovered,
            button::Status::Pressed,
        ] {
            assert_eq!(button_interaction(status, true), Interaction::Selected);
        }
        assert_eq!(
            button_interaction(button::Status::Disabled, true),
            Interaction::Disabled
        );
        assert_eq!(
            button_interaction(button::Status::Hovered, false),
            Interaction::Hovered
        );
    }

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
