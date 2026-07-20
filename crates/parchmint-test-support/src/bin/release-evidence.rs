//! Generates a deterministic dependency SBOM and notice index from Cargo metadata.

use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: BTreeSet<String>,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    source: Option<String>,
}

fn main() {
    let output = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("release-evidence"), PathBuf::from);
    fs::create_dir_all(&output).expect("create release evidence directory");
    let result = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .expect("run cargo metadata");
    assert!(result.status.success(), "cargo metadata failed");
    let mut metadata: Metadata = serde_json::from_slice(&result.stdout).expect("parse metadata");
    metadata
        .packages
        .sort_by(|left, right| (&left.name, &left.version).cmp(&(&right.name, &right.version)));

    let components = metadata
        .packages
        .iter()
        .map(|package| {
            let kind = if metadata.workspace_members.contains(&package.id) {
                "application"
            } else {
                "library"
            };
            json!({
                "type": kind,
                "bom-ref": package.id,
                "name": package.name,
                "version": package.version,
                "licenses": package.license.as_ref().map_or_else(Vec::<Value>::new, |license| vec![json!({"expression": license})]),
                "externalReferences": package.source.as_ref().map_or_else(Vec::<Value>::new, |source| vec![json!({"type": "distribution", "url": source})])
            })
        })
        .collect::<Vec<_>>();
    let sbom = json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": "urn:uuid:6b190eb0-83fd-4b82-96df-c2bd8d193709",
        "version": 1,
        "metadata": {"component": {"type": "application", "name": "ParchMint", "version": env!("CARGO_PKG_VERSION")}},
        "components": components
    });
    fs::write(
        output.join("parchmint.cdx.json"),
        serde_json::to_vec_pretty(&sbom).expect("serialize SBOM"),
    )
    .expect("write SBOM");

    let mut notices = String::from(
        "# Generated Cargo dependency notices\n\nGenerated from the locked release-candidate dependency graph. License texts distributed by Qt are collected separately by the Qt deployment step.\n\n| Package | Version | Declared license | Source |\n|---|---:|---|---|\n",
    );
    for package in metadata
        .packages
        .iter()
        .filter(|package| !metadata.workspace_members.contains(&package.id))
    {
        writeln!(
            notices,
            "| {} | {} | {} | {} |",
            package.name.replace('|', "\\|"),
            package.version,
            package
                .license
                .as_deref()
                .unwrap_or("UNKNOWN")
                .replace('|', "\\|"),
            package
                .source
                .as_deref()
                .unwrap_or("workspace")
                .replace('|', "\\|")
        )
        .expect("append notice row");
    }
    fs::write(output.join("THIRD_PARTY_NOTICES.generated.md"), notices).expect("write notices");
}
