use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs,
    path::{Component, Path, PathBuf},
};

use parchmint_ui_verification::decode_png;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const EXPECTED_SCHEMA: &str = "parchmint.penpot-reference-set/v1";
const EXPECTED_FILE_ID: &str = "2be68822-842f-8175-8008-65eef13b0227";

#[derive(Debug, Deserialize)]
struct ReferenceSet {
    schema: String,
    penpot_file_id: String,
    penpot_file_name: String,
    exported_on: String,
    logical_width: u32,
    logical_height: u32,
    export_scale: u32,
    physical_width: u32,
    physical_height: u32,
    screens: Vec<ScreenReference>,
}

#[derive(Debug, Deserialize)]
struct ScreenReference {
    fixture_id: String,
    board_id: String,
    penpot_name: String,
    light: String,
    dark: String,
}

#[test]
fn checked_in_penpot_references_match_their_manifest_and_checksums() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("references/penpot");
    let reference_set: ReferenceSet = toml::from_str(
        &fs::read_to_string(root.join("reference-set.toml")).expect("read reference manifest"),
    )
    .expect("parse reference manifest");

    assert_eq!(reference_set.schema, EXPECTED_SCHEMA);
    assert_eq!(reference_set.penpot_file_id, EXPECTED_FILE_ID);
    assert_eq!(reference_set.penpot_file_name, "ParchMint");
    assert_eq!(reference_set.exported_on, "2026-08-10");
    assert_eq!(
        (reference_set.logical_width, reference_set.logical_height),
        (1440, 900)
    );
    assert_eq!(reference_set.export_scale, 2);
    assert_eq!(
        (reference_set.physical_width, reference_set.physical_height),
        (2880, 1800)
    );
    assert_eq!(reference_set.screens.len(), 10);

    let mut fixtures = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for screen in &reference_set.screens {
        assert!(fixtures.insert(screen.fixture_id.as_str()));
        assert!(!screen.board_id.is_empty());
        assert!(screen.penpot_name.starts_with("PM / Screen /"));
        for relative in [&screen.light, &screen.dark] {
            assert!(paths.insert(relative.as_str()));
            let path = safe_reference_path(&root, relative);
            let image = decode_png(&path)
                .unwrap_or_else(|error| panic!("decode reference {}: {error}", path.display()));
            assert_eq!(
                (image.width(), image.height()),
                (reference_set.physical_width, reference_set.physical_height),
                "unexpected dimensions for {}",
                path.display()
            );
        }
    }

    let checksum_entries =
        fs::read_to_string(root.join("SHA256SUMS")).expect("read reference checksums");
    let mut checksummed_paths = BTreeSet::new();
    for line in checksum_entries.lines() {
        let (expected, relative) = line.split_once("  ").expect("checksum line separator");
        let path = safe_reference_path(&root, relative);
        let digest = Sha256::digest(fs::read(&path).unwrap());
        let mut actual = String::with_capacity(digest.len() * 2);
        for byte in digest {
            write!(&mut actual, "{byte:02x}").unwrap();
        }
        assert_eq!(actual, expected, "checksum mismatch for {}", path.display());
        assert!(checksummed_paths.insert(relative));
    }
    assert_eq!(checksummed_paths, paths);
}

fn safe_reference_path(root: &Path, relative: &str) -> PathBuf {
    let relative = Path::new(relative);
    assert!(!relative.is_absolute());
    assert!(
        relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    );
    root.join(relative)
}
