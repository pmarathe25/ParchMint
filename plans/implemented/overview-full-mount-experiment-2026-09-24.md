# Overview full-mount experiment — 2026-09-24

Keep the bounded Overview mount. Mounting every card does not improve scrolling
on the small outlines tested and costs substantially more CPU and memory on large
ones, including with the native GPU renderer.

## Method

At commit `8cadd7b`, a temporary environment switch made
`CardsState::viewport_window` return **all** cached grid rows, with zero top and
bottom spacer height. The Iced `Scrollable` kept its normal viewport size; every
offscreen card became a mounted child. The experiment retained the same card
layout, text measurements, group frames, and drag targets in both modes. The
switch and benchmark fixture code were removed after measurement.

Four disposable projects each had six visible metadata fields and all groups
expanded. “Shallow” means one group per branch; “deep” means four or five nested
groups per branch. Each branch ends in documents with a short body:

| Shape | Branches × group levels × documents | Groups | Documents |
| --- | ---: | ---: | ---: |
| Small shallow | 4 × 1 × 10 | 4 | 40 |
| Small deep | 4 × 4 × 10 | 16 | 40 |
| Large shallow | 20 × 1 × 20 | 20 | 400 |
| Large deep | 20 × 5 × 20 | 100 | 400 |

The release headless driver ran 80 alternating 120-pixel scrolls and 80 alternating
1,200-pixel scrolls per case. Each mode and shape ran three times, reversing mode
order between repetitions. The table reports the median of each run's p95 action
time, plus median process RSS after scrolling. The driver uses Iced's software
renderer; its action times are not native frame times.

| Shape | Local p95 bounded / full | Wide p95 bounded / full | RSS bounded / full |
| --- | ---: | ---: | ---: |
| Small shallow | 4.5 / 7.8 ms | 8.4 / 8.2 ms | 38.8 / 40.3 MiB |
| Small deep | 3.3 / 11.1 ms | 6.5 / 10.3 ms | 37.3 / 41.7 MiB |
| Large shallow | 6.5 / 97.3 ms | 11.4 / 97.2 ms | 47.5 / 92.4 MiB |
| Large deep | 6.0 / 128.8 ms | 11.5 / 127.6 ms | 47.1 / 103.9 MiB |

Native 1280-pixel-wide X11 windows used the same projects and 80 paced wheel
events per case. GPU initialization on X11 failed and the application recovered
with its software renderer, so these measurements confirm native event-loop cost
but not GPU cost. The large shallow case consumed 0.73 / 3.40 seconds of app CPU
and 132 / 179 MiB RSS with bounded / full mounting. The large deep case consumed
0.50 / 4.56 seconds and 132 / 190 MiB. Small cases cost 0.33 / 0.41 seconds
(shallow) and 0.49 / 0.70 seconds (deep). A disposable copy of
`Manual-Test-Project-Ready.parchmint` was effectively tied at 0.24 / 0.23 seconds.

The native Wayland run opened a DRM render device and loaded Vulkan. For the large
deep case, 80 scroll events paced at 100 ms consumed 0.20 / 1.02 seconds of app
CPU and ended at 133 / 190 MiB RSS, bounded / full. These CPU totals are over a
fixed input sequence, not measurements of input-to-present latency. Screenshots
confirmed the nested cards and group frames remained intact after scrolling.
Raw run output and screenshots are under `/tmp/parchmint-mount-comparison.json`,
`/tmp/parchmint-native-bench-01_g323o`, and `/tmp/parchmint-wayland-*` on the
measurement workstation, outside the repository.

## Interpretation

Full mounting removes viewport slicing, spacer rows, and refresh triggers: about
70–110 lines of the current implementation and their tests. It does not remove
the row grouping, variable-height cards, nested group boundaries, group-frame
painting, or drag targets. Those still determine layout and behavior. A different
nested-widget design might remove more geometry code, but would require a new
layout and drag implementation; this experiment does not measure it.

The current bounded path caches the complete row geometry, then uses row offsets
to mount only the viewport and overscan. During most scrolling, Iced's scrollable
moves within those mounted rows without asking ParchMint to rebuild the view.
Full mode therefore avoids a refresh only when a scroll crosses the bounded
window's edge. In return, Iced maintains and processes every card widget, even
when most cards are offscreen. The experiment's full mode still cloned the cached
row list when constructing a view; that clone is not on the ordinary scroll path
because the full window has no before/after boundary. Removing the remaining
windowing bookkeeping could shave initial view work, but cannot explain away the
measured per-scroll cost or the larger live widget tree.

For a project around the size of the manual-test fixture, either strategy is
viable on performance grounds. Keeping one bounded strategy avoids a separate
small-project code path and preserves the large-project result.
