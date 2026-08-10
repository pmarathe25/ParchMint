use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use toml::{Table, Value};

const EXCEPTIONS_PATH: &str = "supply-chain/exceptions.toml";
const ARTIFACTS_PATH: &str = "supply-chain/bundled-artifacts.toml";
const LOCKFILE_PATH: &str = "Cargo.lock";
const SBOM_BASELINE_PATH: &str = "supply-chain/sbom-baseline.toml";
const RELEASE_INPUTS_PATH: &str = "packaging/release-inputs.toml";
const RELEASE_MANIFEST_PATH: &str = "packaging/release-candidates.toml";

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ParchMint CI validation failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let command = args.next();
    let action = args.next();
    let extra = args.next();

    match (command.as_deref(), action.as_deref(), extra.as_deref()) {
        (Some("verify"), None, None) => {
            let today = utc_today()?;
            verify_exceptions(Path::new(EXCEPTIONS_PATH), today)?;
            verify_artifacts(Path::new(ARTIFACTS_PATH))?;
            println!("supply-chain exception and bundled-artifact policies are valid");
            Ok(())
        }
        (Some("sbom"), Some("verify"), None) => {
            verify_sbom(
                Path::new(LOCKFILE_PATH),
                Path::new(ARTIFACTS_PATH),
                Path::new(SBOM_BASELINE_PATH),
            )?;
            println!("SBOM baseline matches Cargo.lock and bundled-artifacts.toml");
            Ok(())
        }
        (Some("sbom"), Some("generate"), None) => {
            let sbom = read_current_sbom(Path::new(LOCKFILE_PATH), Path::new(ARTIFACTS_PATH))?;
            print!("{}", render_sbom(&sbom));
            Ok(())
        }
        (Some("release"), Some("verify"), None) => {
            verify_release(Path::new(RELEASE_MANIFEST_PATH))?;
            println!("release candidates satisfy packaging and release-evidence policy");
            Ok(())
        }
        (Some("release"), Some("inputs"), Some("verify")) => {
            verify_release_inputs(Path::new(RELEASE_INPUTS_PATH), false)?;
            println!("release inputs are valid; unresolved inputs remain explicit");
            Ok(())
        }
        _ => Err(
            "usage: cargo parchmint-ci verify\n       cargo parchmint-ci sbom <verify|generate>\n       cargo parchmint-ci release inputs verify\n       cargo parchmint-ci release verify".to_owned(),
        ),
    }
}

fn verify_release_inputs(path: &Path, require_ready: bool) -> Result<(), String> {
    let inputs = parse_release_inputs(&read_toml(path)?)?;
    let repository_root = path
        .parent()
        .and_then(Path::parent)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut missing = Vec::new();

    for artifact in &inputs.artifacts {
        if artifact.status == InputStatus::Missing {
            missing.push(format!(
                "{}: {}",
                artifact.name,
                artifact
                    .missing_reason
                    .as_deref()
                    .expect("validated missing reason")
            ));
        } else {
            verify_release_file_path(
                &repository_root.join(
                    artifact
                        .path
                        .as_deref()
                        .expect("validated release artifact path"),
                ),
                &artifact.name,
            )?;
        }
    }

    for platform in &inputs.platforms {
        verify_release_file_path(
            &repository_root.join(&platform.package_definition),
            "package definition",
        )?;
        if platform.package_assets.status == InputStatus::Missing {
            missing.push(format!(
                "{} package assets: {}",
                platform.name,
                platform
                    .package_assets
                    .missing_reason
                    .as_deref()
                    .expect("validated missing reason")
            ));
        } else {
            for asset in &platform.package_assets.paths {
                verify_release_file_path(&repository_root.join(asset), "package asset")?;
            }
        }
        if platform.minimum_version.status == InputStatus::Missing {
            missing.push(format!(
                "{} minimum version: {}",
                platform.name,
                platform
                    .minimum_version
                    .missing_reason
                    .as_deref()
                    .expect("validated missing reason")
            ));
        } else {
            let evidence_path = platform
                .minimum_version
                .evidence_path
                .as_deref()
                .expect("validated minimum-version evidence path");
            let evidence_path = repository_root.join(evidence_path);
            verify_release_file_path(&evidence_path, "minimum-version evidence")?;
            verify_minimum_version_evidence(
                &read_toml(&evidence_path)?,
                &platform.name,
                platform
                    .minimum_version
                    .value
                    .as_deref()
                    .expect("validated minimum version"),
            )?;
        }
        if platform.signing.status == PolicyStatus::Missing {
            missing.push(format!(
                "{} signing policy: {}",
                platform.name,
                platform
                    .signing
                    .missing_reason
                    .as_deref()
                    .expect("validated missing reason")
            ));
        } else if platform.signing.status == PolicyStatus::Required {
            let input = platform
                .signing
                .input_path
                .as_deref()
                .expect("validated signing input path");
            let input = repository_root.join(input);
            verify_release_file_path(&input, "signing input")?;
            verify_signing_input(
                &read_toml(&input)?,
                &platform.name,
                platform.notarization.status == PolicyStatus::Required,
            )?;
        }
        if platform.notarization.status == PolicyStatus::Missing {
            missing.push(format!(
                "{} notarization policy: {}",
                platform.name,
                platform
                    .notarization
                    .missing_reason
                    .as_deref()
                    .expect("validated missing reason")
            ));
        } else if platform.notarization.status == PolicyStatus::Required
            && platform.notarization.input_path != platform.signing.input_path
        {
            return Err(format!(
                "{} signing-input and notarization-input must reference the same validated input document",
                platform.name
            ));
        }
    }

    if require_ready && !missing.is_empty() {
        return Err(format!(
            "release inputs are incomplete:\n  - {}",
            missing.join("\n  - ")
        ));
    }
    Ok(())
}

fn verify_release(manifest_path: &Path) -> Result<(), String> {
    verify_release_inputs(Path::new(RELEASE_INPUTS_PATH), true)?;
    let today = utc_today()?;
    verify_exceptions(Path::new(EXCEPTIONS_PATH), today)?;
    verify_artifacts(Path::new(ARTIFACTS_PATH))?;
    verify_sbom(
        Path::new(LOCKFILE_PATH),
        Path::new(ARTIFACTS_PATH),
        Path::new(SBOM_BASELINE_PATH),
    )?;

    let manifest = parse_release_manifest(&read_toml(manifest_path)?)?;
    verify_release_tag(&manifest.release_version)?;
    verify_release_manifest_readiness(&manifest)?;
    verify_manifest_against_release_inputs(&manifest)?;
    let checkout_revision = current_source_revision()?;
    if manifest.source_revision != checkout_revision {
        return Err(format!(
            "release source revision {} does not match checked-out revision {checkout_revision}",
            manifest.source_revision
        ));
    }
    verify_release_file_hash(
        &manifest.dependency_notices,
        &manifest.dependency_notices_sha256,
        "dependency notices",
    )?;
    verify_dependency_notices(Path::new(&manifest.dependency_notices))?;
    verify_release_file_hash(&manifest.sbom, &manifest.sbom_sha256, "release SBOM")?;
    verify_release_sbom(Path::new(&manifest.sbom))?;
    verify_release_file_hash(
        &manifest.provenance,
        &manifest.provenance_sha256,
        "release provenance",
    )?;
    verify_release_provenance(Path::new(&manifest.provenance), &manifest)?;
    verify_release_file_hash(
        &manifest.release_gate_evidence,
        &manifest.release_gate_evidence_sha256,
        "release-gate evidence",
    )?;
    verify_release_gate_evidence(Path::new(&manifest.release_gate_evidence), &manifest)?;

    for candidate in &manifest.candidates {
        if candidate.architecture.trim().is_empty() || candidate.minimum_version.trim().is_empty() {
            return Err(format!(
                "{} candidate must identify an architecture and frozen minimum platform version",
                candidate.platform
            ));
        }
        verify_release_file_hash(&candidate.package, &candidate.package_sha256, "package")?;

        if candidate.signature == SignatureRequirement::Required {
            verify_release_evidence(
                candidate
                    .signature_evidence
                    .as_deref()
                    .expect("validated required signature evidence"),
                candidate
                    .signature_evidence_sha256
                    .as_deref()
                    .expect("validated required signature evidence hash"),
                "signature",
                &manifest,
                candidate,
            )?;
        }
        if candidate.notarization == SignatureRequirement::Required {
            verify_release_evidence(
                candidate
                    .notarization_evidence
                    .as_deref()
                    .expect("validated required notarization evidence"),
                candidate
                    .notarization_evidence_sha256
                    .as_deref()
                    .expect("validated required notarization evidence hash"),
                "notarization",
                &manifest,
                candidate,
            )?;
        }
        verify_release_evidence(
            &candidate.install_evidence,
            &candidate.install_evidence_sha256,
            "install",
            &manifest,
            candidate,
        )?;
        verify_release_evidence(
            &candidate.launch_evidence,
            &candidate.launch_evidence_sha256,
            "launch",
            &manifest,
            candidate,
        )?;
        verify_release_evidence(
            &candidate.upgrade_evidence,
            &candidate.upgrade_evidence_sha256,
            "upgrade",
            &manifest,
            candidate,
        )?;
        verify_release_evidence(
            &candidate.uninstall_evidence,
            &candidate.uninstall_evidence_sha256,
            "uninstall",
            &manifest,
            candidate,
        )?;

        match candidate.native_ui_validation {
            NativeUiValidation::Passed => verify_release_evidence(
                candidate
                    .native_ui_evidence
                    .as_deref()
                    .expect("validated native UI evidence"),
                candidate
                    .native_ui_evidence_sha256
                    .as_deref()
                    .expect("validated native UI evidence hash"),
                "native-ui",
                &manifest,
                candidate,
            )?,
            NativeUiValidation::Deferred => {
                return Err(format!(
                    "{} native menus/dialogs/clipboard validation is deferred: {}",
                    candidate.platform,
                    candidate
                        .native_ui_deferred_reason
                        .as_deref()
                        .expect("validated deferred reason")
                ));
            }
        }
    }

    Ok(())
}

