use crate::Primitive;
use crate::core::renderer::Quad;
use crate::core::{
    self, Background, Color, Point, Rectangle, Svg, Transformation,
};
use crate::graphics::damage;
use crate::graphics::layer;
use crate::graphics::text::{Editor, Paragraph, Text};
use crate::graphics::{self, Image};

use std::sync::Arc;

pub type Stack = layer::Stack<Layer>;

#[derive(Debug, Clone)]
pub struct Layer {
    pub bounds: Rectangle,
    pub quads: Vec<(Quad, Background)>,
    pub primitives: Vec<Item<Primitive>>,
    pub images: Vec<Image>,
    pub text: Vec<Item<Text>>,
}

impl Layer {
    pub fn draw_quad(
        &mut self,
        mut quad: Quad,
        background: Background,
        transformation: Transformation,
    ) {
        quad.bounds = quad.bounds * transformation;
        self.quads.push((quad, background));
    }

    pub fn draw_paragraph(
        &mut self,
        paragraph: &Paragraph,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let paragraph = Text::Paragraph {
            paragraph: paragraph.downgrade(),
            position,
            color,
            clip_bounds,
            transformation,
        };

        self.text.push(Item::Live(paragraph));
    }

    pub fn draw_editor(
        &mut self,
        editor: &Editor,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let editor = Text::Editor {
            editor: editor.downgrade(),
            position,
            color,
            clip_bounds,
            transformation,
        };

        self.text.push(Item::Live(editor));
    }

    pub fn draw_text(
        &mut self,
        text: core::Text,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let text = Text::Cached {
            content: text.content,
            bounds: Rectangle::new(position, text.bounds) * transformation,
            color,
            size: text.size * transformation.scale_factor(),
            line_height: text.line_height.to_absolute(text.size)
                * transformation.scale_factor(),
            font: text.font,
            align_x: text.align_x,
            align_y: text.align_y,
            shaping: text.shaping,
            clip_bounds: clip_bounds * transformation,
        };

        self.text.push(Item::Live(text));
    }

    pub fn draw_text_raw(
        &mut self,
        raw: graphics::text::Raw,
        transformation: Transformation,
    ) {
        let raw = Text::Raw {
            raw,
            transformation,
        };

        self.text.push(Item::Live(raw));
    }

    pub fn draw_text_group(
        &mut self,
        text: Vec<Text>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.text.push(Item::Group(
            text,
            clip_bounds * transformation,
            transformation,
        ));
    }

    pub fn draw_text_cache(
        &mut self,
        text: Arc<[Text]>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.text.push(Item::Cached(
            text,
            clip_bounds * transformation,
            transformation,
        ));
    }

    pub fn draw_image(&mut self, image: Image, transformation: Transformation) {
        match image {
            Image::Raster {
                image,
                bounds,
                clip_bounds,
            } => {
                self.draw_raster(image, bounds, clip_bounds, transformation);
            }
            Image::Vector {
                svg,
                bounds,
                clip_bounds,
            } => {
                self.draw_svg(svg, bounds, clip_bounds, transformation);
            }
        }
    }

    pub fn draw_raster(
        &mut self,
        image: core::Image,
        bounds: Rectangle,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let image = Image::Raster {
            image: core::Image {
                border_radius: image.border_radius
                    * transformation.scale_factor(),
                ..image
            },
            bounds: bounds * transformation,
            clip_bounds: clip_bounds * transformation,
        };

        self.images.push(image);
    }

    pub fn draw_svg(
        &mut self,
        svg: Svg,
        bounds: Rectangle,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let svg = Image::Vector {
            svg,
            bounds: bounds * transformation,
            clip_bounds: clip_bounds * transformation,
        };

        self.images.push(svg);
    }

    pub fn draw_primitive_group(
        &mut self,
        primitives: Vec<Primitive>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.primitives.push(Item::Group(
            primitives,
            clip_bounds * transformation,
            transformation,
        ));
    }

    pub fn draw_primitive_cache(
        &mut self,
        primitives: Arc<[Primitive]>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.primitives.push(Item::Cached(
            primitives,
            clip_bounds * transformation,
            transformation,
        ));
    }

    pub fn damage(
        previous: &Self,
        current: &Self,
        engine: &mut crate::engine::Engine,
        scale: f32,
    ) -> Vec<Rectangle> {
        if previous.bounds != current.bounds {
            return vec![previous.bounds, current.bounds];
        }

        let mut damage = damage::list(
            &previous.quads,
            &current.quads,
            |(quad, _)| {
                crate::engine::quad_visible_bounds(quad)
                    .expand(1.0)
                    .intersection(&current.bounds)
                    .into_iter()
                    .collect()
            },
            |(quad_a, background_a), (quad_b, background_b)| {
                quad_a == quad_b && background_a == background_b
            },
        );

        damage.extend(item_damage(
            &previous.text,
            &current.text,
            current.bounds,
            |text, transform| {
                engine
                    .text_bounds(
                        text,
                        core::Transformation::scale(scale) * transform,
                    )
                    .map(|bounds| bounds * (1.0 / scale))
            },
        ));

        damage.extend(item_damage(
            &previous.primitives,
            &current.primitives,
            current.bounds,
            |primitive, transform| {
                Some(
                    (primitive.visible_bounds() * transform)
                        .expand(1.0 / scale),
                )
            },
        ));

        let images = damage::list(
            &previous.images,
            &current.images,
            |image| vec![image.bounds().expand(1.0)],
            Image::eq,
        );

        damage.extend(images);
        // Unmasked antialiased primitives can cover the edge pixel just
        // outside their logical clip. Include it when erasing old geometry.
        damage
            .into_iter()
            .map(|bounds| bounds.expand(1.0 / scale))
            .collect()
    }
}

