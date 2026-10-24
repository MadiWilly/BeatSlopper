# Round 1 winner (superseded): A, "Studio Hardware"

Superseded on 2026-10-24: the user supplied a neumorphism reference image and asked for a
redo. This is kept for history only. The live winner file is `design/WINNER.md`.

**Picked:** 2026-10-24. **Tweaks requested:** none; the user asked for improvements at my
judgement. **Reference mock:** `design/preview-a.html` (its top-of-file comment is the
canonical token list; this file restates it for the Rust `neu` crate).

## Direction

Chunky, tactile drum-machine hardware: raised neumorphic panels holding recessed module wells,
big tick-ringed knobs with an accent value arc and pointer notch, 40px MPC-style step pads,
and a keycap-labelled piano. The lit sequencer pads are the hero; everything else stays quiet.
Three label levels: panel title (16px sentence case), module silkscreen (11px caps, tracked),
knob name (11px caps) with value above it.

## Improvements made after the pick

- Panel titles changed from 15px uppercase to 16px sentence case, so the three label levels
  no longer read as one.
- Playhead: off-steps in the current column press in (tint + small inset); a lit step under
  the playhead flashes to accent-hover. Beat numbers 1 to 4 sit over the playhead pips.
- Keys gained a two-stop vertical highlight gradient, and black keys a lighter cap color, so
  they separate from the surface in dark mode.
- Wordmark lost its accent-colored "M3" fragment. Hints are plain sentences. The redundant
  "Keys: A W S..." line was removed (the keycaps already show it).
- Height trimmed from about 1190px to 1133px at 1280 wide: ADSR slider track 96 to 80, white
  keys 120 to 108, black keys 74 to 66, panel padding 20 to 18, panel head 36 to 32, sequencer
  rows 46 to 44. Pads stay 40px because they define the look.
- Reduced-motion media query disables the toggle thumb transition. Knob value text turns
  accent on hover.

## Color tokens

| Token | Dark | Light | Use |
|---|---|---|---|
| surface | `#16171A` | `#ECF0F5` | window and panel fill (never pure black/white) |
| shadow-highlight | `#24262B` | `#FFFFFF` | top-left soft shadow |
| shadow-dark | `#0A0B0D` | `#C5CCD6` | bottom-right soft shadow |
| accent | `#FF7A1A` | `#1E5BFF` | on-states, arcs, pointer, BPM digits |
| accent-hover | `#FF9440` | `#3368F5` | hover, accented steps, playhead on lit step (light tuned from `#3D74FF` for AA) |
| on-accent | `#141821` | `#FFFFFF` | text/icons on accent fill |
| text-primary | `#F2F2F2` | `#141821` | |
| text-secondary | `#A8A8A8` | `#5A6372` | labels, ticks, hints |
| knob-face-hi | `#1D1F24` | `#F7F9FC` | knob/thumb/white-key gradient start (145deg for knobs, 180deg for keys) |
| knob-face-lo | `#111215` | `#DCE2EA` | knob/thumb gradient end |
| key-black | `#0E0F12` | `#3A414D` | black key fill (gradient end) |
| key-black-hi | `#22252B` | `#4C5566` | black key gradient start |
| key-black-text | `#A8A8A8` | `#F2F2F2` | |
| led-off | `#0F1013` | `#D8DEE6` | unlit LED / pip fill |
| playhead-tint | `#1C1E23` | `#F5F8FC` | off step in current column |
| inset-on dark | `#00000059` | same | inset pair used over accent fill |
| inset-on light | `#FFFFFF40` | same | |

Contrast (AA): on-accent on accent 6.8:1 (dark) / 5.3:1 (light); accent on surface 6.9:1 /
4.6:1; text-secondary on surface 7.5:1 / 5.3:1; white on light accent-hover 4.6:1.

## Shadows (x y blur spread, two per element)

| Name | Highlight (top-left) | Dark (bottom-right) |
|---|---|---|
| panel | -10 -10 24 0 | 10 10 24 0 |
| raised | -6 -6 14 0 | 6 6 14 0 |
| raised-sm | -3 -3 7 0 | 3 3 7 0 |
| inset | inset -4 -4 10 0 highlight | inset 4 4 10 0 dark |
| inset-sm | inset -2 -2 5 0 | inset 2 2 5 0 |
| inset-on | inset -3 -3 8 0 `#FFFFFF40` | inset 3 3 8 0 `#00000059` |

Pressed (`:active`) swaps raised for inset. Dragging a knob drops it to raised-sm.

## Radii, spacing, type

- Radii: panel 20, control/button/module 16, pad/step/key 14, small (keycap, mini button) 10, knob/LED circle.
- Spacing: 4, 8, 12, 16, 20, 24. Page gutter 16, panel padding 18, panel head 32 tall + 10
  below, module padding 14, module gap 12, sequencer row 44, header row 30.
- Type (system UI font): micro 11/700 caps +0.10em; small 12/700; body 13/500; button 14/600;
  panel title 16/700; display 26/700 tabular; hint 12/600 secondary.

## Control sizes

- Knobs (body / tick ring): lg 56/80 (cutoff), md 44/62 (default), sm 30/40 (sequencer rows).
  Pointer 6px wide, 32% of body, accent. 11 ticks from -135 to +135 deg; value arc r47 of a
  100 viewBox, 3.5px, accent, from -135 (or 0 if bipolar). Drag: 160px vertical for full
  travel (800 with shift). Double-click resets.
- ADSR slider: track 18x80 r9 inset, fill inset 4 accent, thumb 30x16 r8 raised-sm with accent bar.
- Buttons: transport 48 tall r16 raised; square 48x48; small 36 tall r14 raised-sm; mini 28x28 r10.
- Pads (wave/target) 40x40 r14; target 64x32. Step 40x40 r14, gap 6, +10 every 4.
- Displays (BPM, octave): inset well 110x48 r16, 26px accent digits over 11px caps label.
- Theme toggle 92x40 pill, 28px accent thumb. Preset select: inset well 36 tall r14.
- Piano: white 72x108, black 44x66 (z above), gap 6, keycap 22x18 r6 inset-sm.
- Window: default 1280x1100, minimum 1240x760 with vertical scroll of the central panel.
