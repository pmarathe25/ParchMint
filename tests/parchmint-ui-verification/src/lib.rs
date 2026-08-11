//! Framework-neutral PNG visual verification primitives.

use std::{
    error::Error,
    fmt,
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub const REPORT_SCHEMA: &str = "parchmint.ui-verification/v1";
pub const CATALOG_SCHEMA: &str = "parchmint.ui-verification-catalog/v1";

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
    OutputExists(PathBuf),
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
            Self::OutputExists(path) => {
                write!(formatter, "output already exists: {}", path.display())
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
            | Self::UnsupportedPngColorType(_)
            | Self::OutputExists(_) => None,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ComparisonReport {
    pub schema: String,
    pub matches: bool,
    pub reference: ImageDimensions,
    pub actual: ImageDimensions,
    pub dimension_mismatch: bool,
    pub differing_pixels: Option<u64>,
    pub max_channel_delta: Option<u8>,
    pub mean_absolute_channel_delta: Option<f64>,
    pub structural: Option<StructuralMetrics>,
}

/// A renderer-tolerant comparison over a fixed 32 x 32 sample grid.
///
/// Exact pixel metrics remain authoritative for byte-identical exports. This
/// metric isolates broad layout/color drift from antialiasing noise; its
/// conservative thresholds are intentionally far below a full-screen design
/// mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct StructuralMetrics {
    pub sample_width: u32,
    pub sample_height: u32,
    pub luminance_mae: f64,
    pub chroma_mae: f64,
    pub alpha_mae: f64,
    pub max_tile_luminance_mae: f64,
    pub max_tile_chroma_mae: f64,
    pub max_tile_alpha_mae: f64,
}

/// Strict generic bounds for renderer-tolerant catalog acceptance.
pub const MAX_STRUCTURAL_LUMINANCE_MAE: f64 = 0.015;
pub const MAX_STRUCTURAL_CHROMA_MAE: f64 = 0.015;
pub const MAX_STRUCTURAL_ALPHA_MAE: f64 = 0.01;
pub const MAX_STRUCTURAL_TILE_LUMINANCE_MAE: f64 = 0.02;
pub const MAX_STRUCTURAL_TILE_CHROMA_MAE: f64 = 0.02;
pub const MAX_STRUCTURAL_TILE_ALPHA_MAE: f64 = 0.01;

/// Compares two images. Pixel metrics are omitted when dimensions differ.
pub fn compare(reference: &RgbaImage, actual: &RgbaImage) -> ComparisonReport {
    let reference_dimensions = dimensions(reference);
    let actual_dimensions = dimensions(actual);
    if reference_dimensions != actual_dimensions {
        return ComparisonReport {
            schema: REPORT_SCHEMA.to_owned(),
            matches: false,
            reference: reference_dimensions,
            actual: actual_dimensions,
            dimension_mismatch: true,
            differing_pixels: None,
            max_channel_delta: None,
            mean_absolute_channel_delta: None,
            structural: None,
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
        schema: REPORT_SCHEMA.to_owned(),
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
        structural: Some(structural_metrics(reference, actual)),
    }
}

/// Whether a same-size image passes the strict renderer-tolerant fallback.
/// Exact equality always passes. Dimension mismatches always fail.
pub fn passes_acceptance(report: &ComparisonReport) -> bool {
    report.matches
        || report.structural.is_some_and(|metrics| {
            metrics.luminance_mae <= MAX_STRUCTURAL_LUMINANCE_MAE
                && metrics.chroma_mae <= MAX_STRUCTURAL_CHROMA_MAE
                && metrics.alpha_mae <= MAX_STRUCTURAL_ALPHA_MAE
                && metrics.max_tile_luminance_mae <= MAX_STRUCTURAL_TILE_LUMINANCE_MAE
                && metrics.max_tile_chroma_mae <= MAX_STRUCTURAL_TILE_CHROMA_MAE
                && metrics.max_tile_alpha_mae <= MAX_STRUCTURAL_TILE_ALPHA_MAE
        })
}

fn structural_metrics(reference: &RgbaImage, actual: &RgbaImage) -> StructuralMetrics {
    const SAMPLE_WIDTH: u32 = 32;
    const SAMPLE_HEIGHT: u32 = 32;
    const TILE_WIDTH: u32 = 8;
    const TILE_HEIGHT: u32 = 8;
    if reference.width == 0 || reference.height == 0 {
        return StructuralMetrics {
            sample_width: SAMPLE_WIDTH,
            sample_height: SAMPLE_HEIGHT,
            luminance_mae: 0.0,
            chroma_mae: 0.0,
            alpha_mae: 0.0,
            max_tile_luminance_mae: 0.0,
            max_tile_chroma_mae: 0.0,
            max_tile_alpha_mae: 0.0,
        };
    }
    let mut luminance_error = 0.0;
    let mut chroma_error = 0.0;
    let mut alpha_error = 0.0;
    let mut tile_errors = vec![[0.0; 3]; usize::try_from(TILE_WIDTH * TILE_HEIGHT).unwrap()];
    for sample_y in 0..SAMPLE_HEIGHT {
        for sample_x in 0..SAMPLE_WIDTH {
            let reference =
                averaged_sample(reference, sample_x, sample_y, SAMPLE_WIDTH, SAMPLE_HEIGHT);
            let actual = averaged_sample(actual, sample_x, sample_y, SAMPLE_WIDTH, SAMPLE_HEIGHT);
            let reference_luma = luma([reference[0], reference[1], reference[2]]);
            let actual_luma = luma([actual[0], actual[1], actual[2]]);
            let luminance_delta = (reference_luma - actual_luma).abs();
            luminance_error += luminance_delta;
            let reference_chroma =
                chroma([reference[0], reference[1], reference[2]], reference_luma);
            let actual_chroma = chroma([actual[0], actual[1], actual[2]], actual_luma);
            let chroma_delta = ((reference_chroma.0 - actual_chroma.0).abs()
                + (reference_chroma.1 - actual_chroma.1).abs())
                / 2.0;
            chroma_error += chroma_delta;
            let alpha_delta = (reference[3] - actual[3]).abs();
            alpha_error += alpha_delta;
            let tile_x = sample_x * TILE_WIDTH / SAMPLE_WIDTH;
            let tile_y = sample_y * TILE_HEIGHT / SAMPLE_HEIGHT;
            let tile_index = usize::try_from(tile_y * TILE_WIDTH + tile_x).unwrap();
            tile_errors[tile_index][0] += luminance_delta;
            tile_errors[tile_index][1] += chroma_delta;
            tile_errors[tile_index][2] += alpha_delta;
        }
    }
    let samples = f64::from(SAMPLE_WIDTH * SAMPLE_HEIGHT);
    let samples_per_tile = f64::from((SAMPLE_WIDTH / TILE_WIDTH) * (SAMPLE_HEIGHT / TILE_HEIGHT));
    let max_tile_luminance_mae = tile_errors
        .iter()
        .map(|errors| errors[0] / samples_per_tile)
        .fold(0.0, f64::max);
    let max_tile_chroma_mae = tile_errors
        .iter()
        .map(|errors| errors[1] / samples_per_tile)
        .fold(0.0, f64::max);
    let max_tile_alpha_mae = tile_errors
        .iter()
        .map(|errors| errors[2] / samples_per_tile)
        .fold(0.0, f64::max);
    StructuralMetrics {
        sample_width: SAMPLE_WIDTH,
        sample_height: SAMPLE_HEIGHT,
        luminance_mae: luminance_error / samples,
        chroma_mae: chroma_error / samples,
        alpha_mae: alpha_error / samples,
        max_tile_luminance_mae,
        max_tile_chroma_mae,
        max_tile_alpha_mae,
    }
}

fn averaged_sample(
    image: &RgbaImage,
    sample_x: u32,
    sample_y: u32,
    sample_width: u32,
    sample_height: u32,
) -> [f64; 4] {
    let start_x = sample_x * image.width / sample_width;
    let end_x = ((sample_x + 1) * image.width / sample_width).max(start_x + 1);
    let start_y = sample_y * image.height / sample_height;
    let end_y = ((sample_y + 1) * image.height / sample_height).max(start_y + 1);
    let mut total = [0.0; 4];
    let mut count = 0.0;
    for y in start_y..end_y.min(image.height) {
        for x in start_x..end_x.min(image.width) {
            let pixel = pixel_at(image, x, y).expect("sample coordinates are in bounds");
            for (index, value) in pixel.iter().enumerate() {
                total[index] += f64::from(*value) / 255.0;
            }
            count += 1.0;
        }
    }
    [
        total[0] / count,
        total[1] / count,
        total[2] / count,
        total[3] / count,
    ]
}

fn luma(rgb: [f64; 3]) -> f64 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

fn chroma(rgb: [f64; 3], luminance: f64) -> (f64, f64) {
    (rgb[0] - luminance, rgb[2] - luminance)
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

/// Artifacts written for one catalog scenario. References are never modified.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CatalogCaseReport {
    pub id: String,
    pub appearance: String,
    pub reference_path: PathBuf,
    pub actual_path: PathBuf,
    pub diff_path: PathBuf,
    pub report_path: PathBuf,
    pub acceptance_passed: bool,
    pub comparison: ComparisonReport,
}

/// Aggregate artifact index for a complete catalog run.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CatalogIndex {
    pub schema: &'static str,
    pub total_cases: usize,
    pub accepted_cases: usize,
    pub failed_cases: usize,
    pub cases: Vec<CatalogCaseReport>,
}

/// Writes a new actual PNG, diff, and detailed report for one scenario.
pub fn write_catalog_case(
    output_directory: impl AsRef<Path>,
    id: &str,
    appearance: &str,
    reference_path: impl AsRef<Path>,
    actual: &RgbaImage,
) -> Result<CatalogCaseReport, VerificationError> {
    let output_directory = output_directory.as_ref().join(appearance);
    fs::create_dir_all(&output_directory)?;
    let actual_path = output_directory.join(format!("{id}-actual.png"));
    let diff_path = output_directory.join(format!("{id}-diff.png"));
    let report_path = output_directory.join(format!("{id}-report.json"));
    for path in [&actual_path, &diff_path, &report_path] {
        if path.exists() {
            return Err(VerificationError::OutputExists(path.clone()));
        }
    }
    let reference_path = reference_path.as_ref().to_path_buf();
    let reference = decode_png(&reference_path)?;
    let comparison = compare(&reference, actual);
    let diff = diff_image(&reference, actual)?;
    encode_png(&actual_path, actual)?;
    encode_png(&diff_path, &diff)?;
    write_report(&report_path, &comparison)?;
    Ok(CatalogCaseReport {
        id: id.to_owned(),
        appearance: appearance.to_owned(),
        reference_path,
        actual_path,
        diff_path,
        report_path,
        acceptance_passed: passes_acceptance(&comparison),
        comparison,
    })
}

/// Writes a stable aggregate index for a completed catalog run.
pub fn write_catalog_index(
    output_directory: impl AsRef<Path>,
    cases: &[CatalogCaseReport],
) -> Result<PathBuf, VerificationError> {
    let output_path = output_directory.as_ref().join("catalog-index.json");
    if output_path.exists() {
        return Err(VerificationError::OutputExists(output_path));
    }
    let file = File::create(&output_path)?;
    let accepted_cases = cases.iter().filter(|case| case.acceptance_passed).count();
    serde_json::to_writer_pretty(
        BufWriter::new(file),
        &CatalogIndex {
            schema: CATALOG_SCHEMA,
            total_cases: cases.len(),
            accepted_cases,
            failed_cases: cases.len() - accepted_cases,
            cases: cases.to_vec(),
        },
    )?;
    Ok(output_path)
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static UNIQUE: AtomicUsize = AtomicUsize::new(0);

    fn image(width: u32, height: u32, pixels: &[u8]) -> RgbaImage {
        RgbaImage::new(width, height, pixels.to_vec()).unwrap()
    }

    fn source_png(
        root: &Path,
        name: &str,
        color: png::ColorType,
        depth: png::BitDepth,
        pixels: &[u8],
    ) -> PathBuf {
        let path = root.join(name);
        let file = File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(BufWriter::new(file), 1, 1);
        encoder.set_color(color);
        encoder.set_depth(depth);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(pixels)
            .unwrap();
        path
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

    #[test]
    fn catalog_case_writes_artifacts_and_keeps_dimension_mismatch_red() {
        let root = std::env::temp_dir().join(format!(
            "parchmint-verification-catalog-{}-{}",
            std::process::id(),
            UNIQUE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let reference = root.join("reference.png");
        encode_png(&reference, &image(1, 1, &[1, 2, 3, 255])).unwrap();
        let report = write_catalog_case(
            root.join("output"),
            "launcher-light",
            "light",
            &reference,
            &image(2, 1, &[1, 2, 3, 255, 4, 5, 6, 255]),
        )
        .unwrap();
        assert!(!report.acceptance_passed);
        assert!(report.actual_path.is_file());
        assert!(report.diff_path.is_file());
        assert!(report.report_path.is_file());
        assert!(report.comparison.dimension_mismatch);
        assert!(
            write_catalog_index(root.join("output"), &[report])
                .unwrap()
                .is_file()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn structural_metric_accepts_antialiasing_but_rejects_a_large_layout_change() {
        let reference = image(
            2,
            2,
            &[
                100, 100, 100, 255, 100, 100, 100, 255, 100, 100, 100, 255, 100, 100, 100, 255,
            ],
        );
        let antialiasing = image(
            2,
            2,
            &[
                101, 100, 99, 255, 100, 99, 101, 255, 99, 101, 100, 255, 100, 100, 100, 255,
            ],
        );
        let layout_change = image(
            2,
            2,
            &[
                250, 0, 0, 255, 250, 0, 0, 255, 250, 0, 0, 255, 250, 0, 0, 255,
            ],
        );
        assert!(passes_acceptance(&compare(&reference, &antialiasing)));
        assert!(!passes_acceptance(&compare(&reference, &layout_change)));
    }

    #[test]
    fn structural_metric_rejects_alpha_loss_and_localized_missing_controls() {
        let opaque = image(64, 64, &vec![255; 64 * 64 * 4]);
        let transparent = image(64, 64, &[255, 255, 255, 0].repeat(64 * 64));
        let alpha_report = compare(&opaque, &transparent);
        assert!(alpha_report.structural.unwrap().alpha_mae > MAX_STRUCTURAL_ALPHA_MAE);
        assert!(!passes_acceptance(&alpha_report));

        let mut missing_control = vec![255; 64 * 64 * 4];
        for y in 8..10 {
            for x in 8..24 {
                let index = pixel_index(64, x, y);
                missing_control[index..index + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
        let localized = image(64, 64, &missing_control);
        let local_report = compare(&opaque, &localized);
        let metrics = local_report.structural.unwrap();
        assert!(metrics.luminance_mae < MAX_STRUCTURAL_LUMINANCE_MAE);
        assert!(metrics.max_tile_luminance_mae > MAX_STRUCTURAL_TILE_LUMINANCE_MAE);
        assert!(!passes_acceptance(&local_report));
    }

    #[test]
    fn png_decode_normalizes_every_supported_color_type_and_strips_16_bit_depth() {
        let root = std::env::temp_dir().join(format!(
            "parchmint-verification-png-normalization-{}-{}",
            std::process::id(),
            UNIQUE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let cases = [
            (
                source_png(
                    &root,
                    "gray.png",
                    png::ColorType::Grayscale,
                    png::BitDepth::Eight,
                    &[16],
                ),
                vec![16, 16, 16, 255],
            ),
            (
                source_png(
                    &root,
                    "rgb.png",
                    png::ColorType::Rgb,
                    png::BitDepth::Eight,
                    &[1, 2, 3],
                ),
                vec![1, 2, 3, 255],
            ),
            (
                source_png(
                    &root,
                    "gray-alpha.png",
                    png::ColorType::GrayscaleAlpha,
                    png::BitDepth::Eight,
                    &[4, 5],
                ),
                vec![4, 4, 4, 5],
            ),
            (
                source_png(
                    &root,
                    "rgba.png",
                    png::ColorType::Rgba,
                    png::BitDepth::Eight,
                    &[6, 7, 8, 9],
                ),
                vec![6, 7, 8, 9],
            ),
            (
                source_png(
                    &root,
                    "gray-16.png",
                    png::ColorType::Grayscale,
                    png::BitDepth::Sixteen,
                    &[0x12, 0x34],
                ),
                vec![0x12, 0x12, 0x12, 255],
            ),
        ];
        for (path, expected) in cases {
            assert_eq!(decode_png(path).unwrap().pixels(), expected);
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