fn verify_manifest_against_release_inputs(manifest: &ReleaseManifest) -> Result<(), String> {
    let inputs = parse_release_inputs(&read_toml(Path::new(RELEASE_INPUTS_PATH))?)?;
    for (name, manifest_path) in [
        ("dependency-notices", manifest.dependency_notices.as_str()),
        ("release-sbom", manifest.sbom.as_str()),
        ("provenance", manifest.provenance.as_str()),
        (
            "release-gate-evidence",
            manifest.release_gate_evidence.as_str(),
        ),
    ] {
        let input = inputs
            .artifacts
            .iter()
            .find(|input| input.name == name)
            .expect("release input parser requires all release artifacts");
        if input.status != InputStatus::Available || input.path.as_deref() != Some(manifest_path) {
            return Err(format!(
                "{name} does not match the available path in release-inputs.toml"
            ));
        }
    }

    for candidate in &manifest.candidates {
        let input = inputs
            .platforms
            .iter()
            .find(|input| input.name == candidate.platform)
            .expect("release input parser requires all platforms");
        if input.architecture != candidate.architecture
            || input.minimum_version.value.as_deref() != Some(candidate.minimum_version.as_str())
        {
            return Err(format!(
                "{} candidate architecture or minimum version does not match its frozen release input",
                candidate.platform
            ));
        }
        let signature = match input.signing.status {
            PolicyStatus::Required => SignatureRequirement::Required,
            PolicyStatus::NotApplicable => SignatureRequirement::NotApplicable,
            PolicyStatus::Missing => {
                return Err(format!("{} signing policy is missing", candidate.platform));
            }
        };
        let notarization = match input.notarization.status {
            PolicyStatus::Required => SignatureRequirement::Required,
            PolicyStatus::NotApplicable => SignatureRequirement::NotApplicable,
            PolicyStatus::Missing => {
                return Err(format!(
                    "{} notarization policy is missing",
                    candidate.platform
                ));
            }
        };
        if candidate.signature != signature || candidate.notarization != notarization {
            return Err(format!(
                "{} candidate signing or notarization policy does not match release-inputs.toml",
                candidate.platform
            ));
        }
        if signature == SignatureRequirement::Required {
            let signing_input = input
                .signing
                .input_path
                .as_deref()
                .expect("required signing input was validated");
            let signing_input = read_toml(Path::new(signing_input))?;
            if required_digest(&signing_input, "package-sha256", "signing input")?
                != candidate.package_sha256
            {
                return Err(format!(
                    "{} signing input package-sha256 does not match the candidate package",
                    candidate.platform
                ));
            }
        }
    }
    Ok(())
}

fn verify_release_tag(release_version: &str) -> Result<(), String> {
    let Ok(tag) = env::var("GITHUB_REF_NAME") else {
        return Ok(());
    };
    verify_release_tag_name(release_version, &tag)
}

fn verify_release_tag_name(release_version: &str, tag: &str) -> Result<(), String> {
    let expected = format!("v{release_version}");
    if tag == expected {
        Ok(())
    } else {
        Err(format!(
            "release tag {tag:?} does not match manifest release version {expected:?}"
        ))
    }
}

fn verify_release_manifest_readiness(manifest: &ReleaseManifest) -> Result<(), String> {
    let mut paths = BTreeSet::new();
    for (path, label) in [
        (&manifest.dependency_notices, "dependency notices"),
        (&manifest.sbom, "release SBOM"),
        (&manifest.provenance, "provenance"),
        (&manifest.release_gate_evidence, "release-gate evidence"),
    ] {
        if !paths.insert(path.as_str()) {
            return Err(format!("release manifest reuses {path:?} for {label}"));
        }
    }

    for candidate in &manifest.candidates {
        if candidate.architecture.contains('@')
            || candidate.minimum_version.contains('@')
            || ["missing", "unknown", "unfrozen"].iter().any(|marker| {
                candidate.minimum_version.eq_ignore_ascii_case(marker)
                    || candidate.architecture.eq_ignore_ascii_case(marker)
            })
        {
            return Err(format!(
                "{} candidate must identify a real architecture and frozen minimum platform version",
                candidate.platform
            ));
        }
        let expected_extension = match candidate.platform.as_str() {
            "windows" => "msix",
            "macos" => "dmg",
            "linux" => "deb",
            _ => unreachable!("platform parser rejects unknown values"),
        };
        if Path::new(&candidate.package)
            .extension()
            .and_then(|value| value.to_str())
            != Some(expected_extension)
        {
            return Err(format!(
                "{} package must use the .{expected_extension} format",
                candidate.platform
            ));
        }
        if candidate.native_ui_validation == NativeUiValidation::Deferred {
            return Err(format!(
                "{} native menus/dialogs/clipboard validation is deferred: {}",
                candidate.platform,
                candidate
                    .native_ui_deferred_reason
                    .as_deref()
                    .expect("validated deferred reason")
            ));
        }
        for (path, label) in candidate.evidence_paths() {
            if !paths.insert(path) {
                return Err(format!(
                    "release manifest reuses {path:?} for {} {label}",
                    candidate.platform
                ));
            }
        }
    }
    Ok(())
}

fn current_source_revision() -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("cannot resolve checked-out Git revision: {error}"))?;
    if !output.status.success() {
        return Err("cannot resolve checked-out Git revision with `git rev-parse HEAD`".to_owned());
    }
    let revision = String::from_utf8(output.stdout)
        .map_err(|error| format!("Git revision is not UTF-8: {error}"))?;
    let revision = revision.trim().to_owned();
    if !is_git_revision(&revision) {
        return Err(format!(
            "Git returned an invalid source revision {revision:?}"
        ));
    }
    Ok(revision)
}

fn verify_release_sbom(path: &Path) -> Result<(), String> {
    let release = parse_sbom_baseline(&read_toml(path)?)
        .map_err(|error| format!("invalid release SBOM {path:?}: {error}"))?;
    let current = read_current_sbom(Path::new(LOCKFILE_PATH), Path::new(ARTIFACTS_PATH))?;
    if release != current {
        return Err(format!(
            "release SBOM {path:?} does not match Cargo.lock and bundled-artifacts.toml"
        ));
    }
    Ok(())
}

fn verify_dependency_notices(path: &Path) -> Result<(), String> {
    let document = read_toml(path)?;
    require_format_version(&document)?;
    reject_unknown_keys(
        &document,
        &["format-version", "generated-by", "package"],
        "dependency notices",
    )?;
    if required_text(&document, "generated-by", "dependency notices")?
        != "cargo parchmint-ci release notices"
    {
        return Err(
            "dependency notices.generated-by must identify `cargo parchmint-ci release notices`"
                .to_owned(),
        );
    }
    let mut notice_ids = Vec::new();
    for (index, value) in required_array(&document, "package")?.iter().enumerate() {
        let context = format!("dependency notices.package[{index}]");
        let table = value
            .as_table()
            .ok_or_else(|| format!("{context} must be a table"))?;
        reject_unknown_keys(
            table,
            &["name", "version", "source", "license", "notice"],
            &context,
        )?;
        let name = required_text(table, "name", &context)?;
        let version = required_text(table, "version", &context)?;
        let source = optional_text(table, "source", &context)?;
        required_text(table, "license", &context)?;
        required_text(table, "notice", &context)?;
        notice_ids.push((name, version, source));
    }
    let mut sorted_ids = notice_ids.clone();
    sorted_ids.sort();
    if notice_ids != sorted_ids {
        return Err(
            "dependency notice packages must be sorted by name, version, and source".to_owned(),
        );
    }
    if notice_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("dependency notices contain a duplicate package".to_owned());
    }

    let current = read_current_sbom(Path::new(LOCKFILE_PATH), Path::new(ARTIFACTS_PATH))?;
    let expected = current
        .packages
        .into_iter()
        .map(|package| (package.name, package.version, package.source))
        .collect::<Vec<_>>();
    if notice_ids != expected {
        return Err(
            "dependency notices do not cover exactly the packages in Cargo.lock".to_owned(),
        );
    }
    Ok(())
}

fn verify_release_provenance(path: &Path, manifest: &ReleaseManifest) -> Result<(), String> {
    let document = read_toml(path)?;
    require_format_version(&document)?;
    reject_unknown_keys(
        &document,
        &[
            "format-version",
            "release-version",
            "source-revision",
            "builder",
            "build-run",
            "cargo-lock-sha256",
            "dependency-notices-sha256",
            "sbom-sha256",
            "subject",
        ],
        "release provenance",
    )?;
    if required_text(&document, "release-version", "release provenance")?
        != manifest.release_version
        || required_text(&document, "source-revision", "release provenance")?
            != manifest.source_revision
    {
        return Err("release provenance identity does not match the release manifest".to_owned());
    }
    required_text(&document, "builder", "release provenance")?;
    required_text(&document, "build-run", "release provenance")?;
    let lock_digest = required_digest(&document, "cargo-lock-sha256", "release provenance")?;
    let actual_lock_digest = sha256_file(Path::new(LOCKFILE_PATH))
        .map_err(|error| format!("cannot hash {LOCKFILE_PATH}: {error}"))?;
    if lock_digest != actual_lock_digest
        || required_digest(&document, "dependency-notices-sha256", "release provenance")?
            != manifest.dependency_notices_sha256
        || required_digest(&document, "sbom-sha256", "release provenance")? != manifest.sbom_sha256
    {
        return Err(
            "release provenance material digests do not match the release inputs".to_owned(),
        );
    }

    let subjects = required_array(&document, "subject")?
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let context = format!("release provenance.subject[{index}]");
            let table = value
                .as_table()
                .ok_or_else(|| format!("{context} must be a table"))?;
            reject_unknown_keys(table, &["platform", "path", "sha256"], &context)?;
            Ok((
                required_text(table, "platform", &context)?,
                required_release_path(table, "path", &context)?,
                required_digest(table, "sha256", &context)?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut sorted_subjects = subjects.clone();
    sorted_subjects.sort();
    if subjects != sorted_subjects {
        return Err("release provenance subjects must be sorted by platform and path".to_owned());
    }
    let expected = manifest
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.platform.clone(),
                candidate.package.clone(),
                candidate.package_sha256.clone(),
            )
        })
        .collect::<Vec<_>>();
    if sorted_subjects != expected {
        return Err("release provenance subjects do not match all candidate packages".to_owned());
    }
    Ok(())
}

fn verify_release_gate_evidence(path: &Path, manifest: &ReleaseManifest) -> Result<(), String> {
    let document = read_toml(path)?;
    require_format_version(&document)?;
    reject_unknown_keys(
        &document,
        &[
            "format-version",
            "release-version",
            "source-revision",
            "result",
            "gate",
        ],
        "release-gate evidence",
    )?;
    if required_text(&document, "release-version", "release-gate evidence")?
        != manifest.release_version
        || required_text(&document, "source-revision", "release-gate evidence")?
            != manifest.source_revision
        || required_text(&document, "result", "release-gate evidence")? != "passed"
    {
        return Err("release-gate evidence must pass for the manifest identity".to_owned());
    }
    let gates = required_array(&document, "gate")?;
    if gates.len() != 11 {
        return Err("release-gate evidence must contain exactly gates 1 through 11".to_owned());
    }
    for (index, value) in gates.iter().enumerate() {
        let context = format!("release-gate evidence.gate[{index}]");
        let table = value
            .as_table()
            .ok_or_else(|| format!("{context} must be a table"))?;
        reject_unknown_keys(table, &["id", "result", "evidence"], &context)?;
        let id = table
            .get("id")
            .and_then(Value::as_integer)
            .ok_or_else(|| format!("{context}.id must be an integer"))?;
        if id != i64::try_from(index + 1).expect("release gate count fits i64")
            || required_text(table, "result", &context)? != "passed"
        {
            return Err(
                "release-gate evidence must contain passed gates 1 through 11 in order".to_owned(),
            );
        }
        required_text(table, "evidence", &context)?;
    }
    Ok(())
}

