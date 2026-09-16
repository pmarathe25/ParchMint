# Editor improvements — 2026-09-16

## Changes

- Share immutable paragraph text across edits, undo, snapshots, and rendering.
- Copy only selected text for selection counts.
- Reuse unchanged line layout and store exact widths as compact codes.
- Build carets in order and resolve styles once per update.
- Join full text only when a caller requests it.
- Release closed sessions and consolidate save/recovery test fixtures.

Saved formats, undo limits, layout precision, and save durability are unchanged.
The [core](../../crates/parchmint-editor-core/README.md) and
[Iced](../../crates/parchmint-editor-iced/README.md) guides describe current behavior.

## Latest release comparison

Five alternating pairs per workload on Linux, Intel i7-8550U, Rust 1.97.1.
These medians compare the final paragraph-backed layout with the preceding
compact-width implementation, not the original commit. See the
[benchmark guide](editor-benchmark-guide.md) for workloads and commands.

| Metric | Before | After |
| --- | ---: | ---: |
| Chapter typing p95 | 0.25 ms | 0.18 ms |
| Frame-layout p95 | 0.214 ms | 0.123 ms |
| RSS after typing | 8.04 MiB | 7.83 MiB |
| Long-paragraph formatting p95 | 0.103 ms | 0.075 ms |
| Small-document typing p95 | 0.05 ms | 0.05 ms |

Typing p95 ranged from 0.23–0.30 ms before and 0.17–0.23 ms after. Small-document
timing and memory show no established change. These are headless measurements;
RSS is a resident snapshot, not peak memory or whole-application memory.

## Remaining work

Chapter typing takes 3.6 times the small-document reference. Layout still scans
line breaks and paragraph metadata. Paragraph-local updates and cheaper edits
within long paragraphs are the next candidates.

Adding the plain-text size difference (1,055,016 minus 4,680 bytes) to the small
workload's RSS gives a 6.10 MiB one-copy reference, versus 7.83 MiB measured.
This excludes extra metadata, layout, undo, and allocator costs; it is not a
proven floor. Native rendering and disk saves need separate measurements.

## Validation

Passed: 591 workspace unit tests, 26 editor integrations, and 45 production UI
workflows (662 distinct tests), plus 96 release-mode unit reruns and 40 release
benchmark runs. Clippy with warnings denied, formatting, and whitespace checks
passed.

Coverage includes Unicode, exact layout comparisons, formatting, undo/redo,
Find/Replace, two panes, comments, delayed saves, and 250,000-word recovery.
No manual native-window review was performed.

## Research references

- [Zed](https://zed.dev/blog/zed-decoded-rope-sumtree): shared snapshots.
- [VS Code](https://code.visualstudio.com/blogs/2018/03/23/text-buffer-reimplementation): text-buffer tradeoffs.
- [VS Code saves](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/services/textfile/common/textFileEditorModel.ts): captured revisions and dirty state.
- [Xi](https://xi-editor.io/docs/rope_science_05.html): incremental wrapping.
- [Emacs](https://www.gnu.org/software/emacs/manual/html_node/elisp/Saving-Buffers.html): temporary-file replacement.
