# Visual Regression Plan

## Environment

- Candidate capture harness: packaged desktop UI fixture harness after S50; load the named fixture through mock/application-service seams without live Penpot.
- Font/runtime controls: 1440x900 logical viewport, scale 1.0, reduced motion off, Source Sans 3/declared fallback availability asserted, deterministic clock/data/scroll/selection, animations settled before capture.
- Native platform captures: shared baseline first on Linux CI; Windows and macOS captures are required later for platform-specific chrome, menus, dialogs, system appearance, fonts, and spelling surfaces.
- Pixel-diff tooling: exact PNG decode/checksum baseline; perceptual diff after mask of native chrome and explicitly variable native controls. Diagnostic threshold is 0.15% changed pixels at per-channel delta 12; any focused/semantic/layout change requires human review regardless of score.
- Review workflow: automated diff plus two-theme human review against hierarchy, spacing, visible state, focus, contrast, truncation, and expected native differences. Approve deviations only through the applicable gate.

## Theme matrix

Every core screen has a Light and Dark baseline. System is validated behaviorally by resolving to each theme; it does not need duplicate visual files. The ten locked pairs below are all 1440x900, scale 1, shared-platform references and use the named deterministic fixture from `specs/reference-fixtures.md`.

## Captures

| Screen ID | Theme | Platform | Width | Height | Scale | Fixture/state | Baseline | Tolerance/notes |
|---|---|---|---:|---:|---:|---|---|---|
| launcher-default | Light | shared | 1440 | 900 | 1 | launcher-default / recent projects | references/light/launcher-light.png | native chrome mask; row focus/name review |
| launcher-default | Dark | shared | 1440 | 900 | 1 | launcher-default / recent projects | references/dark/launcher-dark.png | native chrome mask; row focus/name review |
| editor-single-default | Light | shared | 1440 | 900 | 1 | editor-single-default / primary focused | references/light/editor-single-light.png | inspect toolbar/tab focus and readable column |
| editor-single-default | Dark | shared | 1440 | 900 | 1 | editor-single-default / primary focused | references/dark/editor-single-dark.png | require fully dark manuscript canvas |
| editor-dual-default | Light | shared | 1440 | 900 | 1 | editor-dual-default / companion focused | references/light/editor-dual-light.png | inspect independent-pane visual state |
| editor-dual-default | Dark | shared | 1440 | 900 | 1 | editor-dual-default / companion focused | references/dark/editor-dual-dark.png | inspect focused/unfocused distinction |
| cards-default | Light | shared | 1440 | 900 | 1 | cards-default / expanded hierarchy | references/light/cards-light.png | inspect hierarchy, selection, metadata read-only state |
| cards-default | Dark | shared | 1440 | 900 | 1 | cards-default / expanded hierarchy | references/dark/cards-dark.png | inspect hierarchy and contrast |
| global-search-default | Light | shared | 1440 | 900 | 1 | global-search-default / query entry | references/light/global-search-light.png | Explorer replaced; no scope selector |
| global-search-default | Dark | shared | 1440 | 900 | 1 | global-search-default / query entry | references/dark/global-search-dark.png | search highlight and contrast review |
| history-default | Light | shared | 1440 | 900 | 1 | history-default / session date grouped | references/light/history-light.png | word-level comparison and whole-project restore wording |
| history-default | Dark | shared | 1440 | 900 | 1 | history-default / session date grouped | references/dark/history-dark.png | word-level comparison/contrast review |
| settings-appearance-default | Light | shared | 1440 | 900 | 1 | settings-appearance-default / System selected | references/light/settings-appearance-light.png | radio state and System behavior review |
| settings-appearance-default | Dark | shared | 1440 | 900 | 1 | settings-appearance-default / System selected | references/dark/settings-appearance-dark.png | radio/focus state and dark surface review |
| export-default | Light | shared | 1440 | 900 | 1 | export-default / Entire Manuscript | references/light/export-light.png | no partial-scope control permitted |
| export-default | Dark | shared | 1440 | 900 | 1 | export-default / Entire Manuscript | references/dark/export-dark.png | no partial-scope control permitted |
| error-recovery-default | Light | shared | 1440 | 900 | 1 | error-recovery-default / recovered after crash | references/light/error-recovery-light.png | dialog/error semantic review |
| error-recovery-default | Dark | shared | 1440 | 900 | 1 | error-recovery-default / recovered after crash | references/dark/error-recovery-dark.png | dialog/error semantic review |
| recently-deleted-default | Light | shared | 1440 | 900 | 1 | recently-deleted-default / list and preview | references/light/recently-deleted-light.png | destructive/restore distinctions |
| recently-deleted-default | Dark | shared | 1440 | 900 | 1 | recently-deleted-default / list and preview | references/dark/recently-deleted-dark.png | destructive/restore distinctions |

## Semantic checks

- Layout hierarchy, spacing, resizable panes, disclosure, truncation, wrapping, and 1280x720 minimum contract.
- Focus/selection/active/unfocused and keyboard state; no color-only indication.
- Dark manuscript canvas, semantic token consumption, and Appearance propagation without authored-content/output change.
- Search, comments, spellcheck, save/error/recovery, history restore, and Entire Manuscript export constraints.
- Native menus/file dialogs/window chrome/font/spelling are separately captured and reviewed on Windows, macOS, and Linux; they are not pixel-forced to shared baselines.