fn verify_release_file_hash(path: &str, expected: &str, label: &str) -> Result<(), String> {
    verify_release_file(path, label)?;
    let actual = sha256_file(Path::new(path))
        .map_err(|error| format!("cannot hash {label} {path:?}: {error}"))?;
    if actual != expected {
        return Err(format!(
            "SHA-256 mismatch for {label} {path:?}: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

fn verify_release_evidence(
    path: &str,
    expected_sha256: &str,
    expected_kind: &str,
    manifest: &ReleaseManifest,
    candidate: &ReleaseCandidate,
) -> Result<(), String> {
    verify_release_file_hash(path, expected_sha256, &format!("{expected_kind} evidence"))?;
    let evidence = parse_release_evidence(&read_toml(Path::new(path))?)?;
    if evidence.kind != expected_kind
        || evidence.platform != candidate.platform
        || evidence.release_version != manifest.release_version
        || evidence.source_revision != manifest.source_revision
        || evidence.package_sha256 != candidate.package_sha256
    {
        return Err(format!(
            "{expected_kind} evidence {path:?} does not match its release candidate"
        ));
    }
    Ok(())
}

fn verify_release_file(path: &str, label: &str) -> Result<(), String> {
    if !is_safe_repository_path(Path::new(path)) {
        return Err(format!(
            "{label} {path:?} must be a portable repository-relative path without '.' or '..'"
        ));
    }
    verify_release_file_path(Path::new(path), label)
}

fn verify_release_file_path(path: &Path, label: &str) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("cannot read {label} {path:?}: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{label} {path:?} must not be a symbolic link"));
    }
    if !metadata.is_file() {
        return Err(format!("{label} {path:?} must be a file"));
    }
    if metadata.len() == 0 {
        return Err(format!("{label} {path:?} must not be empty"));
    }
    Ok(())
}

fn verify_exceptions(path: &Path, today: Date) -> Result<(), String> {
    let document = read_toml(path)?;
    let exceptions = parse_exceptions(&document)?;
    let mut ids = HashSet::new();

    for exception in exceptions {
        if !ids.insert(exception.id.clone()) {
            return Err(format!("duplicate exception id {:?}", exception.id));
        }
        if exception.expires < today {
            return Err(format!(
                "exception {:?} owned by {:?} expired on {}",
                exception.id, exception.owner, exception.expires
            ));
        }
    }

    Ok(())
}

fn parse_exceptions(document: &Table) -> Result<Vec<Exception>, String> {
    require_format_version(document)?;
    reject_unknown_keys(
        document,
        &["format-version", "exception"],
        "exception policy",
    )?;
    let entries = required_array(document, "exception")?;

    entries
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let context = format!("exception[{index}]");
            let table = value
                .as_table()
                .ok_or_else(|| format!("{context} must be a table"))?;
            reject_unknown_keys(
                table,
                &["id", "check", "package", "owner", "reason", "expires"],
                &context,
            )?;

            let check = required_text(table, "check", &context)?;
            if ![
                "advisory",
                "license",
                "provenance",
                "source",
                "bundled-artifact",
                "sbom",
            ]
            .contains(&check.as_str())
            {
                return Err(format!("{context}.check has unsupported value {check:?}"));
            }

            required_text(table, "package", &context)?;
            required_text(table, "reason", &context)?;
            let expires_text = required_text(table, "expires", &context)?;
            Ok(Exception {
                id: required_text(table, "id", &context)?,
                owner: required_text(table, "owner", &context)?,
                expires: Date::parse(&expires_text)
                    .map_err(|error| format!("{context}.expires {error}"))?,
            })
        })
        .collect()
}

fn parse_release_inputs(document: &Table) -> Result<ReleaseInputs, String> {
    require_format_version(document)?;
    reject_unknown_keys(
        document,
        &["format-version", "artifact", "platform"],
        "release inputs",
    )?;

    let artifacts = required_array(document, "artifact")?
        .iter()
        .enumerate()
        .map(|(index, value)| parse_release_input_artifact(value, index))
        .collect::<Result<Vec<_>, _>>()?;
    let expected_artifacts = [
        "dependency-notices",
        "provenance",
        "release-gate-evidence",
        "release-sbom",
    ];
    let actual_artifacts = artifacts
        .iter()
        .map(|artifact| artifact.name.as_str())
        .collect::<BTreeSet<_>>();
    if artifacts.len() != expected_artifacts.len()
        || actual_artifacts != expected_artifacts.into_iter().collect()
    {
        return Err("release inputs must contain exactly dependency-notices, provenance, release-gate-evidence, and release-sbom artifacts".to_owned());
    }

    let mut platforms = required_array(document, "platform")?
        .iter()
        .enumerate()
        .map(|(index, value)| parse_platform_input(value, index))
        .collect::<Result<Vec<_>, _>>()?;
    platforms.sort_by(|left, right| left.name.cmp(&right.name));
    let actual_platforms = platforms
        .iter()
        .map(|platform| platform.name.as_str())
        .collect::<Vec<_>>();
    if actual_platforms != ["linux", "macos", "windows"] {
        return Err(
            "release inputs must contain exactly one platform entry for linux, macos, and windows"
                .to_owned(),
        );
    }

    Ok(ReleaseInputs {
        artifacts,
        platforms,
    })
}

fn parse_release_input_artifact(
    value: &Value,
    index: usize,
) -> Result<ReleaseInputArtifact, String> {
    let context = format!("artifact[{index}]");
    let table = value
        .as_table()
        .ok_or_else(|| format!("{context} must be a table"))?;
    reject_unknown_keys(
        table,
        &["name", "status", "path", "missing-reason"],
        &context,
    )?;
    let name = required_text(table, "name", &context)?;
    let status = parse_input_status(table, &context)?;
    let path = optional_release_path(table, "path", &context)?;
    let missing_reason = optional_text(table, "missing-reason", &context)?;
    validate_input_state(status, path.as_deref(), missing_reason.as_deref(), &context)?;
    Ok(ReleaseInputArtifact {
        name,
        status,
        path,
        missing_reason,
    })
}

fn parse_platform_input(value: &Value, index: usize) -> Result<PlatformReleaseInput, String> {
    let context = format!("platform[{index}]");
    let table = value
        .as_table()
        .ok_or_else(|| format!("{context} must be a table"))?;
    reject_unknown_keys(
        table,
        &[
            "name",
            "architecture",
            "package-format",
            "package-definition",
            "package-assets-status",
            "package-assets",
            "package-assets-missing-reason",
            "minimum-version-status",
            "minimum-version",
            "minimum-version-evidence",
            "minimum-version-missing-reason",
            "signing-status",
            "signing-input",
            "signing-missing-reason",
            "notarization-status",
            "notarization-input",
            "notarization-missing-reason",
        ],
        &context,
    )?;

    let name = required_text(table, "name", &context)?;
    if !["windows", "macos", "linux"].contains(&name.as_str()) {
        return Err(format!("{context}.name must be windows, macos, or linux"));
    }
    let architecture = required_text(table, "architecture", &context)?;
    let package_format = required_text(table, "package-format", &context)?;
    let expected_format = match name.as_str() {
        "windows" => "msix",
        "macos" => "dmg",
        "linux" => "deb",
        _ => unreachable!("platform value was validated"),
    };
    if package_format != expected_format {
        return Err(format!(
            "{context}.package-format must be {expected_format} for {name}"
        ));
    }
    let package_definition = required_release_path(table, "package-definition", &context)?;
    let package_assets_status = parse_available_status(table, "package-assets-status", &context)?;
    let package_assets = optional_release_paths(table, "package-assets", &context)?;
    let package_assets_missing_reason =
        optional_text(table, "package-assets-missing-reason", &context)?;
    match package_assets_status {
        InputStatus::Available
            if package_assets.is_empty() || package_assets_missing_reason.is_some() =>
        {
            return Err(format!(
                "{context} requires package-assets and no missing reason when package assets are available"
            ));
        }
        InputStatus::Missing
            if !package_assets.is_empty() || package_assets_missing_reason.is_none() =>
        {
            return Err(format!(
                "{context} requires only package-assets-missing-reason when package assets are missing"
            ));
        }
        _ => {}
    }

    let minimum_status = parse_named_input_status(table, "minimum-version-status", &context)?;
    let minimum_version = optional_text(table, "minimum-version", &context)?;
    let minimum_evidence = optional_release_path(table, "minimum-version-evidence", &context)?;
    let minimum_missing_reason = optional_text(table, "minimum-version-missing-reason", &context)?;
    match minimum_status {
        InputStatus::Available
            if minimum_version.is_none()
                || minimum_evidence.is_none()
                || minimum_missing_reason.is_some() =>
        {
            return Err(format!(
                "{context} requires minimum-version and minimum-version-evidence, with no missing reason, when the version is frozen"
            ));
        }
        InputStatus::Missing
            if minimum_version.is_some()
                || minimum_evidence.is_some()
                || minimum_missing_reason.is_none() =>
        {
            return Err(format!(
                "{context} requires only minimum-version-missing-reason when the minimum version is missing"
            ));
        }
        _ => {}
    }

    let signing = parse_policy_input(table, "signing", &context)?;
    let notarization = parse_policy_input(table, "notarization", &context)?;
    if notarization.status == PolicyStatus::Required && signing.status != PolicyStatus::Required {
        return Err(format!(
            "{context} cannot require notarization without required signing inputs"
        ));
    }

    Ok(PlatformReleaseInput {
        name,
        architecture,
        package_definition,
        package_assets: PackageAssetsInput {
            status: package_assets_status,
            paths: package_assets,
            missing_reason: package_assets_missing_reason,
        },
        minimum_version: InputState {
            status: minimum_status,
            value: minimum_version,
            evidence_path: minimum_evidence,
            missing_reason: minimum_missing_reason,
        },
        signing,
        notarization,
    })
}

fn parse_input_status(table: &Table, context: &str) -> Result<InputStatus, String> {
    parse_available_status(table, "status", context)
}

fn parse_available_status(table: &Table, key: &str, context: &str) -> Result<InputStatus, String> {
    match required_text(table, key, context)?.as_str() {
        "available" => Ok(InputStatus::Available),
        "missing" => Ok(InputStatus::Missing),
        _ => Err(format!("{context}.{key} must be available or missing")),
    }
}

fn parse_named_input_status(
    table: &Table,
    key: &str,
    context: &str,
) -> Result<InputStatus, String> {
    match required_text(table, key, context)?.as_str() {
        "frozen" => Ok(InputStatus::Available),
        "missing" => Ok(InputStatus::Missing),
        _ => Err(format!("{context}.{key} must be frozen or missing")),
    }
}

fn validate_input_state(
    status: InputStatus,
    available_path: Option<&str>,
    missing_reason: Option<&str>,
    context: &str,
) -> Result<(), String> {
    match status {
        InputStatus::Available if available_path.is_none() || missing_reason.is_some() => Err(
            format!("{context} requires path and no missing-reason when available"),
        ),
        InputStatus::Missing if available_path.is_some() || missing_reason.is_none() => Err(
            format!("{context} requires missing-reason and no path when missing"),
        ),
        _ => Ok(()),
    }
}

fn parse_policy_input(table: &Table, prefix: &str, context: &str) -> Result<PolicyInput, String> {
    let status_key = format!("{prefix}-status");
    let input_key = format!("{prefix}-input");
    let reason_key = format!("{prefix}-missing-reason");
    let status = match required_text(table, &status_key, context)?.as_str() {
        "required" => PolicyStatus::Required,
        "not-applicable" => PolicyStatus::NotApplicable,
        "missing" => PolicyStatus::Missing,
        _ => {
            return Err(format!(
                "{context}.{status_key} must be required, not-applicable, or missing"
            ));
        }
    };
    let input = optional_release_path(table, &input_key, context)?;
    let missing_reason = optional_text(table, &reason_key, context)?;
    match status {
        PolicyStatus::Required if input.is_none() || missing_reason.is_some() => {
            return Err(format!(
                "{context} requires {input_key} and no {reason_key} when {prefix} is required"
            ));
        }
        PolicyStatus::NotApplicable if input.is_some() || missing_reason.is_some() => {
            return Err(format!(
                "{context} must not include {prefix} input or reason when it is not-applicable"
            ));
        }
        PolicyStatus::Missing if input.is_some() || missing_reason.is_none() => {
            return Err(format!(
                "{context} requires {reason_key} and no {input_key} when {prefix} policy is missing"
            ));
        }
        _ => {}
    }
    Ok(PolicyInput {
        status,
        input_path: input,
        missing_reason,
    })
}

