//! Small deterministic helpers shared by fixture and stress-corpus tests.
//!
//! The corpus generator deliberately emits metadata and individual documents on
//! demand. It does not require committing a large generated project to the
//! repository, and its output is independent of host, locale, and wall clock.

use serde::{Deserialize, Serialize};

/// Configuration for one reproducible stress corpus.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CorpusConfig {
    /// Stable seed used by the word stream.
    pub seed: u64,
    /// Number of document-backed nodes in the generated corpus.
    pub nodes: u32,
    /// Number of words emitted in each generated document.
    pub words_per_document: u32,
}

impl CorpusConfig {
    /// Creates a corpus configuration after checking the documented bounds.
    pub const fn new(seed: u64, nodes: u32, words_per_document: u32) -> Result<Self, CorpusError> {
        if !matches!(nodes, 100 | 1_000 | 10_000) {
            return Err(CorpusError::UnsupportedNodeCount(nodes));
        }
        if words_per_document == 0 {
            return Err(CorpusError::ZeroWordCount);
        }
        Ok(Self {
            seed,
            nodes,
            words_per_document,
        })
    }

    /// Returns the stable checked-in manifest representation.
    pub fn manifest(&self) -> CorpusManifest {
        CorpusManifest {
            generator: "parchmint-test-support/xorshift-word-stream-1".into(),
            seed: self.seed,
            nodes: self.nodes,
            words_per_document: self.words_per_document,
            total_words: u64::from(self.nodes) * u64::from(self.words_per_document),
        }
    }

    /// Generates one human-readable document without allocating the full corpus.
    pub fn document(&self, node_index: u32) -> Result<String, CorpusError> {
        if node_index >= self.nodes {
            return Err(CorpusError::NodeOutOfRange(node_index));
        }
        let mut stream = WordStream::new(self.seed ^ u64::from(node_index));
        let mut body = format!("# Corpus node {:05}\n\n", node_index + 1);
        for word_index in 0..self.words_per_document {
            if word_index > 0 {
                body.push(' ');
            }
            body.push_str(WORDS[stream.next_index()]);
            if word_index % 32 == 31 {
                body.push('\n');
            }
        }
        body.push('\n');
        Ok(body)
    }
}

/// Metadata written to `tests/fixtures/corpus/*.toml`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CorpusManifest {
    /// Generator and algorithm identifier.
    pub generator: String,
    /// Stable random seed.
    pub seed: u64,
    /// Configured node count.
    pub nodes: u32,
    /// Configured words per document.
    pub words_per_document: u32,
    /// Expected total word count.
    pub total_words: u64,
}

impl CorpusManifest {
    /// Serializes the manifest with deterministic TOML field ordering.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }
}

/// Errors returned by the corpus helper.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CorpusError {
    /// The stage's documented stress sizes are the only accepted sizes.
    #[error("unsupported corpus node count {0}; expected 100, 1000, or 10000")]
    UnsupportedNodeCount(u32),
    /// A document must contain at least one word.
    #[error("corpus word count must be greater than zero")]
    ZeroWordCount,
    /// The requested document does not exist in this corpus.
    #[error("corpus node index {0} is out of range")]
    NodeOutOfRange(u32),
}

struct WordStream(u64);

impl WordStream {
    const fn new(seed: u64) -> Self {
        Self(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    fn next_index(&mut self) -> usize {
        let mut value = self.0;
        value ^= value << 7;
        value ^= value >> 9;
        value ^= value << 8;
        self.0 = value;
        let length = u64::try_from(WORDS.len()).expect("word list length fits in u64");
        usize::try_from(value % length).expect("word index fits in usize")
    }
}

const WORDS: &[&str] = &[
    "amber", "archive", "autumn", "beacon", "blue", "candle", "chapter", "clear", "compass",
    "copper", "distant", "draft", "evening", "field", "harbor", "honest", "island", "lantern",
    "lattice", "letter", "meadow", "method", "morning", "notebook", "outline", "paper", "quiet",
    "research", "river", "scene", "signal", "silver", "story", "summer", "syntax", "table",
    "tidal", "title", "valley", "window", "winter",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_sizes_have_exact_reproducible_manifests() {
        for (nodes, words, total) in [
            (100, 50, 5_000),
            (1_000, 100, 100_000),
            (10_000, 1_000, 10_000_000),
        ] {
            let config = CorpusConfig::new(0x2026_0720, nodes, words).unwrap();
            assert_eq!(config.manifest().total_words, total);
            assert_eq!(config.document(0).unwrap(), config.document(0).unwrap());
            assert!(
                config
                    .document(nodes - 1)
                    .unwrap()
                    .starts_with("# Corpus node")
            );
        }
    }

    #[test]
    fn invalid_sizes_are_rejected_without_partial_generation() {
        assert_eq!(
            CorpusConfig::new(1, 99, 1),
            Err(CorpusError::UnsupportedNodeCount(99))
        );
        assert_eq!(
            CorpusConfig::new(1, 100, 0),
            Err(CorpusError::ZeroWordCount)
        );
        let config = CorpusConfig::new(1, 100, 1).unwrap();
        assert_eq!(config.document(100), Err(CorpusError::NodeOutOfRange(100)));
    }

    #[test]
    fn committed_manifests_match_the_generator_contract() {
        for (file, nodes, words, total) in [
            ("100-nodes.toml", 100, 50, 5_000),
            ("1000-nodes.toml", 1_000, 100, 100_000),
            ("10000-nodes.toml", 10_000, 1_000, 10_000_000),
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/corpus")
                .join(file);
            let manifest: CorpusManifest =
                toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            assert_eq!(
                manifest,
                CorpusConfig::new(20_260_720, nodes, words)
                    .unwrap()
                    .manifest()
            );
            assert_eq!(manifest.total_words, total);
        }
    }
}
