# Performance budgets

> Read when changing project open, graph operations, typing, search, models,
> persistence, compile, export, or worker cancellation.

These are release-mode gates on the reference 10,000-node/10-million-word
corpus. Measure UI-owner time separately from worker and disk time.

| Boundary | Budget |
|---|---:|
| Binder usable after open | 3 s |
| Create, rename, reorder visible | 100 ms owner/UI time |
| Canonical durability for one changed set | 1 s worker time |
| Editor delta and count update | p95 16 ms; p99 50 ms |
| Autosave scheduling | no UI block over 8 ms |
| First bounded search result | 300 ms |
| One model row/subtree delta | 100 ms; no reset |
| Compile cancellation | 250 ms |
| Peak resident memory | 500 MiB |

Storage assertions also verify files and bytes written: metadata/body edits
write the affected document; reorder writes `outline.toml`; unrelated bodies
remain untouched. Failure injection must reopen to a complete old or new state.

## Run

```sh
cargo test --locked -p parchmint-app -p parchmint-markdown \
  -p parchmint-index -p parchmint-compile -p parchmint-storage \
  --release -- --nocapture

cmake -S . -B build-release -DCMAKE_BUILD_TYPE=Release
cmake --build build-release --parallel
ctest --test-dir build-release --output-on-failure
cmake --build build-release --target editor-benchmark
```

Record hardware, filesystem, build type, corpus seed, p50/p95, peak RSS, FFI
bytes, files/bytes written, and cancellation latency with release evidence.
