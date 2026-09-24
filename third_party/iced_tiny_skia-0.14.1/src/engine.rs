use crate::Primitive;
use crate::core::renderer::Quad;
use crate::core::{
    Background, Color, Gradient, Rectangle, Size, Transformation, Vector,
};
use crate::graphics::{Image, Text};
use crate::text;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

type TextGroupKey = (usize, [u32; 3]);
type TextGroupBounds = (Arc<[Text]>, Option<Rectangle>);

#[derive(Debug)]
pub struct Engine {
    text_pipeline: text::Pipeline,
    text_group_bounds: FxHashMap<TextGroupKey, TextGroupBounds>,
    used_text_groups: FxHashSet<TextGroupKey>,
    font_version: crate::graphics::text::Version,

    #[cfg(feature = "image")]
    pub(crate) raster_pipeline: crate::raster::Pipeline,
    #[cfg(feature = "svg")]
    pub(crate) vector_pipeline: crate::vector::Pipeline,
}

impl Engine {
    pub fn text_bounds(
        &mut self,
        text: &Text,
        transformation: Transformation,
    ) -> Option<Rectangle> {
        self.text_pipeline.visible_bounds(text, transformation)
    }

    pub fn new() -> Self {
        Self {
            text_pipeline: text::Pipeline::new(),
            text_group_bounds: FxHashMap::default(),
            used_text_groups: FxHashSet::default(),
            font_version: crate::graphics::text::Version::default(),
            #[cfg(feature = "image")]
            raster_pipeline: crate::raster::Pipeline::new(),
            #[cfg(feature = "svg")]
            vector_pipeline: crate::vector::Pipeline::new(),
        }
    }

    pub fn start(&mut self) {
        let version = crate::graphics::text::font_system()
            .read()
            .expect("Read font system")
            .version();
        if self.font_version != version {
            self.text_group_bounds.clear();
            self.font_version = version;
        }
    }

    pub fn cached_text_bounds(
        &mut self,
        text: &Arc<[Text]>,
        transform: Transformation,
    ) -> Option<Rectangle> {
        let translation = transform.translation();
        let key = (
            Arc::as_ptr(text) as *const () as usize,
            [
                translation.x.to_bits(),
                translation.y.to_bits(),
                transform.scale_factor().to_bits(),
            ],
        );
        let _ = self.used_text_groups.insert(key);
        if let Some((_, bounds)) = self.text_group_bounds.get(&key) {
            return *bounds;
        }
        let bounds = text
            .iter()
            .filter_map(|text| self.text_bounds(text, transform))
            .reduce(|a, b| a.union(&b));
        // Canvas cached strings are immutable. Raw buffers and weak paragraphs
        // may change behind their handles, so do not memoize those bounds.
        if text.iter().all(|text| matches!(text, Text::Cached { .. })) {
            let _ = self
                .text_group_bounds
                .insert(key, (Arc::clone(text), bounds));
        }
        bounds
    }

