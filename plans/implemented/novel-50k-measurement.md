# 50K-word novel measurements

Measured against `c25de52` on 2026-09-17: i7-8550U, 8 GB RAM, Rust 1.97.1
release, warm OS caches, and 1280×720 windows at 1×. No builds or profiling ran
during measurements. Fixtures use 80-word paragraphs without Research or images.
See the [benchmark guide](editor-benchmark-guide.md) for commands.

Three alternating native pairs supplied 60 typing samples per build and three
fresh-process memory runs per layout. Typing pools the samples; CPU and RSS
are medians of runs. Headless results are medians of five alternating runs' p95s.

| Measurement | Before | After |
| --- | ---: | ---: |
| Native typing feedback, median | 15.16 ms | 14.51 ms |
| Native typing feedback, p95 | 18.09 ms | 17.27 ms |
| App CPU per 20 keys | 0.13 s | 0.11 s |
| Headless layout, 250 paragraphs, p95 | 0.105 ms | 0.095 ms |
| Headless formatting, 20K-word paragraph, p95 | 0.080 ms | 0.044 ms |
| Native idle RSS, 10 × 5K words | 43.18 MiB | 43.11 MiB |
| Native idle RSS, one 50K-word document | 47.85 MiB | 47.89 MiB |

Typing measures X11 input-to-window-buffer feedback, not key-to-photon latency.
Memory uses normal Wayland windows and shows no meaningful change. All 12 paired
idle checks recorded zero CPU ticks and no swap. Busy-session RSS was 45.18 →
44.92 MiB. Small native timing differences remain sensitive to display timing.

Canvas callbacks borrow content instead of cloning decorations, and empty
annotation sets skip hit testing. Layout reserves buffers from the previous
viewport, traverses wrap offsets linearly, and scans inline marks once per scalar.
Save durability, undo retention, and animation timing are unchanged.

The same release's small-project reference (10 × 80 words, three runs) is
41.9 MiB, about 1.2 MiB below the chaptered novel. This is not a proven floor.
One small run mapped 21.6 MiB of application code/data and 7.9 MiB of shared
window buffers; fixed desktop overhead is the next memory target.

Validation: 882 workspace tests, eight renderer tests, nine review-helper tests,
documentation tests, formatting, and Clippy passed. Six native authoring runs
preserved saved edits; reopened pairs and dark captures matched exactly.
Of 424 captures, 384 matched; differences were dynamic labels and notification
expiry. The History fixture now expires its banner before unsaved edits; repeat
captures differed only in timestamps. Sampled native split frames were inspected.
Native 2×, gallery playback, and physical display latency remain unverified.
Raw measurements, binaries, and captures stay outside the repository.
