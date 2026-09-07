# Usability review

**Purpose:** Check whether supported writing tasks are understandable and usable
when changing a flow or preparing a release. Tests establish behavior; this review
adds visual judgment and native input checks. Keep reports and screenshots in a
temporary artifact directory.

## Prepare

1. Read the [driver guide](README.md) and affected tests. Use the pinned toolchain,
   `--locked`, and one compilation job.
2. Create isolated application data and a disposable project with two Manuscript
   documents, one Research document, a group, an empty document, and a comment.
   Save a checkpoint, then edit a sentence, rename a group, and add or delete a
   document. Use distinctive sample text.
3. Drive widgets through `DesktopInteractionHarness` or the JSON Lines driver.
   The Rust harness also supports selection, drag, resizing, and fault injection.
4. Repeat key tasks in the native release executable. Record commit, build command,
   OS, window dimensions, appearance, and scale. Obtain permission when the
   environment requires GUI approval. Report blocked native steps when automation
   is unavailable; headless captures do not establish native input behavior.

Generate screenshots from the checked-in review flows with a new output directory:

```console
PARCHMINT_REVIEW_ARTIFACTS=/tmp/parchmint-review cargo test --locked -j 1 -p parchmint-ui-driver --test usability_flows
```

These cover creation, compact dark appearance, notification expiry and retry,
and project-wide History comparison. Open the PNGs and assess the layout.

## Exercise complete tasks

Capture before and after, perform the task, and assert its data outcome. Use
`text_is_visible` for viewport checks; reserve `contains_text` for widget-content
inspection. Click controls that might be obstructed by overlays.

- **Create and organize:** Select a parent, use **+ New**, name a group and document,
  type, save, close, and reopen. Check destination, names, and saved text. Repeat
  in Research and an empty group.
- **Rearrange:** Drag before and after a sibling, into a group, and onto the current
  location three times. Check order and contents; no-op actions must add neither
  checkpoints nor errors.
- **Review comments:** Create an unsaved comment, move away, and select its Inspector
  entry. Check the anchor, insert text before it, and repeat in both panes. Save
  and reopen.
- **Compare History:** Change several documents and the outline, then select an
  earlier checkpoint. Identify old and new text, additions, deletions, unsaved
  drafts, comments, and an unchanged checkpoint. Opening History must not save
  drafts.
- **Use notifications:** Trigger a notification, operate the workspace beneath it,
  and dismiss it. Repeat with expiry. Inject a recoverable failure, dismiss its
  dialog, wait, and retrieve it from Notifications. Check that controls remain
  clickable and the drawer closes.
- **Handle interrupted work:** Hold completions, type or change panes, and release
  results in both orders. Check focus, text, errors, and saved output. Cancel a
  rename or comment with Escape and continue another task.
- **Change settings:** Edit styles and dictionaries in their available scopes.
  Check draft retention, validation, previews, and saved values after reopening.
- **Export and restore:** Export a manuscript and inspect the file. Preview and
  restore deleted items, a History checkpoint, and interrupted-session edits.
  Check that destinations and consequences are clear before confirming.

Use project windows at 1280 × 720 and 1440 × 900, and the launcher at 900 × 620,
in both appearances. In the native app, check normal and larger display scales.
Include long titles, multiline messages, scrolling lists, and long paragraphs;
repeat checks after scrolling and resizing.

## Judge the result

For each task, cite a screenshot or trace and assess:

- **Discoverability:** Can a new user find the primary action and destination?
  Do duplicate controls suggest different workflows? Count creation entry points.
- **Clarity:** Is the next step clear with minimal text and controls? Can optional
  details stay hidden until needed?
- **Feedback:** Does the result match the action? Are additions and removals clear
  without relying on color alone?
- **Access:** Can the user read and click the next control without clipping,
  overlapping banners, or unexpected scrolling?
- **Continuity:** Do focus, drafts, and selections survive cancellation, delayed
  results, errors, and navigation?
- **Responsiveness:** Record noticeable delays with the input and data size.
  Headless execution time does not establish native latency.

## Record and verify findings

Report each case as **pass**, **fail**, or **blocked**, with actions, expected
behavior linked to tests, actual result, artifact paths, and reasoning. Separate
subjective concerns from demonstrated data failures. Include isolated diagnostic
warnings and errors without copying real user prose.

Follow the [regression reduction procedure](README.md#reduce-a-ui-failure-to-a-regression)
for reproducible failures. After a fix, run the regression and repeat the visual
review. Pixel similarity alone cannot establish usability.
