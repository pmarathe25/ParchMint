use std::{collections::BTreeMap, env, fs, path::PathBuf};

use harper_core::{
    CharStringExt,
    spell::{Dictionary, MutableDictionary},
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let dictionary = MutableDictionary::curated();
    let mut words = BTreeMap::new();
    let mut normalized = BTreeMap::new();
    let mut longest = 0;
    for word in dictionary.words_iter() {
        longest = longest.max(word.len());
        let common = u64::from(dictionary.get_word_metadata(word).unwrap().common);
        words.insert(word.to_string(), common);
        normalized.insert(word.normalized().to_lower().to_string(), 0);
    }
    for (name, entries) in [("words", words), ("normalized", normalized)] {
        let mut builder =
            fst::MapBuilder::new(fs::File::create(output.join(format!("{name}.fst"))).unwrap())
                .unwrap();
        builder.extend_iter(entries).unwrap();
        builder.finish().unwrap();
    }
    fs::write(
        output.join("limits.rs"),
        format!("const LONGEST_WORD: usize = {longest};\n"),
    )
    .unwrap();
}
