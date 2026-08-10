use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use parchmint_ui_verification::{RgbaImage, decode_png, encode_png};
use serde_json::Value;

static UNIQUE: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let name = format!(
            "parchmint-ui-verification-{}-{}",
            std::process::id(),
            UNIQUE.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn image(red: u8) -> RgbaImage {
    RgbaImage::new(1, 1, vec![red, 2, 3, 255]).unwrap()
}

#[test]
fn cli_writes_diff_and_machine_readable_report_for_a_difference() {
    let directory = TestDirectory::new();
    let reference = directory.path().join("reference.png");
    let actual = directory.path().join("actual.png");
    let diff = directory.path().join("diff.png");
    let report = directory.path().join("report.json");
    encode_png(&reference, &image(1)).unwrap();
    encode_png(&actual, &image(9)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_parchmint-ui-verify"))
        .args([
            "compare",
            "--reference",
            reference.to_str().unwrap(),
            "--actual",
            actual.to_str().unwrap(),
            "--diff",
            diff.to_str().unwrap(),
            "--report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(diff.is_file());
    let report: Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(report["schema"], "parchmint.ui-verification/v1");
    assert_eq!(report["matches"], false);
    assert_eq!(report["differing_pixels"], 1);
    assert_eq!(report["max_channel_delta"], 8);
}

#[test]
fn cli_returns_success_and_zero_metrics_for_an_exact_match() {
    let directory = TestDirectory::new();
    let reference = directory.path().join("reference.png");
    let actual = directory.path().join("actual.png");
    let diff = directory.path().join("diff.png");
    let report = directory.path().join("report.json");
    encode_png(&reference, &image(4)).unwrap();
    encode_png(&actual, &image(4)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_parchmint-ui-verify"))
        .args([
            "compare",
            "--reference",
            reference.to_str().unwrap(),
            "--actual",
            actual.to_str().unwrap(),
            "--diff",
            diff.to_str().unwrap(),
            "--report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(diff.is_file());
    let report: Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(report["matches"], true);
    assert_eq!(report["differing_pixels"], 0);
    assert_eq!(report["max_channel_delta"], 0);
    assert_eq!(report["mean_absolute_channel_delta"], 0.0);
}

#[test]
fn cli_rejects_an_output_path_that_would_overwrite_the_reference() {
    let directory = TestDirectory::new();
    let reference = directory.path().join("reference.png");
    let actual = directory.path().join("actual.png");
    let report = directory.path().join("report.json");
    encode_png(&reference, &image(1)).unwrap();
    encode_png(&actual, &image(1)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_parchmint-ui-verify"))
        .args([
            "compare",
            "--reference",
            reference.to_str().unwrap(),
            "--actual",
            actual.to_str().unwrap(),
            "--diff",
            reference.to_str().unwrap(),
            "--report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to overwrite input image"));
}

#[test]
fn cli_fails_for_a_missing_input_without_creating_outputs() {
    let directory = TestDirectory::new();
    let actual = directory.path().join("actual.png");
    let diff = directory.path().join("diff.png");
    let report = directory.path().join("report.json");
    encode_png(&actual, &image(1)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_parchmint-ui-verify"))
        .args([
            "compare",
            "--reference",
            directory.path().join("missing.png").to_str().unwrap(),
            "--actual",
            actual.to_str().unwrap(),
            "--diff",
            diff.to_str().unwrap(),
            "--report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(!diff.exists());
    assert!(!report.exists());
}

#[test]
fn capture_writes_a_2x_launcher_png_and_prints_its_renderer_path() {
    let directory = TestDirectory::new();
    let output_stem = directory.path().join("launcher-light");
    let expected = directory.path().join("launcher-light-tiny-skia.png");

    let output = Command::new(env!("CARGO_BIN_EXE_parchmint-ui-verify"))
        .args([
            "capture",
            "--target",
            "launcher",
            "--appearance",
            "light",
            "--output-stem",
            output_stem.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        expected.display().to_string()
    );
    let captured = decode_png(&expected).unwrap();
    assert_eq!((captured.width(), captured.height()), (2880, 1800));
}

#[test]
fn capture_refuses_to_replace_an_existing_renderer_output() {
    let directory = TestDirectory::new();
    let output_stem = directory.path().join("launcher-dark");
    let arguments = [
        "capture",
        "--target",
        "launcher",
        "--appearance",
        "dark",
        "--output-stem",
        output_stem.to_str().unwrap(),
    ];

    let first = Command::new(env!("CARGO_BIN_EXE_parchmint-ui-verify"))
        .args(arguments)
        .output()
        .unwrap();
    assert!(first.status.success(), "{first:?}");

    let repeated = Command::new(env!("CARGO_BIN_EXE_parchmint-ui-verify"))
        .args(arguments)
        .output()
        .unwrap();
    assert_eq!(repeated.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&repeated.stderr).contains("capture output already exists"));
}
