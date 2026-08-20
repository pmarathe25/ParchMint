# Stage 3 bug audit: services and contracts

Scope reviewed: `parchmint-search-api`, `parchmint-search-sqlite`,
`parchmint-spellcheck-api`, `parchmint-spellcheck-en-us`,
`parchmint-export-api`, `parchmint-export-html`, and `parchmint-contracts`,
including their directly relevant tests. No builds or tests were run.

## Confirmed defects

### 1. Project dictionary words leak across projects with the same revision

**Evidence:** `crates/parchmint-spellcheck-en-us/src/lib.rs`,
`PrivateSpellingRuntime::contains` and `PrivateSpellingRuntime::suggestions`.
`project_words` is stored as `BTreeMap<ProjectId, (DictionaryRevision, ...)>`,
but both lookup functions accept only a `DictionaryRevision` and use
`.values().any(...)` / `.values().filter(...)`. The request's project ID is
never passed through `PrivateSpellingRuntime::check` to these lookups.

**Trigger and impact:** Load custom word `AcmeTerm` for project A at revision
1. Check project B with `project_dictionary: DictionaryRevision::from(1)`.
The word is treated as known (and can be suggested from) project B even though
it was never saved there. Any two projects using the same revision number can
cross-contaminate spellcheck results, causing missing misspelling underlines or
project-specific suggestions.

**Minimal fix:** Thread `request.project_id` through the spellcheck request/runtime
boundary (or otherwise bind the loaded project dictionary to the request's
project), and look up only `project_words.get(&project)` before comparing the
revision. Apply the same keying in `suggestions`.

**Regression test:** Load a unique word into project A at revision 1, check the
same word in project B at revision 1, and assert B reports a `SpellingIssue`;
also assert A does not report one. Repeat for `suggest` if the API fixture can
observe suggestions.

### 2. CSS URL/expression sanitization is bypassable with CSS escapes

**Evidence:** `crates/parchmint-export-html/src/lib.rs`, `sanitize_css`.
It lowercases the source and rejects literal substrings such as `url(`,
`expression(`, and `javascript:`, but does not parse CSS escapes or comments.

**Trigger and impact:** Supply project style text containing, for example,
`a{background:u\\72l(https://attacker.invalid/x)}` (CSS's `\\72l` escape
decodes to `url`) or an equivalent escaped `javascript:` token. The emitted
`<style>` retains it, while a browser's CSS parser resolves the escape and may
make a remote request or execute a script-bearing URL. This violates the
exporter's stated self-contained/sanitized output boundary and permits network
observation or script injection through project CSS.

**Minimal fix:** Sanitize parsed CSS (reject URL-bearing/function/import and
script-capable declarations after CSS escape/comment normalization), or use a
strict allowlist of declarations and values. Literal substring checks are not a
safe CSS sanitizer.

**Regression test:** Export a plan whose style contains escaped `url`, escaped
`javascript:`, and comment-obfuscated forms; assert none survives in the
serialized style block (and ideally parse the resulting stylesheet with the
chosen sanitizer's test API).

## Uncertain observations (not counted as confirmed bugs)

- `parchmint-search-sqlite::Cancellation` keeps cancelled generations in a set
  for the lifetime of the index. Reusing a generation therefore makes future
  work immediately cancel, but the API documents generations as UI-assigned
  request numbers and does not explicitly promise reuse; this needs a product
  decision before calling it a defect.
- `parchmint-contracts::validate_schema_name` reports `expected` as the schema
  ID without `/vN`, although the comparison includes `/vN`; this is a diagnostic
  wording issue only and does not affect acceptance/rejection.
