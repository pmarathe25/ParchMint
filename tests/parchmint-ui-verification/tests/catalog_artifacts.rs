use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use parchmint_ui_iced::{VisualAppearance, VisualTarget};
use parchmint_ui_verification::{
    CATALOG_SCHEMA, RgbaImage, encode_png, write_catalog_case, write_catalog_index,
};
use serde_json::Value;

static UNIQUE: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "parchmint-ui-catalog-artifacts-{}-{}",
            std::process::id(),
            UNIQUE.fetch_add(1, Ordering::Relaxed)
        ));
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
fn full_catalog_mapping_writes_all_artifacts_and_an_aggregate_failure() {
    let directory = TestDirectory::new();
    let output = directory.path().join("output");
    let mut cases = Vec::new();
    for (number, (target, appearance)) in VisualTarget::ALL
        .into_iter()
        .flat_map(|target| {
            VisualAppearance::ALL
                .into_iter()
                .map(move |appearance| (target, appearance))
        })
        .enumerate()
    {
        let reference = directory.path().join(format!("reference-{number}.png"));
        encode_png(&reference, &image(1)).unwrap();
        let actual = image(if number == 7 { 240 } else { 1 });
        cases.push(
            write_catalog_case(
                &output,
                target.reference_id(appearance),
                appearance.name(),
                reference,
                &actual,
            )
            .unwrap(),
        );
    }

    let index = write_catalog_index(&output, &cases).unwrap();
    let aggregate: Value = serde_json::from_slice(&fs::read(&index).unwrap()).unwrap();
    assert_eq!(aggregate["schema"], CATALOG_SCHEMA);
    assert_eq!(aggregate["total_cases"], 20);
    assert_eq!(aggregate["accepted_cases"], 19);
    assert_eq!(aggregate["failed_cases"], 1);
    assert!(cases.iter().all(|case| {
        case.actual_path.is_file() && case.diff_path.is_file() && case.report_path.is_file()
    }));
}
