# Visual Regression Plan

## Environment

- Candidate capture harness:
- Font/runtime controls:
- Native platform captures:
- Pixel-diff tooling:
- Review workflow:

## Theme matrix

Every core screen has a Light and Dark baseline. System is validated behaviorally by resolving to each theme; it does not need duplicate visual files.

## Captures

| Screen ID | Theme | Platform | Width | Height | Scale | Fixture/state | Baseline | Tolerance/notes |
|---|---|---|---:|---:|---:|---|---|---|

## Semantic checks

- Layout/hierarchy/spacing.
- Focus/selection/active/unfocused states.
- Dark manuscript canvas.
- Search/comment/spellcheck/save/error states.
- Truncation/wrapping.
- Contrast and icon/text visibility.
- Approved native differences.
