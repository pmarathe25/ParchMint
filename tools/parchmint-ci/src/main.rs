use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use toml::{Table, Value};

const EXCEPTIONS_PATH: &str = "supply-chain/exceptions.toml";
const ARTIFACTS_PATH: &str = "supply-chain/bundled-artifacts.toml";
const LOCKFILE_PATH: &str = "Cargo.lock";
const SBOM_BASELINE_PATH: &str = "supply-chain/sbom-baseline.toml";

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("supply-chain validation failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let command = args.next();
    let action = args.next();
    let extra = args.next();

    match (command.as_deref(), action.as_deref(), extra) {
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
        _ => Err(
            "usage: cargo parchmint-ci verify\n       cargo parchmint-ci sbom <verify|generate>"
                .to_owned(),
        ),
    }
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
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
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
