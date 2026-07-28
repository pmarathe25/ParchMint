# V04 — SQLite FTS5 search validation

**Disposition: pass.** The bundled SQLite FTS5 implementation passes the V04
semantic, integrity, rebuild, concurrency, and 200 ms warm-first-result gates
on the full 20-million-word Linux corpus. Native Linux, Windows, and macOS
smoke runs also pass the required schema/query parity, FTS5 assertion,
incremental-update, corpus-manifest, rebuild, and integrity checks.

## Composition

- `rusqlite =0.40.1` is exact-pinned with `default-features = false` and only
  `bundled`.
- The resolved native layer is `libsqlite3-sys 0.38.1`, compiling bundled
  SQLite 3.53.2. `ENABLE_FTS5` is present.
- Startup asserts FTS5 by creating the actual FTS5 table. A missing FTS5 module
  fails worker startup.
- Search owns one `Connection` on the named `parchmint-search-index` worker.
  Callers communicate through typed channels; queries, updates, integrity work,
  and rebuilds never borrow the caller/UI thread's connection.
- Tantivy was not implemented.

## Schema and query behavior

`documents` owns current project/document revision and hierarchy projections.
`search_block_content` owns stable project, document, block, and revision IDs,
section, tree-path, body, title, synopsis, and metadata projection. The
external-content `search_blocks` FTS5 table indexes the four controllable text
fields with:

```text
unicode61 remove_diacritics 2
```

Insert/delete/update triggers keep FTS content synchronized. Document-level
title, synopsis, and metadata projections are placed on the first stable block
to produce one logical field hit per document. Body remains fully block-level.
Scope filtering joins the content table and supports Manuscript, Research,
both, and exact-or-descendant tree paths.

User input never enters MATCH as grammar. Every term (or the whole requested
phrase) is quoted and embedded only below an allow-listed field name. Empty
text, NUL, no fields, and zero limits are rejected before dispatch. Tests cover
literal quotes, `OR`, and field-like punctuation. FTS supplies candidates;
case-sensitive and Unicode-aware whole-word behavior is then enforced against
the stored field text before returning a hit.

Results are ordered with `bm25` weights body/title/synopsis/metadata =
1/8/4/2, include SQLite snippets, and carry stable IDs, revision, field, byte
range, matched text, and rank. Result batches stream in groups of 32. Starting
a newer query advances an atomic generation immediately, causing the older
worker query to terminate before the replacement runs. Revalidation reports
current, stale-revision/text, and deleted results.

## Linux full-scale measurements

Host: Intel Core i7-8550U (4 cores/8 threads), 7.1 GiB RAM, Linux
7.0.0-28-generic, x86_64, Rust/Cargo 1.97.1, Ubuntu GCC 15.2.0, C.UTF-8.
Measurements use an optimized release build and 30 warm repetitions per query.
The generator produced exactly 550 documents, 167,074 blocks, and 20,000,000
words. Its interchange manifest digest is `671180a77293afe4`.

| Query | Hits | first p50 | first p95 | first p99 | complete p99 |
|---|---:|---:|---:|---:|---:|
| Plain | 550 | 10.783 ms | 11.024 ms | 11.179 ms | 42.621 ms |
| Phrase | 550 | 9.943 ms | 10.275 ms | 10.994 ms | 22.222 ms |
| Case-sensitive | 550 | 9.297 ms | 9.784 ms | 9.802 ms | 11.600 ms |
| Whole-word | 550 | 11.674 ms | 11.955 ms | 11.979 ms | 52.002 ms |
| Research scope | 50 | 4.059 ms | 4.436 ms | 4.579 ms | 5.635 ms |
| Selected subtree | 10 | 2.143 ms | 2.253 ms | 2.267 ms | 2.269 ms |
| Title field | 550 | 9.059 ms | 9.316 ms | 9.378 ms | 13.055 ms |

The worst observed p99 first-result time was 11.979 ms, leaving substantial
headroom under the 200 ms product target on this host. Full initial index build
was 9.634 s. Replacing the generated 5,000-word and 250,000-word documents took
8.380 ms and 141.120 ms respectively. The post-deletion deterministic rebuild
took 9.360 s. The checkpointed database was 281,280,512 bytes. Process peak RSS
reported by Linux was 11,368 KiB; this is process memory only and excludes the
kernel page cache.

The 120,000-word smoke run is preserved separately and passed the same
semantics. Its worst p99 first-result time was 1.649 ms.

## Required-test outcome

| V04 requirement | Linux outcome |
|---|---|
| Initial full build | Pass, 20,000,000 words |
| Replace small and 250k documents | Pass |
| Plain, phrase, case-sensitive, whole-word | Pass |
| Manuscript/Research and subtree filtering | Pass |
| Streaming first and complete results | Pass |
| Snippets and ranking | Pass |
| Cancellation/replacement | Pass |
| Stale and deleted-result revalidation | Pass |
| Index deletion and deterministic rebuild | Pass |
| External-content consistency and integrity | Pass |
| MATCH escaping and invalid input | Pass |
| Concurrent callers without UI-thread connection | Pass |
| Linux schema/query/corpus smoke | Pass |
| Windows native parity/interchange | Pass, Windows 11 x64 |
| macOS native parity/interchange | Pass, macOS 15 arm64 |

`PRAGMA quick_check` returned `ok`; the FTS5 external-content integrity command
passed with rank 1; and content/index row counts both ended at 167,074 after the
full rebuild.

## Native platform parity

All three native runs used Rust 1.97.1, bundled SQLite 3.53.2, the identical
120,000-word smoke corpus manifest `5571bf5b437bbab0`, and 30 warm samples per
query. All reported the eight required semantic behaviors, dedicated worker,
streaming, cancellation/replacement, stale revalidation, deterministic
rebuild, `quick_check: ok`, equal content/index row counts, and a passing FTS5
external-content integrity command.

| Platform | Build | index build | replace 5k | replace 250k | worst first p99 | rebuild |
|---|---|---:|---:|---:|---:|---:|
| Linux x86_64 | Pass | 32.659 ms | 3.192 ms | 73.950 ms | 1.174 ms | 30.390 ms |
| Windows x86_64 | Pass | 37.703 ms | 3.494 ms | 89.841 ms | 2.488 ms | 36.148 ms |
| macOS arm64 | Pass | 20.886 ms | 1.883 ms | 47.908 ms | 1.121 ms | 19.655 ms |

The Windows/macOS process RSS fields are zero because this headless probe's
current RSS reader supports Linux only; they are unreported memory evidence,
not zero memory. Cross-platform memory was not a V04 smoke gate. The Linux
full-scale run reports 11,368 KiB process peak RSS, excluding page cache.

## Recommendation and residual risk

Select bundled SQLite FTS5 as the production search backend behind the planned
search interface. No concrete semantic, integrity, packaging, or latency
failure authorizes a Tantivy comparison.

The benchmark host was not formally designated by the product owner, so its
encouraging full-scale latencies should be rerun as a non-blocking performance
baseline on the final representative hardware matrix. Windows/macOS memory
instrumentation should also be added before product optimization work.

Raw evidence:

- `reports/raw/v04-sqlite-search.yaml`
- `reports/raw/v04-sqlite-search-full-20m.yaml`
- `reports/raw/v04-linux-validation.yaml`
- `reports/ci/Linux/v04-smoke.yaml`
- `reports/ci/Windows/v04-smoke.yaml`
- `reports/ci/macOS/v04-smoke.yaml`
- `reports/platform/v04-platform-parity.md`
- `reports/supply-chain/v04-search-native-inventory.md`
