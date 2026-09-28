//! Card outlines follow the allocated (including animated) grid-row geometry.
//! The window stays virtualized; an enclosing group can start above the window.
use crate::DragDestination;
use crate::design_tokens::ParchMintTheme;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

pub(crate) struct GroupFrame {
    pub id: String,
    pub depth: usize,
    pub first_row: usize,
    pub last_row: usize,
    pub starts_here: bool,
    pub ends_here: bool,
    pub last_card_id: Option<String>,
    pub last_row_disclosures: usize,
    pub members: Vec<(usize, String)>,
}

pub(crate) struct GroupDropTargets<'a> {
    targets: crate::hierarchy_drag::HoverTargets<DragDestination>,
    section_id: String,
    validate: Box<dyn Fn(DragDestination) -> Option<DragDestination> + 'a>,
}
impl<'a> GroupDropTargets<'a> {
    pub(crate) fn new(
        targets: crate::hierarchy_drag::HoverTargets<DragDestination>,
        section_id: String,
        validate: impl Fn(DragDestination) -> Option<DragDestination> + 'a,
    ) -> Self {
        Self {
            targets,
            section_id,
            validate: Box::new(validate),
        }
    }
}

struct FrameBounds<'a> {
    frame: &'a GroupFrame,
    bounds: Rectangle,
    after: Option<Rectangle>,
}

/// Painting and hit testing must use exactly the same allocated row geometry.
/// Reflow offsets belong to the visible cards, never a cached destination grid.
fn frame_bounds<'a>(
    frame: &'a GroupFrame,
    rows: &[Layout<'_>],
    width: f32,
    positions: Option<&crate::motion::Positions>,
) -> FrameBounds<'a> {
    let first = rows[frame.first_row].bounds();
    let last = rows[frame.last_row].bounds();
    let at = crate::motion::now();
    let top_motion_y = positions
        .map(|positions| positions.vertical_offset(&frame.id, at))
        .unwrap_or(0.0);
    let bottom_motion_y = positions
        .and_then(|positions| {
            frame
                .last_card_id
                .as_ref()
                .map(|id| positions.vertical_offset(id, at))
        })
        .unwrap_or(top_motion_y);
    let indent = crate::cards_layout::grid_indent(frame.depth, width);
    let mut top = first.y + top_motion_y - if frame.starts_here { 0.0 } else { 12.0 };
    let mut natural_last = rows[frame.last_row];
    for _ in 0..frame.last_row_disclosures {
        natural_last = natural_last.child(0);
    }
    let fraction = if natural_last.bounds().height > 0.0 {
        (last.height / natural_last.bounds().height).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let gap = crate::cards_layout::GROUP_GAP * fraction;
    let mut bottom = if frame.members.is_empty() {
        (last.y + bottom_motion_y + last.height - if frame.ends_here { gap } else { -12.0 })
            .max(first.y + top_motion_y + first.height)
    } else {
        first.y + top_motion_y + first.height
    };
    // Cards in one row can be moving from different previous rows. A single
    // "last card" offset cannot describe their envelope, nor the separately
    // animated creation control. Enclose every visible member independently.
    for (row_index, id) in &frame.members {
        let row = rows[*row_index].bounds();
        let offset = positions
            .map(|positions| positions.vertical_offset(id, at))
            .unwrap_or(0.0);
        top = top.min(row.y + offset);
        let trailing_gap = if *row_index == frame.last_row && frame.ends_here {
            gap
        } else {
            0.0
        };
        bottom = bottom.max(row.y + offset + row.height - trailing_gap);
    }
    let bounds = Rectangle {
        x: first.x + indent,
        y: top,
        width: width - 2.0 * indent,
        height: bottom - top,
    };
    FrameBounds {
        frame,
        bounds,
        after: (frame.ends_here && gap > 0.0).then_some(Rectangle {
            y: bottom,
            height: gap,
            ..bounds
        }),
    }
}

