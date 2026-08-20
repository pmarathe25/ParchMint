# Stage 4 — test simplification: services and contracts

Scope reviewed (source inspection only; no builds/tests/metadata scans):

- `crates/parchmint-search-sqlite/tests/native_search.rs`
- `crates/parchmint-search-api/src/search_index_contract_tests.rs`
- `crates/parchmint-spellcheck-en-us/src/spellcheck_en_us_contract_tests.rs`
- `crates/parchmint-spellcheck-en-us/src/implementation_tests.rs`
- `crates/parchmint-export-html/src/export_html_contract_tests.rs`
- `crates/parchmint-export-api/src/export_contract_tests.rs`
- `crates/parchmint-contracts/src` (no test module was found there)

## Result

No high-confidence test deletion or scenario merge is justified. The apparent
duplication is mostly deliberate coverage of distinct security inputs, cache
states, Unicode/range behavior, cancellation timing, generation semantics,
schema/contract validation, or project isolation. In particular, do not merge
or weaken the CSS/HTML attack cases, the UTF-8 candidate checks, the search
frontier/background cancellation cases, or the spellcheck project-dictionary
isolation case.

## Narrow safe simplifications

1. **Avoid repeating the deterministic HTML golden assertion**

   In `crates/parchmint-export-html/src/export_html_contract_tests.rs`,
   `export_is_deterministic_golden_html_bytes_and_embeds_project_css` (around
   lines 99–116) renders the same plan twice and compares each render directly
   to the long golden byte string. Keep both renders, but store them in
   `first`/`second`, assert `first == expected`, then assert `second == first`.
   This removes one copy of the expensive/over-specific golden comparison while
   preserving both guarantees: exact escaped HTML/CSS output and deterministic
   repeated rendering.

2. **Factor the repeated per-project suggestion request fixture**

   In `crates/parchmint-spellcheck-en-us/src/spellcheck_en_us_contract_tests.rs`,
   `project_dictionaries_do_not_leak_between_projects_at_same_revision` (around
   lines 151–232) builds two nearly identical `SuggestionRequest` values and
   then repeats the service call/word collection. Add a small local helper
   taking `(document_id, project_id, word, revision)` and return the suggestion
   words; use it for `first` and `second`. Also, if desired, let `check_request`
   accept a `ProjectId` rather than constructing the default and mutating the
   field twice. Keep the two explicit result assertions (first contains the
   saved first-project word; second does not). This reduces fixture noise while
   preserving same-revision project isolation for both checks and suggestions.

## Deliberate non-simplifications

- `native_search.rs` keeps separate literal-FTS, Unicode whole-word/case,
  in-flight cancellation, background rebuild cancellation/publication, rollback,
  and project-worker/cache identity tests. Their synchronization and cache
  states are not interchangeable.
- `search_index_contract_tests.rs` separately models stale revisions, bounded
  batches, cancellation, stale generations, rebuild byte preservation, and
  public UTF-8/range validation. Combining them would hide which contract broke.
- Spellcheck tests separately cover text/dictionary generations,
  cancellation, priority eviction, failed project/global reloads, scalar token
  offsets, request limits, and project isolation. These are distinct regressions.
- HTML export sanitization tests intentionally retain separate safe CSS,
  escaped/comment-obfuscated CSS, unsafe links/attributes, structural page
  breaks, chunking/cancellation, and determinate progress checks.
- Export API tests intentionally retain plan snapshot isolation, inherited
  settings, validation issue combinations, temporary-output cancellation,
  successful commit, and start-failure cleanup.

No production code or tests were modified.
