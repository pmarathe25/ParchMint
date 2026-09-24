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

## Launch and use the native application

Run these commands from the repository root on Linux with Python 3.11+ and a
graphical desktop session. Request GUI permission if your agent sandbox requires
it. Build commands must run one at a time.

```console
cargo build --release --locked -j 1 -p parchmint-desktop
review_root=$(mktemp -d /tmp/parchmint-native.XXXXXX)
python3 .agents/skills/parchmint-ui-review/scripts/review.py native --desktop target/release/parchmint --output "$review_root/interactive" --interactive
```

This opens the normal, resizable app with a copied fixture. The helper isolates
configuration, data, and cache under the output directory, records the commit and
binary hash in `run.json`, and writes application output to `native.log`. The
output directory must not exist yet. Close the window normally to finish the
command; agents should keep the command session running while interacting.

Click a document, type a distinctive sentence, select text, copy/paste, undo,
switch panes, resize, and save. Test the workflow changed by the patch. Choose
appearance and reduced motion in Settings; interactive mode does not apply the
helper's capture-only appearance, scale, size, or target options. Take screenshots
with an available desktop capture tool and inspect them at full size.

In **Settings → Appearance**, check that **System** matches the desktop, then
try **Application zoom** at 100%, 125%, and 150%. Check menus, split editors,
and both sidebars at the smallest window size. Reopen with the same isolated
settings to verify the zoom persists; **Reset** returns it to 100%.

Check that the compact formatting toolbar fits without horizontal scrolling at
100% and 150% zoom. Select text and use **Format** to change its font and paragraph
alignment. Check nested menus, Escape, and clicking back into the document; save
and reopen to confirm the formatting persists.

Check **Focus document** in each pane: tabs, the companion control, sidebars, and
the status bar and navigation rail should disappear. The native titlebar should
hide only when maximized or fullscreen. Check that the maximized window still
reaches the bottom of the display; a floating window must retain its titlebar. Type and undo, then use **Exit focus** and
**Escape** to restore the prior layout. F6 should skip the hidden controls.
Check that Export opens from the navigation rail and that style choices have
the same readable contrast as other enabled menu items.

In **Settings → Keyboard shortcuts**, reassign Save and a formatting action.
Check the new binding, the disabled old binding, conflict feedback, Clear, Reset,
and persistence after reopening. Try copy/paste and select-all in both the document
and a Settings text field. The same resolver must handle defaults and overrides.
Create a document and a group from Explorer and Overview; Escape during naming
must remove the placeholder without creating a Recently Deleted entry. Enter
must create exactly one item with its final name.
Repeat creation with its keyboard shortcut while the document has focus; typing
the name must not change the document body, whether confirmed or cancelled.

Check Overview at wide and narrow widths: cards should share rows when space
allows, metadata should form columns below the synopsis, and Explorer should be hidden.
Use both halves of the creation placeholder. Save a new tab inside a nested
group using the expandable location tree.

In History, compare a checkpoint with the current project. Verify nested group
headers, struck-through renamed titles, changed-only content, and separate compact
metadata/comment changes. Filter to one document and back to Entire project.
Select a group in Explorer and check comments from its nested documents; selecting
a comment must open the correct document. Open each search and type immediately,
then dismiss it with Escape. Open an editor or Explorer context menu and
right-click outside its area; the old menu must disappear.

To check persistence, close the app and reopen the same copied project with the
same isolated settings (do not rerun the helper, which creates a fresh fixture):

```console
env XDG_CONFIG_HOME="$review_root/interactive/config" XDG_DATA_HOME="$review_root/interactive/data" XDG_CACHE_HOME="$review_root/interactive/cache" target/release/parchmint "$review_root/interactive/project"
```

