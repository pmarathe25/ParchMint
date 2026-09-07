# Agent usability review

Run this review when changing authoring flows or preparing a release. Automated
tests define supported behavior. This procedure asks an agent to exercise those
behaviors and judge whether the visible result is understandable and usable.
Keep observations and screenshots in a temporary artifact directory.

## Prepare the run

1. Read the [driver README](README.md) and the tests for the affected flow.
   Build with the pinned toolchain, `--locked`, and one compilation job.
2. Create an isolated application-data directory and a disposable project.
   Use short, distinctive text in two Manuscript documents and one Research
   document. Include a group, an empty document, and a comment. Save a checkpoint,
   then change a sentence, rename a group, and add or delete a document.
3. Drive the production widgets through `DesktopInteractionHarness`, or use the
   JSON Lines command from the README. The Rust harness exposes more gestures,
   including selection, drag, resizing, and fault injection.
4. Repeat the important flows in the native release executable. Use isolated
   application data and the same disposable project. Record the commit, build
   command, platform, window dimensions, appearance, and display scale. Obtain
   permission when required to launch the GUI. Mark native steps blocked if no
   desktop automation is available; headless screenshots do not prove native
   input behavior.

The checked-in usability flows can produce review screenshots:

```console
PARCHMINT_REVIEW_ARTIFACTS=/tmp/parchmint-review cargo test --locked -j 1 -p parchmint-ui-driver --test usability_flows
```

Use a new output directory for each run. These flows cover creation, compact dark
appearance, notification expiry and retry, and a project-wide History comparison.
Open the PNGs and record your judgment; passing tests do not approve their layout.

## Exercise complete tasks

For each task, capture the starting screen, perform the actions, capture the
result, and assert the data outcome. Check the current viewport with
`text_is_visible`. Use `contains_text` only when inspecting widget content is
intentional. Screenshots preserve the harness's live scroll and overlay state.

| Task | Actions and evidence |
| --- | --- |
| Create and organize | Select a parent, use **+ New**, name a group and a document, type, save, close, and reopen. Verify the destination, names, and saved text. Repeat in Research and with an empty group. |
| Rearrange | Drag a document before and after a sibling, into a group, and onto its current location three times. Verify order and contents; harmless repeated actions must not add checkpoints or errors. |
| Review a comment | Create an unsaved comment, move the caret away, click the Inspector entry, and verify the selected anchor. Insert text before the anchor and repeat. Try both panes, then save and reopen. |
| Understand a checkpoint | Select the earlier checkpoint after changing several documents and the outline. Identify the old and new sentence without opening a separate editor. Check added and deleted documents, an unsaved draft, comments, and an unchanged checkpoint. Verify that opening History does not save the draft. |
| Clear transient messages | Trigger a successful structural action, use the workspace while its banner is visible, then dismiss it. Repeat and advance notification time. Inject a recoverable failure, dismiss its dialog, wait, and retrieve the error from Notifications. Verify controls remain clickable and the drawer can close. |
| Resume interrupted work | Hold completion delivery, continue typing or change panes, then release in both orders. Confirm focus, text, errors, and saved output. Cancel a rename or comment with Escape and immediately perform another task. |

Run project windows at their supported minimum of 1280 × 720 and at 1440 × 900,
in light and dark appearance. Check the launcher at 900 × 620. In the native app,
also check the platform's normal scale and one larger text/display scale. Use a
long title, a multiline message, enough rows to scroll, and a long paragraph.
Check the app after scrolling and resizing, not only on initial load.

## Judge the visible result

For each task, answer these questions in plain language and cite a screenshot or
action trace. A reasoned failure is useful even when all assertions pass.

- **Discoverability:** Can a new user identify the primary action and destination
  from the screen? Are repeated controls equivalent shortcuts, or do they suggest
  competing workflows? Count the visible primary creation entry points.
- **Feedback:** Is completion visible? Does the result match the action? Can the
  user tell what a diff removes and adds without relying on color alone?
- **Access:** Can the user read and operate the next control? Look for clipping,
  overlapping banners, inaccessible dismiss controls, and unexpected scrolling.
  Click the affected control; a text selector alone is insufficient.
- **Continuity:** Does focus stay where typing is expected? Do cancellation,
  delayed results, error recovery, and route changes preserve the draft?
- **Responsiveness:** Is typing or navigation noticeably delayed? Record the
  input, data size, and observed delay. Do not label a run fast from test duration
  alone, and do not infer native latency from headless execution.

## Record and harden findings

For each case, report **pass**, **fail**, or **blocked**, with actions, expected
outcome linked to a test, actual result, screenshot paths, and the reason for the
judgment. Distinguish a subjective concern from a demonstrated data failure.
Include warnings and errors from the isolated diagnostics log. Do not copy real
user document text into a report.

Turn reproducible failures into focused regressions. Keep a UI test for routing,
visibility, timing, or interactions across components; use a component test for
an isolated rule. Run the regression after fixing the issue, then repeat the
visual review. A screenshot comparison measures rendering similarity and cannot
approve the task on its own.
