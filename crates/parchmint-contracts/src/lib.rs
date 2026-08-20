//! Versioned JSON contracts used at ParchMint's durable data boundaries.

use std::{collections::BTreeMap, error::Error, fmt};

/// A lossless JSON value retained for fields introduced by compatible readers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationMessage {
    pub id: [u8; 16],
    pub body: String,
    pub unknown_fields: BTreeMap<String, AnnotationValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationAnchor {
    Document {
        unknown_fields: BTreeMap<String, AnnotationValue>,
    },
    Text {
        block: [u8; 16],
        start: u64,
        end: u64,
        quote: String,
        context_before: String,
        context_after: String,
        orphaned: bool,
        unknown_fields: BTreeMap<String, AnnotationValue>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationThread {
    pub id: [u8; 16],
    pub messages: Vec<AnnotationMessage>,
    pub resolved: bool,
    pub anchor: AnnotationAnchor,
    pub unknown_fields: BTreeMap<String, AnnotationValue>,
}

pub mod generated;

/// The published identity and source checksum of one contract version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractDescriptor {
    pub schema_id: &'static str,
    pub schema_version: u32,
    pub source_checksum: &'static str,
}

struct ContractSpec {
    descriptor: ContractDescriptor,
    #[cfg(test)]
    schema_file: &'static str,
    #[cfg(test)]
    fixtures_dir: &'static str,
}

const CONTRACTS: &[ContractSpec] = &[
    ContractSpec {
        descriptor: ContractDescriptor {
            schema_id: "parchmint.annotation-sidecar",
            schema_version: 1,
            source_checksum: "a717f10bd68211181d6266dd2f2e238562792f23431251ed268f97a959bd7869",
        },
        #[cfg(test)]
        schema_file: "schemas/annotation-sidecar/v1.schema.json",
        #[cfg(test)]
        fixtures_dir: "schemas/annotation-sidecar/fixtures/v1",
    },
    ContractSpec {
        descriptor: ContractDescriptor {
            schema_id: "parchmint.recovery-record",
            schema_version: 1,
            source_checksum: "bbee00b3c3260714eb78fee50990c157663adf48496bd34a560fc92d62aacc59",
        },
        #[cfg(test)]
        schema_file: "schemas/recovery-record/v1.schema.json",
        #[cfg(test)]
        fixtures_dir: "schemas/recovery-record/fixtures/v1",
    },
    ContractSpec {
        descriptor: ContractDescriptor {
            schema_id: "parchmint.cli-output",
            schema_version: 1,
            source_checksum: "3e7e51781b958997f892e3774424beeeb1ef1d997f0ae81ab9b807511df5d212",
        },
        #[cfg(test)]
        schema_file: "schemas/cli-output/v1.schema.json",
        #[cfg(test)]
        fixtures_dir: "schemas/cli-output/fixtures/v1",
    },
];

/// Errors returned while decoding and re-encoding a fixture.
#[derive(Debug)]
pub enum ContractError {
    Json(serde_json::Error),
    SchemaMismatch {
        expected: &'static str,
        actual: String,
    },
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid contract JSON: {error}"),
            Self::SchemaMismatch { expected, actual } => {
                write!(
                    formatter,
                    "expected contract schema {expected}, got {actual}"
                )
            }
        }
    }
}

impl Error for ContractError {}

impl From<serde_json::Error> for ContractError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

macro_rules! decode_validate_reencode {
    ($type:ty, $descriptor:expr, $json:expr) => {{
        let value: $type = serde_json::from_slice($json)?;
        validate_schema_name($descriptor, &value.schema)?;
        let _canonical = serde_json::to_vec(&value)?;
    }};
}

/// Returns the published descriptor for a contract ID.
pub fn descriptor(schema_id: &str) -> Option<&'static ContractDescriptor> {
    CONTRACTS
        .iter()
        .find(|contract| contract.descriptor.schema_id == schema_id)
        .map(|contract| &contract.descriptor)
}

/// Parses a fixture and re-encodes its JSON representation.
pub fn validate_fixture(descriptor: &ContractDescriptor, json: &[u8]) -> Result<(), ContractError> {
    match descriptor.schema_id {
        "parchmint.annotation-sidecar" => {
            decode_validate_reencode!(generated::AnnotationSidecarV1, descriptor, json);
        }
        "parchmint.recovery-record" => {
            decode_validate_reencode!(generated::RecoveryRecordV1, descriptor, json);
        }
        "parchmint.cli-output" => {
            decode_validate_reencode!(generated::CliOutputV1, descriptor, json);
        }
        _ => {
            let value: serde_json::Value = serde_json::from_slice(json)?;
            let _canonical = serde_json::to_vec(&value)?;
        }
    }
    Ok(())
}

