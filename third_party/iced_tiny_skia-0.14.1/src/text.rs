use crate::core::alignment;
use crate::core::text::{Alignment, Shaping};
use crate::core::{Color, Font, Pixels, Point, Rectangle, Transformation};
use crate::graphics::text::cache::{self, Cache};
use crate::graphics::text::editor;
use crate::graphics::text::font_system;
use crate::graphics::text::paragraph;

use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::hash_map;

#[derive(Debug)]
pub struct Pipeline {
    glyph_cache: GlyphCache,
    cache: RefCell<Cache>,
}

impl Pipeline {
    pub fn new() -> Self {
        Pipeline {
            glyph_cache: GlyphCache::new(),
            cache: RefCell::new(Cache::new()),
        }
    }

    // TODO: Shared engine
    #[allow(dead_code)]
    pub fn load_font(&mut self, bytes: Cow<'static, [u8]>) {
        font_system()
            .write()
            .expect("Write font system")
            .load_font(bytes);

        self.cache = RefCell::new(Cache::new());
    }

    pub fn draw_paragraph(
        &mut self,
        paragraph: &paragraph::Weak,
        position: Point,
        color: Color,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: Option<&tiny_skia::Mask>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let Some(paragraph) = paragraph.upgrade() else {
            return;
        };

        let mut font_system = font_system().write().expect("Write font system");

        draw(
            font_system.raw(),
            &mut self.glyph_cache,
            paragraph.buffer(),
            position,
            color,
            pixels,
            clip_mask,
            clip_bounds,
            transformation,
        );
    }

    pub fn draw_editor(
        &mut self,
        editor: &editor::Weak,
        position: Point,
        color: Color,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: Option<&tiny_skia::Mask>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let Some(editor) = editor.upgrade() else {
            return;
        };

        let mut font_system = font_system().write().expect("Write font system");

        draw(
            font_system.raw(),
            &mut self.glyph_cache,
            editor.buffer(),
            position,
            color,
            pixels,
            clip_mask,
            clip_bounds,
            transformation,
        );
    }

    pub fn draw_cached(
        &mut self,
        content: &str,
        bounds: Rectangle,
        color: Color,
        size: Pixels,
        line_height: Pixels,
        font: Font,
        align_x: Alignment,
        align_y: alignment::Vertical,
        shaping: Shaping,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: Option<&tiny_skia::Mask>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let mut font_system = font_system().write().expect("Write font system");
        let font_system = font_system.raw();
        let (buffer, position) = cached_buffer(
            self.cache.get_mut(),
            font_system,
            content,
            bounds,
            size,
            line_height,
            font,
            align_x,
            align_y,
            shaping,
        );

        draw(
            font_system,
            &mut self.glyph_cache,
            buffer,
            position,
            color,
            pixels,
            clip_mask,
            clip_bounds,
            transformation,
        );
    }

    pub fn draw_raw(
        &mut self,
        buffer: &cosmic_text::Buffer,
        position: Point,
        color: Color,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: Option<&tiny_skia::Mask>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let mut font_system = font_system().write().expect("Write font system");

        draw(
            font_system.raw(),
            &mut self.glyph_cache,
            buffer,
            position,
            color,
            pixels,
            clip_mask,
            clip_bounds,
            transformation,
        );
    }

    pub fn visible_bounds(
        &mut self,
        text: &crate::graphics::Text,
        transformation: Transformation,
    ) -> Option<Rectangle> {
        let crate::graphics::Text::Cached {
            content,
            bounds,
            color,
            size,
            line_height,
            font,
            align_x,
            align_y,
            shaping,
            clip_bounds,
        } = text
        else {
            return text.visible_bounds().map(|bounds| bounds * transformation);
        };
        let mut font_system = font_system().write().expect("Write font system");
        let font_system = font_system.raw();
        let (buffer, position) = cached_buffer(
            self.cache.get_mut(),
            font_system,
            content,
            *bounds,
            *size,
            *line_height,
            *font,
            *align_x,
            *align_y,
            *shaping,
        );
        let position = position * transformation;
        let scale = transformation.scale_factor();
        let mut swash = cosmic_text::SwashCache::new();
        let mut ink: Option<Rectangle> = None;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((position.x, position.y), scale);
                if let Some((_, placement)) = self.glyph_cache.allocate(
                    physical.cache_key,
                    glyph.color_opt.map(from_color).unwrap_or(*color),
                    font_system,
                    &mut swash,
                ) {
                    let bounds =
                        glyph_bounds(&physical, placement, run.line_y, scale);
                    ink = Some(ink.map_or(bounds, |ink| ink.union(&bounds)));
                }
            }
        }
        ink.and_then(|ink| ink.intersection(&(*clip_bounds * transformation)))
    }

    pub fn trim_cache(&mut self) {
        self.cache.get_mut().trim();
        self.glyph_cache.trim();
    }
}