fn verify_signing_input(
    document: &Table,
    expected_platform: &str,
    notarization_required: bool,
) -> Result<(), String> {
    require_format_version(document)?;
    reject_unknown_keys(
        document,
        &[
            "format-version",
            "platform",
            "package-sha256",
            "signing",
            "notarization",
        ],
        "signing input",
    )?;
    let platform = required_text(document, "platform", "signing input")?;
    if platform != expected_platform {
        return Err(format!(
            "signing input platform {platform:?} does not match {expected_platform:?}"
        ));
    }
    required_digest(document, "package-sha256", "signing input")?;

    let signing = document
        .get("signing")
        .and_then(Value::as_table)
        .ok_or_else(|| "signing input.signing must be a table".to_owned())?;
    reject_unknown_keys(
        signing,
        &[
            "tool",
            "identity",
            "credential-environment",
            "timestamp-url",
        ],
        "signing input.signing",
    )?;
    required_text(signing, "tool", "signing input.signing")?;
    required_text(signing, "identity", "signing input.signing")?;
    let credentials =
        required_text_array(signing, "credential-environment", "signing input.signing")?;
    if credentials.iter().any(|name| !is_environment_name(name)) {
        return Err("signing input credential-environment values must be uppercase environment-variable names".to_owned());
    }
    if let Some(url) = optional_text(signing, "timestamp-url", "signing input.signing")?
        && !(url.starts_with("https://") || url.starts_with("http://"))
    {
        return Err("signing input.signing.timestamp-url must be an HTTP(S) URL".to_owned());
    }

    let notarization = document.get("notarization").and_then(Value::as_table);
    match (notarization_required, notarization) {
        (true, Some(notarization)) => {
            reject_unknown_keys(
                notarization,
                &["tool", "profile-environment", "staple"],
                "signing input.notarization",
            )?;
            required_text(notarization, "tool", "signing input.notarization")?;
            let profile = required_text(
                notarization,
                "profile-environment",
                "signing input.notarization",
            )?;
            if !is_environment_name(&profile) {
                return Err("signing input.notarization.profile-environment must be an uppercase environment-variable name".to_owned());
            }
            if notarization.get("staple").and_then(Value::as_bool) != Some(true) {
                return Err("signing input.notarization.staple must be true".to_owned());
            }
        }
        (true, None) => return Err("macOS signing input requires notarization inputs".to_owned()),
        (false, Some(_)) => {
            return Err(
                "signing input must not include notarization when it is not required".to_owned(),
            );
        }
        (false, None) => {}
    }
    Ok(())
}

fn verify_minimum_version_evidence(
    document: &Table,
    expected_platform: &str,
    expected_version: &str,
) -> Result<(), String> {
    require_format_version(document)?;
    reject_unknown_keys(
        document,
        &[
            "format-version",
            "platform",
            "minimum-version",
            "result",
            "runner",
            "observed-at",
            "checks",
            "details",
        ],
        "minimum-version evidence",
    )?;
    if required_text(document, "platform", "minimum-version evidence")? != expected_platform {
        return Err(
            "minimum-version evidence platform does not match its release input".to_owned(),
        );
    }
    if required_text(document, "minimum-version", "minimum-version evidence")? != expected_version {
        return Err("minimum-version evidence version does not match its release input".to_owned());
    }
    if required_text(document, "result", "minimum-version evidence")? != "passed" {
        return Err("minimum-version evidence must record result = \"passed\"".to_owned());
    }
    required_text(document, "runner", "minimum-version evidence")?;
    required_text(document, "observed-at", "minimum-version evidence")?;
    required_text(document, "details", "minimum-version evidence")?;
    let checks = required_text_array(document, "checks", "minimum-version evidence")?;
    if checks != ["install", "launch", "native-ui"] {
        return Err(
            "minimum-version evidence must contain exactly install, launch, and native-ui checks"
                .to_owned(),
        );
    }
    Ok(())
}

fn required_text_array(table: &Table, key: &str, context: &str) -> Result<Vec<String>, String> {
    let values = table
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{context}.{key} must be an array"))?;
    if values.is_empty() {
        return Err(format!("{context}.{key} must not be empty"));
    }
    let mut parsed = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("{context}.{key}[{index}] must be a non-empty string"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    parsed.sort();
    if parsed.windows(2).any(|values| values[0] == values[1]) {
        return Err(format!("{context}.{key} contains a duplicate"));
    }
    Ok(parsed)
}

fn is_environment_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_uppercase())
        && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn parse_release_evidence(document: &Table) -> Result<ReleaseEvidence, String> {
    require_format_version(document)?;
    reject_unknown_keys(
        document,
        &[
            "format-version",
            "kind",
            "platform",
            "release-version",
            "source-revision",
            "package-sha256",
            "result",
            "runner",
            "observed-at",
            "details",
            "checks",
            "tool",
            "identity",
            "ticket-id",
        ],
        "release evidence",
    )?;
    let kind = required_text(document, "kind", "release evidence")?;
    if ![
        "signature",
        "notarization",
        "install",
        "launch",
        "upgrade",
        "uninstall",
        "native-ui",
    ]
    .contains(&kind.as_str())
    {
        return Err(format!(
            "release evidence.kind has unsupported value {kind:?}"
        ));
    }
    let result = required_text(document, "result", "release evidence")?;
    if result != "passed" {
        return Err(format!(
            "release evidence must record result = \"passed\"; got {result:?}"
        ));
    }
    let platform = required_text(document, "platform", "release evidence")?;
    if !["windows", "macos", "linux"].contains(&platform.as_str()) {
        return Err("release evidence.platform must be windows, macos, or linux".to_owned());
    }
    let release_version = required_text(document, "release-version", "release evidence")?;
    if !is_release_candidate_version(&release_version) {
        return Err("release evidence.release-version must use N.N.N-rc.N".to_owned());
    }
    let source_revision = required_text(document, "source-revision", "release evidence")?;
    if !is_git_revision(&source_revision) {
        return Err("release evidence.source-revision must be a lowercase 40-character hexadecimal Git revision".to_owned());
    }
    let package_sha256 = required_digest(document, "package-sha256", "release evidence")?;
    required_text(document, "runner", "release evidence")?;
    let observed_at = required_text(document, "observed-at", "release evidence")?;
    if !is_rfc3339_timestamp(&observed_at) {
        return Err("release evidence.observed-at must use an RFC 3339 timestamp".to_owned());
    }
    required_text(document, "details", "release evidence")?;

    let checks = document
        .get("checks")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| "release evidence.checks must be an array".to_owned())?
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    value
                        .as_str()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            format!("release evidence.checks[{index}] must be a non-empty string")
                        })
                })
                .collect::<Result<BTreeSet<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    if kind == "native-ui"
        && checks
            != BTreeSet::from([
                "clipboard".to_owned(),
                "dialogs".to_owned(),
                "menus".to_owned(),
            ])
    {
        return Err(
            "native-ui evidence must contain exactly menus, dialogs, and clipboard checks"
                .to_owned(),
        );
    }
    if kind == "signature" {
        required_text(document, "tool", "release evidence")?;
        required_text(document, "identity", "release evidence")?;
    }
    if kind == "notarization" {
        required_text(document, "tool", "release evidence")?;
        required_text(document, "ticket-id", "release evidence")?;
    }

    Ok(ReleaseEvidence {
        kind,
        platform,
        release_version,
        source_revision,
        package_sha256,
    })
}

fn parse_release_manifest(document: &Table) -> Result<ReleaseManifest, String> {
    require_format_version(document)?;
    reject_unknown_keys(
        document,
        &[
            "format-version",
            "release-version",
            "source-revision",
            "dependency-notices",
            "dependency-notices-sha256",
            "sbom",
            "sbom-sha256",
            "provenance",
            "provenance-sha256",
            "release-gate-evidence",
            "release-gate-evidence-sha256",
            "platform",
        ],
        "release manifest",
    )?;

    let entries = required_array(document, "platform")?;
    let mut candidates = entries
        .iter()
        .enumerate()
        .map(|(index, value)| parse_release_candidate(value, index))
        .collect::<Result<Vec<_>, _>>()?;
    candidates.sort_by(|left, right| left.platform.cmp(&right.platform));

    let expected = ["linux", "macos", "windows"];
    let actual = candidates
        .iter()
        .map(|candidate| candidate.platform.as_str())
        .collect::<Vec<_>>();
    if actual.as_slice() != expected.as_slice() {
        return Err(
            "release manifest must contain exactly one platform entry for linux, macos, and windows"
                .to_owned(),
        );
    }

    let release_version = required_text(document, "release-version", "release manifest")?;
    if !is_release_candidate_version(&release_version) {
        return Err("release manifest.release-version must use N.N.N-rc.N".to_owned());
    }
    let source_revision = required_text(document, "source-revision", "release manifest")?;
    if !is_git_revision(&source_revision) {
        return Err("release manifest.source-revision must be a lowercase 40-character hexadecimal Git revision".to_owned());
    }

    Ok(ReleaseManifest {
        release_version,
        source_revision,
        dependency_notices: required_release_path(
            document,
            "dependency-notices",
            "release manifest",
        )?,
        dependency_notices_sha256: required_digest(
            document,
            "dependency-notices-sha256",
            "release manifest",
        )?,
        sbom: required_release_path(document, "sbom", "release manifest")?,
        sbom_sha256: required_digest(document, "sbom-sha256", "release manifest")?,
        provenance: required_release_path(document, "provenance", "release manifest")?,
        provenance_sha256: required_digest(document, "provenance-sha256", "release manifest")?,
        release_gate_evidence: required_release_path(
            document,
            "release-gate-evidence",
            "release manifest",
        )?,
        release_gate_evidence_sha256: required_digest(
            document,
            "release-gate-evidence-sha256",
            "release manifest",
        )?,
        candidates,
    })
}

