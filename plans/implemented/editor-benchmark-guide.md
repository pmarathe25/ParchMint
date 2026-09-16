# Editor benchmarks

Use the pinned toolchain and release profile, with one compilation job:

```console
cargo test --release --locked -j 1 -p parchmint-editor-iced --test mounted_editor_binding chapter_authoring_performance -- --exact --ignored --nocapture --test-threads=1
```

Replace the test name to run another workload:

| Test | Workload |
| --- | --- |
| `chapter_authoring_performance` | Eight 20,000-word chapters and 2,000 words of Research; 512 inserts, 100 selections, 100 scrolls, eight tab switches, and an exact projection |
| `small_document_authoring_reference` | Same operations and sessions, with one 80-word paragraph per document |
| `chapter_edit_stage_costs` | Adapter input, frame layout, host refresh, and style lookup measured separately |
| `long_paragraph_formatting_performance` | Toggle bold 512 times in one 20,000-word paragraph; verify unchanged text and undo/redo |

Build both versions first, preserve their executables, and alternate fresh-process
runs with no builds in progress. If checkouts share a Cargo target directory,
force changed packages to rebuild and check binary hashes to avoid stale binaries.

These headless tests measure command and layout latency, not native key-to-photon
latency or disk saves. RSS is a resident snapshot, not peak memory or live heap.
The formatting test samples RSS before its text and undo/redo checks. Do not add
individual stage p95 values to estimate end-to-end p95.

Keep raw output and binaries outside the repository. Store only short findings
and reproduction instructions in `plans/implemented/`.
