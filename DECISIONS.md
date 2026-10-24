# DECISIONS

## 2026-10-24: Copyright holder name
The brief's `[YOUR NAME]` placeholder was not filled in before sending. Used the git
`user.name` ("Madison") in `LICENSE` and `docs/PLAN.md`. Change it in both files if a
different legal name is wanted.

## 2026-10-24: Default branch
Repository initialised with `main` as the default branch.

## 2026-10-24: Design previews are HTML, not Rust
Per the brief, Phase 0 produces three self-contained HTML mockups restricted to effects egui's
painter can reproduce (rounded rects, two offset soft shadows, solid fills, simple linear
gradients, circles/arcs). No Rust is written until `design/WINNER.md` exists.

## 2026-10-24: Light-theme accent-hover tuned for contrast
All three previews independently found that white text on the brief's light accent-hover
`#3D74FF` lands around 4.1:1, below WCAG AA. Each darkened it slightly (A: #3368F5, B: #2E6AFF,
C: #2A66FF). The winner's value becomes the token in `design/WINNER.md`.