fn parse_release_candidate(value: &Value, index: usize) -> Result<ReleaseCandidate, String> {
    let context = format!("platform[{index}]");
    let table = value
        .as_table()
        .ok_or_else(|| format!("{context} must be a table"))?;
    reject_unknown_keys(
        table,
        &[
            "name",
            "architecture",
            "minimum-version",
            "package",
            "package-sha256",
            "signature",
            "signature-verifier",
            "signature-evidence",
            "signature-evidence-sha256",
            "notarization",
            "notarization-verifier",
            "notarization-evidence",
            "notarization-evidence-sha256",
            "install-evidence",
            "install-evidence-sha256",
            "launch-evidence",
            "launch-evidence-sha256",
            "upgrade-evidence",
            "upgrade-evidence-sha256",
            "uninstall-evidence",
            "uninstall-evidence-sha256",
            "native-ui-validation",
            "native-ui-evidence",
            "native-ui-evidence-sha256",
            "native-ui-deferred-reason",
        ],
        &context,
    )?;

    let platform = required_text(table, "name", &context)?;
    if !["windows", "macos", "linux"].contains(&platform.as_str()) {
        return Err(format!("{context}.name must be windows, macos, or linux"));
    }
    let package_sha256 = required_text(table, "package-sha256", &context)?;
    if !is_sha256(&package_sha256) {
        return Err(format!(
            "{context}.package-sha256 must be a lowercase 64-character hexadecimal digest"
        ));
    }

    let signature = match required_text(table, "signature", &context)?.as_str() {
        "required" => SignatureRequirement::Required,
        "not-applicable" => SignatureRequirement::NotApplicable,
        _ => {
            return Err(format!(
                "{context}.signature must be required or not-applicable"
            ));
        }
    };
    let signature_verifier = optional_text(table, "signature-verifier", &context)?;
    let signature_evidence = optional_release_path(table, "signature-evidence", &context)?;
    let signature_evidence_sha256 = optional_digest(table, "signature-evidence-sha256", &context)?;
    if signature == SignatureRequirement::Required
        && (signature_verifier.is_none()
            || signature_evidence.is_none()
            || signature_evidence_sha256.is_none())
    {
        return Err(format!(
            "{context} requires signature-verifier and signature-evidence when signature is required"
        ));
    }
    if signature == SignatureRequirement::NotApplicable
        && (signature_verifier.is_some()
            || signature_evidence.is_some()
            || signature_evidence_sha256.is_some())
    {
        return Err(format!(
            "{context} must not include signature evidence when signature is not-applicable"
        ));
    }

    let notarization = match required_text(table, "notarization", &context)?.as_str() {
        "required" => SignatureRequirement::Required,
        "not-applicable" => SignatureRequirement::NotApplicable,
        _ => {
            return Err(format!(
                "{context}.notarization must be required or not-applicable"
            ));
        }
    };
    let notarization_verifier = optional_text(table, "notarization-verifier", &context)?;
    let notarization_evidence = optional_release_path(table, "notarization-evidence", &context)?;
    let notarization_evidence_sha256 =
        optional_digest(table, "notarization-evidence-sha256", &context)?;
    if notarization == SignatureRequirement::Required
        && (notarization_verifier.is_none()
            || notarization_evidence.is_none()
            || notarization_evidence_sha256.is_none())
    {
        return Err(format!(
            "{context} requires notarization-verifier, notarization-evidence, and its digest when notarization is required"
        ));
    }
    if notarization == SignatureRequirement::NotApplicable
        && (notarization_verifier.is_some()
            || notarization_evidence.is_some()
            || notarization_evidence_sha256.is_some())
    {
        return Err(format!(
            "{context} must not include notarization evidence when notarization is not-applicable"
        ));
    }

    let native_ui_validation =
        match required_text(table, "native-ui-validation", &context)?.as_str() {
            "passed" => NativeUiValidation::Passed,
            "deferred" => NativeUiValidation::Deferred,
            _ => {
                return Err(format!(
                    "{context}.native-ui-validation must be passed or deferred"
                ));
            }
        };
    let native_ui_evidence = optional_release_path(table, "native-ui-evidence", &context)?;
    let native_ui_evidence_sha256 = optional_digest(table, "native-ui-evidence-sha256", &context)?;
    let native_ui_deferred_reason = optional_text(table, "native-ui-deferred-reason", &context)?;
    match native_ui_validation {
        NativeUiValidation::Passed
            if native_ui_evidence.is_none()
                || native_ui_evidence_sha256.is_none()
                || native_ui_deferred_reason.is_some() =>
        {
            return Err(format!(
                "{context} requires native-ui-evidence and no deferred reason when native UI validation passed"
            ));
        }
        NativeUiValidation::Deferred
            if native_ui_evidence.is_some()
                || native_ui_evidence_sha256.is_some()
                || native_ui_deferred_reason.is_none() =>
        {
            return Err(format!(
                "{context} requires a deferred reason and no native UI evidence when validation is deferred"
            ));
        }
        _ => {}
    }

    Ok(ReleaseCandidate {
        platform,
        architecture: required_text(table, "architecture", &context)?,
        minimum_version: required_text(table, "minimum-version", &context)?,
        package: required_release_path(table, "package", &context)?,
        package_sha256,
        signature,
        signature_evidence,
        signature_evidence_sha256,
        notarization,
        notarization_evidence,
        notarization_evidence_sha256,
        install_evidence: required_release_path(table, "install-evidence", &context)?,
        install_evidence_sha256: required_digest(table, "install-evidence-sha256", &context)?,
        launch_evidence: required_release_path(table, "launch-evidence", &context)?,
        launch_evidence_sha256: required_digest(table, "launch-evidence-sha256", &context)?,
        upgrade_evidence: required_release_path(table, "upgrade-evidence", &context)?,
        upgrade_evidence_sha256: required_digest(table, "upgrade-evidence-sha256", &context)?,
        uninstall_evidence: required_release_path(table, "uninstall-evidence", &context)?,
        uninstall_evidence_sha256: required_digest(table, "uninstall-evidence-sha256", &context)?,
        native_ui_validation,
        native_ui_evidence,
        native_ui_evidence_sha256,
        native_ui_deferred_reason,
    })
}

fn required_release_path(table: &Table, key: &str, context: &str) -> Result<String, String> {
    let path = required_text(table, key, context)?;
    if !is_safe_repository_path(Path::new(&path)) {
        return Err(format!(
            "{context}.{key} must be a repository-relative path without '..'"
        ));
    }
    Ok(path)
}

fn optional_release_path(
    table: &Table,
    key: &str,
    context: &str,
) -> Result<Option<String>, String> {
    let path = optional_text(table, key, context)?;
    if let Some(path) = &path
        && !is_safe_repository_path(Path::new(path))
    {
        return Err(format!(
            "{context}.{key} must be a repository-relative path without '..'"
        ));
    }
    Ok(path)
}

fn optional_release_paths(table: &Table, key: &str, context: &str) -> Result<Vec<String>, String> {
    let Some(values) = table.get(key) else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| format!("{context}.{key} must be an array"))?;
    let mut paths = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let path = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("{context}.{key}[{index}] must be a non-empty string"))?;
            if !is_safe_repository_path(Path::new(path)) {
                return Err(format!(
                    "{context}.{key}[{index}] must be a repository-relative path without '..'"
                ));
            }
            Ok(path.to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    paths.sort();
    if paths.windows(2).any(|values| values[0] == values[1]) {
        return Err(format!("{context}.{key} contains a duplicate"));
    }
    Ok(paths)
}

fn required_digest(table: &Table, key: &str, context: &str) -> Result<String, String> {
    let digest = required_text(table, key, context)?;
    if !is_sha256(&digest) {
        return Err(format!(
            "{context}.{key} must be a lowercase 64-character hexadecimal digest"
        ));
    }
    Ok(digest)
}

fn optional_digest(table: &Table, key: &str, context: &str) -> Result<Option<String>, String> {
    optional_text(table, key, context)?
        .map(|digest| {
            if !is_sha256(&digest) {
                return Err(format!(
                    "{context}.{key} must be a lowercase 64-character hexadecimal digest"
                ));
            }
            Ok(digest)
        })
        .transpose()
}

fn verify_artifacts(path: &Path) -> Result<(), String> {
    let document = read_toml(path)?;
    let artifacts = parse_artifacts(&document)?;
    let mut paths = HashSet::new();

    for artifact in artifacts {
        if !paths.insert(artifact.path.clone()) {
            return Err(format!(
                "duplicate bundled-artifact path {:?}",
                artifact.path
            ));
        }

        let actual = sha256_file(Path::new(&artifact.path))
            .map_err(|error| format!("cannot hash {:?}: {error}", artifact.path))?;
        if actual != artifact.sha256 {
            return Err(format!(
                "SHA-256 mismatch for {:?}: expected {}, got {actual}",
                artifact.path, artifact.sha256
            ));
        }
    }

    Ok(())
}

fn parse_artifacts(document: &Table) -> Result<Vec<Artifact>, String> {
    require_format_version(document)?;
    reject_unknown_keys(document, &["format-version", "artifact"], "artifact policy")?;
    let entries = required_array(document, "artifact")?;

    parse_artifact_entries(entries)
}

fn parse_artifact_entries(entries: &[Value]) -> Result<Vec<Artifact>, String> {
    entries
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let context = format!("artifact[{index}]");
            let table = value
                .as_table()
                .ok_or_else(|| format!("{context} must be a table"))?;
            reject_unknown_keys(table, &["path", "sha256", "source", "license"], &context)?;

            let path_text = required_text(table, "path", &context)?;
            let path = Path::new(&path_text);
            if !is_safe_repository_path(path) {
                return Err(format!(
                    "{context}.path must be a repository-relative path without '..'"
                ));
            }

            let sha256 = required_text(table, "sha256", &context)?;
            if !is_sha256(&sha256) {
                return Err(format!(
                    "{context}.sha256 must be a lowercase 64-character hexadecimal digest"
                ));
            }

            let source = required_text(table, "source", &context)?;
            let license = required_text(table, "license", &context)?;

            Ok(Artifact {
                path: path_text,
                sha256,
                source,
                license,
            })
        })
        .collect()
}

fn verify_sbom(
    lockfile_path: &Path,
    artifacts_path: &Path,
    baseline_path: &Path,
) -> Result<(), String> {
    let current = read_current_sbom(lockfile_path, artifacts_path)?;
    let baseline_document = read_toml(baseline_path)?;
    let baseline = parse_sbom_baseline(&baseline_document)
        .map_err(|error| format!("invalid SBOM baseline {baseline_path:?}: {error}"))?;

    if current == baseline {
        return Ok(());
    }

    Err(format_sbom_diff(&baseline, &current))
}

fn read_current_sbom(lockfile_path: &Path, artifacts_path: &Path) -> Result<Sbom, String> {
    let document = read_toml(lockfile_path)?;
    let mut sbom = parse_lockfile_sbom(&document)
        .map_err(|error| format!("invalid lockfile {lockfile_path:?}: {error}"))?;
    let artifact_document = read_toml(artifacts_path)?;
    sbom.artifacts = parse_artifacts(&artifact_document)
        .map_err(|error| format!("invalid artifact policy {artifacts_path:?}: {error}"))?;
    sbom.artifacts
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(sbom)
}

fn parse_lockfile_sbom(document: &Table) -> Result<Sbom, String> {
    let lock_version = document
        .get("version")
        .and_then(Value::as_integer)
        .ok_or_else(|| "version must be an integer".to_owned())?;
    let entries = required_array(document, "package")?;
    let packages = parse_sbom_packages(entries, "package", false)?;
    Ok(Sbom {
        lock_version,
        packages,
        artifacts: Vec::new(),
    })
}

fn parse_sbom_baseline(document: &Table) -> Result<Sbom, String> {
    require_format_version(document)?;
    reject_unknown_keys(
        document,
        &["format-version", "lock-version", "package", "artifact"],
        "SBOM baseline",
    )?;
    let lock_version = document
        .get("lock-version")
        .and_then(Value::as_integer)
        .ok_or_else(|| "lock-version must be an integer".to_owned())?;
    let entries = required_array(document, "package")?;
    let packages = parse_sbom_packages(entries, "package", true)?;
    let artifact_entries = required_array(document, "artifact")?;
    let mut artifacts = parse_artifact_entries(artifact_entries)?;
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Sbom {
        lock_version,
        packages,
        artifacts,
    })
}

