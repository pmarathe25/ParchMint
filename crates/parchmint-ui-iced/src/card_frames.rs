//! Card outlines follow the allocated (including animated) grid-row geometry.
//! The window stays virtualized; an enclosing group can start above the window.
use crate::design_tokens::ParchMintTheme;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree},
};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

pub(crate) struct GroupFrame {
    pub id: String,
    pub depth: usize,
    pub first_row: usize,
    pub last_row: usize,
    pub starts_here: bool,
    pub ends_here: bool,
}

pub(crate) fn groups<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    frames: Vec<GroupFrame>,
    positions: crate::motion::Positions,
    theme: ParchMintTheme,
) -> Element<'a, Message> {
    Element::new(CardFrames {
        content: content.into(),
        frames: Some(frames),
        positions: Some(positions),
        theme,
    })
}

pub(crate) fn placeholder<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    theme: ParchMintTheme,
) -> Element<'a, Message> {
    Element::new(CardFrames {
        content: content.into(),
        frames: None,
        positions: None,
        theme,
    })
}

struct CardFrames<'a, Message> {
    content: Element<'a, Message>,
    frames: Option<Vec<GroupFrame>>,
    positions: Option<crate::motion::Positions>,
    theme: ParchMintTheme,
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for CardFrames<'_, Message> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use renderer::Renderer;
        // The containing scrollable already clips the group and its contents.
        let palette = self.theme.palette();
        if let Some(frames) = &self.frames {
            let rows: Vec<_> = layout.children().collect();
            for frame in frames {
                let first = rows[frame.first_row].bounds();
                let last = rows[frame.last_row].bounds();
                let motion_y = self
                    .positions
                    .as_ref()
                    .map(|positions| positions.vertical_offset(&frame.id, crate::motion::now()))
                    .unwrap_or(0.0);
                let indent = crate::cards_layout::grid_indent(frame.depth, layout.bounds().width);
                let top = first.y + motion_y - if frame.starts_here { 0.0 } else { 12.0 };
                let bottom = last.y + motion_y + last.height
                    - if frame.ends_here {
                        crate::cards_layout::GROUP_GAP
                    } else {
                        -12.0
                    };
                let bounds = Rectangle {
                    x: first.x + indent,
                    y: top,
                    width: layout.bounds().width - 2.0 * indent,
                    height: bottom - top,
                };
                if bounds.width <= 0.0 || bounds.height <= 0.0 {
                    continue;
                }
                // Every descendant has the same panel color. Paint it only
                // for the outer group, then draw inexpensive one-pixel lines
                // to preserve the nested hierarchy.
                if frame.depth == 0 {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds,
                            ..Default::default()
                        },
                        palette.panel,
                    );
                }
                let mut line = |bounds| {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds,
                            ..Default::default()
                        },
                        palette.divider,
                    );
                };
                line(Rectangle {
                    x: bounds.x,
                    width: 1.0,
                    ..bounds
                });
                line(Rectangle {
                    x: bounds.x + bounds.width - 1.0,
                    width: 1.0,
                    ..bounds
                });
                if frame.starts_here {
                    line(Rectangle {
                        height: 1.0,
                        ..bounds
                    });
                }
                if frame.ends_here {
                    line(Rectangle {
                        y: bounds.y + bounds.height - 1.0,
                        height: 1.0,
                        ..bounds
                    });
                }
            }
        } else {
            let bounds = layout.bounds();
            let mut color = palette.divider;
            color.a *= if cursor.is_over(bounds) { 1.0 } else { 0.55 };
            // Short dashes distinguish an empty creation slot from saved content.
            let mut dash = |bounds| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        ..Default::default()
                    },
                    color,
                )
            };
            let mut x = bounds.x + 5.0;
            while x < bounds.x + bounds.width - 5.0 {
                let width = 5.0_f32.min(bounds.x + bounds.width - 5.0 - x);
                dash(Rectangle {
                    x,
                    y: bounds.y,
                    width,
                    height: 1.0,
                });
                dash(Rectangle {
                    x,
                    y: bounds.y + bounds.height - 1.0,
                    width,
                    height: 1.0,
                });
                x += 10.0;
            }
            let mut y = bounds.y + 5.0;
            while y < bounds.y + bounds.height - 5.0 {
                let height = 5.0_f32.min(bounds.y + bounds.height - 5.0 - y);
                dash(Rectangle {
                    x: bounds.x,
                    y,
                    width: 1.0,
                    height,
                });
                dash(Rectangle {
                    x: bounds.x + bounds.width - 1.0,
                    y,
                    width: 1.0,
                    height,
                });
                y += 10.0;
            }
        }
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::{Headless, Renderer};
    use iced::widget::{Space, column, container};

    #[test]
    fn group_backdrop_starts_at_header_and_never_covers_creation_content() {
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let theme = ParchMintTheme::new(crate::ResolvedAppearance::Light);
        let creation = placeholder(
            container(Space::new().width(100).height(60)).style(|_| {
                iced::widget::container::Style {
                    background: Some(iced::Color::from_rgb(1.0, 0.0, 0.0).into()),
                    ..Default::default()
                }
            }),
            theme,
        );
        let mut content: Element<'_, ()> = groups(
            column![
                Space::new().height(0),
                Space::new().width(100).height(40),
                column![
                    creation,
                    Space::new().height(crate::cards_layout::GROUP_GAP)
                ]
            ],
            vec![GroupFrame {
                id: "group".to_owned(),
                depth: 0,
                first_row: 0,
                last_row: 1,
                starts_here: true,
                ends_here: true,
            }],
            crate::motion::Positions::default(),
            theme,
        );
        let mut tree = Tree::new(&content);
        let viewport = Rectangle::with_size(Size::new(100.0, 120.0));
        let node = content.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        renderer.reset(viewport);
        content.as_widget().draw(
            &tree,
            &mut renderer,
            &theme.iced_theme(),
            &renderer::Style {
                text_color: iced::Color::BLACK,
            },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &viewport,
        );
        let pixels = renderer.screenshot(
            Size::new(100, 120),
            1.0,
            iced::Color::from_rgb(1.0, 0.0, 1.0),
        );
        let at = |x: usize, y: usize| &pixels[(y * 100 + x) * 4..(y * 100 + x) * 4 + 3];
        assert_eq!(
            at(20, 10),
            &[255, 255, 255],
            "header must be inside the group"
        );
        assert_eq!(
            at(20, 60),
            &[255, 0, 0],
            "creation content must paint above its group"
        );
        assert_eq!(
            at(20, 110),
            &[255, 0, 255],
            "group gap must be outside its border"
        );
    }
}
