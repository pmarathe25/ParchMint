# 50K-word novel measurements

Measured 2026-09-17 on an i7-8550U with 8 GB RAM, Rust 1.97.1 release,
warm OS caches, and 1280×720 windows at 1×. No builds or profiling ran during
measurements. Fixtures use 80-word paragraphs without Research or images.
See the [benchmark guide](editor-benchmark-guide.md) for the harness commands.

Three alternating before/after pairs used 60 typing samples per build and
three fresh-process memory runs per layout. Typing values pool the samples;
CPU and memory values are medians of runs.

| Native measurement | Before | After |
| --- | ---: | ---: |
| Typing feedback, median | 40.3 ms | 14.7 ms |
| Typing feedback, p95 | 46.1 ms | 17.8 ms |
| App CPU per 20 keys | 0.61 s | 0.12 s |
| Idle RSS, 10 × 5K words | 44.7 MiB | 43.3 MiB |
| Idle RSS, one 50K-word document | 49.4 MiB | 47.9 MiB |

Typing measures X11 input-to-window-buffer feedback, not key-to-photon latency.
Memory uses normal Wayland windows. Anonymous memory fell by 1.5 MiB (chaptered)
and 1.3 MiB (single document). All 12 paired idle checks recorded zero CPU ticks
and zero swap; busy-session median RSS was 45.2 → 44.7 MiB.

The renderer caches unchanged line geometry and ink bounds, limits damage to
changed items, skips off-region glyphs, reuses clipping masks, and directly fills
eligible opaque rectangles. It keeps no rasterized-line images.

Temporary release probes measured medians of 0.87 ms for editor updates,
0.85 ms for painting, and 4.5 ms from update entry to frame submission.
Submission-to-observed-buffer time was about 9 ms, including the display path
and polling. Probes were removed before the paired runs.

The same release's small-project reference (10 × 80 words, three runs) is
41.8 MiB, about 1.5 MiB below the chaptered novel. This is a practical reference,
not a proven floor. Save durability is unchanged; earlier filesystem-sync stalls
remain outside these improvements.

Validation: 880 workspace tests, eight renderer tests, nine review-helper tests,
documentation tests, formatting, and Clippy passed. All six native authoring runs
preserved saved edits; all three reopened before/after images matched exactly.
Of 424 captures, 384 matched exactly and 40 differed only in reviewed timestamps
or temporary paths. Native dark mode and split-animation frames were inspected
using the UI review skill. Native 2× and physical display latency remain unverified.
Raw measurements, profiles, and captures stay outside the repository.
