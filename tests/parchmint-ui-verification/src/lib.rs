//! Framework-neutral PNG visual verification primitives.

use std::{
    error::Error,
    fmt,
    fs::File,
    io::{BufReader, BufWriter},
    path::Path,
};

use serde::Serialize;

pub const REPORT_SCHEMA: &str = "parchmint.ui-verification/v1";

/// A tightly packed RGBA8 image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, VerificationError> {
        let expected = rgba_byte_len(width, height)?;
        if pixels.len() != expected {
            return Err(VerificationError::InvalidImageBuffer {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

#[derive(Debug)]
pub enum VerificationError {
    Io(std::io::Error),
    Decode(png::DecodingError),
    Encode(png::EncodingError),
    Json(serde_json::Error),
    InvalidImageBuffer { expected: usize, actual: usize },
    ImageTooLarge,
    UnsupportedPngColorType(png::ColorType),
}

impl fmt::Display for VerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Decode(error) => write!(formatter, "PNG decode error: {error}"),
            Self::Encode(error) => write!(formatter, "PNG encode error: {error}"),
            Self::Json(error) => write!(formatter, "JSON error: {error}"),
            Self::InvalidImageBuffer { expected, actual } => write!(
                formatter,
                "RGBA image buffer has {actual} bytes; expected {expected}"
            ),
            Self::ImageTooLarge => {
                formatter.write_str("image dimensions exceed supported memory size")
            }
            Self::UnsupportedPngColorType(color_type) => {
                write!(
                    formatter,
                    "unsupported normalized PNG color type: {color_type:?}"
                )
            }
        }
    }
}

impl Error for VerificationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::InvalidImageBuffer { .. }
            | Self::ImageTooLarge
            | Self::UnsupportedPngColorType(_) => None,
        }
    }
}

impl From<std::io::Error> for VerificationError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<png::DecodingError> for VerificationError {
    fn from(error: png::DecodingError) -> Self {
        Self::Decode(error)
    }
}

impl From<png::EncodingError> for VerificationError {
    fn from(error: png::EncodingError) -> Self {
        Self::Encode(error)
    }
}

impl From<serde_json::Error> for VerificationError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Decodes a PNG and normalizes its pixels to RGBA8.
pub fn decode_png(path: impl AsRef<Path>) -> Result<RgbaImage, VerificationError> {
    let file = File::open(path)?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let output_size = reader
        .output_buffer_size()
        .ok_or(VerificationError::ImageTooLarge)?;
    let mut buffer = vec![0; output_size];
    let frame = reader.next_frame(&mut buffer)?;
    let data = &buffer[..frame.buffer_size()];

    normalize_rgba8(frame.width, frame.height, frame.color_type, data)
}

/// Encodes an RGBA8 image as a PNG.
pub fn encode_png(path: impl AsRef<Path>, image: &RgbaImage) -> Result<(), VerificationError> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder.write_header()?;
    png_writer.write_image_data(&image.pixels)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComparisonReport {
    pub schema: &'static str,
    pub matches: bool,
    pub reference: ImageDimensions,
    pub actual: ImageDimensions,
    pub dimension_mismatch: bool,
    pub differing_pixels: Option<u64>,
    pub max_channel_delta: Option<u8>,
    pub mean_absolute_channel_delta: Option<f64>,
}

/// Compares two images. Pixel metrics are omitted when dimensions differ.
pub fn compare(reference: &RgbaImage, actual: &RgbaImage) -> ComparisonReport {
    let reference_dimensions = dimensions(reference);
    let actual_dimensions = dimensions(actual);
    if reference_dimensions != actual_dimensions {
        return ComparisonReport {
            schema: REPORT_SCHEMA,
            matches: false,
            reference: reference_dimensions,
            actual: actual_dimensions,
            dimension_mismatch: true,
            differing_pixels: None,
            max_channel_delta: None,
            mean_absolute_channel_delta: None,
        };
    }

    let mut differing_pixels = 0_u64;
    let mut max_channel_delta = 0_u8;
    let mut total_absolute_channel_delta = 0_u64;
    for (reference_pixel, actual_pixel) in reference
        .pixels
        .chunks_exact(4)
        .zip(actual.pixels.chunks_exact(4))
    {
        let mut pixel_differs = false;
        for (&reference_channel, &actual_channel) in reference_pixel.iter().zip(actual_pixel) {
            let delta = reference_channel.abs_diff(actual_channel);
            pixel_differs |= delta != 0;
            max_channel_delta = max_channel_delta.max(delta);
            total_absolute_channel_delta += u64::from(delta);
        }
        differing_pixels += u64::from(pixel_differs);
    }

    let channel_count = u64::from(reference.width) * u64::from(reference.height) * 4;
    ComparisonReport {
        schema: REPORT_SCHEMA,
        matches: differing_pixels == 0,
        reference: reference_dimensions,
        actual: actual_dimensions,
        dimension_mismatch: false,
        differing_pixels: Some(differing_pixels),
        max_channel_delta: Some(max_channel_delta),
        mean_absolute_channel_delta: Some(if channel_count == 0 {
            0.0
        } else {
            total_absolute_channel_delta as f64 / channel_count as f64
        }),
    }
}

