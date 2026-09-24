use crate::core::{Color, Rectangle, Size};
use crate::graphics::compositor::{self, Information};
use crate::graphics::damage;
use crate::graphics::error::{self, Error};
use crate::graphics::{self, Shell, Viewport};
use crate::{Layer, Renderer, Settings};

use std::collections::VecDeque;
use std::num::NonZeroU32;

pub struct Compositor {
    context: softbuffer::Context<Box<dyn compositor::Display>>,
    settings: Settings,
}

pub struct Surface {
    window: softbuffer::Surface<
        Box<dyn compositor::Display>,
        Box<dyn compositor::Window>,
    >,
    clip_mask: tiny_skia::Mask,
    history: FrameHistory,
    latest_pixels: Vec<u32>,
}

#[derive(Default)]
struct FrameHistory {
    frames: VecDeque<Frame>,
    max_age: u8,
}

struct Frame {
    layers: Vec<Layer>,
    background: Color,
    size: Size<u32>,
    scale: f32,
}

impl FrameHistory {
    fn damage(
        &self,
        renderer: &mut Renderer,
        age: u8,
        viewport: &Viewport,
        background: Color,
    ) -> Vec<Rectangle> {
        age.checked_sub(1)
            .and_then(|index| self.frames.get(index as usize))
            .filter(|frame| {
                frame.background == background
                    && frame.size == viewport.physical_size()
                    && frame.scale == viewport.scale_factor()
            })
            .map(|frame| {
                renderer.damage(&frame.layers, viewport.scale_factor())
            })
            .unwrap_or_else(|| {
                vec![Rectangle::with_size(viewport.logical_size())]
            })
    }

    fn presented(
        &mut self,
        renderer: &mut Renderer,
        age: u8,
        viewport: &Viewport,
        background: Color,
    ) {
        self.max_age = self.max_age.max(age).max(1);
        self.frames.push_front(Frame {
            layers: renderer.layers().to_vec(),
            background,
            size: viewport.physical_size(),
            scale: viewport.scale_factor(),
        });
        self.frames.truncate(self.max_age as usize);
    }

    fn clear(&mut self) {
        self.frames.clear();
        self.max_age = 0;
    }
}

impl crate::graphics::Compositor for Compositor {
    type Renderer = Renderer;
    type Surface = Surface;

    async fn with_backend(
        settings: graphics::Settings,
        display: impl compositor::Display,
        _compatible_window: impl compositor::Window,
        _shell: Shell,
        backend: Option<&str>,
    ) -> Result<Self, Error> {
        match backend {
            None | Some("tiny-skia") | Some("tiny_skia") => {
                Ok(new(settings.into(), display))
            }
            Some(backend) => Err(Error::GraphicsAdapterNotFound {
                backend: "tiny-skia",
                reason: error::Reason::DidNotMatch {
                    preferred_backend: backend.to_owned(),
                },
            }),
        }
    }

    fn create_renderer(&self) -> Self::Renderer {
        Renderer::new(
            self.settings.default_font,
            self.settings.default_text_size,
        )
    }

    fn create_surface<W: compositor::Window + Clone>(
        &mut self,
        window: W,
        width: u32,
        height: u32,
    ) -> Self::Surface {
        let window = softbuffer::Surface::new(
            &self.context,
            Box::new(window.clone()) as _,
        )
        .expect("Create softbuffer surface for window");

        let mut surface = Surface {
            window,
            clip_mask: tiny_skia::Mask::new(1, 1).expect("Create clip mask"),
            history: FrameHistory::default(),
            latest_pixels: Vec::new(),
        };

        if width > 0 && height > 0 {
            self.configure_surface(&mut surface, width, height);
        }

        surface
    }

    fn configure_surface(
        &mut self,
        surface: &mut Self::Surface,
        width: u32,
        height: u32,
    ) {
        surface
            .window
            .resize(
                NonZeroU32::new(width).expect("Non-zero width"),
                NonZeroU32::new(height).expect("Non-zero height"),
            )
            .expect("Resize surface");

        surface.clip_mask =
            tiny_skia::Mask::new(width, height).expect("Create clip mask");
        surface.history.clear();
        surface.latest_pixels = Vec::new();
    }

