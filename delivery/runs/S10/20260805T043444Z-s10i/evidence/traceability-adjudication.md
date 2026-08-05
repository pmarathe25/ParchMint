# S10 Traceability Adjudication

The failed validation run remains immutable. Its aggregate observation that
traceability had unmapped rows correctly identified a repair need, but two
named examples were false positives:

- `SAVE-014` was already mapped to six screen boards and `PM/RecoveryDialog`.
- `EXP-009` was already mapped to six screen boards, `PM/ExportDialog`, and
  `PM/Toast`.

`WS-011` was genuinely blank. This repair maps it to the Page 13
`layout-1280x720` reference board `c5362ef2-ec03-8060-8008-68ac7f8d72b3`.
That reference does not prove native resize prevention; its traceability note
and cross-platform specification retain that limitation.

Directly represented requirements are annotated in the component/screen
inventories before copied to traceability: `PRJ-010`, `WS-011`, `APPR-006`,
`FMT-019`, `META-011`, `CARD-010`, `WORD-002`, `SPELL-004`, `SPELL-005`,
`A11Y-005`, `A11Y-007`, and `A11Y-009`.

For spellcheck, `SPELL-004` maps only to `PM/SpellingContextMenu`'s existing
`dictionaryerror` state. `PM/ErrorBanner` has no spellcheck state and
`PM/SpellcheckUnderline` represents a misspelling rather than a service
failure, so neither is used for `SPELL-004`. `SPELL-005` remains mapped to the
underline and word-anchored spelling context menu.

Every remaining row without a Penpot ID is classified in
`requirement-dispositions.yaml`. No visual mapping is fabricated for deferred,
nonvisual, performance, security, or native-only requirements.