fn group_destination(
    frames: &[FrameBounds<'_>],
    point: Point,
    section: &str,
) -> (DragDestination, Option<Rectangle>) {
    let hit = frames
        .iter()
        .filter_map(|frame| {
            if frame.bounds.contains(point) {
                Some((
                    frame,
                    DragDestination::IntoGroup(frame.frame.id.clone()),
                    frame.bounds,
                ))
            } else {
                frame
                    .after
                    .filter(|bounds| bounds.contains(point))
                    .map(|bounds| {
                        (
                            frame,
                            DragDestination::AfterSibling(frame.frame.id.clone()),
                            bounds,
                        )
                    })
            }
        })
        .max_by_key(|(frame, _, _)| frame.frame.depth);
    hit.map(|(_, destination, bounds)| (destination, Some(bounds)))
        .unwrap_or_else(|| (DragDestination::IntoGroup(section.to_owned()), None))
}

pub(crate) fn groups<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    frames: Vec<GroupFrame>,
    positions: crate::motion::Positions,
    theme: ParchMintTheme,
    drop_destination: Option<DragDestination>,
    drop_targets: Option<GroupDropTargets<'a>>,
) -> Element<'a, Message> {
    Element::new(CardFrames {
        content: content.into(),
        frames: Some(frames),
        positions: Some(positions),
        theme,
        drop_destination,
        drop_targets,
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
        drop_destination: None,
        drop_targets: None,
    })
}