fn parse_sbom_packages(
    entries: &[Value],
    entry_name: &str,
    reject_unknown: bool,
) -> Result<Vec<SbomPackage>, String> {
    let mut packages = entries
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let context = format!("{entry_name}[{index}]");
            let table = value
                .as_table()
                .ok_or_else(|| format!("{context} must be a table"))?;
            if reject_unknown {
                reject_unknown_keys(
                    table,
                    &["name", "version", "source", "checksum", "dependencies"],
                    &context,
                )?;
            }

            let source = optional_text(table, "source", &context)?;
            let checksum = optional_text(table, "checksum", &context)?;
            if let Some(checksum) = &checksum
                && !is_sha256(checksum)
            {
                return Err(format!(
                    "{context}.checksum must be a lowercase 64-character hexadecimal digest"
                ));
            }

            let mut dependencies = table
                .get("dependencies")
                .map(|value| {
                    value
                        .as_array()
                        .ok_or_else(|| format!("{context}.dependencies must be an array"))?
                        .iter()
                        .enumerate()
                        .map(|(dependency_index, value)| {
                            value
                                .as_str()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(str::to_owned)
                                .ok_or_else(|| {
                                    format!(
                                        "{context}.dependencies[{dependency_index}] must be a non-empty string"
                                    )
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            dependencies.sort();
            if dependencies.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(format!("{context}.dependencies contains a duplicate"));
            }

            Ok(SbomPackage {
                name: required_text(table, "name", &context)?,
                version: required_text(table, "version", &context)?,
                source,
                checksum,
                dependencies,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    packages.sort_by(|left, right| left.id().cmp(&right.id()));
    let mut ids = BTreeSet::new();
    for package in &packages {
        if !ids.insert(package.id()) {
            return Err(format!("duplicate package {}", package.id()));
        }
    }
    Ok(packages)
}

fn optional_text(table: &Table, key: &str, context: &str) -> Result<Option<String>, String> {
    table
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("{context}.{key} must be a non-empty string"))
        })
        .transpose()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_git_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_release_candidate_version(value: &str) -> bool {
    let Some((version, candidate)) = value.split_once("-rc.") else {
        return false;
    };
    let components = version.split('.').collect::<Vec<_>>();
    components.len() == 3
        && components.iter().all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        })
        && !candidate.is_empty()
        && candidate.bytes().all(|byte| byte.is_ascii_digit())
}

fn render_sbom(sbom: &Sbom) -> String {
    let mut output = format!(
        "# Generated by `cargo parchmint-ci sbom generate`; review inventory changes before updating.\nformat-version = 1\nlock-version = {}\n",
        sbom.lock_version
    );
    if sbom.packages.is_empty() {
        output.push_str("package = []\n");
    }
    if sbom.artifacts.is_empty() {
        output.push_str("artifact = []\n");
    }

    for package in &sbom.packages {
        output.push_str("\n[[package]]\nname = ");
        output.push_str(&toml_string(&package.name));
        output.push_str("\nversion = ");
        output.push_str(&toml_string(&package.version));
        output.push('\n');
        if let Some(source) = &package.source {
            output.push_str("source = ");
            output.push_str(&toml_string(source));
            output.push('\n');
        }
        if let Some(checksum) = &package.checksum {
            output.push_str("checksum = ");
            output.push_str(&toml_string(checksum));
            output.push('\n');
        }
        if !package.dependencies.is_empty() {
            output.push_str("dependencies = [\n");
            for dependency in &package.dependencies {
                output.push_str("    ");
                output.push_str(&toml_string(dependency));
                output.push_str(",\n");
            }
            output.push_str("]\n");
        }
    }

    for artifact in &sbom.artifacts {
        output.push_str("\n[[artifact]]\npath = ");
        output.push_str(&toml_string(&artifact.path));
        output.push_str("\nsha256 = ");
        output.push_str(&toml_string(&artifact.sha256));
        output.push_str("\nsource = ");
        output.push_str(&toml_string(&artifact.source));
        output.push_str("\nlicense = ");
        output.push_str(&toml_string(&artifact.license));
        output.push('\n');
    }
    output
}

fn toml_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\u{000c}' => escaped.push_str("\\f"),
            '\r' => escaped.push_str("\\r"),
            character if character <= '\u{001f}' || character == '\u{007f}' => {
                escaped.push_str(&format!("\\u{:04X}", u32::from(character)));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn format_sbom_diff(baseline: &Sbom, current: &Sbom) -> String {
    let mut changes = Vec::new();
    if baseline.lock_version != current.lock_version {
        changes.push(format!(
            "  lock format: {} -> {}",
            baseline.lock_version, current.lock_version
        ));
    }

    let baseline_packages: BTreeMap<_, _> = baseline
        .packages
        .iter()
        .map(|package| (package.id(), package))
        .collect();
    let current_packages: BTreeMap<_, _> = current
        .packages
        .iter()
        .map(|package| (package.id(), package))
        .collect();

    for (id, package) in &current_packages {
        match baseline_packages.get(id) {
            None => changes.push(format!("  added: {id}")),
            Some(baseline_package) if *baseline_package != *package => {
                changes.push(format!("  changed: {id}"));
            }
            Some(_) => {}
        }
    }
    for id in baseline_packages.keys() {
        if !current_packages.contains_key(id) {
            changes.push(format!("  removed: {id}"));
        }
    }

    let baseline_artifacts: BTreeMap<_, _> = baseline
        .artifacts
        .iter()
        .map(|artifact| (artifact.path.as_str(), artifact))
        .collect();
    let current_artifacts: BTreeMap<_, _> = current
        .artifacts
        .iter()
        .map(|artifact| (artifact.path.as_str(), artifact))
        .collect();
    for (path, artifact) in &current_artifacts {
        match baseline_artifacts.get(path) {
            None => changes.push(format!("  artifact added: {path}")),
            Some(baseline_artifact) if *baseline_artifact != *artifact => {
                changes.push(format!("  artifact changed: {path}"));
            }
            Some(_) => {}
        }
    }
    for path in baseline_artifacts.keys() {
        if !current_artifacts.contains_key(path) {
            changes.push(format!("  artifact removed: {path}"));
        }
    }

    format!(
        "SBOM baseline differs from Cargo.lock or bundled-artifacts.toml:\n{}\nreview the inventory changes, then regenerate {SBOM_BASELINE_PATH} with `cargo parchmint-ci sbom generate`",
        changes.join("\n")
    )
}

fn read_toml(path: &Path) -> Result<Table, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("cannot read {path:?}: {error}"))?;
    toml::from_str(&text).map_err(|error| format!("cannot parse {path:?}: {error}"))
}

fn require_format_version(document: &Table) -> Result<(), String> {
    match document.get("format-version").and_then(Value::as_integer) {
        Some(1) => Ok(()),
        _ => Err("format-version must be the integer 1".to_owned()),
    }
}

fn required_array<'a>(document: &'a Table, key: &str) -> Result<&'a [Value], String> {
    document
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{key} must be an array"))
}

fn required_text(table: &Table, key: &str, context: &str) -> Result<String, String> {
    let value = table
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{context}.{key} must be a non-empty string"))?;
    Ok(value.to_owned())
}

fn reject_unknown_keys(table: &Table, allowed: &[&str], context: &str) -> Result<(), String> {
    if let Some(key) = table.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{context} contains unknown key {key:?}"));
    }
    Ok(())
}

