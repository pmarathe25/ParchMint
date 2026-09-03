# ParchMint Screen Catalog

This catalog maps the stable names used by the Penpot design, generated design
tokens, and visual tests. The other UI design pages explain how the components
and screens behave.

The editor's active Light or Dark theme metadata is non-authoritative: it records
an editor selection only. The complete Light and Dark semantic token sets are
both canonical; System resolves to one of them at runtime.

## Light/Dark baseline fixtures

Each fixture is a 1440 × 900 logical-pixel, scale-1, shared-platform state with
one Light and one Dark reference. The pair names below are stable reference IDs.

| Fixture ID | Light / Dark reference IDs | Intended reproducible state |
|---|---|---|
| `launcher-default` | `launcher-light` / `launcher-dark` | Launcher with two recent-project entries showing name, path, and last-opened time; first launch has no entries. |
| `editor-single-default` | `editor-single-light` / `editor-single-dark` | Manuscript → Chapter One is focused in one primary pane; Research has notes; Explorer, Inspector, and status bar are shown. |
| `editor-dual-default` | `editor-dual-light` / `editor-dual-dark` | Chapter One and Chapter Two are separate Manuscript documents in primary and companion panes, each with a tab strip; the companion is focused. |
| `cards-default` | `cards-light` / `cards-dark` | Manuscript and Research hierarchy with expanded groups and synopsis-density document cards. |
| `global-search-default` | `global-search-light` / `global-search-dark` | Project-wide query state with Global Search replacing Explorer; no scope selector. |
| `history-default` | `history-light` / `history-dark` | Session/date-grouped checkpoints, a named snapshot, and whole-project restore comparison with word-level changes. |
| `settings-appearance-default` | `settings-appearance-light` / `settings-appearance-dark` | Appearance settings with System selected; explicit Light/Dark overrides are separate product states. |
| `export-default` | `export-project-output-controls-light` / `export-project-output-controls-dark` | Four-document Manuscript with fixed Entire Manuscript export, title/page-break, numbering, destination, progress, success, and failure states. |
| `error-recovery-default` | `error-recovery-light` / `error-recovery-dark` | Recovery after a crash replays valid unsaved edits on top of the last completed autosave, focuses the editor after acceptance, and keeps recovery state disposable after durable save. |
| `recently-deleted-default` | `recently-deleted-light` / `recently-deleted-dark` | Several deleted documents with formatted preview and a confirmable fallback destination when the original parent is unavailable. |

The same-document dual-view state is intentionally separate from
`editor-dual-default`: `editor-same-document-two-views-light` uses one
document in both panes with independent view state.

## Shared component mains (79)