    fn information(&self) -> Information {
        Information {
            adapter: String::from("CPU"),
            backend: String::from("tiny-skia"),
        }
    }

    fn present(
        &mut self,
        renderer: &mut Self::Renderer,
        surface: &mut Self::Surface,
        viewport: &Viewport,
        background_color: Color,
        on_pre_present: impl FnOnce(),
    ) -> Result<(), compositor::SurfaceError> {
        present(
            renderer,
            surface,
            viewport,
            background_color,
            on_pre_present,
        )
    }

    fn screenshot(
        &mut self,
        renderer: &mut Self::Renderer,
        viewport: &Viewport,
        background_color: Color,
    ) -> Vec<u8> {
        screenshot(renderer, viewport, background_color)
    }
}

pub fn new(
    settings: Settings,
    display: impl compositor::Display,
) -> Compositor {
    #[allow(unsafe_code)]
    let context = softbuffer::Context::new(Box::new(display) as _)
        .expect("Create softbuffer context");

    Compositor { context, settings }
}

pub fn present(
    renderer: &mut Renderer,
    surface: &mut Surface,
    viewport: &Viewport,
    background_color: Color,
    on_pre_present: impl FnOnce(),
) -> Result<(), compositor::SurfaceError> {
    let physical_size = viewport.physical_size();
    let same_scene = if !surface.latest_pixels.is_empty() {
        surface.history.frames.front().and_then(|last| {
            (last.background == background_color
                && last.size == physical_size
                && last.scale == viewport.scale_factor())
                .then(|| renderer.damage(&last.layers, viewport.scale_factor()).is_empty())
        })
    } else {
        None
    } == Some(true);
    let mut buffer = surface.window.buffer_mut().map_err(|_| {
        surface.history.clear();
        surface.latest_pixels.clear();
        compositor::SurfaceError::Lost
    })?;

    let age = buffer.age();
    // A repeated scene can land in an older softbuffer back buffer. Copy the
    // last presented pixels instead of traversing and rasterizing every layer.
    let reuse_pixels = same_scene
        && surface.latest_pixels.len() == buffer.len()
        && age != 1;
    if reuse_pixels {
        buffer.copy_from_slice(&surface.latest_pixels);
    }
    let damage = if same_scene && (age == 1 || reuse_pixels) {
        Vec::new()
    } else {
        surface
            .history
            .damage(renderer, age, viewport, background_color)
    };

    if !damage.is_empty() {
        let viewport_bounds = Rectangle::with_size(viewport.logical_size());
        let damage = choose_damage(damage::group(damage, viewport_bounds), viewport_bounds);

        let mut pixels = tiny_skia::PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(&mut buffer),
            physical_size.width,
            physical_size.height,
        )
        .expect("Create pixel map");

        renderer.draw(
            &mut pixels,
            &mut surface.clip_mask,
            viewport,
            &damage,
            background_color,
        );
    }

    #[cfg(feature = "damage-verification")]
    verify_frame(renderer, &buffer, viewport, background_color);

    if !same_scene || surface.latest_pixels.len() != buffer.len() {
        surface.latest_pixels.clear();
        surface.latest_pixels.extend_from_slice(&buffer);
    }

    on_pre_present();
    buffer.present().map_err(|_| {
        // A failed presentation may have modified a reused buffer without
        // advancing its age. Its pixels are no longer represented by history.
        surface.history.clear();
        surface.latest_pixels.clear();
        compositor::SurfaceError::Lost
    })?;
    surface
        .history
        .presented(renderer, age, viewport, background_color);
    Ok(())
}

