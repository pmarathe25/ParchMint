# 50K-word novel measurements

Measured against `9e3c84d` on 2026-09-17: i7-8550U, 8 GB RAM, Rust 1.97.1
release, warm OS caches, and 1280×720 windows at 1×. No builds or profiling ran
during measurements. Fixtures use 80-word paragraphs without Research or images;
the formatted novel has 1,250 emphasis ranges.
See the [benchmark guide](editor-benchmark-guide.md) for commands.

Three alternating native pairs supplied 60 typing samples per build and workload.
CPU and RSS are medians of three runs; idle memory uses fresh processes.
Headless results are medians of five alternating runs' p95s.

| Measurement | Before | After |
| --- | ---: | ---: |
| Formatted novel: native typing median | 22.97 ms | 14.88 ms |
| Formatted novel: native typing p95 | 29.90 ms | 18.99 ms |
| Formatted novel: app CPU per 20 keys | 0.32 s | 0.15 s |
| Formatted novel: headless layout p95 | 0.632 ms | 0.213 ms |
| Plain chaptered novel: native typing median | 14.30 ms | 14.50 ms |
| Plain 20K-word headless layout p95 | 0.104 ms | 0.105 ms |
| Plain native idle RSS, 10 × 5K words | 43.12 MiB | 43.18 MiB |
| Plain native idle RSS, one 50K-word document | 47.98 MiB | 47.83 MiB |

Typing measures X11 input-to-window-buffer feedback, not key-to-photon latency.
Memory uses normal Wayland windows and shows no meaningful change. All 12 idle
checks recorded zero CPU ticks and no swap. Formatted two-pane busy RSS varied
between 65 and 70 MiB across both builds. Plain-text timing is effectively unchanged.

Layout filters formatting ranges once per visible text chunk instead of scanning
offscreen ranges for every character. It adds no persistent cache. Save durability,
undo retention, animation timing, and release settings are unchanged.

The formatted 80-word reference has a 0.044 ms headless layout p95. This is a
comparison point, not a proven floor.

Validation: 882 workspace tests, documentation tests, nine review-helper tests,
the formatted release workflow, formatting, and Clippy passed. Twelve native
authoring runs preserved edits; formatted runs retained all
1,250 emphasis ranges. All six reopened-image pairs and the dark capture matched
exactly. Of 424 harness captures, 384 matched exactly; 40 differed only in
timestamps or temporary paths. Sampled native split frames were inspected.
Native 2×, gallery playback, and physical display latency remain unverified.
Raw measurements, binaries, and captures stay outside the repository.
