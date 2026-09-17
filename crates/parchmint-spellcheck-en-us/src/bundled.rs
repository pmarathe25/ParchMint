//! Build-time expansion of Harper's dictionary, without its runtime grammar
//! metadata or duplicate UTF-32 word tables. Only the ranking's `common` bit is
//! needed for spelling. FST data is borrowed directly from the executable.

use std::{borrow::Cow, sync::OnceLock};

use fst::{IntoStreamer, Streamer};
use levenshtein_automata::LevenshteinAutomatonBuilder;

use crate::{ENGINE_CANDIDATE_LIMIT, SUGGESTION_DISTANCE};

include!(concat!(env!("OUT_DIR"), "/limits.rs"));

pub(crate) struct BundledDictionary {
    words: fst::Map<&'static [u8]>,
    normalized: fst::Map<&'static [u8]>,
}

impl BundledDictionary {
    pub(crate) fn new() -> Self {
        Self {
            words: fst::Map::new(include_bytes!(concat!(env!("OUT_DIR"), "/words.fst")).as_slice())
                .expect("built dictionary"),
            normalized: fst::Map::new(
                include_bytes!(concat!(env!("OUT_DIR"), "/normalized.fst")).as_slice(),
            )
            .expect("built normalized dictionary"),
        }
    }

    pub(crate) fn contains(&self, word: &str) -> bool {
        self.normalized.contains_key(normalize(word).to_lowercase())
    }

    pub(crate) fn suggestions(&self, word: &str) -> Vec<String> {
        // No dictionary word can match beyond this bound. In particular, do
        // not construct large DFAs for long identifiers or uninterrupted input.
        if word.chars().count() > LONGEST_WORD + usize::from(SUGGESTION_DISTANCE) {
            return Vec::new();
        }
        static BUILDER: OnceLock<LevenshteinAutomatonBuilder> = OnceLock::new();
        let builder =
            BUILDER.get_or_init(|| LevenshteinAutomatonBuilder::new(SUGGESTION_DISTANCE, false));
        let normalized = normalize(word);
        let lowercase = normalized.to_lowercase();
        let mut matches = Vec::new();
        let queries = std::iter::once(normalized.as_ref())
            .chain((normalized != lowercase).then_some(lowercase.as_str()));
        for query in queries {
            let dfa = builder.build_dfa(query);
            let mut stream = self.words.search_with_state(&dfa).into_stream();
            while let Some((bytes, common, state)) = stream.next() {
                matches.push((
                    String::from_utf8(bytes.to_vec()).expect("dictionary UTF-8"),
                    dfa.distance(state).to_u8(),
                    common != 0,
                ));
            }
        }
        // Merge by word, not by stream position: case-folded streams can have
        // different lengths and different words at the same position.
        matches.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        matches.dedup_by(|a, b| a.0 == b.0);
        matches.sort_unstable_by_key(|candidate| candidate.1);
        matches.truncate(ENGINE_CANDIDATE_LIMIT);
        // Harper's spelling ranking (not its unrelated grammar metadata).
        matches.sort_by_key(|(candidate, distance, common)| {
            i32::from(*distance) * 10
                - i32::from(word.chars().next() == candidate.chars().next()) * 10
                - i32::from(word.ends_with('s') && candidate.ends_with('s')) * 5
                - i32::from(*common) * 5
                - i32::from(candidate.chars().filter(|c| *c == '\'').count() == 1) * 5
        });
        matches.into_iter().map(|(word, _, _)| word).collect()
    }
}

fn normalize(word: &str) -> Cow<'_, str> {
    if word.contains(['’', '‘', '＇']) {
        Cow::Owned(word.replace(['’', '‘', '＇'], "'"))
    } else {
        Cow::Borrowed(word)
    }
}

#[cfg(test)]
mod tests {
    use harper_core::{
        CharStringExt,
        spell::{Dictionary, FstDictionary, MutableDictionary, suggest_correct_spelling_str},
    };

    use super::*;

    #[test]
    fn compact_dictionary_preserves_every_word_and_case_fold() {
        let compact = BundledDictionary::new();
        let original = MutableDictionary::curated();
        assert_eq!(compact.words.len(), original.word_count());
        for chars in original.words_iter() {
            let word = chars.to_string();
            for variant in [
                word.clone(),
                word.to_lowercase(),
                word.to_uppercase(),
                word.replace('\'', "’"),
            ] {
                assert_eq!(
                    compact.contains(&variant),
                    original.contains_word_str(&variant),
                    "{variant}"
                );
            }
        }
        assert!(!compact.contains("zzquillfluxzz"));
    }

    #[test]
    fn suggestions_preserve_lowercase_ranking_and_capitalized_corrections() {
        let compact = BundledDictionary::new();
        let original = FstDictionary::curated();
        for word in [
            "teh",
            "hvllo",
            "punctation",
            "ths",
            "youre",
            "weve",
            "adviced",
            "aout",
            "mispelled",
            "rainn",
            "latntern",
            "zzquillfluxzz",
        ] {
            assert_eq!(
                compact.suggestions(word),
                suggest_correct_spelling_str(
                    word,
                    ENGINE_CANDIDATE_LIMIT,
                    SUGGESTION_DISTANCE,
                    original.as_ref()
                ),
                "{word}"
            );
        }
        assert!(
            compact
                .suggestions("Hvllo")
                .iter()
                .any(|word| word == "hello")
        );
        assert!(compact.suggestions(&"x".repeat(256)).is_empty());
    }
}