    pub fn draw_quad(
        &mut self,
        quad: &Quad,
        background: &Background,
        transformation: Transformation,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        clip_bounds: Rectangle,
    ) {
        if !clip_bounds
            .intersects(&(quad_visible_bounds(quad) * transformation))
        {
            return;
        }

        if draw_opaque_quad(
            quad,
            background,
            transformation,
            pixels,
            clip_mask,
            clip_bounds,
        ) {
            return;
        }

        let transform = into_transform(transformation);

        // Make sure the border radius is not larger than the bounds
        let border_width = quad
            .border
            .width
            .min(quad.bounds.width / 2.0)
            .min(quad.bounds.height / 2.0);

        let mut fill_border_radius = <[f32; 4]>::from(quad.border.radius);

        for radius in &mut fill_border_radius {
            *radius = (*radius)
                .min(quad.bounds.width / 2.0)
                .min(quad.bounds.height / 2.0);
        }

        let path = rounded_rectangle(quad.bounds, fill_border_radius);

        draw_shadow(
            quad,
            fill_border_radius,
            transformation,
            pixels,
            clip_mask,
            clip_bounds,
        );

        // Keep antialiased edges on the same blending path in full and partial
        // repaints. Switching between masked and unmasked paths changes rounding
        // at overlapping fills and borders.
        let clip_mask = Some(clip_mask as &_);

        // A border-only container uses a transparent background. Rasterizing
        // its full interior cannot change any pixels and is especially costly
        // for large Overview cards.
        if !matches!(background, Background::Color(color) if color.a == 0.0) {
            pixels.fill_path(
            &path,
            &tiny_skia::Paint {
                shader: match background {
                    Background::Color(color) => {
                        tiny_skia::Shader::SolidColor(into_color(*color))
                    }
                    Background::Gradient(Gradient::Linear(linear)) => {
                        let (start, end) =
                            linear.angle.to_distance(&quad.bounds);

                        let stops: Vec<tiny_skia::GradientStop> = linear
                            .stops
                            .into_iter()
                            .flatten()
                            .map(|stop| {
                                tiny_skia::GradientStop::new(
                                    stop.offset,
                                    tiny_skia::Color::from_rgba(
                                        stop.color.b,
                                        stop.color.g,
                                        stop.color.r,
                                        stop.color.a,
                                    )
                                    .expect("Create color"),
                                )
                            })
                            .collect();

                        tiny_skia::LinearGradient::new(
                            tiny_skia::Point {
                                x: start.x,
                                y: start.y,
                            },
                            tiny_skia::Point { x: end.x, y: end.y },
                            if stops.is_empty() {
                                vec![tiny_skia::GradientStop::new(
                                    0.0,
                                    tiny_skia::Color::BLACK,
                                )]
                            } else {
                                stops
                            },
                            tiny_skia::SpreadMode::Pad,
                            tiny_skia::Transform::identity(),
                        )
                        .expect("Create linear gradient")
                    }
                },
                anti_alias: true,
                ..tiny_skia::Paint::default()
            },
            tiny_skia::FillRule::EvenOdd,
            transform,
            clip_mask,
            );
        }

        if border_width > 0.0 {
            // Border path is offset by half the border width
            let border_bounds = Rectangle {
                x: quad.bounds.x + border_width / 2.0,
                y: quad.bounds.y + border_width / 2.0,
                width: quad.bounds.width - border_width,
                height: quad.bounds.height - border_width,
            };

            // Make sure the border radius is correct
            let mut border_radius = <[f32; 4]>::from(quad.border.radius);
            let mut is_simple_border = true;

            for radius in &mut border_radius {
                *radius = if *radius == 0.0 {
                    // Path should handle this fine
                    0.0
                } else if *radius > border_width / 2.0 {
                    *radius - border_width / 2.0
                } else {
                    is_simple_border = false;
                    0.0
                }
                .min(border_bounds.width / 2.0)
                .min(border_bounds.height / 2.0);
            }

            // Stroking a path works well in this case
            if is_simple_border {
                let border_path =
                    rounded_rectangle(border_bounds, border_radius);

                pixels.stroke_path(
                    &border_path,
                    &tiny_skia::Paint {
                        shader: tiny_skia::Shader::SolidColor(into_color(
                            quad.border.color,
                        )),
                        anti_alias: true,
                        ..tiny_skia::Paint::default()
                    },
                    &tiny_skia::Stroke {
                        width: border_width,
                        ..tiny_skia::Stroke::default()
                    },
                    transform,
                    clip_mask,
                );
            } else {
                // Draw corners that have too small border radii as having no border radius,
                // but mask them with the rounded rectangle with the correct border radius.
                let mut temp_pixmap = tiny_skia::Pixmap::new(
                    quad.bounds.width as u32,
                    quad.bounds.height as u32,
                )
                .unwrap();

                let mut quad_mask = tiny_skia::Mask::new(
                    quad.bounds.width as u32,
                    quad.bounds.height as u32,
                )
                .unwrap();

                let zero_bounds = Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: quad.bounds.width,
                    height: quad.bounds.height,
                };
                let path = rounded_rectangle(zero_bounds, fill_border_radius);

                quad_mask.fill_path(
                    &path,
                    tiny_skia::FillRule::EvenOdd,
                    true,
                    transform,
                );
                let path_bounds = Rectangle {
                    x: border_width / 2.0,
                    y: border_width / 2.0,
                    width: quad.bounds.width - border_width,
                    height: quad.bounds.height - border_width,
                };

                let border_radius_path =
                    rounded_rectangle(path_bounds, border_radius);

                temp_pixmap.stroke_path(
                    &border_radius_path,
                    &tiny_skia::Paint {
                        shader: tiny_skia::Shader::SolidColor(into_color(
                            quad.border.color,
                        )),
                        anti_alias: true,
                        ..tiny_skia::Paint::default()
                    },
                    &tiny_skia::Stroke {
                        width: border_width,
                        ..tiny_skia::Stroke::default()
                    },
                    transform,
                    Some(&quad_mask),
                );

                pixels.draw_pixmap(
                    quad.bounds.x as i32,
                    quad.bounds.y as i32,
                    temp_pixmap.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    transform,
                    clip_mask,
                );
            }
        }
    }

    pub fn draw_text(
        &mut self,
        text: &Text,
        transformation: Transformation,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        clip_bounds: Rectangle,
        mask_bounds: &mut Option<Rectangle>,
    ) {
        match text {
            Text::Paragraph {
                paragraph,
                position,
                color,
                clip_bounds: local_clip_bounds,
                transformation: local_transformation,
            } => {
                let transformation = transformation * *local_transformation;
                let Some(clip_bounds) = clip_bounds
                    .intersection(&(*local_clip_bounds * transformation))
                else {
                    return;
                };

                let physical_bounds =
                    Rectangle::new(*position, paragraph.min_bounds)
                        * transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = match physical_bounds.is_within(&clip_bounds) {
                    true => None,
                    false => {
                        ensure_clip_mask(clip_mask, mask_bounds, clip_bounds);
                        Some(clip_mask as &_)
                    }
                };

                self.text_pipeline.draw_paragraph(
                    paragraph,
                    *position,
                    *color,
                    pixels,
                    clip_mask,
                    clip_bounds,
                    transformation,
                );
            }
            Text::Editor {
                editor,
                position,
                color,
                clip_bounds: local_clip_bounds,
                transformation: local_transformation,
            } => {
                let transformation = transformation * *local_transformation;
                let Some(clip_bounds) = clip_bounds
                    .intersection(&(*local_clip_bounds * transformation))
                else {
                    return;
                };

                let physical_bounds =
                    Rectangle::new(*position, editor.bounds) * transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = match physical_bounds.is_within(&clip_bounds) {
                    true => None,
                    false => {
                        ensure_clip_mask(clip_mask, mask_bounds, clip_bounds);
                        Some(clip_mask as &_)
                    }
                };

                self.text_pipeline.draw_editor(
                    editor,
                    *position,
                    *color,
                    pixels,
                    clip_mask,
                    clip_bounds,
                    transformation,
                );
            }
            Text::Cached {
                content,
                bounds,
                color,
                size,
                line_height,
                font,
                align_x,
                align_y,
                shaping,
                clip_bounds: local_clip_bounds,
            } => {
                let Some(physical_bounds) =
                    self.text_pipeline.visible_bounds(text, transformation)
                else {
                    return;
                };
                let Some(clip_bounds) = clip_bounds
                    .intersection(&(*local_clip_bounds * transformation))
                else {
                    return;
                };

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = match physical_bounds.is_within(&clip_bounds) {
                    true => None,
                    false => {
                        ensure_clip_mask(clip_mask, mask_bounds, clip_bounds);
                        Some(clip_mask as &_)
                    }
                };

                self.text_pipeline.draw_cached(
                    content,
                    *bounds,
                    *color,
                    *size,
                    *line_height,
                    *font,
                    *align_x,
                    *align_y,
                    *shaping,
                    pixels,
                    clip_mask,
                    clip_bounds,
                    transformation,
                );
            }
            Text::Raw {
                raw,
                transformation: local_transformation,
            } => {
                let Some(buffer) = raw.buffer.upgrade() else {
                    return;
                };

                let transformation = transformation * *local_transformation;
                let (width, height) = buffer.size();

                let physical_bounds = Rectangle::new(
                    raw.position,
                    Size::new(
                        width.unwrap_or(clip_bounds.width),
                        height.unwrap_or(clip_bounds.height),
                    ),
                ) * transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = if physical_bounds.is_within(&clip_bounds) {
                    None
                } else {
                    ensure_clip_mask(clip_mask, mask_bounds, clip_bounds);
                    Some(clip_mask as &_)
                };

                self.text_pipeline.draw_raw(
                    &buffer,
                    raw.position,
                    raw.color,
                    pixels,
                    clip_mask,
                    clip_bounds,
                    transformation,
                );
            }
        }
    }

    pub fn draw_primitive(
        &mut self,
        primitive: &Primitive,
        transformation: Transformation,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        clip_bounds: Rectangle,
    ) {
        match primitive {
            Primitive::Fill { path, paint, rule } => {
                if draw_opaque_rectangle(
                    path,
                    paint,
                    transformation,
                    pixels,
                    clip_bounds,
                ) {
                    return;
                }
                let physical_bounds = {
                    let bounds = path.bounds();

                    Rectangle {
                        x: bounds.x(),
                        y: bounds.y(),
                        width: bounds.width(),
                        height: bounds.height(),
                    } * transformation
                };

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                pixels.fill_path(
                    path,
                    paint,
                    *rule,
                    into_transform(transformation),
                    Some(clip_mask),
                );
            }
            Primitive::Stroke {
                path,
                paint,
                stroke,
            } => {
                let physical_bounds = {
                    let bounds = path.bounds();

                    Rectangle {
                        x: bounds.x() - stroke.width / 2.0,
                        y: bounds.y() - stroke.width / 2.0,
                        width: bounds.width() + stroke.width,
                        height: bounds.height() + stroke.width,
                    } * transformation
                };

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                pixels.stroke_path(
                    path,
                    paint,
                    stroke,
                    into_transform(transformation),
                    Some(clip_mask),
                );
            }
        }
    }

    pub fn draw_image(
        &mut self,
        image: &Image,
        _transformation: Transformation,
        _pixels: &mut tiny_skia::PixmapMut<'_>,
        _clip_mask: &mut tiny_skia::Mask,
        _clip_bounds: Rectangle,
    ) {
        match image {
            #[cfg(feature = "image")]
            Image::Raster { image, bounds, .. } => {
                let physical_bounds = *bounds * _transformation;

                if !_clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = (!physical_bounds.is_within(&_clip_bounds))
                    .then_some(_clip_mask as &_);

                let center = physical_bounds.center();
                let radians = f32::from(image.rotation);

                let transform = into_transform(_transformation).post_rotate_at(
                    radians.to_degrees(),
                    center.x,
                    center.y,
                );

                self.raster_pipeline.draw(
                    &image.handle,
                    image.filter_method,
                    *bounds,
                    image.opacity,
                    _pixels,
                    transform,
                    clip_mask,
                );
            }
            #[cfg(feature = "svg")]
            Image::Vector { svg, bounds, .. } => {
                let physical_bounds = *bounds * _transformation;

                if !_clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = (!physical_bounds.is_within(&_clip_bounds))
                    .then_some(_clip_mask as &_);

                let center = physical_bounds.center();
                let radians = f32::from(svg.rotation);

                let transform = into_transform(_transformation).post_rotate_at(
                    radians.to_degrees(),
                    center.x,
                    center.y,
                );

                self.vector_pipeline.draw(
                    &svg.handle,
                    svg.color,
                    *bounds,
                    svg.opacity,
                    _pixels,
                    transform,
                    clip_mask,
                );
            }
            #[cfg(not(feature = "image"))]
            Image::Raster { .. } => {
                log::warn!(
                    "Unsupported primitive in `iced_tiny_skia`: {image:?}",
                );
            }
            #[cfg(not(feature = "svg"))]
            Image::Vector { .. } => {
                log::warn!(
                    "Unsupported primitive in `iced_tiny_skia`: {image:?}",
                );
            }
        }
    }

    pub fn trim(&mut self) {
        self.text_group_bounds
            .retain(|key, _| self.used_text_groups.contains(key));
        self.used_text_groups.clear();
        self.text_pipeline.trim_cache();

        #[cfg(feature = "image")]
        self.raster_pipeline.trim_cache();

        #[cfg(feature = "svg")]
        self.vector_pipeline.trim_cache();
    }
}

