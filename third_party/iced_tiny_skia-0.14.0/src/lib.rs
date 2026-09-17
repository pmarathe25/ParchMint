#![allow(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
pub mod window;

mod engine;
mod layer;
mod primitive;
mod settings;
mod text;

#[cfg(feature = "image")]
mod raster;

#[cfg(feature = "svg")]
mod vector;

#[cfg(feature = "geometry")]
pub mod geometry;

use iced_debug as debug;
pub use iced_graphics as graphics;
pub use iced_graphics::core;

pub use layer::Layer;
pub use primitive::Primitive;
pub use settings::Settings;

#[cfg(feature = "geometry")]
pub use geometry::Geometry;

use crate::core::renderer;
use crate::core::{
    Background, Color, Font, Pixels, Point, Rectangle, Size, Transformation,
};
use crate::engine::Engine;
use crate::graphics::Viewport;
use crate::graphics::compositor;
use crate::graphics::text::{Editor, Paragraph};

/// A [`tiny-skia`] graphics renderer for [`iced`].
///
/// [`tiny-skia`]: https://github.com/RazrFalcon/tiny-skia
/// [`iced`]: https://github.com/iced-rs/iced
#[derive(Debug)]
pub struct Renderer {
    default_font: Font,
    default_text_size: Pixels,
    layers: layer::Stack,
    engine: Engine, // TODO: Shared engine
}

#[cfg(test)]
mod clipping_tests {
    use super::*;
    use crate::core::{alignment, text};