fn validate_schema_name(
    descriptor: &ContractDescriptor,
    actual: &str,
) -> Result<(), ContractError> {
    let expected = format!("{}/v{}", descriptor.schema_id, descriptor.schema_version);
    if actual == expected {
        Ok(())
    } else {
        Err(ContractError::SchemaMismatch {
            expected: descriptor.schema_id,
            actual: actual.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use sha2::{Digest, Sha256};

    use super::*;

    #[test]
    fn descriptors_match_schema_checksums() {
        for contract in CONTRACTS {
            let descriptor = super::descriptor(contract.descriptor.schema_id).unwrap();
            let schema = read_file(contract.schema_file);

            assert_eq!(descriptor, &contract.descriptor);
            assert_eq!(descriptor.source_checksum, sha256(&schema));
            assert!(
                descriptor
                    .source_checksum
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "{} must use a lowercase hexadecimal source checksum",
                descriptor.schema_id,
            );
        }

        assert!(super::descriptor("parchmint.unknown-contract").is_none());
    }

    #[test]
    fn fixtures_decode_and_reencode() {
        for contract in CONTRACTS {
            let fixtures = fixture_paths(contract);
            assert!(
                !fixtures.is_empty(),
                "{} must keep a fixture beside its schema",
                contract.descriptor.schema_id,
            );

            for fixture in fixtures {
                validate_fixture(&contract.descriptor, &read_file(&fixture)).unwrap_or_else(
                    |error| {
                        panic!(
                            "{} does not decode through {}: {error}",
                            fixture.display(),
                            contract.descriptor.schema_id,
                        )
                    },
                );
            }
        }
    }

    #[test]
    fn malformed_and_non_utf8_json_is_rejected() {
        for contract in CONTRACTS {
            assert!(
                validate_fixture(&contract.descriptor, br#"{"#).is_err(),
                "{} accepted malformed JSON",
                contract.descriptor.schema_id,
            );
            assert!(
                validate_fixture(&contract.descriptor, &[0xff]).is_err(),
                "{} accepted non-UTF-8 JSON",
                contract.descriptor.schema_id,
            );
        }
    }

    #[test]
    fn generated_bindings_match_schema_manifest() {
        let mut generated = String::new();
        let mut contracts = CONTRACTS.iter().collect::<Vec<_>>();
        contracts.sort_by_key(|contract| contract.descriptor.schema_id);

        for contract in contracts {
            let schema_bytes = read_file(contract.schema_file);
            let schema = serde_json::from_slice::<serde_json::Value>(&schema_bytes).unwrap();
            let properties = schema["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(",");
            let required = schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(",");
            generated.push_str(&format!(
                "{}/v{}\t{}\t{}\tproperties={properties}\trequired={required}\n",
                contract.descriptor.schema_id,
                contract.descriptor.schema_version,
                contract.descriptor.schema_version,
                sha256(&schema_bytes),
            ));
        }

        assert_eq!(generated, generated::SCHEMA_MANIFEST);
    }

    #[test]
    fn generated_bindings_reject_unknown_fields_and_wrong_schema() {
        let descriptor = descriptor("parchmint.cli-output").unwrap();
        assert!(
            validate_fixture(
                descriptor,
                br#"{"schema":"parchmint.cli-output/v1","ok":true,"extra":1}"#
            )
            .is_err()
        );
        assert!(matches!(
            validate_fixture(descriptor, br#"{"schema":"parchmint.other/v1","ok":true}"#),
            Err(ContractError::SchemaMismatch { .. })
        ));
    }

    #[test]
    fn every_typed_descriptor_rejects_wrong_schema() {
        for contract in CONTRACTS {
            let fixture = read_file(fixture_paths(contract).first().unwrap());
            let mut value = serde_json::from_slice::<serde_json::Value>(&fixture).unwrap();
            value["schema"] = serde_json::Value::String("parchmint.other/v1".to_owned());

            assert!(matches!(
                validate_fixture(&contract.descriptor, &serde_json::to_vec(&value).unwrap()),
                Err(ContractError::SchemaMismatch { .. })
            ));
        }
    }

    fn fixture_paths(contract: &ContractSpec) -> Vec<PathBuf> {
        let directory = contract_path(contract.fixtures_dir);
        let mut fixtures = fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect::<Vec<_>>();
        fixtures.sort();
        fixtures
    }

    fn read_file(relative_path: impl AsRef<std::path::Path>) -> Vec<u8> {
        let path = contract_path(relative_path);
        fs::read(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
    }

    fn contract_path(relative_path: impl AsRef<std::path::Path>) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path)
    }

    fn sha256(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
}