fn cached_buffer<'a>(
    cache: &'a mut Cache,
    font_system: &mut cosmic_text::FontSystem,
    content: &str,
    bounds: Rectangle,
    size: Pixels,
    line_height: Pixels,
    font: Font,
    align_x: Alignment,
    align_y: alignment::Vertical,
    shaping: Shaping,
) -> (&'a cosmic_text::Buffer, Point) {
    let (_, entry) = cache.allocate(
        font_system,
        cache::Key {
            bounds: bounds.size(),
            content,
            font,
            size: size.into(),
            line_height: line_height.into(),
            shaping,
            align_x,
        },
    );
    let x = match align_x {
        Alignment::Default | Alignment::Left | Alignment::Justified => bounds.x,
        Alignment::Center => bounds.x - entry.min_bounds.width / 2.0,
        Alignment::Right => bounds.x - entry.min_bounds.width,
    };
    let y = match align_y {
        alignment::Vertical::Top => bounds.y,
        alignment::Vertical::Center => bounds.y - entry.min_bounds.height / 2.0,
        alignment::Vertical::Bottom => bounds.y - entry.min_bounds.height,
    };
    (&entry.buffer, Point::new(x, y))
}

fn glyph_bounds(
    physical: &cosmic_text::PhysicalGlyph,
    placement: cosmic_text::Placement,
    line_y: f32,
    scale: f32,
) -> Rectangle {
    Rectangle {
        x: (physical.x + placement.left) as f32,
        y: (physical.y - placement.top + (line_y * scale).round() as i32)
            as f32,
        width: placement.width as f32,
        height: placement.height as f32,
    }
}

fn draw(
    font_system: &mut cosmic_text::FontSystem,
    glyph_cache: &mut GlyphCache,
    buffer: &cosmic_text::Buffer,
    position: Point,
    color: Color,
    pixels: &mut tiny_skia::PixmapMut<'_>,
    clip_mask: Option<&tiny_skia::Mask>,
    clip_bounds: Rectangle,
    transformation: Transformation,
) {
    let position = position * transformation;

    let mut swash = cosmic_text::SwashCache::new();

    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            let physical_glyph = glyph.physical(
                (position.x, position.y),
                transformation.scale_factor(),
            );

            if let Some((buffer, placement)) = glyph_cache.allocate(
                physical_glyph.cache_key,
                glyph.color_opt.map(from_color).unwrap_or(color),
                font_system,
                &mut swash,
            ) {
                let bounds = glyph_bounds(
                    &physical_glyph,
                    placement,
                    run.line_y,
                    transformation.scale_factor(),
                );
                if !bounds.intersects(&clip_bounds) {
                    continue;
                }
                let pixmap = tiny_skia::PixmapRef::from_bytes(
                    buffer,
                    placement.width,
                    placement.height,
                )
                .expect("Create glyph pixel map");

                let opacity = color.a
                    * glyph
                        .color_opt
                        .map(|c| c.a() as f32 / 255.0)
                        .unwrap_or(1.0);

                pixels.draw_pixmap(
                    bounds.x as i32,
                    bounds.y as i32,
                    pixmap,
                    &tiny_skia::PixmapPaint {
                        opacity,
                        ..tiny_skia::PixmapPaint::default()
                    },
                    tiny_skia::Transform::identity(),
                    clip_mask.filter(|_| !bounds.is_within(&clip_bounds)),
                );
            }
        }
    }
}

fn from_color(color: cosmic_text::Color) -> Color {
    let [r, g, b, a] = color.as_rgba();

    Color::from_rgba8(r, g, b, a as f32 / 255.0)
}

#[derive(Debug, Clone, Default)]
struct GlyphCache {
    entries: FxHashMap<
        (cosmic_text::CacheKey, [u8; 3]),
        Option<(Vec<u32>, cosmic_text::Placement)>,
    >,
    recently_used: FxHashSet<(cosmic_text::CacheKey, [u8; 3])>,
    trim_count: usize,
}

impl GlyphCache {
    const TRIM_INTERVAL: usize = 300;
    const CAPACITY_LIMIT: usize = 16 * 1024;

    fn new() -> Self {
        GlyphCache::default()
    }