pub fn into_color(color: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(color.b, color.g, color.r, color.a)
        .expect("Convert color from iced to tiny_skia")
}

fn into_transform(transformation: Transformation) -> tiny_skia::Transform {
    let translation = transformation.translation();

    tiny_skia::Transform {
        sx: transformation.scale_factor(),
        kx: 0.0,
        ky: 0.0,
        sy: transformation.scale_factor(),
        tx: translation.x,
        ty: translation.y,
    }
}

fn draw_opaque_rectangle(
    path: &tiny_skia::Path,
    paint: &tiny_skia::Paint<'_>,
    transformation: Transformation,
    pixels: &mut tiny_skia::PixmapMut<'_>,
    clip: Rectangle,
) -> bool {
    let tiny_skia::Shader::SolidColor(color) = paint.shader else {
        return false;
    };
    if color.alpha() != 1.0
        || paint.blend_mode != tiny_skia::BlendMode::SourceOver
    {
        return false;
    }
    let rect = path.bounds();
    let corners = [
        tiny_skia::Point::from_xy(rect.left(), rect.top()),
        tiny_skia::Point::from_xy(rect.right(), rect.top()),
        tiny_skia::Point::from_xy(rect.right(), rect.bottom()),
        tiny_skia::Point::from_xy(rect.left(), rect.bottom()),
    ];
    use tiny_skia::PathSegment::{Close, LineTo, MoveTo};
    if !path.segments().eq([
        MoveTo(corners[0]),
        LineTo(corners[1]),
        LineTo(corners[2]),
        LineTo(corners[3]),
        Close,
    ]) {
        return false;
    }
    let bounds = Rectangle {
        x: rect.x(),
        y: rect.y(),
        width: rect.width(),
        height: rect.height(),
    } * transformation;
    if [
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        clip.x,
        clip.y,
        clip.width,
        clip.height,
    ]
    .into_iter()
    .any(|value| !value.is_finite() || value.fract() != 0.0)
    {
        return false;
    }
    let Some(visible) = bounds.intersection(&clip) else {
        return false;
    };
    pixels.fill_path(
        &rounded_rectangle(visible, [0.0; 4]),
        paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
    true
}

fn draw_opaque_quad(
    quad: &Quad,
    background: &Background,
    transformation: Transformation,
    pixels: &mut tiny_skia::PixmapMut<'_>,
    clip_mask: &tiny_skia::Mask,
    clip: Rectangle,
) -> bool {
    let Background::Color(color) = background else {
        return false;
    };
    if color.a != 1.0 || quad.border.width != 0.0 || quad.shadow.color.a != 0.0
    {
        return false;
    }
    let bounds = quad.bounds * transformation;
    // Integer edges have full coverage in the flat middle. Fractional layouts
    // retain the original antialiased path, including during animations.
    if [
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        clip.x,
        clip.y,
        clip.width,
        clip.height,
    ]
    .into_iter()
    .any(|value| !value.is_finite() || value.fract() != 0.0)
    {
        return false;
    }
    let radii = <[f32; 4]>::from(quad.border.radius).map(|radius| {
        radius
            .min(quad.bounds.width / 2.0)
            .min(quad.bounds.height / 2.0)
    });
    if radii
        .iter()
        .any(|radius| !radius.is_finite() || *radius < 0.0)
    {
        return false;
    }
    let radius = radii.into_iter().fold(0.0_f32, f32::max)
        * transformation.scale_factor();
    // Bound temporary corner masks and avoid splitting small controls.
    if radius > 64.0 || bounds.width < 64.0 || bounds.height < 64.0 {
        return false;
    }
    let top = (bounds.y + radius + 1.0).ceil();
    let bottom = (bounds.y + bounds.height - radius - 1.0).floor();
    if bottom - top < 32.0 {
        return false;
    }
    let Some(visible) = bounds.intersection(&clip).and_then(|bounds| {
        bounds.intersection(&Rectangle::with_size(Size::new(
            pixels.width() as f32,
            pixels.height() as f32,
        )))
    }) else {
        return false;
    };
    let Some(middle) = visible.intersection(&Rectangle {
        x: bounds.x,
        y: top,
        width: bounds.width,
        height: bottom - top,
    }) else {
        return false;
    };
    let paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(into_color(*color)),
        anti_alias: true,
        ..Default::default()
    };
    pixels.fill_path(
        &rounded_rectangle(middle, [0.0; 4]),
        &paint,
        tiny_skia::FillRule::EvenOdd,
        tiny_skia::Transform::identity(),
        None,
    );

    // Keep the original path and blending pipeline for antialiased corners.
    // Borrow full-width row bands, so the large middle never enters the masked
    // rasterizer. Only the small corresponding mask rows need copying.
    let path = rounded_rectangle(quad.bounds, radii)
        .transform(into_transform(transformation))
        .unwrap();
    let width = pixels.width();
    for (start, end) in [
        (visible.y as u32, middle.y as u32),
        (
            (middle.y + middle.height) as u32,
            (visible.y + visible.height) as u32,
        ),
    ] {
        if start == end {
            continue;
        }
        let rows =
            start as usize * width as usize..end as usize * width as usize;
        let mask = tiny_skia::Mask::from_vec(
            clip_mask.data()[rows.clone()].to_vec(),
            tiny_skia::IntSize::from_wh(width, end - start).unwrap(),
        )
        .unwrap();
        let mut band = tiny_skia::PixmapMut::from_bytes(
            &mut pixels.data_mut()[rows.start * 4..rows.end * 4],
            width,
            end - start,
        )
        .unwrap();
        band.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::EvenOdd,
            tiny_skia::Transform::from_translate(0.0, -(start as f32)),
            Some(&mask),
        );
    }
    true
}

