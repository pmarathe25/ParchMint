---
name: parchmint-ui-review
description: Verify ParchMint workflows, visuals, and animations using production harness captures and native-window checks.
---

# ParchMint UI review

Read `tests/parchmint-ui-driver/README.md` and its adjacent `USABILITY.md`.
Use disposable projects and new artifact directories outside the repository.
Compile serially with the pinned toolchain, `--locked`, and one job.

## Capture and inspect

Run the bundled helper from the repository root (Python 3.11+):

```console
python .agents/skills/parchmint-ui-review/scripts/review.py capture --output /tmp/review-run
python .agents/skills/parchmint-ui-review/scripts/review.py gallery --frames /tmp/review-run --output /tmp/review-gallery
```

Open `index.html`, scrub frames, and play transitions at their recorded intervals.
Open individual PNGs at full size for clipping, text, caret, and selection checks.
The gallery is sampled motion, not a measurement of native frame pacing.
Build `parchmint-ui-verification` for comparisons, then use
`review.py compare --before DIR --after DIR --output NEW_DIR --verifier
target/debug/parchmint-ui-verify` to check matching captures for exact pixel equality.
`--sheets` adds contact sheets if Pillow is installed. `capture --suite motion`
and `--filter split,focus` narrow capture; tests still run completely.

Check readable text during pane resizing; stable carets and selections; complete
moving cards/tabs; correct drop targets; and cancellation without jumps or lost
input. Exercise typing, undo, and delayed results during transitions. Check light,
dark, compact layouts, and reduced motion. Assert saved text after reopening.

## Native checks

Build the desktop alone with `cargo build --release --locked -j 1 -p parchmint-desktop`.
After any required GUI approval, Linux users can run:

```console
python .agents/skills/parchmint-ui-review/scripts/review.py native --desktop target/release/parchmint --output /tmp/native-review --appearance dark
```

This isolates app data and copies the bundled project fixture. Size mismatches fail.
Project windows require at least 1280×720 logical pixels; `--scale 2` needs a
display large enough for 2560×1440 physical pixels.

Use a new output directory with `native --interactive` for the normal, resizable
app; close it to finish. Choose appearance and scale in the app/OS in this mode.
Type in both panes, select and copy/paste text, undo, resize, save, and reopen the
copied project. Record pane toggles and tab drags with an available OS recorder;
inspect intermediate frames for clipping, jumps, lost input, and delayed feedback.
For other systems, use a disposable OS profile and the native capture command in
`tests/parchmint-ui-verification/README.md`. Deterministic captures cannot prove
native smoothness. Never treat
unavailable native checks as passing.

Report pass/fail/blocked with artifact paths and concrete observations. Reduce
reproducible failures to small automated tests; keep human visual judgments separate
from pixel equality. Do not update references merely to accept a difference.