fn is_safe_repository_path(path: &Path) -> bool {
    let Some(path_text) = path.to_str() else {
        return false;
    };
    let bytes = path_text.as_bytes();
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !path_text.contains('\\')
        && !(bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_rfc3339_timestamp(value: &str) -> bool {
    let Some((date, time_and_zone)) = value.split_once('T') else {
        return false;
    };
    if Date::parse(date).is_err() {
        return false;
    }

    let (time, zone) = if let Some(time) = time_and_zone.strip_suffix('Z') {
        (time, "Z")
    } else if let Some(index) = time_and_zone
        .char_indices()
        .skip(8)
        .find_map(|(index, character)| matches!(character, '+' | '-').then_some(index))
    {
        (&time_and_zone[..index], &time_and_zone[index..])
    } else {
        return false;
    };
    let (clock, fraction) = match time.split_once('.') {
        Some((clock, fraction)) => (clock, Some(fraction)),
        None => (time, None),
    };
    let clock = clock.as_bytes();
    if clock.len() != 8
        || clock[2] != b':'
        || clock[5] != b':'
        || [0, 1, 3, 4, 6, 7]
            .into_iter()
            .any(|index| !clock[index].is_ascii_digit())
    {
        return false;
    }
    let hour = decimal_pair(clock[0], clock[1]);
    let minute = decimal_pair(clock[3], clock[4]);
    let second = decimal_pair(clock[6], clock[7]);
    if hour > 23
        || minute > 59
        || second > 59
        || fraction.is_some_and(|fraction| {
            fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return false;
    }
    zone == "Z" || is_rfc3339_offset(zone)
}

fn is_rfc3339_offset(zone: &str) -> bool {
    let bytes = zone.as_bytes();
    if bytes.len() != 6
        || !matches!(bytes[0], b'+' | b'-')
        || bytes[3] != b':'
        || [1, 2, 4, 5]
            .into_iter()
            .any(|index| !bytes[index].is_ascii_digit())
    {
        return false;
    }
    let hour = decimal_pair(bytes[1], bytes[2]);
    let minute = decimal_pair(bytes[4], bytes[5]);
    hour <= 23 && minute <= 59
}

fn decimal_pair(tens: u8, ones: u8) -> u8 {
    (tens - b'0') * 10 + (ones - b'0')
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(hex_encode(hasher.finalize().as_ref()))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Sbom {
    lock_version: i64,
    packages: Vec<SbomPackage>,
    artifacts: Vec<Artifact>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SbomPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
    dependencies: Vec<String>,
}

impl SbomPackage {
    fn id(&self) -> SbomPackageId<'_> {
        SbomPackageId {
            name: &self.name,
            version: &self.version,
            source: self.source.as_deref(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SbomPackageId<'a> {
    name: &'a str,
    version: &'a str,
    source: Option<&'a str>,
}

impl std::fmt::Display for SbomPackageId<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}@{}", self.name, self.version)?;
        if let Some(source) = self.source {
            write!(formatter, " ({source})")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Exception {
    id: String,
    owner: String,
    expires: Date,
}

#[derive(Debug)]
struct ReleaseInputs {
    artifacts: Vec<ReleaseInputArtifact>,
    platforms: Vec<PlatformReleaseInput>,
}

#[derive(Debug)]
struct ReleaseInputArtifact {
    name: String,
    status: InputStatus,
    path: Option<String>,
    missing_reason: Option<String>,
}

#[derive(Debug)]
struct PlatformReleaseInput {
    name: String,
    architecture: String,
    package_definition: String,
    package_assets: PackageAssetsInput,
    minimum_version: InputState,
    signing: PolicyInput,
    notarization: PolicyInput,
}

#[derive(Debug)]
struct PackageAssetsInput {
    status: InputStatus,
    paths: Vec<String>,
    missing_reason: Option<String>,
}

#[derive(Debug)]
struct InputState {
    status: InputStatus,
    value: Option<String>,
    evidence_path: Option<String>,
    missing_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputStatus {
    Available,
    Missing,
}

#[derive(Debug)]
struct PolicyInput {
    status: PolicyStatus,
    input_path: Option<String>,
    missing_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PolicyStatus {
    Required,
    NotApplicable,
    Missing,
}

#[derive(Debug)]
struct ReleaseEvidence {
    kind: String,
    platform: String,
    release_version: String,
    source_revision: String,
    package_sha256: String,
}

#[derive(Debug)]
struct ReleaseManifest {
    release_version: String,
    source_revision: String,
    dependency_notices: String,
    dependency_notices_sha256: String,
    sbom: String,
    sbom_sha256: String,
    provenance: String,
    provenance_sha256: String,
    release_gate_evidence: String,
    release_gate_evidence_sha256: String,
    candidates: Vec<ReleaseCandidate>,
}

#[derive(Debug)]
struct ReleaseCandidate {
    platform: String,
    architecture: String,
    minimum_version: String,
    package: String,
    package_sha256: String,
    signature: SignatureRequirement,
    signature_evidence: Option<String>,
    signature_evidence_sha256: Option<String>,
    notarization: SignatureRequirement,
    notarization_evidence: Option<String>,
    notarization_evidence_sha256: Option<String>,
    install_evidence: String,
    install_evidence_sha256: String,
    launch_evidence: String,
    launch_evidence_sha256: String,
    upgrade_evidence: String,
    upgrade_evidence_sha256: String,
    uninstall_evidence: String,
    uninstall_evidence_sha256: String,
    native_ui_validation: NativeUiValidation,
    native_ui_evidence: Option<String>,
    native_ui_evidence_sha256: Option<String>,
    native_ui_deferred_reason: Option<String>,
}

impl ReleaseCandidate {
    fn evidence_paths(&self) -> Vec<(&str, &'static str)> {
        let mut paths = vec![
            (self.package.as_str(), "package"),
            (self.install_evidence.as_str(), "install evidence"),
            (self.launch_evidence.as_str(), "launch evidence"),
            (self.upgrade_evidence.as_str(), "upgrade evidence"),
            (self.uninstall_evidence.as_str(), "uninstall evidence"),
        ];
        if let Some(path) = self.signature_evidence.as_deref() {
            paths.push((path, "signature evidence"));
        }
        if let Some(path) = self.notarization_evidence.as_deref() {
            paths.push((path, "notarization evidence"));
        }
        if let Some(path) = self.native_ui_evidence.as_deref() {
            paths.push((path, "native UI evidence"));
        }
        paths
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SignatureRequirement {
    Required,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeUiValidation {
    Passed,
    Deferred,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Artifact {
    path: String,
    sha256: String,
    source: String,
    license: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Date {
    year: i32,
    month: u8,
    day: u8,
}

impl Date {
    fn parse(text: &str) -> Result<Self, &'static str> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return Err("must use YYYY-MM-DD");
        }

        let year = parse_digits(&bytes[0..4]).ok_or("must use YYYY-MM-DD")? as i32;
        let month = parse_digits(&bytes[5..7]).ok_or("must use YYYY-MM-DD")? as u8;
        let day = parse_digits(&bytes[8..10]).ok_or("must use YYYY-MM-DD")? as u8;
        let date = Self { year, month, day };

        if year == 0 || month == 0 || month > 12 || day == 0 || day > days_in_month(year, month) {
            return Err("is not a valid calendar date");
        }
        Ok(date)
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

fn parse_digits(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0_u32, |value, byte| {
        byte.is_ascii_digit()
            .then(|| value * 10 + u32::from(byte - b'0'))
    })
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn utc_today() -> Result<Date, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock predates Unix epoch: {error}"))?
        .as_secs();
    let days = i64::try_from(seconds / 86_400)
        .map_err(|_| "system clock is outside the supported date range".to_owned())?;
    Ok(civil_date_from_unix_days(days))
}

// Converts days since 1970-01-01 to a Gregorian date.
fn civil_date_from_unix_days(days: i64) -> Date {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);

    Date {
        year: i32::try_from(year).expect("current year fits in i32"),
        month: u8::try_from(month).expect("calculated month is valid"),
        day: u8::try_from(day).expect("calculated day is valid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        toml::from_str(text).expect("test TOML is valid")
    }

    fn release_fixture(name: &str) -> Table {
        let text = match name {
            "complete" => include_str!("../tests/fixtures/release/complete.toml"),
            "deferred-native-ui" => {
                include_str!("../tests/fixtures/release/deferred-native-ui.toml")
            }
            "missing-lifecycle-evidence" => {
                include_str!("../tests/fixtures/release/missing-lifecycle-evidence.toml")
            }
            _ => panic!("unknown release fixture {name}"),
        };
        table(text)
    }

    fn release_evidence_fixture(name: &str) -> Table {
        let text = match name {
            "valid-install" => {
                include_str!("../tests/fixtures/release/evidence/valid-install.toml")
            }
            "failed-install" => {
                include_str!("../tests/fixtures/release/evidence/failed-install.toml")
            }
            "valid-native-ui" => {
                include_str!("../tests/fixtures/release/evidence/valid-native-ui.toml")
            }
            _ => panic!("unknown release evidence fixture {name}"),
        };
        table(text)
    }

    fn release_schema(name: &str) -> serde_json::Value {
        let text = match name {
            "dependency-notices" => {
                include_str!("../../../packaging/schemas/dependency-notices.schema.json")
            }
            "installer-evidence" => {
                include_str!("../../../packaging/schemas/installer-evidence.schema.json")
            }
            "minimum-version-evidence" => {
                include_str!("../../../packaging/schemas/minimum-version-evidence.schema.json")
            }
            "provenance" => include_str!("../../../packaging/schemas/provenance.schema.json"),
            "release-candidates" => {
                include_str!("../../../packaging/schemas/release-candidates.schema.json")
            }
            "release-gates" => {
                include_str!("../../../packaging/schemas/release-gates.schema.json")
            }
            "release-inputs" => {
                include_str!("../../../packaging/schemas/release-inputs.schema.json")
            }
            "signing-inputs" => {
                include_str!("../../../packaging/schemas/signing-inputs.schema.json")
            }
            _ => panic!("unknown release schema {name}"),
        };
        serde_json::from_str(text).expect("release schema must be valid JSON")
    }

    #[test]
    fn parses_complete_exception_record() {
        let exceptions = parse_exceptions(&table(
            r#"
                format-version = 1

                [[exception]]
                id = "RUSTSEC-2026-0001"
                check = "advisory"
                package = "example@1.0.0"
                owner = "@maintainers"
                reason = "A fixed release is being evaluated."
                expires = "2026-12-31"
            "#,
        ))
        .expect("record should be accepted");

        assert_eq!(exceptions.len(), 1);
        assert_eq!(exceptions[0].expires.to_string(), "2026-12-31");
    }

    #[test]
    fn rejects_incomplete_exception_record() {
        let error = parse_exceptions(&table(
            r#"
                format-version = 1

                [[exception]]
                id = "RUSTSEC-2026-0001"
                check = "advisory"
                package = "example@1.0.0"
                owner = "@maintainers"
                expires = "2026-12-31"
            "#,
        ))
        .expect_err("reason is required");

        assert!(error.contains(".reason"));
    }

    #[test]
    fn rejects_exception_without_package() {
        let error = parse_exceptions(&table(
            r#"
                format-version = 1

                [[exception]]
                id = "RUSTSEC-2026-0001"
                check = "advisory"
                owner = "@maintainers"
                reason = "A fixed release is being evaluated."
                expires = "2026-12-31"
            "#,
        ))
        .expect_err("package is required");

        assert!(error.contains(".package"));
    }

    #[test]
    fn advisory_and_license_exceptions_require_an_owner_reason_and_expiry() {
        for (missing_key, entry) in [
            (
                "owner",
                r#"
                    id = "RUSTSEC-2026-0001"
                    check = "advisory"
                    package = "example@1.0.0"
                    reason = "A fixed release is being evaluated."
                    expires = "2026-12-31"
                "#,
            ),
            (
                "reason",
                r#"
                    id = "license-example"
                    check = "license"
                    package = "example@1.0.0"
                    owner = "@maintainers"
                    expires = "2026-12-31"
                "#,
            ),
            (
                "expires",
                r#"
                    id = "license-example"
                    check = "license"
                    package = "example@1.0.0"
                    owner = "@maintainers"
                    reason = "A fixed release is being evaluated."
                "#,
            ),
        ] {
            let error = parse_exceptions(&table(&format!(
                "format-version = 1\n[[exception]]\n{entry}"
            )))
            .expect_err("exception record must be reviewable");
            assert!(error.contains(missing_key));
        }
    }

    #[test]
    fn release_manifest_covers_all_platforms_with_frozen_versions_and_lifecycle_evidence() {
        let manifest = parse_release_manifest(&release_fixture("complete"))
            .expect("complete release fixture should be accepted");

        assert_eq!(manifest.release_version, "0.1.0-rc.1");
        assert_eq!(manifest.source_revision.len(), 40);
        assert_eq!(
            manifest.dependency_notices,
            "release/DEPENDENCY-NOTICES.toml"
        );
        assert_eq!(manifest.sbom, "release/parchmint.sbom.toml");
        assert_eq!(manifest.provenance, "release/provenance.toml");
        assert_eq!(manifest.release_gate_evidence, "release/release-gates.toml");
        assert_eq!(manifest.candidates.len(), 3);
        assert_eq!(manifest.candidates[0].platform, "linux");
        assert_eq!(manifest.candidates[1].platform, "macos");
        assert_eq!(manifest.candidates[2].platform, "windows");
        assert!(
            manifest
                .candidates
                .iter()
                .all(|candidate| !candidate.minimum_version.is_empty())
        );
        assert!(
            manifest
                .candidates
                .iter()
                .all(|candidate| candidate.package_sha256.len() == 64)
        );
        assert!(manifest.candidates.iter().all(|candidate| {
            !candidate.install_evidence.is_empty()
                && !candidate.launch_evidence.is_empty()
                && !candidate.upgrade_evidence.is_empty()
                && !candidate.uninstall_evidence.is_empty()
        }));
    }

    #[test]
    fn release_manifest_requires_signing_evidence_only_when_signing_applies() {
        let manifest = parse_release_manifest(&release_fixture("complete"))
            .expect("complete release fixture should be accepted");
        let windows = manifest
            .candidates
            .iter()
            .find(|candidate| candidate.platform == "windows")
            .expect("Windows candidate");
        let linux = manifest
            .candidates
            .iter()
            .find(|candidate| candidate.platform == "linux")
            .expect("Linux candidate");

        assert_eq!(windows.signature, SignatureRequirement::Required);
        assert!(windows.signature_evidence.is_some());
        assert_eq!(linux.signature, SignatureRequirement::NotApplicable);
        assert!(linux.signature_evidence.is_none());
    }

    #[test]
    fn deferred_native_ui_validation_is_explicitly_not_release_ready() {
        let manifest = parse_release_manifest(&release_fixture("deferred-native-ui"))
            .expect("a deferred native UI boundary must be representable without falsifying it");
        assert!(manifest.candidates.iter().all(|candidate| {
            matches!(candidate.native_ui_validation, NativeUiValidation::Deferred)
                && candidate.native_ui_evidence.is_none()
                && candidate
                    .native_ui_deferred_reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains("unavailable"))
        }));
        let windows = manifest
            .candidates
            .iter()
            .find(|candidate| candidate.platform == "windows")
            .expect("Windows candidate");

        assert!(matches!(
            windows.native_ui_validation,
            NativeUiValidation::Deferred
        ));
        let error = verify_release_manifest_readiness(&manifest)
            .expect_err("deferred native UI evidence must block release");
        assert!(error.contains("deferred"));
    }

    #[test]
    fn complete_manifest_is_structurally_release_ready() {
        let manifest = parse_release_manifest(&release_fixture("complete"))
            .expect("complete fixture should parse");
        verify_release_manifest_readiness(&manifest)
            .expect("complete fixture has distinct package and evidence paths");
    }

    #[test]
    fn release_evidence_accepts_passed_native_observations_and_rejects_failures() {
        let install = parse_release_evidence(&release_evidence_fixture("valid-install"))
            .expect("passed install evidence should parse");
        assert_eq!(install.kind, "install");

        parse_release_evidence(&release_evidence_fixture("valid-native-ui"))
            .expect("native UI evidence covers menus, dialogs, and clipboard");

        let error = parse_release_evidence(&release_evidence_fixture("failed-install"))
            .expect_err("failed lifecycle evidence must never satisfy release verification");
        assert!(error.contains("passed"));
    }

    #[test]
    fn release_evidence_rejects_invalid_identity_timestamp_and_native_checks() {
        let valid = include_str!("../tests/fixtures/release/evidence/valid-native-ui.toml");
        for (invalid, expected) in [
            (
                valid.replacen("platform = \"windows\"", "platform = \"browser\"", 1),
                "platform",
            ),
            (
                valid.replacen(
                    "release-version = \"0.1.0-rc.1\"",
                    "release-version = \"unfrozen\"",
                    1,
                ),
                "release-version",
            ),
            (
                valid.replacen(
                    "observed-at = \"2026-08-09T12:00:00Z\"",
                    "observed-at = \"not-a-timestamp\"",
                    1,
                ),
                "observed-at",
            ),
            (
                valid.replacen(
                    "checks = [\"clipboard\", \"dialogs\", \"menus\"]",
                    "checks = [\"clipboard\", \"dialogs\", \"menus\", \"extra\"]",
                    1,
                ),
                "exactly",
            ),
        ] {
            let error = parse_release_evidence(&table(&invalid))
                .expect_err("release evidence must remain schema-safe and identity-bound");
            assert!(error.contains(expected), "unexpected error: {error}");
        }
    }

    #[test]
    fn notarization_evidence_requires_a_tool_and_ticket() {
        let notarization = table(
            r#"
                format-version = 1
                kind = "notarization"
                platform = "macos"
                release-version = "0.1.0-rc.1"
                source-revision = "0123456789abcdef0123456789abcdef01234567"
                package-sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
                result = "passed"
                runner = "native-macos-runner"
                observed-at = "2026-08-09T12:00:00+00:00"
                details = "The stapled disk image passed Gatekeeper assessment."
            "#,
        );
        let error = parse_release_evidence(&notarization)
            .expect_err("notarization needs a verifier and service ticket");
        assert!(error.contains("tool"));
    }

    #[test]
    fn signing_input_references_secret_names_and_requires_macos_notarization() {
        let signing = table(include_str!(
            "../tests/fixtures/release/signing/valid-macos.toml"
        ));
        verify_signing_input(&signing, "macos", true)
            .expect("valid macOS signing and notarization input");

        let missing_notarization = signing
            .into_iter()
            .filter(|(key, _)| key != "notarization")
            .collect::<Table>();
        let error = verify_signing_input(&missing_notarization, "macos", true)
            .expect_err("macOS release signing must include notarization inputs");
        assert!(error.contains("notarization"));
    }

    #[test]
    fn committed_release_inputs_are_explicitly_incomplete() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("CI crate is nested under the workspace root");
        let inputs = root.join(RELEASE_INPUTS_PATH);

        verify_release_inputs(&inputs, false)
            .expect("explicit missing release inputs are valid infrastructure state");
        let error = verify_release_inputs(&inputs, true)
            .expect_err("missing real release inputs must block candidate verification");
        assert!(error.contains("minimum version"));
        assert!(error.contains("dependency-notices"));
    }

    #[test]
    fn absent_release_artifacts_fail_closed() {
        let error = verify_release_file("release/this-candidate-does-not-exist.msix", "package")
            .expect_err("an absent package cannot be release evidence");
        assert!(error.contains("cannot read"));
    }

    #[test]
    fn release_manifest_rejects_missing_install_launch_upgrade_or_uninstall_evidence() {
        let error = parse_release_manifest(&release_fixture("missing-lifecycle-evidence"))
            .expect_err("every candidate must prove the full lifecycle");

        assert!(error.contains("uninstall-evidence"));
    }

    #[test]
    fn release_manifest_rejects_evidence_paths_outside_the_repository() {
        let text = include_str!("../tests/fixtures/release/complete.toml").replacen(
            "release/DEPENDENCY-NOTICES.toml",
            "../DEPENDENCY-NOTICES.txt",
            1,
        );
        let error = parse_release_manifest(&table(&text))
            .expect_err("release evidence must remain repository-relative");

        assert!(error.contains("dependency-notices"));
    }

    #[test]
    fn release_paths_reject_current_directory_windows_and_symlink_escape_forms() {
        for path in [
            ".",
            "./release/package.msix",
            "../release/package.msix",
            "/release/package.msix",
            "C:/release/package.msix",
            "release\\package.msix",
        ] {
            assert!(
                !is_safe_repository_path(Path::new(path)),
                "unsafe release path {path:?} was accepted"
            );
        }
        assert!(is_safe_repository_path(Path::new("release/package.msix")));
    }

    #[test]
    fn json_schemas_parse_and_match_the_fail_closed_release_contract() {
        let expected_path_pattern =
            "^(?!/)(?![A-Za-z]:)(?!.*\\\\)(?!.*(?:^|/)\\.{1,2}(?:/|$))[^\\r\\n]+$";
        for name in [
            "dependency-notices",
            "installer-evidence",
            "minimum-version-evidence",
            "provenance",
            "release-candidates",
            "release-gates",
            "release-inputs",
            "signing-inputs",
        ] {
            let schema = release_schema(name);
            assert_eq!(
                schema["$schema"],
                "https://json-schema.org/draft/2020-12/schema"
            );
            assert_eq!(schema["type"], "object");
        }
        for name in ["release-candidates", "release-inputs", "provenance"] {
            let schema = release_schema(name);
            assert_eq!(
                schema["$defs"]["path"]["pattern"], expected_path_pattern,
                "{schema} must use the verifier's portable path contract"
            );
        }

        let installer = release_schema("installer-evidence");
        assert_eq!(installer["additionalProperties"], false);
        assert_eq!(installer["properties"]["result"]["const"], "passed");
        assert!(
            !installer["properties"]["kind"]["enum"]
                .as_array()
                .expect("kind enum")
                .iter()
                .any(|kind| kind == "release-gates"),
            "release gates use their dedicated evidence schema"
        );
        assert_eq!(
            installer["properties"]["checks"]["items"]["enum"],
            serde_json::json!(["clipboard", "dialogs", "menus"])
        );
    }

    #[test]
    fn release_tag_must_match_the_manifest_version_when_ci_supplies_one() {
        verify_release_tag_name("0.1.0-rc.1", "v0.1.0-rc.1")
            .expect("matching release tag is accepted");
        let error = verify_release_tag_name("0.1.0-rc.1", "v0.1.0-rc.2")
            .expect_err("CI tag cannot name a different candidate");
        assert!(error.contains("does not match"));
    }

    #[test]
    fn encodes_sha256_bytes_as_lowercase_hex() {
        assert_eq!(hex_encode(&[0x00, 0x7f, 0x80, 0xff]), "007f80ff");
    }

    #[test]
    fn validates_calendar_dates() {
        assert!(Date::parse("2024-02-29").is_ok());
        assert!(Date::parse("2026-02-29").is_err());
        assert!(Date::parse("2026-13-01").is_err());
    }

    #[test]
    fn converts_unix_epoch_dates() {
        assert_eq!(
            civil_date_from_unix_days(0),
            Date {
                year: 1970,
                month: 1,
                day: 1
            }
        );
        assert_eq!(
            civil_date_from_unix_days(20_673),
            Date {
                year: 2026,
                month: 8,
                day: 8
            }
        );
    }

    #[test]
    fn rejects_unsafe_artifact_paths() {
        assert!(is_safe_repository_path(Path::new("assets/dictionary.bin")));
        assert!(!is_safe_repository_path(Path::new("../dictionary.bin")));
        assert!(!is_safe_repository_path(Path::new("/dictionary.bin")));
    }

    #[test]
    fn renders_a_deterministic_package_and_artifact_inventory() {
        let mut sbom = parse_lockfile_sbom(&table(
            r#"
                version = 4

                [[package]]
                name = "zebra"
                version = "2.0.0"
                dependencies = ["z-dependency", "a-dependency"]

                [[package]]
                name = "alpha"
                version = "1.0.0"
            "#,
        ))
        .expect("lockfile should be accepted");
        sbom.artifacts = parse_artifacts(&table(
            r#"
                format-version = 1

                [[artifact]]
                path = "assets/dictionary.bin"
                sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
                source = "https://example.invalid/dictionary.bin"
                license = "CC0-1.0"
            "#,
        ))
        .expect("artifact policy should be accepted");

        let rendered = render_sbom(&sbom);
        assert!(rendered.find("alpha").unwrap() < rendered.find("zebra").unwrap());
        assert!(rendered.find("a-dependency").unwrap() < rendered.find("z-dependency").unwrap());
        assert!(rendered.contains("[[artifact]]"));
        assert_eq!(
            parse_sbom_baseline(&table(&rendered)).expect("rendered SBOM should parse"),
            sbom
        );
    }

    #[test]
    fn sbom_diff_reports_package_and_artifact_metadata_changes() {
        let baseline = Sbom {
            lock_version: 4,
            packages: vec![SbomPackage {
                name: "dependency".to_owned(),
                version: "1.0.0".to_owned(),
                source: None,
                checksum: None,
                dependencies: Vec::new(),
            }],
            artifacts: vec![Artifact {
                path: "assets/data.bin".to_owned(),
                sha256: "0".repeat(64),
                source: "https://example.invalid/old".to_owned(),
                license: "MIT".to_owned(),
            }],
        };
        let mut current = baseline.clone();
        current.packages[0]
            .dependencies
            .push("new-dependency".to_owned());
        current.artifacts[0].source = "https://example.invalid/new".to_owned();

        let diff = format_sbom_diff(&baseline, &current);
        assert!(diff.contains("changed: dependency@1.0.0"));
        assert!(diff.contains("artifact changed: assets/data.bin"));
    }

    #[test]
    fn committed_sbom_matches_current_inventory() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("CI crate is nested under the workspace root");

        verify_sbom(
            &root.join(LOCKFILE_PATH),
            &root.join(ARTIFACTS_PATH),
            &root.join(SBOM_BASELINE_PATH),
        )
        .expect("committed SBOM baseline should be current");
    }
}