fn rounded_rectangle(
    bounds: Rectangle,
    border_radius: [f32; 4],
) -> tiny_skia::Path {
    let [top_left, top_right, bottom_right, bottom_left] = border_radius;

    if top_left == 0.0
        && top_right == 0.0
        && bottom_right == 0.0
        && bottom_left == 0.0
    {
        return tiny_skia::PathBuilder::from_rect(
            tiny_skia::Rect::from_xywh(
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
            )
            .expect("Build quad rectangle"),
        );
    }

    if top_left == top_right
        && top_left == bottom_right
        && top_left == bottom_left
        && top_left == bounds.width / 2.0
        && top_left == bounds.height / 2.0
    {
        return tiny_skia::PathBuilder::from_circle(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
            top_left,
        )
        .expect("Build circle path");
    }

    let mut builder = tiny_skia::PathBuilder::new();

    builder.move_to(bounds.x + top_left, bounds.y);
    builder.line_to(bounds.x + bounds.width - top_right, bounds.y);

    if top_right > 0.0 {
        arc_to(
            &mut builder,
            bounds.x + bounds.width - top_right,
            bounds.y,
            bounds.x + bounds.width,
            bounds.y + top_right,
            top_right,
        );
    }

    maybe_line_to(
        &mut builder,
        bounds.x + bounds.width,
        bounds.y + bounds.height - bottom_right,
    );

    if bottom_right > 0.0 {
        arc_to(
            &mut builder,
            bounds.x + bounds.width,
            bounds.y + bounds.height - bottom_right,
            bounds.x + bounds.width - bottom_right,
            bounds.y + bounds.height,
            bottom_right,
        );
    }

    maybe_line_to(
        &mut builder,
        bounds.x + bottom_left,
        bounds.y + bounds.height,
    );

    if bottom_left > 0.0 {
        arc_to(
            &mut builder,
            bounds.x + bottom_left,
            bounds.y + bounds.height,
            bounds.x,
            bounds.y + bounds.height - bottom_left,
            bottom_left,
        );
    }

    maybe_line_to(&mut builder, bounds.x, bounds.y + top_left);

    if top_left > 0.0 {
        arc_to(
            &mut builder,
            bounds.x,
            bounds.y + top_left,
            bounds.x + top_left,
            bounds.y,
            top_left,
        );
    }

    builder.finish().expect("Build rounded rectangle path")
}