// Keep identical prefixes and suffixes, including when an edit inserts or
// removes items. Unchanged Canvas backgrounds must not dirty the whole pane.
fn changed<'a, T: PartialEq>(
    mut previous: &'a [T],
    mut current: &'a [T],
) -> (&'a [T], &'a [T]) {
    while !previous.is_empty()
        && !current.is_empty()
        && previous[0] == current[0]
    {
        previous = &previous[1..];
        current = &current[1..];
    }
    while !previous.is_empty()
        && !current.is_empty()
        && previous.last() == current.last()
    {
        previous = &previous[..previous.len() - 1];
        current = &current[..current.len() - 1];
    }
    (previous, current)
}

fn item_damage<T: PartialEq>(
    previous: &[Item<T>],
    current: &[Item<T>],
    layer_bounds: Rectangle,
    mut bounds: impl FnMut(&T, Transformation) -> Option<Rectangle>,
) -> Vec<Rectangle> {
    let mut damage = Vec::new();
    for index in 0..previous.len().max(current.len()) {
        let old = previous.get(index);
        let new = current.get(index);
        let (old_items, new_items) = match (old, new) {
            (Some(old), Some(new))
                if old.transformation() == new.transformation()
                    && old.clip_bounds() == new.clip_bounds() =>
            {
                if let (Item::Cached(a, _, _), Item::Cached(b, _, _)) =
                    (old, new)
                    && Arc::ptr_eq(a, b)
                {
                    continue;
                }
                changed(old.as_slice(), new.as_slice())
            }
            _ => (
                old.map_or(&[][..], Item::as_slice),
                new.map_or(&[][..], Item::as_slice),
            ),
        };
        for (group, items) in [(old, old_items), (new, new_items)] {
            if let Some(group) = group {
                for item in items {
                    if let Some(bounds) = bounds(item, group.transformation())
                        .and_then(|bounds| {
                            bounds.intersection(&group.clip_bounds())
                        })
                        .and_then(|bounds| bounds.intersection(&layer_bounds))
                    {
                        damage.push(bounds);
                    }
                }
            }
        }
    }
    damage
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            bounds: Rectangle::INFINITE,
            quads: Vec::new(),
            primitives: Vec::new(),
            text: Vec::new(),
            images: Vec::new(),
        }
    }
}

impl graphics::Layer for Layer {
    fn with_bounds(bounds: Rectangle) -> Self {
        Self {
            bounds,
            ..Self::default()
        }
    }

    fn bounds(&self) -> Rectangle {
        self.bounds
    }

    fn flush(&mut self) {}

    fn resize(&mut self, bounds: Rectangle) {
        self.bounds = bounds;
    }

    fn reset(&mut self) {
        self.bounds = Rectangle::INFINITE;

        self.quads.clear();
        self.primitives.clear();
        self.text.clear();
        self.images.clear();
    }

    fn start(&self) -> usize {
        if !self.quads.is_empty() {
            return 1;
        }

        if !self.primitives.is_empty() {
            return 2;
        }

        if !self.images.is_empty() {
            return 3;
        }

        if !self.text.is_empty() {
            return 4;
        }

        usize::MAX
    }

    fn end(&self) -> usize {
        if !self.text.is_empty() {
            return 4;
        }

        if !self.images.is_empty() {
            return 3;
        }

        if !self.primitives.is_empty() {
            return 2;
        }

        if !self.quads.is_empty() {
            return 1;
        }

        0
    }

    fn merge(&mut self, layer: &mut Self) {
        self.quads.append(&mut layer.quads);
        self.primitives.append(&mut layer.primitives);
        self.text.append(&mut layer.text);
        self.images.append(&mut layer.images);
    }
}

#[derive(Debug, Clone)]
pub enum Item<T> {
    Live(T),
    Group(Vec<T>, Rectangle, Transformation),
    Cached(Arc<[T]>, Rectangle, Transformation),
}

impl<T> Item<T> {
    pub fn transformation(&self) -> Transformation {
        match self {
            Item::Live(_) => Transformation::IDENTITY,
            Item::Group(_, _, transformation)
            | Item::Cached(_, _, transformation) => *transformation,
        }
    }

    pub fn clip_bounds(&self) -> Rectangle {
        match self {
            Item::Live(_) => Rectangle::INFINITE,
            Item::Group(_, clip_bounds, _)
            | Item::Cached(_, clip_bounds, _) => *clip_bounds,
        }
    }

    pub fn as_slice(&self) -> &[T] {
        match self {
            Item::Live(item) => std::slice::from_ref(item),
            Item::Group(group, _, _) => group.as_slice(),
            Item::Cached(cache, _, _) => cache,
        }
    }
}