| PM name | Penpot main ID |
|---|---|
| `PM/AppearanceChoice` | `469ffc7d-964a-806d-8008-6c9283050cac` |
| `PM/Button` | `e96ec683-a782-802c-8008-65f522b059ea` |
| `PM/Checkbox` | `e96ec683-a782-802c-8008-65f59d52590f` |
| `PM/CommentAnchorState` | `e96ec683-a782-802c-8008-65f6958ed787` |
| `PM/CommentMessage` | `e96ec683-a782-802c-8008-65f693791b92` |
| `PM/CommentRepliesToggle` | `c5362ef2-ec03-8060-8008-69aa626ca753` |
| `PM/CommentReplyComposer` | `e96ec683-a782-802c-8008-65f694827b40` |
| `PM/CommentThread` | `e96ec683-a782-802c-8008-65f6926d4e0f` |
| `PM/ContextMenu` | `e96ec683-a782-802c-8008-65f5f217ce21` |
| `PM/ContextMenuDivider` | `c5362ef2-ec03-8060-8008-69aa6247d6ae` |
| `PM/ContextMenuItem` | `c5362ef2-ec03-8060-8008-69aa620e9593` |
| `PM/ContextMenuSurface` | `c5362ef2-ec03-8060-8008-69aa61d813e3` |
| `PM/Tree/CutState` | `e96ec683-a782-802c-8008-65f5ef230290` |
| `PM/DeletedItem` | `e96ec683-a782-802c-8008-65f700c45012` |
| `PM/DeletedItemPreview` | `039701d9-7e2b-8031-8008-676c2bcc6956` |
| `PM/DictionarySettings` | `e96ec683-a782-802c-8008-66077f2960a3` |
| `PM/Disclosure` | `e96ec683-a782-802c-8008-65f5a00e8a95` |
| `PM/Card/Document` | `e96ec683-a782-802c-8008-65f699c6475e` |
| `PM/EditorCanvas` | `e96ec683-a782-802c-8008-65f641eca84d` |
| `PM/EditorContextMenu` | `e96ec683-a782-802c-8008-66077a35ef6c` |
| `PM/EditorPane` | `e96ec683-a782-802c-8008-65f64362a649` |
| `PM/EditorPaneHeader` | `e96ec683-a782-802c-8008-660773416c6c` |
| `PM/EmptyState` | `e96ec683-a782-802c-8008-65f5a56d7081` |
| `PM/ErrorBanner` | `e96ec683-a782-802c-8008-65f709a7779e` |
| `PM/ExportDialog` | `e96ec683-a782-802c-8008-65f7076a5ebf` |
| `PM/FocusVisible` | `e96ec683-a782-802c-8008-65f5a74ac4f2` |
| `PM/FormattingToolbar` | `e96ec683-a782-802c-8008-65f644651aae` |
| `PM/GlobalSearchPanel` | `e96ec683-a782-802c-8008-65f6f12da672` |
| `PM/TreeRow/Group` | `e96ec683-a782-802c-8008-65f5ec3722d4` |
| `PM/HistoryCompare` | `e96ec683-a782-802c-8008-65f6fd4e533d` |
| `PM/HistoryEntry` | `e96ec683-a782-802c-8008-65f6fa0243cf` |
| `PM/HistoryPreview` | `e96ec683-a782-802c-8008-65f6fc338bd1` |
| `PM/HistoryTimeline` | `e96ec683-a782-802c-8008-65f6f8ec20ee` |
| `PM/InlineRename` | `e96ec683-a782-802c-8008-65f5f11a916f` |
| `PM/Card/InsertionMarker` | `e96ec683-a782-802c-8008-65f6bfdd12c6` |
| `PM/Inspector` | `e96ec683-a782-802c-8008-65f5e1526037` |
| `PM/InspectorSection` | `e96ec683-a782-802c-8008-65f68f2c7a00` |
| `PM/LauncherProjectCard` | `e96ec683-a782-802c-8008-65f701e0505c` |
| `PM/LoadingState` | `e96ec683-a782-802c-8008-65f5a65dd2a8` |
| `PM/LocalFindBar` | `e96ec683-a782-802c-8008-65f6476c9d47` |
| `PM/MetadataDefinitionEditor` | `e96ec683-a782-802c-8008-65f7064e136b` |
| `PM/MetadataField` | `e96ec683-a782-802c-8008-65f69160b6a2` |
| `PM/Card/MetadataValue` | `e96ec683-a782-802c-8008-65f6c1031f26` |
| `PM/Tree/MultiSelection` | `e96ec683-a782-802c-8008-65f5f01ca60e` |
| `PM/MultilineField` | `e96ec683-a782-802c-8008-65f59b7d7f49` |
| `PM/NamedSnapshot` | `e96ec683-a782-802c-8008-65f6fb1b86ee` |
| `PM/NewProjectDialog` | `e96ec683-a782-802c-8008-65f702f8240b` |
| `PM/OrphanedComment` | `e96ec683-a782-802c-8008-65f69699fe2b` |
| `PM/AtomicBreak/Page` | `e96ec683-a782-802c-8008-65f64a806611` |
| `PM/ProgressState` | `e96ec683-a782-802c-8008-65f70887ed14` |
| `PM/RecentlyDeletedList` | `e96ec683-a782-802c-8008-65f6ffa154ca` |
| `PM/RecoveryDialog` | `e96ec683-a782-802c-8008-65f70ac67a6f` |
| `PM/ReplacePreview` | `e96ec683-a782-802c-8008-65f6f5a72f9f` |
| `PM/ReplacePreviewRow` | `e96ec683-a782-802c-8008-65f6f6bd4043` |
| `PM/ReplacementSelectionControl` | `e96ec683-a782-802c-8008-65f6f7d76096` |
| `PM/RestoreDialog` | `e96ec683-a782-802c-8008-65f6fe661b3c` |
| `PM/TreeRow/Root` | `e96ec683-a782-802c-8008-65f5eb2cd3f9` |
| `PM/AtomicBreak/Scene` | `e96ec683-a782-802c-8008-65f6497d189b` |
| `PM/SearchMatch` | `469ffc7d-964a-806d-8008-6c9699f2bd9f` |
| `PM/SearchResult` | `e96ec683-a782-802c-8008-65f6f48f6b39` |
| `PM/SearchResultGroup` | `e96ec683-a782-802c-8008-65f6f3773e63` |
| `PM/Select` | `e96ec683-a782-802c-8008-65f59c69c917` |
| `PM/SettingsNav` | `469ffc7d-964a-806d-8008-6c952fec7805` |
| `PM/Sidebar` | `e96ec683-a782-802c-8008-65f5e0564ae5` |
| `PM/SpellcheckUnderline` | `e96ec683-a782-802c-8008-66077bd31eb2` |
| `PM/SpellingContextMenu` | `e96ec683-a782-802c-8008-66077d6e0583` |
| `PM/Splitter` | `e96ec683-a782-802c-8008-65f5e247224c` |
| `PM/StatusBar` | `e96ec683-a782-802c-8008-65f5e7195d18` |
| `PM/StyleActionDialog` | `c6b492ab-8095-8078-8008-66f598c25172` |
| `PM/StyleEditor` | `e96ec683-a782-802c-8008-65f7052e0f41` |
| `PM/StyleSelect` | `e96ec683-a782-802c-8008-65f64567de5c` |
| `PM/SynopsisEditor` | `e96ec683-a782-802c-8008-65f69039ae10` |
| `PM/Tab` | `e96ec683-a782-802c-8008-65f5e620057d` |
| `PM/TextField` | `e96ec683-a782-802c-8008-65f59a903029` |
| `PM/Toast` | `e96ec683-a782-802c-8008-65f5a47e7859` |
| `PM/Tooltip` | `e96ec683-a782-802c-8008-65f5a0f731ac` |
| `PM/Tree` | `e96ec683-a782-802c-8008-65f5ea0be98b` |
| `PM/TreeRow/ActiveDocument` | `e96ec683-a782-802c-8008-660778942c0f` |
| `PM/WorkspaceTopBar/Export` | `e96ec683-a782-802c-8008-6607f362d598` |