    fn allocate(
        &mut self,
        cache_key: cosmic_text::CacheKey,
        color: Color,
        font_system: &mut cosmic_text::FontSystem,
        swash: &mut cosmic_text::SwashCache,
    ) -> Option<(&[u8], cosmic_text::Placement)> {
        let [r, g, b, _a] = color.into_rgba8();
        let key = (cache_key, [r, g, b]);

        if let hash_map::Entry::Vacant(entry) = self.entries.entry(key) {
            // TODO: Outline support
            // Blank glyphs (including spaces) need a cache entry too; otherwise
            // every frame repeats their font scaling and rasterization work.
            let image = swash
                .get_image_uncached(font_system, cache_key)
                .and_then(|image| {
                    let glyph_size = image.placement.width as usize
                        * image.placement.height as usize;

                    if glyph_size == 0 {
                        return None;
                    }

                    let mut buffer = vec![0u32; glyph_size];

                    match image.content {
                        cosmic_text::SwashContent::Mask => {
                            let mut i = 0;

                            // TODO: Blend alpha

                            for _y in 0..image.placement.height {
                                for _x in 0..image.placement.width {
                                    buffer[i] = bytemuck::cast(
                                        tiny_skia::ColorU8::from_rgba(
                                            b,
                                            g,
                                            r,
                                            image.data[i],
                                        )
                                        .premultiply(),
                                    );

                                    i += 1;
                                }
                            }
                        }
                        cosmic_text::SwashContent::Color => {
                            let mut i = 0;

                            for _y in 0..image.placement.height {
                                for _x in 0..image.placement.width {
                                    // TODO: Blend alpha
                                    buffer[i >> 2] = bytemuck::cast(
                                        tiny_skia::ColorU8::from_rgba(
                                            image.data[i + 2],
                                            image.data[i + 1],
                                            image.data[i],
                                            image.data[i + 3],
                                        )
                                        .premultiply(),
                                    );

                                    i += 4;
                                }
                            }
                        }
                        cosmic_text::SwashContent::SubpixelMask => {
                            // TODO
                        }
                    }

                    Some((buffer, image.placement))
                });
            let _ = entry.insert(image);
        }

        let _ = self.recently_used.insert(key);

        self.entries.get(&key).and_then(Option::as_ref).map(
            |(buffer, placement)| {
                (bytemuck::cast_slice(buffer.as_slice()), *placement)
            },
        )
    }

    pub fn trim(&mut self) {
        if self.trim_count > Self::TRIM_INTERVAL
            || self.recently_used.len() >= Self::CAPACITY_LIMIT
        {
            self.entries
                .retain(|key, _| self.recently_used.contains(key));

            self.recently_used.clear();

            self.entries.shrink_to(Self::CAPACITY_LIMIT);
            self.recently_used.shrink_to(Self::CAPACITY_LIMIT);

            self.trim_count = 0;
        } else {
            self.trim_count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invisible_glyphs_are_cached_and_evicted_like_visible_glyphs() {
        let mut fonts = font_system().write().unwrap();
        let mut text = Cache::new();
        let (_, entry) = text.allocate(
            fonts.raw(),
            cache::Key {
                content: " A",
                size: 16.0,
                line_height: 20.0,
                font: Font::DEFAULT,
                bounds: crate::core::Size::INFINITE,
                shaping: Shaping::Basic,
                align_x: Alignment::Left,
            },
        );
        let run = entry.buffer.layout_runs().next().unwrap();
        let glyph = run.glyphs[0].physical((0.0, 0.0), 1.0).cache_key;
        let visible = run.glyphs[1].physical((0.0, 0.0), 1.0).cache_key;
        let mut cache = GlyphCache::new();
        let mut swash = cosmic_text::SwashCache::new();
        for _ in 0..3 {
            assert!(
                cache
                    .allocate(glyph, Color::BLACK, fonts.raw(), &mut swash)
                    .is_none()
            );
            assert_eq!(
                cache.entries.len(),
                1,
                "remember glyphs with no pixels"
            );
            assert_eq!(cache.recently_used.len(), 1);
            cache.trim_count = GlyphCache::TRIM_INTERVAL + 1;
            cache.trim();
            assert_eq!(cache.entries.len(), 1, "retain recently used blanks");
        }
        cache.trim_count = GlyphCache::TRIM_INTERVAL + 1;
        cache.trim();
        assert!(cache.entries.is_empty(), "evict unused blanks");

        let black = cache
            .allocate(visible, Color::BLACK, fonts.raw(), &mut swash)
            .unwrap()
            .0
            .to_vec();
        assert!(
            cache
                .allocate(glyph, Color::BLACK, fonts.raw(), &mut swash)
                .is_none()
        );
        let red = cache
            .allocate(
                visible,
                Color::from_rgb(1.0, 0.0, 0.0),
                fonts.raw(),
                &mut swash,
            )
            .unwrap()
            .0
            .to_vec();
        assert_ne!(black, red, "blank entries cannot suppress visible colors");
        assert_eq!(
            cache
                .allocate(visible, Color::BLACK, fonts.raw(), &mut swash)
                .unwrap()
                .0,
            black
        );
    }
}