fn choose_damage(regions: Vec<Rectangle>, viewport: Rectangle) -> Vec<Rectangle> {
    // Grouping can leave overlapping rectangles. If visiting them would cover
    // more area than the entire window, one full repaint is cheaper and visits
    // each pixel only once.
    if regions.iter().map(Rectangle::area).sum::<f32>() >= viewport.area() {
        vec![viewport]
    } else {
        regions
    }
}

// Opt-in native diagnostic: check every buffer before presentation, including
// intermediate animation frames. Normal builds incur no allocation or work.
#[cfg(feature = "damage-verification")]
fn verify_frame(
    renderer: &mut Renderer,
    actual: &[u32],
    viewport: &Viewport,
    background: Color,
) {
    let size = viewport.physical_size();
    let mut expected = tiny_skia::Pixmap::new(size.width, size.height)
        .expect("Create verification pixels");
    let mut mask = tiny_skia::Mask::new(size.width, size.height)
        .expect("Create verification mask");
    renderer.draw(
        &mut expected.as_mut(),
        &mut mask,
        viewport,
        &[Rectangle::with_size(viewport.logical_size())],
        background,
    );
    let actual: &[u8] = bytemuck::cast_slice(actual);
    assert_eq!(actual.len(), expected.data().len());
    // Even a fully open mask can change tiny-skia's antialias rounding by
    // one color level. Keep alpha exact and reject larger color differences.
    if let Some(byte) =
        actual.iter().zip(expected.data()).enumerate().position(
            |(index, (a, b))| a.abs_diff(*b) > u8::from(index % 4 != 3),
        )
    {
        let pixel = byte / 4;
        if let Some(directory) = std::env::var_os("PARCHMINT_RENDER_FAILURE") {
            let directory = std::path::PathBuf::from(directory);
            for (name, data) in
                [("actual.ppm", actual), ("expected.ppm", expected.data())]
            {
                let mut ppm =
                    format!("P6\n{} {}\n255\n", size.width, size.height)
                        .into_bytes();
                for pixel in data.chunks_exact(4) {
                    ppm.extend([pixel[2], pixel[1], pixel[0]]);
                }
                std::fs::write(directory.join(name), ppm)
                    .expect("Write verification failure");
            }
        }
        panic!(
            "incremental frame differs from full repaint at ({}, {}), scale={}, actual={:?}, expected={:?}",
            pixel % size.width as usize,
            pixel / size.width as usize,
            viewport.scale_factor(),
            &actual[pixel * 4..pixel * 4 + 4],
            &expected.data()[pixel * 4..pixel * 4 + 4],
        );
    }
}