## Screen mappings (80)

A `PM / Screen /` entry identifies a screen or visible state. `fixture_id`
identifies the reproducible content and view state used by visual tooling.

| Screen ID | PM name | Source page | Penpot board ID | Fixture ID |
|---|---|---|---|---|
| `cards-collapsed-groups-light` | `PM / Screen / cards-collapsed-groups` | 05 Cards Workspace | `e96ec683-a782-802c-8008-65f92923c1da` | `cards-collapsed-groups-default` |
| `cards-deep-expanded-light` | `PM / Screen / cards-deep-expanded` | 05 Cards Workspace | `e96ec683-a782-802c-8008-65f91ad51384` | `cards-deep-expanded-default` |
| `cards-density-compact-light` | `PM / Screen / cards-density-compact · Production` | 05 Cards Workspace | `e96ec683-a782-802c-8008-65f93f232290` | `cards-density-compact-default` |
| `cards-drag-multiselect-light` | `PM / Screen / cards-drag-multiselect` | 05 Cards Workspace | `e96ec683-a782-802c-8008-65f95dbbe446` | `cards-drag-multiselect-default` |
| `cards-dark` | `PM / Screen / cards-manuscript-default` | 05 Cards Workspace | `e96ec683-a782-802c-8008-65f906f35093` | `cards-default` |
| `cards-light` | `PM / Screen / cards-manuscript-default` | 05 Cards Workspace | `e96ec683-a782-802c-8008-65f906f35093` | `cards-default` |
| `cards-research-selected-light` | `PM / Screen / cards-research-selected` | 05 Cards Workspace | `e96ec683-a782-802c-8008-65f9119c4804` | `cards-research-selected-default` |
| `close-save-failure-light` | `PM / Screen / close-save-failure` | 10 Export & Save States | `e96ec683-a782-802c-8008-65fb28e7bd3d` | `close-save-failure-default` |
| `comments-orphaned-light` | `PM / Screen / comments-orphaned` | 07 Comments & Inspector | `e96ec683-a782-802c-8008-65fa19493512` | `comments-orphaned-default` |
| `comments-replies-collapsed-light` | `PM / Screen / comments-replies-collapsed` | 07 Comments & Inspector | `e96ec683-a782-802c-8008-65f9fa8f73f4` | `comments-replies-collapsed-default` |
| `comments-replies-expanded-composer-light` | `PM / Screen / comments-replies-expanded-composer` | 07 Comments & Inspector | `e96ec683-a782-802c-8008-65fa002ff390` | `comments-replies-expanded-composer-default` |
| `comments-resolved-filter-light` | `PM / Screen / comments-resolved-thread` | 07 Comments & Inspector | `e96ec683-a782-802c-8008-65fa061c369c` | `comments-resolved-filter-default` |
| `comments-unresolved-thread-light` | `PM / Screen / comments-unresolved-thread` | 07 Comments & Inspector | `e96ec683-a782-802c-8008-65f9f48c32a9` | `comments-unresolved-thread-default` |
| `corrupt-canonical-file-light` | `PM / Screen / corrupt-canonical-file` | 11 Empty, Loading & Recovery | `e96ec683-a782-802c-8008-65fb5463b665` | `corrupt-canonical-file-default` |
| `create-project-dialog-light` | `PM / Screen / create-project-dialog` | 03 Launcher & Project Creation | `e96ec683-a782-802c-8008-65f78b1ad42a` | `create-project-dialog-default` |
| `editor-both-sidebars-collapsed-light` | `PM / Screen / editor-both-sidebars-collapsed` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f80715d743` | `editor-both-sidebars-collapsed-default` |
| `editor-context-menu-add-comment-light` | `PM / Screen / editor-context-menu-add-comment` | 04 Editor Workspace | `e96ec683-a782-802c-8008-660965c89f60` | `editor-context-menu-add-comment-default` |
| `editor-document-loading-light` | `PM / Screen / editor-document-loading` | 11 Empty, Loading & Recovery | `c6b492ab-8095-8078-8008-6702042ef83c` | `editor-document-loading-default` |
| `editor-dual-manuscript-research-left-focus-light` | `PM / Screen / editor-dual-manuscript-research-left-focus` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f86db30e62` | `editor-dual-manuscript-research-left-focus-default` |
| `editor-dual-manuscript-research-right-focus-light` | `PM / Screen / editor-dual-manuscript-research-right-focus` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f879cd4b83` | `editor-dual-manuscript-research-right-focus-default` |
| `editor-dual-dark` | `PM / Screen / editor-dual-two-manuscript` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f886281b72` | `editor-dual-default` |
| `editor-dual-light` | `PM / Screen / editor-dual-two-manuscript` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f886281b72` | `editor-dual-default` |
| `editor-explorer-collapsed-light` | `PM / Screen / editor-explorer-collapsed` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f80de435f3` | `editor-explorer-collapsed-default` |
| `editor-inspector-collapsed-light` | `PM / Screen / editor-inspector-collapsed` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f815dec29f` | `editor-inspector-collapsed-default` |
| `editor-local-find-light` | `PM / Screen / editor-local-find` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f8414ccd0f` | `editor-local-find-default` |
| `editor-local-replace-light` | `PM / Screen / editor-local-replace` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f84b13b6c6` | `editor-local-replace-default` |
| `editor-long-comments-light` | `PM / Screen / editor-long-comments` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f83837a5e5` | `editor-long-comments-default` |
| `editor-same-document-two-views-light` | `PM / Screen / editor-same-document-two-views` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f89294547e` | `editor-same-document-two-views-default` |
| `editor-semantic-content-and-breaks-light` | `PM / Screen / editor-semantic-content-and-breaks` | 04 Editor Workspace | `c6b492ab-8095-8078-8008-6701b08cbb40` | `editor-semantic-content-and-breaks-default` |
| `editor-single-dark` | `PM / Screen / editor-single-default` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f7fdf858b9` | `editor-single-default` |
| `editor-single-light` | `PM / Screen / editor-single-default` | 04 Editor Workspace | `e96ec683-a782-802c-8008-65f7fdf858b9` | `editor-single-default` |
| `editor-spellcheck-suggestions-light` | `PM / Screen / editor-spellcheck-suggestions` | 04 Editor Workspace | `e96ec683-a782-802c-8008-6609938011df` | `editor-spellcheck-suggestions-default` |
| `empty-manuscript-research-roots-light` | `PM / Screen / empty-manuscript-research-roots` | 11 Empty, Loading & Recovery | `e96ec683-a782-802c-8008-65fb64349f88` | `empty-manuscript-research-roots-default` |
| `explorer-context-menu-actions-light` | `PM / Screen / explorer-context-menu-actions` | 04 Editor Workspace | `e96ec683-a782-802c-8008-660966256515` | `explorer-context-menu-actions-default` |
| `export-entire-manuscript-light` | `PM / Screen / export-entire-manuscript` | 10 Export & Save States | `e96ec683-a782-802c-8008-65faeaa3e09e` | `export-entire-manuscript-default` |
| `export-failure-light` | `PM / Screen / export-failure` | 10 Export & Save States | `e96ec683-a782-802c-8008-65fb074b023d` | `export-failure-default` |
| `export-numbering-light` | `PM / Screen / export-numbering` | 10 Export & Save States | `e96ec683-a782-802c-8008-65fb01630314` | `export-numbering-default` |
| `export-progress-light` | `PM / Screen / export-progress` | 10 Export & Save States | `c5362ef2-ec03-8060-8008-68aaf1104250` | `export-progress-default` |
| `export-project-output-controls-dark` | `PM / Screen / export-project-output-controls` | 10 Export & Save States | `e96ec683-a782-802c-8008-65faf34784d8` | `export-default` |
| `export-project-output-controls-light` | `PM / Screen / export-project-output-controls` | 10 Export & Save States | `e96ec683-a782-802c-8008-65faf34784d8` | `export-default` |
| `export-success-light` | `PM / Screen / export-success` | 10 Export & Save States | `e96ec683-a782-802c-8008-65fb047acc6d` | `export-success-default` |
| `global-replace-preview-exclusions-light` | `PM / Screen / global-replace-preview-exclusions` | 06 Search & Replace | `e96ec683-a782-802c-8008-65f9c16a2709` | `global-replace-preview-exclusions-default` |
| `history-named-snapshot-light` | `PM / Screen / history-named-snapshot` | 08 History & Recently Deleted | `e96ec683-a782-802c-8008-65fa5be73781` | `history-named-snapshot-default` |
| `history-restore-project-light` | `PM / Screen / history-restore-checkpoint` | 08 History & Recently Deleted | `e96ec683-a782-802c-8008-65fa75e1d788` | `history-restore-project-default` |
| `history-dark` | `PM / Screen / history-session-date-grouped` | 08 History & Recently Deleted | `e96ec683-a782-802c-8008-65fa56f4c808` | `history-default` |
| `history-light` | `PM / Screen / history-session-date-grouped` | 08 History & Recently Deleted | `e96ec683-a782-802c-8008-65fa56f4c808` | `history-default` |
| `history-unavailable-light` | `PM / Screen / history-unavailable` | 11 Empty, Loading & Recovery | `e96ec683-a782-802c-8008-65fb4bdc4cf4` | `history-unavailable-default` |
| `inspector-group-no-comments-light` | `PM / Screen / inspector-group-no-comments` | 07 Comments & Inspector | `e96ec683-a782-802c-8008-65fa1f2ce4b0` | `inspector-group-no-comments-default` |
| `inspector-many-metadata-fields-light` | `PM / Screen / inspector-many-metadata-fields` | 07 Comments & Inspector | `e96ec683-a782-802c-8008-65fa2a7463fd` | `inspector-many-metadata-fields-default` |
| `inspector-no-metadata-fields-light` | `PM / Screen / inspector-no-metadata-fields` | 07 Comments & Inspector | `e96ec683-a782-802c-8008-65fa24b8839c` | `inspector-no-metadata-fields-default` |
| `launcher-first-launch-light` | `PM / Screen / launcher-first-launch` | 03 Launcher & Project Creation | `e96ec683-a782-802c-8008-65f7642df84d` | `launcher-first-launch-default` |
| `launcher-locked-project-light` | `PM / Screen / launcher-locked-project` | 03 Launcher & Project Creation | `e96ec683-a782-802c-8008-65f770ca73a0` | `launcher-locked-project-default` |
| `launcher-missing-project-light` | `PM / Screen / launcher-missing-project` | 03 Launcher & Project Creation | `e96ec683-a782-802c-8008-65f76cdbc050` | `launcher-missing-project-default` |
| `launcher-dark` | `PM / Screen / launcher-recent` | 03 Launcher & Project Creation | `e96ec683-a782-802c-8008-65f767f20cfd` | `launcher-default` |
| `launcher-light` | `PM / Screen / launcher-recent` | 03 Launcher & Project Creation | `e96ec683-a782-802c-8008-65f767f20cfd` | `launcher-default` |
| `lost-comment-anchor-light` | `PM / Screen / lost-comment-anchor` | 11 Empty, Loading & Recovery | `e96ec683-a782-802c-8008-65fb59b02e93` | `lost-comment-anchor-default` |
| `project-format-migration-light` | `PM / Screen / project-format-migration` | 11 Empty, Loading & Recovery | `e96ec683-a782-802c-8008-65fb51954307` | `project-format-migration-default` |
| `recently-deleted-dark` | `PM / Screen / recently-deleted` | 08 History & Recently Deleted | `e96ec683-a782-802c-8008-65fa7a7978c7` | `recently-deleted-default` |
| `recently-deleted-light` | `PM / Screen / recently-deleted` | 08 History & Recently Deleted | `e96ec683-a782-802c-8008-65fa7a7978c7` | `recently-deleted-default` |
| `recently-deleted-fallback-location-light` | `PM / Screen / recently-deleted-fallback-location` | 08 History & Recently Deleted | `e96ec683-a782-802c-8008-65fa7f1e374c` | `recently-deleted-fallback-location-default` |
| `error-recovery-dark` | `PM / Screen / recovered-after-crash` | 11 Empty, Loading & Recovery | `e96ec683-a782-802c-8008-65fb6192c697` | `error-recovery-default` |
| `error-recovery-light` | `PM / Screen / recovered-after-crash` | 11 Empty, Loading & Recovery | `e96ec683-a782-802c-8008-65fb6192c697` | `error-recovery-default` |
| `search-index-rebuilding-light` | `PM / Screen / search-index-rebuilding` | 11 Empty, Loading & Recovery | `e96ec683-a782-802c-8008-65fb4efce34b` | `search-index-rebuilding-default` |
| `search-no-results-light` | `PM / Screen / search-no-results` | 06 Search & Replace | `e96ec683-a782-802c-8008-65f9a2cffacf` | `search-no-results-default` |
| `global-search-dark` | `PM / Screen / search-query-entry` | 06 Search & Replace | `e96ec683-a782-802c-8008-65f9972c44e6` | `global-search-default` |
| `global-search-light` | `PM / Screen / search-query-entry` | 06 Search & Replace | `e96ec683-a782-802c-8008-65f9972c44e6` | `global-search-default` |
| `search-result-navigation-light` | `PM / Screen / search-result-navigation` | 06 Search & Replace | `e96ec683-a782-802c-8008-65f9bbc1b569` | `search-result-navigation-default` |
| `search-stale-deleted-results-light` | `PM / Screen / search-stale-deleted-results` | 06 Search & Replace | `e96ec683-a782-802c-8008-65f9c73f12a6` | `search-stale-deleted-results-default` |
| `search-streaming-results-light` | `PM / Screen / search-streaming-results` | 06 Search & Replace | `e96ec683-a782-802c-8008-65f99d3542bd` | `search-streaming-results-default` |
| `settings-appearance-dark-override-light` | `PM / Screen / settings-appearance-dark-override` | 09 Project Settings & Appearance | `469ffc7d-964a-806d-8008-6c33b6cca37b` | `settings-appearance-dark-override-default` |
| `settings-appearance-light-override-light` | `PM / Screen / settings-appearance-light-override` | 09 Project Settings & Appearance | `a75ba61d-99f5-8033-8008-6d4dd58f3557` | `settings-appearance-light-override-default` |
| `settings-appearance-dark` | `PM / Screen / settings-appearance-system` | 09 Project Settings & Appearance | `469ffc7d-964a-806d-8008-6c33b32674a9` | `settings-appearance-default` |
| `settings-appearance-light` | `PM / Screen / settings-appearance-system` | 09 Project Settings & Appearance | `469ffc7d-964a-806d-8008-6c33b32674a9` | `settings-appearance-default` |
| `settings-delete-unused-style-light` | `PM / Screen / settings-delete-unused-style` | 09 Project Settings & Appearance | `e96ec683-a782-802c-8008-65fab0e9731e` | `settings-delete-unused-style-default` |
| `settings-dictionaries-light` | `PM / Screen / settings-dictionaries` | 09 Project Settings & Appearance | `e96ec683-a782-802c-8008-65fac637bfea` | `settings-dictionaries-default` |
| `settings-general-light` | `PM / Screen / settings-general` | 09 Project Settings & Appearance | `e96ec683-a782-802c-8008-65faa931a6ee` | `settings-general-default` |
| `settings-metadata-fields-light` | `PM / Screen / settings-metadata-fields` | 09 Project Settings & Appearance | `e96ec683-a782-802c-8008-65fac2af3c48` | `settings-metadata-fields-default` |
| `settings-replace-in-use-style-light` | `PM / Screen / settings-replace-in-use-style` | 09 Project Settings & Appearance | `e96ec683-a782-802c-8008-65fab46a9e2b` | `settings-replace-in-use-style-default` |
| `settings-styles-inheritance-light` | `PM / Screen / settings-styles-inheritance` | 09 Project Settings & Appearance | `e96ec683-a782-802c-8008-65faad3d6d8c` | `settings-styles-inheritance-default` |
| `unsupported-pasted-image-light` | `PM / Screen / unsupported-pasted-image` | 11 Empty, Loading & Recovery | `e96ec683-a782-802c-8008-65fb56ee8f75` | `unsupported-pasted-image-default` |

