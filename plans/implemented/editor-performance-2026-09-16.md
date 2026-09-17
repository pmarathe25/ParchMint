# Editor improvements — 2026-09-16

## Changes

- Share immutable paragraph text across edits, undo, snapshots, and rendering.
- Copy only selected text for selection counts.
- Reuse unchanged line layout and store exact widths as compact codes.
- Build carets in order and resolve styles once per update.
- Join full text only when a caller requests it.
- Share geometry snapshots and group unchanged line metrics; reuse line breaks.
- Release closed sessions and consolidate save/recovery test fixtures.

Saved formats, undo limits, layout precision, and save durability are unchanged.
The [core](../../crates/parchmint-editor-core/README.md) and
[Iced](../../crates/parchmint-editor-iced/README.md) guides describe current behavior.

## Latest release comparison

Five alternating pairs per workload on Linux, Intel i7-8550U, Rust 1.97.1.
These medians compare shared geometry and grouped line metrics with commit
`a230758`. See the
[benchmark guide](editor-benchmark-guide.md) for workloads and commands.

| Metric | Before | After |
| --- | ---: | ---: |
| Chapter typing p95 | 0.18 ms | 0.14 ms |
| Frame-layout p95 | 0.134 ms | 0.106 ms |
| Host-refresh p95 | 0.0152 ms | 0.0032 ms |
| RSS after typing | 7.71 MiB | 7.58 MiB |
| Long-paragraph formatting p95 | 0.0756 ms | 0.0582 ms |
| Small-document typing p95 | 0.05 ms | 0.05 ms |

Typing p95 ranged from 0.16–0.19 ms before and 0.12–0.19 ms after. Small-document
RSS fell from 5,172 to 5,080 KiB; typing was unchanged. Opening and projection
timings were noisy, including one slow after-run; no gain is claimed. These are
headless measurements; RSS is a resident snapshot, not peak or whole-app memory.

## Remaining work

Chapter typing takes 2.8 times the small-document reference. Layout still walks
paragraph metadata. Paragraph-local updates and cheaper edits
within long paragraphs are the next candidates.

Adding the plain-text size difference (1,055,016 minus 4,680 bytes) to the small
workload's RSS gives a 5.96 MiB one-copy reference, versus 7.58 MiB measured.
This excludes extra metadata, layout, undo, and allocator costs; it is not a
proven floor. Native rendering and disk saves need separate measurements.

## Validation

Passed: 592 workspace unit tests, 26 editor integration tests, 51 production UI
workflows, 97 release-mode editor tests, 9 review-helper tests, and 40 benchmark
runs. The 54 paired animation frames are pixel-identical; reduced motion settles
on the first frame. Reviewed representative intermediate frames and native release
captures in light/dark at 1×. Native input, frame pacing, and supported-size 2×
review remain unverified here. The repository's
[UI-review skill](../../.agents/skills/parchmint-ui-review/SKILL.md) covers those checks.

## Research references

- [Zed](https://zed.dev/blog/zed-decoded-rope-sumtree): shared snapshots.
- [VS Code](https://code.visualstudio.com/blogs/2018/03/23/text-buffer-reimplementation): text-buffer tradeoffs.
- [VS Code saves](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/services/textfile/common/textFileEditorModel.ts): captured revisions and dirty state.
- [Xi](https://xi-editor.io/docs/rope_science_05.html): incremental wrapping.
- [Emacs](https://www.gnu.org/software/emacs/manual/html_node/elisp/Saving-Buffers.html): temporary-file replacement.