pub fn screenshot(
    renderer: &mut Renderer,
    viewport: &Viewport,
    background_color: Color,
) -> Vec<u8> {
    let size = viewport.physical_size();

    let mut offscreen_buffer: Vec<u32> =
        vec![0; size.width as usize * size.height as usize];

    let mut clip_mask = tiny_skia::Mask::new(size.width, size.height)
        .expect("Create clip mask");

    renderer.draw(
        &mut tiny_skia::PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(&mut offscreen_buffer),
            size.width,
            size.height,
        )
        .expect("Create offscreen pixel map"),
        &mut clip_mask,
        viewport,
        &[Rectangle::with_size(Size::new(
            size.width as f32,
            size.height as f32,
        ))],
        background_color,
    );

    offscreen_buffer.iter().fold(
        Vec::with_capacity(offscreen_buffer.len() * 4),
        |mut acc, pixel| {
            const A_MASK: u32 = 0xFF_00_00_00;
            const R_MASK: u32 = 0x00_FF_00_00;
            const G_MASK: u32 = 0x00_00_FF_00;
            const B_MASK: u32 = 0x00_00_00_FF;

            let a = ((A_MASK & pixel) >> 24) as u8;
            let r = ((R_MASK & pixel) >> 16) as u8;
            let g = ((G_MASK & pixel) >> 8) as u8;
            let b = (B_MASK & pixel) as u8;

            acc.extend([r, g, b, a]);
            acc
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Font, Pixels, Renderer as _, renderer::Quad};

    #[test]
    fn overlapping_damage_uses_one_full_repaint() {
        let viewport = Rectangle::with_size(Size::new(100.0, 100.0));
        let overlapping = vec![
            Rectangle {
                width: 80.0,
                ..viewport
            },
            Rectangle {
                x: 20.0,
                width: 80.0,
                ..viewport
            },
        ];
        assert_eq!(choose_damage(overlapping, viewport), vec![viewport]);

        let small = vec![Rectangle {
            width: 10.0,
            height: 10.0,
            ..viewport
        }];
        assert_eq!(choose_damage(small.clone(), viewport), small);
    }

    #[test]
    #[cfg(feature = "damage-verification")]
    #[should_panic(expected = "incremental frame differs from full repaint")]
    fn verification_detects_corrupted_pixels() {
        let viewport = Viewport::with_physical_size(Size::new(20, 20), 1.0);
        let mut renderer = Renderer::new(Font::DEFAULT, Pixels(20.0));
        renderer.reset(Rectangle::with_size(viewport.logical_size()));
        verify_frame(&mut renderer, &[0; 400], &viewport, Color::WHITE);
    }

    #[test]
    fn reused_buffers_match_full_frames_through_theme_and_scale_changes() {
        let size = Size::new(240, 180);
        let mut renderer = Renderer::new(Font::DEFAULT, Pixels(20.0));
        let mut history = FrameHistory::default();
        let mut buffers: Vec<_> = (0..3)
            .map(|_| {
                (
                    tiny_skia::Pixmap::new(size.width, size.height).unwrap(),
                    None,
                )
            })
            .collect();
        let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
        for step in 0..24 {
            let scale = if (12..18).contains(&step) { 1.5 } else { 1.0 };
            let viewport = Viewport::with_physical_size(size, scale);
            let bounds = Rectangle::with_size(viewport.logical_size());
            let background = if (6..18).contains(&step) {
                Color::BLACK
            } else {
                Color::WHITE
            };
            renderer.reset(bounds);
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: 20.0,
                        y: 20.0,
                        width: 30.0,
                        height: 30.0,
                    },
                    ..Default::default()
                },
                Color::from_rgb8(160, 100, 70),
            );
            let (pixels, last_step) = &mut buffers[step % 3];
            let age = last_step.map_or(0, |last| (step - last) as u8);
            let dirty = choose_damage(
                damage::group(
                    history.damage(&mut renderer, age, &viewport, background),
                    bounds,
                ),
                bounds,
            );
            renderer.draw(
                &mut pixels.as_mut(),
                &mut mask,
                &viewport,
                &dirty,
                background,
            );
            let mut full =
                tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            renderer.draw(
                &mut full.as_mut(),
                &mut mask,
                &viewport,
                &[bounds],
                background,
            );
            assert!(
                pixels.data() == full.data(),
                "stale buffer: step={step}, age={age}"
            );
            #[cfg(feature = "damage-verification")]
            verify_frame(
                &mut renderer,
                bytemuck::cast_slice(pixels.data()),
                &viewport,
                background,
            );
            history.presented(&mut renderer, age, &viewport, background);
            *last_step = Some(step);
            assert!(history.frames.len() <= 3);
        }
        // Unknown buffers and invalidation after resize or failed presentation
        // must repaint fully, even when widget layers are unchanged.
        let viewport = Viewport::with_physical_size(size, 1.0);
        let full = vec![Rectangle::with_size(viewport.logical_size())];
        assert_eq!(
            history.damage(&mut renderer, 0, &viewport, Color::WHITE),
            full
        );
        history.clear();
        assert_eq!(
            history.damage(&mut renderer, 1, &viewport, Color::WHITE),
            full
        );
        for _ in 0..10 {
            history.presented(&mut renderer, 1, &viewport, Color::WHITE);
        }
        assert_eq!(
            history.frames.len(),
            1,
            "retain only the required buffer history"
        );
        assert!(
            history
                .damage(&mut renderer, 1, &viewport, Color::WHITE)
                .is_empty()
        );
    }
}