/// Produces a high-contrast diff image. Transparent pixels match; magenta pixels differ.
/// For dimension mismatches, reference-only pixels are blue and actual-only pixels are red.
pub fn diff_image(
    reference: &RgbaImage,
    actual: &RgbaImage,
) -> Result<RgbaImage, VerificationError> {
    let width = reference.width.max(actual.width);
    let height = reference.height.max(actual.height);
    let mut pixels = vec![0; rgba_byte_len(width, height)?];

    for y in 0..height {
        for x in 0..width {
            let reference_pixel = pixel_at(reference, x, y);
            let actual_pixel = pixel_at(actual, x, y);
            let output = match (reference_pixel, actual_pixel) {
                (Some(reference_pixel), Some(actual_pixel)) if reference_pixel == actual_pixel => {
                    [0, 0, 0, 0]
                }
                (Some(_), Some(_)) => [255, 0, 255, 255],
                (Some(_), None) => [0, 96, 255, 255],
                (None, Some(_)) => [255, 80, 0, 255],
                (None, None) => unreachable!("diff canvas is the union of both images"),
            };
            let index = pixel_index(width, x, y);
            pixels[index..index + 4].copy_from_slice(&output);
        }
    }

    RgbaImage::new(width, height, pixels)
}

/// Writes a stable, pretty-printed JSON comparison report.
pub fn write_report(
    path: impl AsRef<Path>,
    report: &ComparisonReport,
) -> Result<(), VerificationError> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(BufWriter::new(file), report)?;
    Ok(())
}

fn normalize_rgba8(
    width: u32,
    height: u32,
    color_type: png::ColorType,
    data: &[u8],
) -> Result<RgbaImage, VerificationError> {
    let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| VerificationError::ImageTooLarge)?;
    let channels = match color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err(VerificationError::UnsupportedPngColorType(color_type));
        }
    };
    let expected_input = pixel_count
        .checked_mul(channels)
        .ok_or(VerificationError::ImageTooLarge)?;
    if data.len() != expected_input {
        return Err(VerificationError::InvalidImageBuffer {
            expected: expected_input,
            actual: data.len(),
        });
    }

    let mut pixels = Vec::with_capacity(rgba_byte_len(width, height)?);
    match color_type {
        png::ColorType::Grayscale => data.iter().for_each(|&gray| {
            pixels.extend_from_slice(&[gray, gray, gray, 255]);
        }),
        png::ColorType::Rgb => data.chunks_exact(3).for_each(|pixel| {
            pixels.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).for_each(|pixel| {
            pixels.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
        }),
        png::ColorType::Rgba => pixels.extend_from_slice(data),
        png::ColorType::Indexed => unreachable!("indexed color was rejected above"),
    }
    RgbaImage::new(width, height, pixels)
}

fn dimensions(image: &RgbaImage) -> ImageDimensions {
    ImageDimensions {
        width: image.width,
        height: image.height,
    }
}

fn rgba_byte_len(width: u32, height: u32) -> Result<usize, VerificationError> {
    usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| VerificationError::ImageTooLarge)
}

fn pixel_at(image: &RgbaImage, x: u32, y: u32) -> Option<&[u8]> {
    (x < image.width && y < image.height).then(|| {
        let index = pixel_index(image.width, x, y);
        &image.pixels[index..index + 4]
    })
}

fn pixel_index(width: u32, x: u32, y: u32) -> usize {
    usize::try_from((u64::from(y) * u64::from(width) + u64::from(x)) * 4)
        .expect("validated image dimensions must fit in usize")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, pixels: &[u8]) -> RgbaImage {
        RgbaImage::new(width, height, pixels.to_vec()).unwrap()
    }

    #[test]
    fn comparison_reports_deterministic_pixel_metrics() {
        let reference = image(2, 1, &[0, 10, 20, 30, 255, 255, 255, 255]);
        let actual = image(2, 1, &[0, 13, 14, 30, 255, 255, 255, 250]);

        let report = compare(&reference, &actual);

        assert!(!report.matches);
        assert!(!report.dimension_mismatch);
        assert_eq!(report.differing_pixels, Some(2));
        assert_eq!(report.max_channel_delta, Some(6));
        assert_eq!(report.mean_absolute_channel_delta, Some(14.0 / 8.0));
    }

    #[test]
    fn comparison_accepts_identical_images() {
        let reference = image(1, 1, &[1, 2, 3, 255]);

        let report = compare(&reference, &reference);

        assert!(report.matches);
        assert!(!report.dimension_mismatch);
        assert_eq!(report.differing_pixels, Some(0));
        assert_eq!(report.max_channel_delta, Some(0));
        assert_eq!(report.mean_absolute_channel_delta, Some(0.0));
    }

    #[test]
    fn comparison_marks_dimension_mismatches_without_pixel_metrics() {
        let reference = image(1, 1, &[0, 0, 0, 255]);
        let actual = image(2, 1, &[0, 0, 0, 255, 0, 0, 0, 255]);

        let report = compare(&reference, &actual);

        assert!(report.dimension_mismatch);
        assert!(!report.matches);
        assert_eq!(report.differing_pixels, None);
        assert_eq!(report.max_channel_delta, None);
        assert_eq!(report.mean_absolute_channel_delta, None);
    }

    #[test]
    fn diff_uses_visible_colors_for_changed_and_unshared_pixels() {
        let reference = image(1, 1, &[1, 2, 3, 255]);
        let actual = image(2, 1, &[1, 2, 3, 255, 4, 5, 6, 255]);

        assert_eq!(
            diff_image(&reference, &actual).unwrap().pixels(),
            &[0, 0, 0, 0, 255, 80, 0, 255]
        );
    }
}