fn maybe_line_to(path: &mut tiny_skia::PathBuilder, x: f32, y: f32) {
    if path.last_point() != Some(tiny_skia::Point { x, y }) {
        path.line_to(x, y);
    }
}

fn arc_to(
    path: &mut tiny_skia::PathBuilder,
    x_from: f32,
    y_from: f32,
    x_to: f32,
    y_to: f32,
    radius: f32,
) {
    let svg_arc = kurbo::SvgArc {
        from: kurbo::Point::new(f64::from(x_from), f64::from(y_from)),
        to: kurbo::Point::new(f64::from(x_to), f64::from(y_to)),
        radii: kurbo::Vec2::new(f64::from(radius), f64::from(radius)),
        x_rotation: 0.0,
        large_arc: false,
        sweep: true,
    };

    match kurbo::Arc::from_svg_arc(&svg_arc) {
        Some(arc) => {
            arc.to_cubic_beziers(0.1, |p1, p2, p| {
                path.cubic_to(
                    p1.x as f32,
                    p1.y as f32,
                    p2.x as f32,
                    p2.y as f32,
                    p.x as f32,
                    p.y as f32,
                );
            });
        }
        None => {
            path.line_to(x_to, y_to);
        }
    }
}

fn shadow_bounds(quad: &Quad) -> Rectangle {
    let blur = quad.shadow.blur_radius.max(0.0);
    Rectangle {
        x: quad.bounds.x + quad.shadow.offset.x - blur,
        y: quad.bounds.y + quad.shadow.offset.y - blur,
        width: quad.bounds.width + 2.0 * blur,
        height: quad.bounds.height + 2.0 * blur,
    }
}

pub(crate) fn quad_visible_bounds(quad: &Quad) -> Rectangle {
    if quad.shadow.color.a > 0.0 {
        quad.bounds.union(&shadow_bounds(quad))
    } else {
        quad.bounds
    }
}

