# Design winner: A, "Soft Console" (round 2)

**Picked:** 2026-10-24. **Tweaks requested:** none. **Reference mock:** `design/preview-a.html`
(its top-of-file comment is the canonical token sheet; this file restates it for the Rust `neu`
crate). Round 1 (a chunkier "Studio Hardware" look) is archived in `design/round1/` and is not
the target.

## Direction

Classic soft UI, cut from the user's reference kit: one surface colour everywhere, depth only
from paired large-blur shadows, pills and circles, light geometric type, accent used sparingly.
The hero is the concentric-ring dial (master volume in the transport, smaller for filter
cutoff). ON states are accent-filled with a soft accent glow. Nothing chunky, no borders, no
hard edges.

Label levels: title 12px/500 caps tracked (panel names), label 11px/400 caps tracked (control
names, secondary colour), value 13px/400 tabular. Weights never exceed 500; the
BPM display is 22px/300.

## Colour tokens

| Token | Dark | Light | Use |
|---|---|---|---|
| surface | `#262A31` | `#E8ECF3` | window, panels, every control fill |
| shadow-highlight | `#333941` | `#FFFFFF` | top-left soft shadow |
| shadow-dark | `#171A1F` | `#C3CAD6` | bottom-right soft shadow, knob arc track, idle playhead dot |
| accent | `#FF7A1A` | `#1E5BFF` | on-state fill, arcs, pointers, thumbs, markers |
| accent-hover | `#FF9440` | `#2E6AFF` | hover on accent elements |
| on-accent | `#141821` | `#FFFFFF` | text/icons on accent fill |
| text-primary | `#F2F2F2` | `#1C2130` | values, titles, selected segment |
| text-secondary | `#A8ADB5` | `#5E6677` | labels, unselected segments, key note names |
| key-black | `#1C1F25` | `#B4BCCA` | piano black-key fill (the only non-surface fill besides accent) |
| glow | `#FF7A1A` at 55% alpha | `#1E5BFF` at 45% alpha | the on-state outer glow shadow |

Contrast (AA): text-secondary on surface 6.4:1 dark / 4.86:1 light; on-accent on accent
6.8:1 dark / 5.3:1 light; white on light accent-hover 4.55:1.

## Shadows (x y blur spread, two per element, never more)

| Name | Highlight (top-left) | Dark (bottom-right) | Used on |
|---|---|---|---|
| raised-l | -8 -8 20 0 | 8 8 20 0 | panels |
| raised-m | -5 -5 12 0 | 5 5 12 0 | 48px icon buttons, dial rings, piano keys |
| raised-s | -3 -3 8 0 | 3 3 8 0 | knob caps, stepper buttons, step cells, toggle knob |
| inset-m | inset -4 -4 10 0 highlight | inset 4 4 10 0 dark | segmented tracks, toggle tracks, pressed buttons |
| inset-s | inset -2 -2 6 0 highlight | inset 2 2 6 0 dark | slider tracks, playhead column, lamp off |
| on-glow | 0 0 14 0 glow | 4 4 10 0 dark | every ON state (fill accent, text on-accent) |

Pressed swaps raised for inset. The concentric dial is three raised-m rings (no inset).

## Radii, spacing, type

- Radii: panel 24, white key 12, black key 10, pill 999, circles 50%.
- Spacing scale: 4 / 8 / 12 / 16 / 20 / 24 / 32. Panel padding 20 (sequencer and piano
  16 20 18), page gap 10, page padding 12 20 16, synth column gap 26, FX column gap 18, knob
  gap 10, control-group gap 10 or 16.
- Type (system UI font, weights 300 to 500 only): label 11/400 caps +0.12em secondary;
  title 12/500 caps +0.12em primary; value 13/400 tabular; display 22/300 (BPM); segment item
  12/400 secondary, selected 12/500 primary; note cell 11/500; key letter 12/500 +0.12em; key
  note 11/400 secondary.

## Control sizes

- Icon button: 48 circle, icon stroke 1.5 in a 20 box. Stepper button: 28 circle, value
  min-width 36.
- Knob: cap 40 circle, arc ring 50 (r23, stroke 2.5, 270 degree sweep from 7:30), pointer 2x9
  accent, block width 52. Bipolar knobs draw the arc from 12 o'clock. Drag 150px = full range.
  Double-click resets. Small knob (sequencer rows): cap 26, arc 34.
- Dial, large (master): arc 96 (r23 in a 50-unit viewBox, stroke 2), rings 84 / 64 / 44,
  pointer dot 6. Dial, small (cutoff): arc 72, rings 62 / 46 / 32, pointer dot 5.
- Slider: track 6x100 inset r3, thumb 14 circle accent with on-glow, end markers 4 circle accent.
- Segmented: height 30, item padding 0 8, track padding 0 6, item gap 2.
- Toggle: track 50x28 inset, knob 22 circle (ON: accent + on-glow).
- Step cell: 32 circle, gap 8, group gap 20, row height 34, row label column 80, accent marker
  6 dot in on-accent. Playhead dot 6 idle (shadow-dark), 8 accent + on-glow on the current column.
- Piano: white 78x100 r12, black 44x62 r10, key gap 6, 14 white keys (2 octaves); keys with
  computer-key labels light accent with on-glow while held.
- Limiter lamp: 10 circle.
- Layout: content about 1250x1042 at 1280 wide. Default window 1280x1080, minimum 1240x760
  with vertical scroll of the central panel.

## Implementation notes for `neu`

- Everything is one fill colour plus two shadows; build a single `shadow_pair(kind)` painter
  and derive raised/inset/on-glow from it. egui shadows take a colour, so the glow is just a
  shadow in the accent colour with alpha.
- Arcs: draw with the painter as a polyline of ~48 segments. The knob arc track is
  shadow-dark, the value arc is accent.
- Theme follows the OS on launch; only a manual toggle is persisted (the mock had to be fixed
  for this, so test it).

## As-built tweaks (recorded after the M3 review, 2026-10-24)

- Playhead column: off cells in the current column are inset-s with the surface lerped 18 %
  towards shadow-dark (`neu::playhead_tint`), so the column reads even where the inset shadow
  is faint. This is the one extra fill besides key-black and accent.
- Panel padding is 20 on every panel (the sequencer and piano use 20 instead of 16 20 18).
- Sequencer row-label column is 64 wide (not 80) and rows are 4 apart so the six rows plus
  the PROJECT panel fit beside each other at 1280.
- A PROJECT panel (250 wide, right of the sequencer) holds New / Open / Save / Save as, the
  export bar count and Export WAV; it is built from the kit's pills, segmented control and lamp.
- Default window is 1280x1140 so the whole sequencer is visible without scrolling; minimum
  stays 1240x760 with vertical scroll.
- Brand wordmark is 15 px with the mock's +0.16em tracking (added in M4).
- Piano roll: the hovered cell shows a faint highlight (shadow-highlight at 50 %) and black-key rows a 6 % shadow-dark tint; accented notes carry an on-accent dot; meters mark -6 dBFS with an on-accent line.
- Black-key note names use text-primary (text-secondary on key-black is 3.0:1 in light).
- The velocity control is a soft knob (the mock has no velocity control).