struct CardFrames<'a, Message> {
    content: Element<'a, Message>,
    frames: Option<Vec<GroupFrame>>,
    positions: Option<crate::motion::Positions>,
    theme: ParchMintTheme,
    drop_destination: Option<DragDestination>,
    drop_targets: Option<GroupDropTargets<'a>>,
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
        // Specific card positions win. Blank group space resolves only after
        // the children have registered their current painted card bounds.
        let no_card_target = self
            .drop_targets
            .as_ref()
            .is_some_and(|targets| targets.targets.borrow().is_none());
        if let Some(targets) = &self.drop_targets
            && no_card_target
            && let Some(point) = cursor
                .position()
                .filter(|point| viewport.contains(*point) && layout.bounds().contains(*point))
        {
            let rows: Vec<_> = layout.children().collect();
            let frames: Vec<_> = self
                .frames
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|frame| {
                    frame_bounds(frame, &rows, layout.bounds().width, self.positions.as_ref())
                })
                .collect();
            let (destination, zone) = group_destination(&frames, point, &targets.section_id);
            if let Some(destination) = (targets.validate)(destination) {
                let zone = zone.unwrap_or_else(|| layout.bounds());
                *targets.targets.borrow_mut() = Some((
                    destination,
                    Rectangle {
                        x: zone.x - point.x,
                        y: zone.y - point.y,
                        ..zone
                    },
                ));
            }
        }
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
                let bounds =
                    frame_bounds(frame, &rows, layout.bounds().width, self.positions.as_ref())
                        .bounds;
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
                // Insertion lines for group-atomic drops: the per-card
                // indicator cannot reach past an expanded group's contents,
                // so the frame draws the line where the placeholder lands —
                // after the last row for AfterSibling, before the first row
                // for BeforeSibling.
                let insertion = match &self.drop_destination {
                    Some(DragDestination::AfterSibling(id)) if id == &frame.id => Some(Rectangle {
                        y: (bounds.y + bounds.height - 2.0).max(bounds.y),
                        width: bounds.width,
                        height: 2.0,
                        ..bounds
                    }),
                    Some(DragDestination::BeforeSibling(id)) if id == &frame.id => {
                        Some(Rectangle {
                            width: bounds.width,
                            height: 2.0,
                            ..bounds
                        })
                    }
                    _ => None,
                };
                if let Some(area) = insertion {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: area,
                            ..Default::default()
                        },
                        palette.accent,
                    );
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
        if let Some(frames) = &self.frames {
            let rows: Vec<_> = layout.children().collect();
            for frame in frames {
                let bounds =
                    frame_bounds(frame, &rows, layout.bounds().width, self.positions.as_ref())
                        .bounds;
                if bounds.width > 0.0 && bounds.height > 0.0 {
                    let id = iced::widget::Id::from(format!("card-group-frame-{}", frame.id));
                    operation.container(Some(&id), bounds);
                }
            }
        }
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

    fn group(id: &str, depth: usize, first_row: usize, last_row: usize) -> GroupFrame {
        GroupFrame {
            id: id.to_owned(),
            depth,
            first_row,
            last_row,
            starts_here: true,
            ends_here: true,
            last_card_id: None,
            last_row_disclosures: 0,
            members: Vec::new(),
        }
    }

    #[test]
    fn blank_space_uses_innermost_painted_group_and_only_external_gap_goes_after() {
        let outer = group("outer", 0, 0, 3);
        let inner = group("inner", 1, 1, 2);
        let rows = [
            layout::Node::new(Size::new(500.0, 40.0)).move_to(Point::new(0.0, 0.0)),
            layout::Node::new(Size::new(500.0, 40.0)).move_to(Point::new(0.0, 40.0)),
            layout::Node::new(Size::new(500.0, 140.0)).move_to(Point::new(0.0, 80.0)),
            layout::Node::new(Size::new(500.0, 100.0)).move_to(Point::new(0.0, 220.0)),
        ];
        let layouts: Vec<_> = rows.iter().map(Layout::new).collect();
        let frames = [
            frame_bounds(&outer, &layouts, 500.0, None),
            frame_bounds(&inner, &layouts, 500.0, None),
        ];
        let at = |x, y| group_destination(&frames, Point::new(x, y), "manuscript").0;
        assert_eq!(at(200.0, 120.0), DragDestination::IntoGroup("inner".into()));
        assert_eq!(
            at(1.0, 120.0),
            DragDestination::IntoGroup("outer".into()),
            "indent is outside the nested group"
        );
        let inner_bottom = frames[1].bounds.y + frames[1].bounds.height;
        assert_eq!(
            at(200.0, inner_bottom - 0.5),
            DragDestination::IntoGroup("inner".into())
        );
        assert_eq!(
            at(200.0, inner_bottom + 0.5),
            DragDestination::AfterSibling("inner".into())
        );
        assert_eq!(at(200.0, 260.0), DragDestination::IntoGroup("outer".into()));
        assert_eq!(
            at(200.0, 350.0),
            DragDestination::IntoGroup("manuscript".into())
        );
    }

    #[test]
    fn group_hit_bounds_follow_visible_reflow_not_the_destination_row() {
        crate::motion::set_reduced(false);
        let at = crate::motion::now();
        let _clock = crate::motion::FixedTime::new(at);
        let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let positions = crate::motion::Positions::default();
        let mut element: Element<'_, ()> = crate::motion::reflow(
            positions.clone(),
            "moving",
            0,
            true,
            Space::new().width(500).height(40),
        );
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(500.0, 900.0));
        let mut messages = Vec::new();
        let viewport = Rectangle::with_size(Size::new(500.0, 900.0));
        let initial = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(0.0, 20.0));
        element.as_widget_mut().update(
            &mut tree,
            &Event::Window(iced::window::Event::RedrawRequested(at)),
            Layout::new(&initial),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &viewport,
        );
        element = crate::motion::reflow(
            positions.clone(),
            "moving",
            1,
            true,
            Space::new().width(500).height(40),
        );
        tree.diff(&element);
        let destination = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(0.0, 220.0));
        element.as_widget_mut().update(
            &mut tree,
            &Event::Window(iced::window::Event::RedrawRequested(at)),
            Layout::new(&destination),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &viewport,
        );
        assert_eq!(positions.vertical_offset("moving", at), -200.0);
        let last = layout::Node::new(Size::new(500.0, 100.0)).move_to(Point::new(0.0, 260.0));
        let rows = [Layout::new(&destination), Layout::new(&last)];
        let frame = group("moving", 0, 0, 1);
        let bounds = [frame_bounds(&frame, &rows, 500.0, Some(&positions))];
        assert_eq!(bounds[0].bounds.y, 20.0);
        assert_eq!(
            group_destination(&bounds, Point::new(100.0, 50.0), "root").0,
            DragDestination::IntoGroup("moving".into())
        );
        assert_eq!(
            group_destination(&bounds, Point::new(100.0, 250.0), "root").0,
            DragDestination::IntoGroup("root".into())
        );
        // The heading is still high on screen while another member is at its
        // destination. Its outline must enclose both, including creation slots.
        let mut frame = frame;
        frame.members = vec![(0, "moving".into()), (1, "add:moving".into())];
        let bounds = [frame_bounds(&frame, &rows, 500.0, Some(&positions))];
        assert_eq!(bounds[0].bounds.y, 20.0);
        assert_eq!(
            bounds[0].bounds.y + bounds[0].bounds.height,
            360.0 - crate::cards_layout::GROUP_GAP
        );
        assert_eq!(
            group_destination(&bounds, Point::new(100.0, 300.0), "root").0,
            DragDestination::IntoGroup("moving".into())
        );
    }

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
                last_card_id: None,
                last_row_disclosures: 0,
                members: Vec::new(),
            }],
            crate::motion::Positions::default(),
            theme,
            None,
            None,
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
