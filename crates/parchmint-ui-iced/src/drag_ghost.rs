use crate::design_tokens::ParchMintTheme;
use iced::{Point, Size, Vector, mouse, widget::canvas};

pub(crate) struct DragGhost {
    title: String,
    font: iced::Font,
    size: Size,
    theme: ParchMintTheme,
    grab_offset: Vector,
}

impl DragGhost {
    pub(crate) fn tab(
        tab: &crate::TabSpec,
        offset: crate::Point,
        width: f32,
        theme: ParchMintTheme,
    ) -> Self {
        let mut title = crate::editor_workspace::fit_tab_title(
            tab.title(),
            width - if tab.is_dirty() { 12.0 } else { 0.0 },
            tab.is_preview(),
        )
        .0;
        if tab.is_dirty() {
            title.push_str(" •");
        }
        Self {
            title,
            font: crate::editor_workspace::tab_title_font(tab.is_preview()),
            size: Size::new(width, 34.0),
            theme,
            grab_offset: Vector::new(offset.x(), offset.y()),
        }
    }
}

impl<Message> canvas::Program<Message> for DragGhost {
    type State = ();
    fn update(
        &self,
        _: &mut (),
        event: &iced::Event,
        _: iced::Rectangle,
        _: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        matches!(event, iced::Event::Mouse(mouse::Event::CursorMoved { .. }))
            .then(canvas::Action::request_redraw)
    }
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        _: &iced::Theme,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let Some(point) = cursor.position_in(bounds) else {
            return Vec::new();
        };
        let origin = point - self.grab_offset;
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let shadow =
            canvas::Path::rounded_rectangle(origin + Vector::new(3.0, 5.0), self.size, 6.0.into());
        frame.fill(&shadow, self.theme.palette().scrim.scale_alpha(0.35));
        let card = canvas::Path::rounded_rectangle(origin, self.size, 6.0.into());
        let mut background = self.theme.palette().control_hover;
        background.a = 0.94;
        frame.fill(&card, background);
        frame.stroke(
            &card,
            canvas::Stroke::default()
                .with_color(self.theme.palette().strong_border)
                .with_width(1.0),
        );
        frame.fill_text(canvas::Text {
            content: self.title.clone(),
            position: Point::new(origin.x + 8.0, origin.y + 7.0),
            color: self.theme.palette().secondary_text,
            size: 13.0.into(),
            font: self.font,
            line_height: iced::Pixels(18.0).into(),
            ..Default::default()
        });
        frame.fill_text(canvas::Text {
            content: "×".to_owned(),
            position: Point::new(origin.x + self.size.width - 17.0, origin.y + 7.0),
            color: self.theme.palette().secondary_text,
            size: 14.0.into(),
            font: self.font,
            ..Default::default()
        });
        vec![frame.into_geometry()]
    }
    fn mouse_interaction(
        &self,
        _: &(),
        _: iced::Rectangle,
        _: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::None
    }
}

pub(crate) fn floating<'a, Message: 'a>(
    content: iced::Element<'a, Message>,
    offset: crate::Point,
    width: f32,
) -> iced::Element<'a, Message> {
    iced::Element::new(Floating {
        content,
        offset: Vector::new(offset.x(), offset.y()),
        width,
    })
}

struct Floating<'a, Message> {
    content: iced::Element<'a, Message>,
    offset: Vector,
    width: f32,
}

impl<Message> iced::advanced::Widget<Message, iced::Theme, iced::Renderer>
    for Floating<'_, Message>
{
    fn children(&self) -> Vec<iced::advanced::widget::Tree> {
        vec![iced::advanced::widget::Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut iced::advanced::widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<iced::Length> {
        Size::new(iced::Length::Fill, iced::Length::Fill)
    }
    fn layout(
        &mut self,
        tree: &mut iced::advanced::widget::Tree,
        renderer: &iced::Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        let child = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &iced::advanced::layout::Limits::new(
                Size::new(self.width, 0.0),
                Size::new(self.width, f32::INFINITY),
            ),
        );
        iced::advanced::layout::Node::with_children(limits.max(), vec![child])
    }
    fn update(
        &mut self,
        _: &mut iced::advanced::widget::Tree,
        event: &iced::Event,
        _: iced::advanced::Layout<'_>,
        _: mouse::Cursor,
        _: &iced::Renderer,
        _: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        _: &iced::Rectangle,
    ) {
        if matches!(event, iced::Event::Mouse(mouse::Event::CursorMoved { .. })) {
            shell.request_redraw();
        }
    }
    fn draw(
        &self,
        tree: &iced::advanced::widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
        use iced::advanced::Renderer;
        let Some(point) = cursor.position() else {
            return;
        };
        let translation = point - self.offset - layout.child(0).position();
        renderer.with_layer(*viewport, |renderer| {
            renderer.with_translation(translation, |renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    layout.child(0),
                    mouse::Cursor::Unavailable,
                    &iced::Rectangle {
                        x: viewport.x - translation.x,
                        y: viewport.y - translation.y,
                        ..*viewport
                    },
                )
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::{
        renderer::{Headless, Renderer},
        widget::Tree,
    };

    #[test]
    fn floating_card_tracks_every_cursor_move_without_a_workspace_update() {
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let ghost: iced::Element<'_, ()> = floating(
            iced::widget::container(iced::widget::Space::new().width(80).height(40))
                .style(|_| iced::widget::container::Style {
                    background: Some(iced::Color::from_rgb(1.0, 0.0, 0.0).into()),
                    ..Default::default()
                })
                .into(),
            crate::Point::new(10.0, 8.0),
            80.0,
        );
        let mut element: iced::Element<'_, ()> = iced::widget::stack![
            iced::widget::container(
                iced::widget::text("Writing underneath").color(iced::Color::BLACK)
            )
            .padding(iced::Padding {
                top: 45.0,
                left: 90.0,
                ..iced::Padding::ZERO
            }),
            ghost,
        ]
        .into();
        let mut tree = Tree::new(&element);
        let viewport = iced::Rectangle::with_size(Size::new(300.0, 200.0));
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &iced::advanced::layout::Limits::new(Size::ZERO, viewport.size()),
        );
        for point in [
            Point::new(100.0, 50.0),
            Point::new(210.0, 160.0),
            Point::new(50.0, 90.0),
        ] {
            renderer.reset(viewport);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &iced::Theme::Light,
                &iced::advanced::renderer::Style {
                    text_color: iced::Color::BLACK,
                },
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Available(point),
                &viewport,
            );
            let pixels = renderer.screenshot(iced::Size::new(300, 200), 1.0, iced::Color::WHITE);
            let red = pixels
                .chunks_exact(4)
                .enumerate()
                .filter_map(|(i, pixel)| {
                    (pixel[0] > 240 && pixel[1] < 10 && pixel[2] < 10).then_some((i % 300, i / 300))
                })
                .collect::<Vec<_>>();
            assert_eq!(red.len(), 80 * 40);
            assert_eq!(
                red.iter().map(|p| p.0).min(),
                Some((point.x - 10.0) as usize)
            );
            assert_eq!(
                red.iter().map(|p| p.1).min(),
                Some((point.y - 8.0) as usize)
            );
        }
    }
}
