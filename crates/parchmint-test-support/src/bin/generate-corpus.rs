//! CLI for validating or writing a small corpus manifest.

use std::{env, fs, path::PathBuf, process::ExitCode};

use parchmint_test_support::CorpusConfig;

fn main() -> ExitCode {
    let mut seed = 20_260_720;
    let mut nodes = None;
    let mut words = None;
    let mut manifest = None;
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| usage("missing option value"));
        match argument.as_str() {
            "--seed" => seed = parse(&value(), "seed"),
            "--nodes" => nodes = Some(parse(&value(), "nodes")),
            "--words" => words = Some(parse(&value(), "words per document")),
            "--manifest" => manifest = Some(PathBuf::from(value())),
            "--help" | "-h" => {
                println!(
                    "usage: cargo run -p parchmint-test-support --bin generate-corpus -- --nodes N --words N [--seed N] [--manifest PATH]"
                );
                return ExitCode::SUCCESS;
            }
            _ => usage("unknown option"),
        }
    }
    let config = CorpusConfig::new(seed, nodes.unwrap_or(100), words.unwrap_or(50))
        .unwrap_or_else(|error| usage(&error.to_string()));
    let text = config.manifest().to_toml().expect("manifest serialization");
    if let Some(path) = manifest {
        fs::write(path, text).unwrap_or_else(|error| usage(&error.to_string()));
    } else {
        print!("{text}");
    }
    ExitCode::SUCCESS
}

fn parse<T: std::str::FromStr>(value: &str, name: &str) -> T {
    value
        .parse()
        .unwrap_or_else(|_| usage(&format!("invalid {name}")))
}

fn usage(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!("use --help for usage");
    std::process::exit(2)
}
