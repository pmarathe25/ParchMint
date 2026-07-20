//! Deterministic parser smoke corpus for scheduled CI.

use parchmint_markdown::Document;

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map_or(Ok(10_000), |value| value.parse::<u32>())
        .expect("iterations must be an integer")
        .clamp(1, 100_000);
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for index in 0..iterations {
        let length = usize::try_from(next(&mut state) % 4_096).unwrap_or_default();
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length {
            bytes.push(next(&mut state).to_le_bytes()[0]);
        }
        let source = String::from_utf8_lossy(&bytes);
        let _ = Document::parse(&source);
        let _ = parchmint_compile::validate_html(&bytes);
        let _ = parchmint_compile::validate_pdf(&bytes);
        let _ = parchmint_compile::validate_epub(&bytes);
        let _ = parchmint_compile::validate_docx(&bytes);
        if index % 1_000 == 0 {
            eprintln!("fuzz-smoke cases={index}");
        }
    }
    println!("fuzz-smoke completed {iterations} deterministic cases");
}

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 7;
    *state ^= *state >> 9;
    *state ^= *state << 8;
    *state
}