## Page-13 platform and layout references (7)

These are design references, not product screens or destinations.

| Reference ID | PM name | Size / platform | Penpot board ID |
|---|---|---|---|
| `platform-windows-reference` | `PM / Reference / Platform / Windows · linked` | 1440 × 900 / windows | `c5362ef2-ec03-8060-8008-68ad0591c1f0` |
| `platform-macos-reference` | `PM / Reference / Platform / macOS · linked` | 1440 × 900 / macOS | `c5362ef2-ec03-8060-8008-68ad06fe7e7e` |
| `platform-linux-reference` | `PM / Reference / Platform / Linux · linked` | 1440 × 900 / linux | `c5362ef2-ec03-8060-8008-68ad0874c501` |
| `layout-1280x720-reference` | `PM / Reference / Layout / layout-1280x720 · linked` | 1280 × 720 / shared | `c5362ef2-ec03-8060-8008-68ac7f8d72b3` |
| `layout-1440x900-reference` | `PM / Reference / Layout / layout-1440x900 · linked` | 1440 × 900 / shared | `c5362ef2-ec03-8060-8008-68ac809dae04` |
| `layout-1920x1080-reference` | `PM / Reference / Layout / layout-1920x1080 · linked` | 1920 × 1080 / shared | `c5362ef2-ec03-8060-8008-68acee88e488` |
| `layout-2560x1440-reference` | `PM / Reference / Layout / layout-2560x1440 · linked` | 2560 × 1440 / shared | `c5362ef2-ec03-8060-8008-68acefeb81c9` |