Confirm the sentence remains in the editor and the project's saved document.
Keep artifacts under `review_root`, and close only the window launched for the
review. For other platforms, use a disposable OS profile and the
[native capture instructions](../parchmint-ui-verification/README.md#capture-a-native-window).

### Static captures and automation limits

For an automatically saved native screenshot, use a separate output directory:

```console
python3 .agents/skills/parchmint-ui-review/scripts/review.py native --desktop target/release/parchmint --output "$review_root/capture" --appearance dark
```

This writes `native.png` and exits after capture. It verifies rendering, not
typing, focus, clipboard behavior, or animation pacing.

### GNOME remote-desktop input and screenshots

On GNOME Wayland, use [gnome_remote.py](scripts/gnome_remote.py) to drive the real
app through Mutter's local RemoteDesktop API and capture the visible desktop
through ScreenCast/PipeWire. Keep the normal Wayland environment when launching
ParchMint. No RDP server, network sharing, or desktop security setting is enabled.

The helper needs `dbus-python`, PyGObject with GStreamer, and the `pipewiresrc`,
`videoconvert`, `pngenc`, and `appsink` elements. Run it in the logged-in desktop
session with any required sandbox approval. Probe support with:

```console
gdbus introspect --session --dest org.gnome.Mutter.RemoteDesktop --object-path /org/gnome/Mutter/RemoteDesktop
python3 tests/parchmint-ui-driver/scripts/gnome_remote.py --help
```

First capture the display without input. Replace the sample dimensions with the
display's logical dimensions, then inspect the image to locate the review app:

```console
python3 tests/parchmint-ui-driver/scripts/gnome_remote.py --area 0 0 1920 1080 --output "$review_root/desktop.png"
```

Restrict subsequent captures to its visible area. `--area` is desktop logical
`X Y WIDTH HEIGHT`; click coordinates are relative to that area. For example,
after verifying a 1280×720 editor at (320, 214):

```console
python3 tests/parchmint-ui-driver/scripts/gnome_remote.py --area 320 214 1280 720 --actions '[{"click":[500,162]},{"key":"Ctrl+End"},{"key":"Return"},{"text":"remote desktop input works."},{"key":"Ctrl+s"}]' --output "$review_root/typed.png"
```

Actions run in order. `text` supports printable ASCII; `key` supports single
characters and the names in the script's `KEYS` table. `right_click` takes the
same coordinates as `click` and opens document/tab menus. `move` positions the
pointer without clicking. `scroll` takes `[x, y, vertical_pixels]` and sends a
wheel event at an area-relative position; consecutive scrolls use a short pause
for responsiveness checks. `drag` takes a list of at least two area-relative
coordinates; it presses at the first point, moves through the rest, and releases
at the last point. Include intermediate points when checking hover targets and
reordering feedback. The helper paces key
presses/releases, releases modifiers, stops its temporary session, and refuses
existing output files. Inspect each screenshot and saved data before continuing.
These sampled screenshots do not measure frame pacing.

Permission prompts can cover the app and take focus. Capture again after an
approval; use the window switcher and verify the app is visible before typing.
Do not send a long action sequence to an unverified foreground window. A direct
X11 window capture can show a hidden window and does not establish desktop focus.

The [RemoteDesktop](https://github.com/GNOME/mutter/blob/main/data/dbus-interfaces/org.gnome.Mutter.RemoteDesktop.xml)
and [ScreenCast](https://github.com/GNOME/mutter/blob/main/data/dbus-interfaces/org.gnome.Mutter.ScreenCast.xml)
interfaces are private, version-dependent Mutter APIs. If access is denied or an
interface is missing, use the desktop's supported remote-control tooling or
report the blocker; do not disable security checks.

### X11 fallback

On X11, `xdotool` can target the review window for input and resizing. On Wayland,
it cannot control native Wayland windows. If XWayland is available, launching
with `env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET` before the helper command selects
the X11 backend through `DISPLAY`; record that backend in the review. It does not
verify native Wayland behavior. Identify the window by the launched process and
inspect its screenshot before sending input; do not reuse coordinates from a
different window size. If input or capture tools are unavailable, report the
specific blocker rather than treating a static capture as an interactive pass.

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
  drafts. Open History from a document's tab or outline menu, then switch editor
  panes; its target must stay unchanged. Check project milestones in this scope.
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
  restore deleted items, project and document History, and interrupted-session
  edits. Check the named target before confirming. A document restore must keep
  other documents' latest text on disk and their undo usable.

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

### Compact cards and project menu

Open Projects from the brand icon with Explorer visible and collapsed, and from
Overview. The adjacent project title opens the same menu. Verify menu dismissal
with Escape and outside clicks. Check pencil icons in light and dark themes.

Compare document and creation card sizes with long titles, synopsis, and metadata.
Expand several cards in place, edit fields beyond their previews, collapse one,
and verify the others stay expanded. Save and reopen to verify the values. Group metadata uses the same muted preview styling. Open Fields and
check the applicability/type dropdowns and destructive Delete button.

Click breadcrumb search twice to open and close it; opening must focus the input.
Empty local and global search should show no instructional text. Local search uses
arrow, case, and whole-word icons, with no Close button; check its tooltips and
selected option states. In History,
verify Checkpoint and Current project headings on content and non-content changes,
with metadata, comments, and synopsis visually distinct from manuscript text.

Check narrow Overview cards with missing fields and several expanded cards.
Record expansion and collapse; following rows must never overlap a card. Clicking
a synopsis appends; clicking metadata selects its current value. Open Fields near
the middle of the window and confirm choices open below when they fit.

Check that each group outline encloses its children, including nested groups and
creation slots. Scroll past a group's heading and verify the enclosing border
continues through the visible children. Expand document details and verify group
borders follow the changing height. Creation slots should remain muted until
hovered; collapse details and Exit focus should both use four inward arrows.

Drag each splitter, release outside its original hit area, then move the pointer
and click elsewhere. Pane widths must stay fixed. Click to the right of short,
empty, and wrapped editor lines; the caret must stay on the clicked visual line.
Check the bottom-right notification popover without any workspace shift.

### Editing and overview regression checks

Use a copied project to test Backspace at the start of an empty paragraph, deleting
selected words, deleting a paragraph's last character, Enter at paragraph ends,
Enter across a selected range, and Tab in text and lists. Undo/redo, save, close,
and reopen to verify both text and formatting. Select text and test the comment/link
popover. Choose an internal link through the Document tree, then move or rename its
destination and follow the link again.

In Overview, collapse a group's children and confirm its full details remain visible.
Expand multiple document cards: metadata must stay below the synopsis with labels
above values. Scroll a large outline in both directions and record expansion/scrolling;
check that group frames, placeholders, and the scroll position remain stable.
Test dropdown hover highlights and global search disclosure controls in both themes.