fn draw_shadow(
    quad: &Quad,
    radii: [f32; 4],
    transformation: Transformation,
    pixels: &mut tiny_skia::PixmapMut<'_>,
    clip_mask: &tiny_skia::Mask,
    clip_bounds: Rectangle,
) {
    if quad.shadow.color.a <= 0.0 {
        return;
    }
    let Some(visible) = (shadow_bounds(quad) * transformation)
        .intersection(&clip_bounds)
        .and_then(|bounds| {
            bounds.intersection(&Rectangle::with_size(Size::new(
                pixels.width() as f32,
                pixels.height() as f32,
            )))
        })
    else {
        return;
    };
    // Allocate and shade only damaged, on-screen pixels. Rounding outward
    // preserves fractional edges; the layer mask supplies the exact clip.
    let x = visible.x.floor() as u32;
    let y = visible.y.floor() as u32;
    let width = (visible.x + visible.width).ceil() as u32 - x;
    let height = (visible.y + visible.height).ceil() as u32 - y;
    let bounds = quad.bounds * transformation;
    let Some(half_size) =
        tiny_skia::Size::from_wh(bounds.width / 2.0, bounds.height / 2.0)
    else {
        return;
    };
    let scale = transformation.scale_factor();
    let radii = radii.map(|radius| radius * scale);
    let blur = quad.shadow.blur_radius.max(0.0) * scale;
    let center = Vector::new(
        bounds.x + quad.shadow.offset.x * scale + half_size.width(),
        bounds.y + quad.shadow.offset.y * scale + half_size.height(),
    );
    let colors: Vec<_> = (y..y + height)
        .flat_map(|y| (x..x + width).map(move |x| (x as f32, y as f32)))
        .map(|(x, y)| {
            let distance = rounded_box_sdf(
                Vector::new(x - center.x, y - center.y),
                half_size,
                &radii,
            );
            let alpha = if blur > 0.0 {
                1.0 - smoothstep(-blur, blur, distance.max(0.0))
            } else if distance <= 0.0 {
                1.0
            } else {
                0.0
            };
            let mut color = into_color(quad.shadow.color);
            color.apply_opacity(alpha);
            color.to_color_u8().premultiply()
        })
        .collect();
    if let Some(pixmap) =
        tiny_skia::IntSize::from_wh(width, height).and_then(|size| {
            tiny_skia::Pixmap::from_vec(bytemuck::cast_vec(colors), size)
        })
    {
        pixels.draw_pixmap(
            x as i32,
            y as i32,
            pixmap.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            Some(clip_mask),
        );
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let x = ((x - a) / (b - a)).clamp(0.0, 1.0);

    x * x * (3.0 - 2.0 * x)
}

fn rounded_box_sdf(
    to_center: Vector,
    size: tiny_skia::Size,
    radii: &[f32],
) -> f32 {
    let radius = match (to_center.x > 0.0, to_center.y > 0.0) {
        (true, true) => radii[2],
        (true, false) => radii[1],
        (false, true) => radii[3],
        (false, false) => radii[0],
    };

    let x = (to_center.x.abs() - size.width() + radius).max(0.0);
    let y = (to_center.y.abs() - size.height() + radius).max(0.0);

    (x.powf(2.0) + y.powf(2.0)).sqrt() - radius
}

// All mask writes in a paint pass go through this key. Reset it when beginning
// a new pass, since the caller may supply a different pixel buffer or mask.
pub(crate) fn ensure_clip_mask(
    clip_mask: &mut tiny_skia::Mask,
    current: &mut Option<Rectangle>,
    bounds: Rectangle,
) {
    if *current != Some(bounds) {
        if let Some(previous) = *current {
            // The preceding mask contains one rectangle. Clear only its rows
            // and columns, not the rest of the window-sized allocation.
            let width = clip_mask.width() as usize;
            let height = clip_mask.height() as usize;
            let left = previous.x.floor().clamp(0.0, width as f32) as usize;
            let right = (previous.x + previous.width)
                .ceil()
                .clamp(0.0, width as f32) as usize;
            let top = previous.y.floor().clamp(0.0, height as f32) as usize;
            let bottom = (previous.y + previous.height)
                .ceil()
                .clamp(0.0, height as f32) as usize;
            for row in clip_mask
                .data_mut()
                .chunks_exact_mut(width)
                .take(bottom)
                .skip(top)
            {
                row[left..right].fill(0);
            }
        } else {
            clip_mask.clear();
        }
        fill_clip_mask(clip_mask, bounds);
        *current = Some(bounds);
    }
}

#[cfg(test)]
fn adjust_clip_mask(clip_mask: &mut tiny_skia::Mask, bounds: Rectangle) {
    clip_mask.clear();
    fill_clip_mask(clip_mask, bounds);
}

fn fill_clip_mask(clip_mask: &mut tiny_skia::Mask, bounds: Rectangle) {
    let path = {
        let mut builder = tiny_skia::PathBuilder::new();
        builder.push_rect(
            tiny_skia::Rect::from_xywh(
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
            )
            .unwrap(),
        );

        builder.finish().unwrap()
    };

    clip_mask.fill_path(
        &path,
        tiny_skia::FillRule::EvenOdd,
        false,
        tiny_skia::Transform::default(),
    );
}

#[cfg(test)]
mod clip_mask_tests {
    use super::*;

    #[test]
    fn partial_bordered_quad_repaints_are_stable() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for border_width in [0.0, 1.0] {
                let quad = Quad {
                    bounds: Rectangle {
                        x: 15.0,
                        y: 15.0,
                        width: 140.0,
                        height: 90.0,
                    },
                    border: crate::core::Border {
                        color: Color::from_rgba(0.2, 0.3, 0.2, 0.6),
                        width: border_width,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                };
                let background = Color::from_rgb8(239, 242, 239).into();
                let mut engine = Engine::new();
                let mut full = tiny_skia::Pixmap::new(360, 240).unwrap();
                full.fill(tiny_skia::Color::WHITE);
                let mut mask = tiny_skia::Mask::new(360, 240).unwrap();
                let viewport = Rectangle::with_size(Size::new(360.0, 240.0));
                adjust_clip_mask(&mut mask, viewport);
                let transform = Transformation::scale(scale);
                engine.draw_quad(
                    &quad,
                    &background,
                    transform,
                    &mut full.as_mut(),
                    &mut mask,
                    viewport,
                );
                for clip in [
                    Rectangle {
                        x: 15.0,
                        y: 15.0,
                        width: 5.0,
                        height: 90.0,
                    },
                    Rectangle {
                        x: 150.0,
                        y: 15.0,
                        width: 5.0,
                        height: 90.0,
                    },
                    Rectangle {
                        x: 15.0,
                        y: 15.0,
                        width: 140.0,
                        height: 5.0,
                    },
                    Rectangle {
                        x: 15.0,
                        y: 100.0,
                        width: 140.0,
                        height: 5.0,
                    },
                ] {
                    let clip = clip * scale;
                    let clip = Rectangle {
                        x: clip.x.floor(),
                        y: clip.y.floor(),
                        width: (clip.x + clip.width).ceil() - clip.x.floor(),
                        height: (clip.y + clip.height).ceil() - clip.y.floor(),
                    };
                    let mut partial = full.clone();
                    partial.fill_path(
                        &rounded_rectangle(clip, [0.0; 4]),
                        &tiny_skia::Paint {
                            shader: tiny_skia::Shader::SolidColor(
                                tiny_skia::Color::WHITE,
                            ),
                            anti_alias: false,
                            ..Default::default()
                        },
                        tiny_skia::FillRule::Winding,
                        tiny_skia::Transform::identity(),
                        None,
                    );
                    adjust_clip_mask(&mut mask, clip);
                    engine.draw_quad(
                        &quad,
                        &background,
                        transform,
                        &mut partial.as_mut(),
                        &mut mask,
                        clip,
                    );
                    assert!(
                        partial.data() == full.data(),
                        "partial border: scale={scale}, width={border_width}, clip={clip:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn fully_open_masks_only_change_antialias_rounding() {
        let bounds = Rectangle {
            x: 0.0,
            y: 52.0,
            width: 280.0,
            height: 636.0,
        };
        let mut full = tiny_skia::Pixmap::new(320, 720).unwrap();
        full.fill(tiny_skia::Color::WHITE);
        let mut masked = full.clone();
        let mut mask = tiny_skia::Mask::new(320, 720).unwrap();
        mask.data_mut().fill(255);
        let path = rounded_rectangle(bounds, [4.0; 4]);
        let paint = tiny_skia::Paint {
            shader: tiny_skia::Shader::SolidColor(into_color(
                Color::from_rgb8(239, 242, 239),
            )),
            anti_alias: true,
            ..Default::default()
        };
        for (pixels, clip) in [(&mut full, None), (&mut masked, Some(&mask))] {
            pixels.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::EvenOdd,
                tiny_skia::Transform::identity(),
                clip,
            );
        }
        let differences: Vec<_> = full
            .data()
            .iter()
            .zip(masked.data())
            .map(|(a, b)| a.abs_diff(*b))
            .filter(|difference| *difference != 0)
            .collect();
        assert!(
            !differences.is_empty(),
            "the two rasterizer paths round differently"
        );
        assert!(differences.iter().all(|difference| *difference == 1));
    }

    #[test]
    fn shadow_clipping_matches_a_crop_including_offscreen_and_hard_edges() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for blur_radius in [0.0, 12.0] {
                let quad = Quad {
                    bounds: Rectangle {
                        x: -6.25,
                        y: -3.5,
                        width: 70.0,
                        height: 55.0,
                    },
                    border: crate::core::border::rounded(8.0),
                    shadow: crate::core::Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                        offset: Vector::new(-4.0, 6.0),
                        blur_radius,
                    },
                    ..Default::default()
                };
                let mut engine = Engine::new();
                let render = |engine: &mut Engine, transform, bounds| {
                    let mut pixels = tiny_skia::Pixmap::new(320, 240).unwrap();
                    pixels.fill(tiny_skia::Color::WHITE);
                    let mut mask = tiny_skia::Mask::new(320, 240).unwrap();
                    ensure_clip_mask(&mut mask, &mut None, bounds);
                    engine.draw_quad(
                        &quad,
                        &Color::TRANSPARENT.into(),
                        transform,
                        &mut pixels.as_mut(),
                        &mut mask,
                        bounds,
                    );
                    pixels
                };
                let clip = Rectangle {
                    x: 5.0,
                    y: 7.0,
                    width: 90.0,
                    height: 75.0,
                };
                let clipped =
                    render(&mut engine, Transformation::scale(scale), clip);
                let full = render(
                    &mut engine,
                    Transformation::translate(100.0, 100.0)
                        * Transformation::scale(scale),
                    Rectangle::with_size(Size::new(320.0, 240.0)),
                );
                let mut ink = 0;
                for y in 0..240 {
                    for x in 0..320 {
                        let actual = clipped.pixel(x, y).unwrap();
                        if (5..95).contains(&x) && (7..82).contains(&y) {
                            assert_eq!(
                                actual,
                                full.pixel(x + 100, y + 100).unwrap(),
                                "shadow crop: scale={scale}, blur={blur_radius}, x={x}, y={y}"
                            );
                            ink += usize::from(actual.red() != 255);
                        } else {
                            assert_eq!(
                                actual.red(),
                                255,
                                "shadow escaped the clip"
                            );
                        }
                    }
                }
                assert!(ink > 0);
            }
        }
    }

    #[test]
    fn cached_ink_bounds_follow_transforms_and_release_unused_groups() {
        let mut engine = Engine::new();
        engine.start();
        let group: Arc<[Text]> = vec![Text::Cached {
            content: "Ink".into(),
            bounds: Rectangle::new(
                crate::core::Point::new(10.0, 10.0),
                Size::INFINITE,
            ),
            color: Color::BLACK,
            size: crate::core::Pixels(20.0),
            line_height: crate::core::Pixels(24.0),
            font: crate::core::Font::DEFAULT,
            align_x: crate::core::text::Alignment::Left,
            align_y: crate::core::alignment::Vertical::Top,
            shaping: crate::core::text::Shaping::Basic,
            clip_bounds: Rectangle::INFINITE,
        }]
        .into();
        let bounds = engine
            .cached_text_bounds(&group, Transformation::IDENTITY)
            .unwrap();
        assert_eq!(
            engine.cached_text_bounds(&group, Transformation::IDENTITY),
            Some(bounds)
        );
        assert_eq!(engine.text_group_bounds.len(), 1);
        engine.trim();
        let moved = engine
            .cached_text_bounds(&group, Transformation::translate(9.0, 7.0))
            .unwrap();
        assert_eq!(
            moved,
            Rectangle {
                x: bounds.x + 9.0,
                y: bounds.y + 7.0,
                ..bounds
            }
        );
        engine.trim();
        assert_eq!(engine.text_group_bounds.len(), 1);
        engine.trim();
        assert!(engine.text_group_bounds.is_empty());
        assert_eq!(Arc::strong_count(&group), 1);
    }

    #[test]
    fn opaque_rectangles_match_masked_paths() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for offset in [-8.0, 0.0, 0.25, 0.5, 4.0] {
                for color in [
                    Color::BLACK,
                    Color::WHITE,
                    Color::from_rgb(0.12345, 0.54321, 0.98765),
                ] {
                    let path = rounded_rectangle(
                        Rectangle {
                            x: offset,
                            y: offset,
                            width: 100.0,
                            height: 80.0,
                        },
                        [0.0; 4],
                    );
                    let paint = tiny_skia::Paint {
                        shader: tiny_skia::Shader::SolidColor(into_color(
                            color,
                        )),
                        ..Default::default()
                    };
                    let transform = Transformation::scale(scale);
                    let clip = Rectangle {
                        x: 10.0,
                        y: 10.0,
                        width: 80.0,
                        height: 5.0,
                    };
                    let mut expected =
                        tiny_skia::Pixmap::new(220, 180).unwrap();
                    expected
                        .fill(tiny_skia::Color::from_rgba8(80, 70, 90, 170));
                    let mut actual = expected.clone();
                    let mut mask = tiny_skia::Mask::new(220, 180).unwrap();
                    adjust_clip_mask(&mut mask, clip);
                    expected.fill_path(
                        &path,
                        &paint,
                        tiny_skia::FillRule::Winding,
                        into_transform(transform),
                        Some(&mask),
                    );
                    let fast = draw_opaque_rectangle(
                        &path,
                        &paint,
                        transform,
                        &mut actual.as_mut(),
                        clip,
                    );
                    if !fast {
                        actual.fill_path(
                            &path,
                            &paint,
                            tiny_skia::FillRule::Winding,
                            into_transform(transform),
                            Some(&mask),
                        );
                    }
                    assert_eq!(fast, (offset * scale).fract() == 0.0);
                    assert!(
                        actual.data() == expected.data(),
                        "scale={scale}, offset={offset}, color={color:?}"
                    );
                }
            }
        }
        let bounds = Rectangle::with_size(Size::new(80.0, 60.0));
        let path = rounded_rectangle(bounds, [0.0; 4]);
        let mut pixels = tiny_skia::Pixmap::new(80, 60).unwrap();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(10, 20, 30, 128);
        assert!(!draw_opaque_rectangle(
            &path,
            &paint,
            Transformation::IDENTITY,
            &mut pixels.as_mut(),
            bounds
        ));
        paint.set_color_rgba8(10, 20, 30, 255);
        paint.blend_mode = tiny_skia::BlendMode::DestinationOut;
        assert!(!draw_opaque_rectangle(
            &path,
            &paint,
            Transformation::IDENTITY,
            &mut pixels.as_mut(),
            bounds
        ));
        paint.blend_mode = tiny_skia::BlendMode::SourceOver;
        assert!(!draw_opaque_rectangle(
            &rounded_rectangle(bounds, [4.0; 4]),
            &paint,
            Transformation::IDENTITY,
            &mut pixels.as_mut(),
            bounds
        ));
    }

    #[test]
    fn opaque_quad_bands_match_full_paths_at_each_scale() {
        let mut engine = Engine::new();
        let mut fast_cases = 0;
        let mut edge_cases = 0;
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for offset in [-8.25, -8.0, 0.0, 0.49, 0.5, 0.51, 9.75] {
                for radii in [[0.0; 4], [4.0; 4], [3.0, 12.0, 5.0, 9.0]] {
                    let quad = Quad {
                        bounds: Rectangle {
                            x: offset,
                            y: offset,
                            width: 160.0,
                            height: 100.0,
                        },
                        border: crate::core::Border {
                            radius: crate::core::border::Radius {
                                top_left: radii[0],
                                top_right: radii[1],
                                bottom_right: radii[2],
                                bottom_left: radii[3],
                            },
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    let transform = Transformation::scale(scale)
                        * Transformation::translate(4.0, 4.0);
                    for clip in [
                        Rectangle {
                            x: 35.0 + offset,
                            y: 30.0 + offset,
                            width: 80.0,
                            height: 35.0,
                        },
                        Rectangle {
                            x: 0.0,
                            y: 0.0,
                            width: 130.0,
                            height: 85.0,
                        },
                        quad.bounds,
                    ] {
                        let clip = clip * transform;
                        for color in [
                            Color::BLACK,
                            Color::WHITE,
                            Color::from_rgb8(239, 242, 239),
                            Color::from_rgb(0.12345, 0.54321, 0.98765),
                        ] {
                            let background = Background::Color(color);
                            let mut expected =
                                tiny_skia::Pixmap::new(360, 240).unwrap();
                            expected.fill(tiny_skia::Color::from_rgba8(
                                53, 81, 113, 170,
                            ));
                            let mut actual = expected.clone();
                            let mut mask =
                                tiny_skia::Mask::new(360, 240).unwrap();
                            adjust_clip_mask(&mut mask, clip);
                            expected.fill_path(
                                &rounded_rectangle(quad.bounds, radii),
                                &tiny_skia::Paint {
                                    shader: tiny_skia::Shader::SolidColor(
                                        into_color(color),
                                    ),
                                    anti_alias: true,
                                    ..Default::default()
                                },
                                tiny_skia::FillRule::EvenOdd,
                                into_transform(transform),
                                Some(&mask),
                            );
                            if draw_opaque_quad(
                                &quad,
                                &background,
                                transform,
                                &mut actual.as_mut(),
                                &mask,
                                clip,
                            ) {
                                fast_cases += 1;
                            } else {
                                edge_cases += 1;
                                engine.draw_quad(
                                    &quad,
                                    &background,
                                    transform,
                                    &mut actual.as_mut(),
                                    &mut mask,
                                    clip,
                                );
                            }
                            assert!(
                                actual.data() == expected.data(),
                                "scale={scale}, offset={offset}, radii={radii:?}, clip={clip:?}, color={color:?}"
                            );
                        }
                    }
                }
            }
        }
        assert!(fast_cases > 0 && edge_cases > 0);
    }

    #[test]
    fn quad_bands_exclude_borders_shadows_transparency_and_gradients() {
        let mut quad = Quad {
            bounds: Rectangle {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
            ..Default::default()
        };
        let clip = Rectangle {
            x: 20.0,
            y: 20.0,
            width: 30.0,
            height: 20.0,
        };
        let eligible = |quad: &Quad, color| {
            let mut pixels = tiny_skia::Pixmap::new(100, 80).unwrap();
            let mut mask = tiny_skia::Mask::new(100, 80).unwrap();
            adjust_clip_mask(&mut mask, clip);
            draw_opaque_quad(
                quad,
                &Background::Color(color),
                Transformation::IDENTITY,
                &mut pixels.as_mut(),
                &mask,
                clip,
            )
        };
        assert!(eligible(&quad, Color::WHITE));
        assert!(!eligible(&quad, Color::TRANSPARENT));
        assert!(!eligible(
            &quad,
            Color {
                a: 0.5,
                ..Color::WHITE
            }
        ));
        quad.border.width = 1.0;
        assert!(!eligible(&quad, Color::WHITE));
        quad.border.width = 0.0;
        quad.border.radius = (-4.0).into();
        assert!(!eligible(&quad, Color::WHITE));
        quad.border.radius = 0.0.into();
        quad.shadow.color = Color::BLACK;
        assert!(!eligible(&quad, Color::WHITE));
        quad.shadow.color = Color::TRANSPARENT;
        let gradient = crate::core::gradient::Linear::new(0.0)
            .add_stop(0.0, Color::BLACK)
            .add_stop(1.0, Color::WHITE);
        assert!(!draw_opaque_quad(
            &quad,
            &Background::Gradient(gradient.into()),
            Transformation::IDENTITY,
            &mut tiny_skia::Pixmap::new(100, 80).unwrap().as_mut(),
            &tiny_skia::Mask::new(100, 80).unwrap(),
            clip
        ));
    }

    #[test]
    fn reused_masks_match_fresh_masks_when_bounds_change() {
        let mut reused = tiny_skia::Mask::new(80, 60).unwrap();
        let mut fresh = reused.clone();
        let mut current = None;
        let a = Rectangle {
            x: 3.5,
            y: 4.0,
            width: 25.0,
            height: 40.0,
        };
        let b = Rectangle {
            x: 20.0,
            y: 7.5,
            width: 45.0,
            height: 20.0,
        };
        for bounds in [a, a, b, b, a] {
            ensure_clip_mask(&mut reused, &mut current, bounds);
            adjust_clip_mask(&mut fresh, bounds);
            assert_eq!(reused.data(), fresh.data());
        }
        for offset in [-90.0, -0.75, 0.25, 0.5, 0.75, 75.5] {
            for extent in [0.25, 1.0, 10.5, 110.0] {
                let bounds = Rectangle {
                    x: offset,
                    y: offset,
                    width: extent,
                    height: extent,
                };
                ensure_clip_mask(&mut reused, &mut current, bounds);
                adjust_clip_mask(&mut fresh, bounds);
                assert_eq!(reused.data(), fresh.data());
            }
        }
        // A new paint pass cannot assume which mask the caller supplies.
        adjust_clip_mask(&mut reused, b);
        ensure_clip_mask(&mut reused, &mut None, a);
        adjust_clip_mask(&mut fresh, a);
        assert_eq!(reused.data(), fresh.data());
    }
}