    #[test]
    fn incremental_canvas_frames_match_full_repaints() {
        use crate::core::Renderer as _;
        let bounds = Rectangle::with_size(Size::new(320.0, 200.0));
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for offset in [0.0, 0.25, 0.5, 0.75] {
                for cached in [false, true] {
                    let mut renderer =
                        Renderer::new(Font::DEFAULT, Pixels(20.0));
                    let size = Size::new(
                        (320.0 * scale) as u32,
                        (200.0 * scale) as u32,
                    );
                    let viewport = Viewport::with_physical_size(size, scale);
                    let mut pixels =
                        tiny_skia::Pixmap::new(size.width, size.height)
                            .unwrap();
                    let mut mask =
                        tiny_skia::Mask::new(size.width, size.height).unwrap();
                    let mut previous = Vec::new();
                    // Insert/remove text, move the caret/selection, recolor,
                    // scroll/animate, shrink clipping, and remove a group.
                    for step in 0..10 {
                        renderer.reset(bounds);
                        let (layer, _) = renderer.layers.current_mut();
                        let fill = |rect: Rectangle, color| Primitive::Fill {
                            path: tiny_skia::PathBuilder::from_rect(
                                tiny_skia::Rect::from_xywh(
                                    rect.x,
                                    rect.y,
                                    rect.width,
                                    rect.height,
                                )
                                .unwrap(),
                            ),
                            paint: tiny_skia::Paint {
                                shader: tiny_skia::Shader::SolidColor(
                                    engine::into_color(color),
                                ),
                                ..Default::default()
                            },
                            rule: tiny_skia::FillRule::Winding,
                        };
                        let clip = Rectangle::with_size(Size::new(
                            if step == 8 { 90.0 } else { 240.0 },
                            150.0,
                        ));
                        let transform = Transformation::translate(
                            10.0 + offset,
                            10.0 + if step == 7 { 9.75 } else { offset },
                        );
                        let selection = if step == 4 { 30.0 } else { 1.0 };
                        let mut paths = vec![
                            fill(
                                Rectangle::with_size(Size::new(240.0, 150.0)),
                                Color::from_rgb8(240, 245, 238),
                            ),
                            fill(
                                Rectangle {
                                    x: 30.25
                                        + if step == 3 { 12.0 } else { 0.0 },
                                    y: 18.5,
                                    width: selection,
                                    height: 25.0,
                                },
                                Color::from_rgba(0.2, 0.5, 0.3, 0.5),
                            ),
                        ];
                        let stroke = tiny_skia::Stroke {
                            width: 5.0,
                            ..Default::default()
                        };
                        if step == 5 {
                            paths.push(Primitive::Stroke {
                                path: tiny_skia::PathBuilder::from_rect(
                                    tiny_skia::Rect::from_xywh(
                                        70.0, 100.0, 25.0, 10.0,
                                    )
                                    .unwrap(),
                                ),
                                paint: tiny_skia::Paint::default(),
                                stroke,
                            });
                        }
                        let words = if step == 1 {
                            ["fjéZ", "unchanged"]
                        } else if step == 2 {
                            ["f", "unchanged"]
                        } else {
                            ["fjé", "unchanged"]
                        };
                        let text = words
                            .into_iter()
                            .enumerate()
                            .map(|(line, content)| graphics::Text::Cached {
                                content: content.into(),
                                bounds: Rectangle::new(
                                    Point::new(
                                        12.25,
                                        15.5 + line as f32 * 60.0,
                                    ),
                                    Size::INFINITE,
                                ),
                                color: if step == 6 {
                                    Color::from_rgb8(180, 20, 70)
                                } else {
                                    Color::BLACK
                                },
                                size: Pixels(22.0),
                                line_height: Pixels(28.0),
                                font: Font {
                                    style: core::font::Style::Italic,
                                    ..Font::DEFAULT
                                },
                                align_x: text::Alignment::Left,
                                align_y: alignment::Vertical::Top,
                                shaping: text::Shaping::Advanced,
                                clip_bounds: Rectangle::INFINITE,
                            })
                            .collect::<Vec<_>>();
                        if step != 9 {
                            if cached {
                                layer.draw_primitive_cache(
                                    paths.into(),
                                    clip,
                                    transform,
                                );
                                layer.draw_text_cache(
                                    text.into(),
                                    clip,
                                    transform,
                                );
                            } else {
                                layer.draw_primitive_group(
                                    paths, clip, transform,
                                );
                                layer.draw_text_group(text, clip, transform);
                            }
                        }
                        let damage = if step == 0 {
                            vec![bounds]
                        } else {
                            graphics::damage::group(
                                renderer.damage(&previous, scale),
                                bounds,
                            )
                        };
                        if step == 1 {
                            let area: f32 =
                                damage.iter().map(Rectangle::area).sum();
                            assert!(
                                area < clip.area() / 4.0,
                                "typing repainted the pane: {damage:?}"
                            );
                        }
                        renderer.draw(
                            &mut pixels.as_mut(),
                            &mut mask,
                            &viewport,
                            &damage,
                            Color::WHITE,
                        );
                        let mut full =
                            tiny_skia::Pixmap::new(size.width, size.height)
                                .unwrap();
                        renderer.draw(
                            &mut full.as_mut(),
                            &mut mask,
                            &viewport,
                            &[bounds],
                            Color::WHITE,
                        );
                        assert!(
                            pixels.data() == full.data(),
                            "incremental pixels: scale={scale}, offset={offset}, cached={cached}, step={step}"
                        );
                        previous = renderer.layers().to_vec();
                        assert!(
                            renderer.damage(&previous, scale).is_empty(),
                            "unchanged canvas is dirty"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn translated_canvas_text_is_clipped_at_each_display_scale() {
        for scale in [1.0_f32, 2.0] {
            for cached in [false, true] {
                let mut renderer = Renderer::new(Font::DEFAULT, Pixels(20.0));
                let make_text = |y| graphics::text::Text::Cached {
                    content: "MMMMMMMM".into(),
                    bounds: Rectangle::new(
                        Point::new(2.0, y),
                        Size::new(200.0, 24.0),
                    ),
                    color: Color::BLACK,
                    size: Pixels(20.0),
                    line_height: Pixels(24.0),
                    font: Font::DEFAULT,
                    align_x: text::Alignment::Left,
                    align_y: alignment::Vertical::Top,
                    shaping: text::Shaping::Basic,
                    clip_bounds: Rectangle::INFINITE,
                };
                let text = vec![make_text(2.0), make_text(45.0)];
                let clip = Rectangle::with_size(Size::new(40.0, 30.0));
                let transform = Transformation::translate(20.0, 20.0);
                let (layer, _) = renderer.layers.current_mut();
                if cached {
                    layer.draw_text_cache(text.into(), clip, transform);
                } else {
                    layer.draw_text_group(text, clip, transform);
                }
                let size =
                    Size::new((200.0 * scale) as u32, (120.0 * scale) as u32);
                let viewport = Viewport::with_physical_size(size, scale);
                let mut pixels =
                    tiny_skia::Pixmap::new(size.width, size.height).unwrap();
                let mut mask =
                    tiny_skia::Mask::new(size.width, size.height).unwrap();
                renderer.draw(
                    &mut pixels.as_mut(),
                    &mut mask,
                    &viewport,
                    &[Rectangle::with_size(Size::new(200.0, 120.0))],
                    Color::WHITE,
                );
                let mut ink = 0;
                for y in 0..size.height {
                    for x in 0..size.width {
                        let pixel = pixels.pixel(x, y).unwrap();
                        if pixel.red() != 255
                            || pixel.green() != 255
                            || pixel.blue() != 255
                        {
                            ink += 1;
                            assert!(
                                x >= (20.0 * scale) as u32
                                    && x < (60.0 * scale) as u32
                                    && y >= (20.0 * scale) as u32
                                    && y < (50.0 * scale) as u32,
                                "text escaped its pane at ({x}, {y}), scale={scale}, cached={cached}"
                            );
                        }
                    }
                }
                assert!(
                    ink > 0,
                    "the visible part of the text must still be painted"
                );
            }
        }
    }
}

impl Renderer {
    pub fn new(default_font: Font, default_text_size: Pixels) -> Self {
        Self {
            default_font,
            default_text_size,
            layers: layer::Stack::new(),
            engine: Engine::new(),
        }
    }

    pub fn layers(&mut self) -> &[Layer] {
        self.layers.flush();
        self.layers.as_slice()
    }

    pub fn damage(&mut self, previous: &[Layer], scale: f32) -> Vec<Rectangle> {
        self.layers.flush();
        let current = self.layers.as_slice();
        let mut damage = Vec::new();
        for (old, new) in previous.iter().zip(current) {
            damage.extend(Layer::damage(old, new, &mut self.engine, scale));
        }
        damage.extend(
            previous[current.len().min(previous.len())..]
                .iter()
                .map(|layer| layer.bounds),
        );
        damage.extend(
            current[previous.len().min(current.len())..]
                .iter()
                .map(|layer| layer.bounds),
        );
        damage
    }

    pub fn draw(
        &mut self,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        viewport: &Viewport,
        damage: &[Rectangle],
        background_color: Color,
    ) {
        let scale_factor = viewport.scale_factor();
        let mut mask_bounds = None;
        self.engine.start();

        self.layers.flush();

        for &damage_bounds in damage {
            let damage_bounds = damage_bounds * scale_factor;
            // Clear whole physical pixels. Fractional dirty edges otherwise
            // leave partially covered pixels from the preceding frame.
            let damage_bounds = Rectangle {
                x: damage_bounds.x.floor(),
                y: damage_bounds.y.floor(),
                width: (damage_bounds.x + damage_bounds.width).ceil()
                    - damage_bounds.x.floor(),
                height: (damage_bounds.y + damage_bounds.height).ceil()
                    - damage_bounds.y.floor(),
            };

            let path = tiny_skia::PathBuilder::from_rect(
                tiny_skia::Rect::from_xywh(
                    damage_bounds.x,
                    damage_bounds.y,
                    damage_bounds.width,
                    damage_bounds.height,
                )
                .expect("Create damage rectangle"),
            );

            pixels.fill_path(
                &path,
                &tiny_skia::Paint {
                    shader: tiny_skia::Shader::SolidColor(engine::into_color(
                        background_color,
                    )),
                    anti_alias: false,
                    blend_mode: tiny_skia::BlendMode::Source,
                    ..Default::default()
                },
                tiny_skia::FillRule::default(),
                tiny_skia::Transform::identity(),
                None,
            );

            for layer in self.layers.iter() {
                let Some(layer_bounds) =
                    damage_bounds.intersection(&(layer.bounds * scale_factor))
                else {
                    continue;
                };

                if !layer.quads.is_empty() {
                    engine::ensure_clip_mask(
                        clip_mask,
                        &mut mask_bounds,
                        layer_bounds,
                    );
                    let render_span = debug::render(debug::Primitive::Quad);
                    for (quad, background) in &layer.quads {
                        self.engine.draw_quad(
                            quad,
                            background,
                            Transformation::scale(scale_factor),
                            pixels,
                            clip_mask,
                            layer_bounds,
                        );
                    }
                    render_span.finish();
                }

                if !layer.primitives.is_empty() {
                    let render_span = debug::render(debug::Primitive::Triangle);

                    for group in &layer.primitives {
                        let Some(group_bounds) = (group.clip_bounds()
                            * scale_factor)
                            .intersection(&layer_bounds)
                        else {
                            continue;
                        };

                        engine::ensure_clip_mask(
                            clip_mask,
                            &mut mask_bounds,
                            group_bounds,
                        );

                        for primitive in group.as_slice() {
                            self.engine.draw_primitive(
                                primitive,
                                Transformation::scale(scale_factor)
                                    * group.transformation(),
                                pixels,
                                clip_mask,
                                group_bounds,
                            );
                        }
                    }

                    render_span.finish();
                }

                if !layer.images.is_empty() {
                    engine::ensure_clip_mask(
                        clip_mask,
                        &mut mask_bounds,
                        layer_bounds,
                    );
                    let render_span = debug::render(debug::Primitive::Image);

                    for image in &layer.images {
                        self.engine.draw_image(
                            image,
                            Transformation::scale(scale_factor),
                            pixels,
                            clip_mask,
                            layer_bounds,
                        );
                    }

                    render_span.finish();
                }

                if !layer.text.is_empty() {
                    let render_span = debug::render(debug::Primitive::Image);

                    for group in &layer.text {
                        let Some(group_bounds) = (group.clip_bounds()
                            * scale_factor)
                            .intersection(&layer_bounds)
                        else {
                            continue;
                        };
                        let transformation =
                            Transformation::scale(scale_factor)
                                * group.transformation();
                        if let layer::Item::Cached(text, _, _) = group
                            && self
                                .engine
                                .cached_text_bounds(text, transformation)
                                .is_none_or(|bounds| {
                                    !bounds.intersects(&group_bounds)
                                })
                        {
                            continue;
                        }
                        for text in group.as_slice() {
                            self.engine.draw_text(
                                text,
                                transformation,
                                pixels,
                                clip_mask,
                                group_bounds,
                                &mut mask_bounds,
                            );
                        }
                    }

                    render_span.finish();
                }
            }
        }

        self.engine.trim();
    }
}

impl core::Renderer for Renderer {
    fn start_layer(&mut self, bounds: Rectangle) {
        self.layers.push_clip(bounds);
    }

    fn end_layer(&mut self) {
        self.layers.pop_clip();
    }

    fn start_transformation(&mut self, transformation: Transformation) {
        self.layers.push_transformation(transformation);
    }

    fn end_transformation(&mut self) {
        self.layers.pop_transformation();
    }

    fn fill_quad(
        &mut self,
        quad: renderer::Quad,
        background: impl Into<Background>,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_quad(quad, background.into(), transformation);
    }

    fn reset(&mut self, new_bounds: Rectangle) {
        self.layers.reset(new_bounds);
    }

    fn allocate_image(
        &mut self,
        _handle: &core::image::Handle,
        callback: impl FnOnce(Result<core::image::Allocation, core::image::Error>)
        + Send
        + 'static,
    ) {
        #[cfg(feature = "image")]
        #[allow(unsafe_code)]
        // TODO: Concurrency
        callback(self.engine.raster_pipeline.load(_handle));

        #[cfg(not(feature = "image"))]
        callback(Err(core::image::Error::Unsupported));
    }
}

impl core::text::Renderer for Renderer {
    type Font = Font;
    type Paragraph = Paragraph;
    type Editor = Editor;

    const ICON_FONT: Font = Font::with_name("Iced-Icons");
    const CHECKMARK_ICON: char = '\u{f00c}';
    const ARROW_DOWN_ICON: char = '\u{e800}';
    const ICED_LOGO: char = '\u{e801}';
    const SCROLL_UP_ICON: char = '\u{e802}';
    const SCROLL_DOWN_ICON: char = '\u{e803}';
    const SCROLL_LEFT_ICON: char = '\u{e804}';
    const SCROLL_RIGHT_ICON: char = '\u{e805}';

    fn default_font(&self) -> Self::Font {
        self.default_font
    }

    fn default_size(&self) -> Pixels {
        self.default_text_size
    }

    fn fill_paragraph(
        &mut self,
        text: &Self::Paragraph,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();

        layer.draw_paragraph(
            text,
            position,
            color,
            clip_bounds,
            transformation,
        );
    }

    fn fill_editor(
        &mut self,
        editor: &Self::Editor,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_editor(editor, position, color, clip_bounds, transformation);
    }

    fn fill_text(
        &mut self,
        text: core::Text,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_text(text, position, color, clip_bounds, transformation);
    }
}

impl graphics::text::Renderer for Renderer {
    fn fill_raw(&mut self, raw: graphics::text::Raw) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_text_raw(raw, transformation);
    }
}

#[cfg(feature = "geometry")]
impl graphics::geometry::Renderer for Renderer {
    type Geometry = Geometry;
    type Frame = geometry::Frame;

    fn new_frame(&self, bounds: Rectangle) -> Self::Frame {
        geometry::Frame::new(bounds)
    }

    fn draw_geometry(&mut self, geometry: Self::Geometry) {
        let (layer, transformation) = self.layers.current_mut();

        match geometry {
            Geometry::Live {
                primitives,
                images,
                text,
                clip_bounds,
            } => {
                layer.draw_primitive_group(
                    primitives,
                    clip_bounds,
                    transformation,
                );

                for image in images {
                    layer.draw_image(image, transformation);
                }

                layer.draw_text_group(text, clip_bounds, transformation);
            }
            Geometry::Cache(cache) => {
                layer.draw_primitive_cache(
                    cache.primitives,
                    cache.clip_bounds,
                    transformation,
                );

                for image in cache.images.iter() {
                    layer.draw_image(image.clone(), transformation);
                }

                layer.draw_text_cache(
                    cache.text,
                    cache.clip_bounds,
                    transformation,
                );
            }
        }
    }
}

impl graphics::mesh::Renderer for Renderer {
    fn draw_mesh(&mut self, _mesh: graphics::Mesh) {
        log::warn!("iced_tiny_skia does not support drawing meshes");
    }

    fn draw_mesh_cache(&mut self, _cache: iced_graphics::mesh::Cache) {
        log::warn!("iced_tiny_skia does not support drawing meshes");
    }
}

#[cfg(feature = "image")]
impl core::image::Renderer for Renderer {
    type Handle = core::image::Handle;

    fn load_image(
        &self,
        handle: &Self::Handle,
    ) -> Result<core::image::Allocation, core::image::Error> {
        self.engine.raster_pipeline.load(handle)
    }

    fn measure_image(
        &self,
        handle: &Self::Handle,
    ) -> Option<crate::core::Size<u32>> {
        self.engine.raster_pipeline.dimensions(handle)
    }

    fn draw_image(
        &mut self,
        image: core::Image,
        bounds: Rectangle,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_raster(image, bounds, clip_bounds, transformation);
    }
}

#[cfg(feature = "svg")]
impl core::svg::Renderer for Renderer {
    fn measure_svg(
        &self,
        handle: &core::svg::Handle,
    ) -> crate::core::Size<u32> {
        self.engine.vector_pipeline.viewport_dimensions(handle)
    }

    fn draw_svg(
        &mut self,
        svg: core::Svg,
        bounds: Rectangle,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_svg(svg, bounds, clip_bounds, transformation);
    }
}

impl compositor::Default for Renderer {
    type Compositor = window::Compositor;
}

impl renderer::Headless for Renderer {
    async fn new(
        default_font: Font,
        default_text_size: Pixels,
        backend: Option<&str>,
    ) -> Option<Self> {
        if backend.is_some_and(|backend| {
            !["tiny-skia", "tiny_skia"].contains(&backend)
        }) {
            return None;
        }

        Some(Self::new(default_font, default_text_size))
    }

    fn name(&self) -> String {
        "tiny-skia".to_owned()
    }

    fn screenshot(
        &mut self,
        size: Size<u32>,
        scale_factor: f32,
        background_color: Color,
    ) -> Vec<u8> {
        let viewport = Viewport::with_physical_size(size, scale_factor);

        window::compositor::screenshot(self, &viewport, background_color)
    }
}
