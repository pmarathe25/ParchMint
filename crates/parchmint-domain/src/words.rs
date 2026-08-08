use crate::SemanticBlockRef;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WordCount {
    pub total: usize,
}

pub fn count_words<'a>(blocks: impl Iterator<Item = SemanticBlockRef<'a>>) -> WordCount {
    WordCount {
        total: blocks
            .filter(|block| block.is_exportable_text())
            .filter_map(|block| block.text)
            .map(count_text_words)
            .sum(),
    }
}

fn count_text_words(text: &str) -> usize {
    let characters = text.chars().collect::<Vec<_>>();
    let mut count = 0;
    let mut in_word = false;

    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_alphanumeric() {
            if !in_word {
                count += 1;
                in_word = true;
            }
        } else if !is_inner_connector(&characters, index) {
            in_word = false;
        }
    }
    count
}

fn is_inner_connector(characters: &[char], index: usize) -> bool {
    matches!(characters[index], '\'' | '\u{2019}' | '-')
        && index > 0
        && index + 1 < characters.len()
        && characters[index - 1].is_alphanumeric()
        && characters[index + 1].is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_counts_are_deterministic_and_exclude_non_exportable_blocks() {
        let blocks = [
            SemanticBlockRef::heading("A heading"),
            SemanticBlockRef::prose("One two can't state-of-the-art 42 café l’esprit."),
            SemanticBlockRef::comment("ignore these words"),
            SemanticBlockRef::synopsis("ignore this too"),
            SemanticBlockRef::metadata("ignore this metadata"),
            SemanticBlockRef::scene_break(),
            SemanticBlockRef::page_break(),
        ];

        assert_eq!(
            count_words(blocks.iter().copied()),
            count_words(blocks.iter().copied())
        );
        assert_eq!(count_words(blocks.into_iter()).total, 9);
        assert_eq!(
            count_text_words("can't state-of-the-art 42 café l’esprit"),
            5
        );
        assert_eq!(count_text_words("-- spaced - punctuation ' quoted"), 3);
    }
}
