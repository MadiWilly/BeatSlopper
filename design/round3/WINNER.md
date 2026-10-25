# Design winner: B, "Deck" (round 3)

**Picked:** 2026-10-25. **Tweaks requested:** none. **Reference mock:** `design/round3/preview-b.html`
(its top-of-file comment is the canonical token sheet; this file restates it, with the layout
numbers read off every frame, for the Rust `neu` crate and the Phase 5 re-layout, M12..M14 in
`docs/PLAN.md`). The previous target, `design/WINNER.md` ("Soft Console"), is history: none of
its values carry over. All numbers below are logical points.

## 1. Direction

Deck is the FL Studio mental model done with discipline: a BROWSER column on the left (presets
by category, project, recent), the CHANNEL RACK as its own window (mute LED, solo, two mini
knobs, insert number box, coloured name button, sixteen step buttons in beat groups), the
PIANO ROLL as the main window with FL's key column, velocity lane and ghost channels, and the
MIXER as a bottom drawer whose numbered inserts run left to right with the selected insert's
slots, EQ3, sends and route enlarged in a side column. Everything is flat: four panel tones and
1 px borders carry depth, one amber accent carries state, the eight track colours carry
identity, and every scrollable area draws its scrollbar at all times. Density comes from 28 pt
control rows on a 32 pt pitch, not from shrinking type (body 13, labels 12, nothing below 10).

**What it gives up:** the single-page "everything visible" layout (SYNTH & FX, SONG and the
BROWSER live behind F-keys as windows); soft shadows, pills, circles-as-buttons and the
concentric dial, entirely; the user has to learn the F5..F9 view switcher. In return every view
gets the full width, nothing is ever cut off at 850 logical points, and a 850 x 600 window is
still a working DAW.

## 2. Colour tokens

Contrast = WCAG ratio against the surface named in brackets. AA text >= 4.5, AA UI >= 3.

| Token | Dark | Light | Role |
|---|---|---|---|
| bg | `#111316` | `#DFE2E7` | window background, the 4 pt gutters between docks |
| panel | `#1B1E23` | `#F3F4F6` | window / dock body, toolbar, groups, strips, rows |
| panel-2 | `#23272D` | `#FFFFFF` | window headers, fields, dropdowns, wells, scrollbar tracks, menus, roll ruler + key column, timeline |
| panel-3 | `#2B3038` | `#E8EAEE` | button face, row hover, ghost-button hover |
| panel-4 | `#343A43` | `#DCDFE5` | button hover, selected segment, tooltip face, menu item hover, LED off, slot ON, black-key rows |
| border | `#30353D` (1.35:1) | `#C6CBD3` (1.48:1) | hairlines: window frames, header/toolbar dividers, rows, step cells off, pressed buttons; decorative |
| border-2 | `#434A54` (1.9:1) | `#A9B0BA` (2.0:1) | control borders: buttons, fields, knob track arc + face ring, faders, keycaps, tags, separators |
| text | `#E9EBEE` 14.0:1 [panel] 11.1:1 [panel-3] | `#15181D` 16.2:1 / 14.8:1 | primary: values, titles, buttons, selected segment, knob pointer |
| text-2 | `#9AA3AE` 6.5:1 [panel] 5.9:1 [panel-2] | `#525A66` 6.3:1 / 7.0:1 | labels, column heads, unselected segments / tabs, sub-titles, scrollbar thumb hover, switch knob off |
| text-3 | `#667080` 3.3:1 | `#7E8794` 3.3:1 | disabled, keycaps, placeholders, hints, section labels, ruler numbers, scrollbar thumb, empty slots; never body text |
| accent | `#F0B429` 9.0:1 [panel] | `#B45309` 4.6:1 [panel] | ON step, primary button, selection bar, underline, playhead, knob value arc, slider fill, limiter LED |
| accent-hover | `#F8C64E` | `#C9640F` | primary hover; accent-step fill |
| accent-press | `#D39B14` | `#9A4607` | primary border, ON-step border, accent-step inset ring, LED accent border |
| on-accent | `#17120A` 10.0:1 [accent] | `#FFFFFF` 5.0:1 [accent] | text / icons on accent, the accent-step notch, pressed piano key text |
| accent-wash | accent @ 16 % | accent @ 14 % | toggle ON face, selected row / strip / timeline block, switch ON track |
| rec | `#F0433A` 4.4:1 (icon only) | `#C81E1E` 5.2:1 | record-armed button (face rec @ 16 %, border + glyph rec), record LED |
| ok | `#3DD68C` 8.9:1 | `#15803D` 4.6:1 | solo ON (face ok @ 16 %), mute LED audible, MIDI lamp, meter body |
| clip | = rec | = rec | meter top 12 % |
| grid-beat | `#2A2F36` | `#E1E4E9` | roll step lines, row dividers, scope / EQ-curve grid |
| grid-bar | `#3C434D` | `#C6CBD3` | roll beat and bar lines, C-row dividers |
| step-off | `#24282E` | `#E6E8EC` | step cell off, beats 2 and 4 |
| step-off-alt | `#292E35` | `#DEE1E6` | step cell off, beats 1 and 3 (FL-style alternation) |
| on-track | `#15181D` | `#FFFFFF` | text on a track colour (note-step labels) |
| track 1..8 | `#F87171` `#A3E635` `#4ADE80` `#2DD4BF` `#38BDF8` `#818CF8` `#C084FC` `#F472B6` (>= 5.6:1 vs panel; on-track >= 5.9:1 on every swatch) | `#B91C1C` `#4D7C0F` `#15803D` `#0F766E` `#0369A1` `#4F46E5` `#7E22CE` `#BE185D` (>= 4.5:1 vs panel; white >= 5.0:1 on every swatch) | track chips, name-button bar, note bars, note steps, strip colour bar, ghosts |
| shadow | 0 8 24 `rgba(0,0,0,.55)` | 0 8 24 `rgba(20,30,50,.22)` | menus, tooltips, the mixer drawer when floating. **No other shadow anywhere.** |

Derived values (fixed formulas, not new tokens): note-bar outline = track colour mixed 60 %
towards black; note-bar handle = colour 50 % towards black; note-step border = colour 70 %
towards black; ghost fill = colour @ 35 % alpha, ghost border = colour @ 60 % alpha; LED-on
border = ok 60 % towards black (rec LED: rec 60 % towards black); tag colours for `vinyl` =
track 7, `drone` = track 4, each with the border at 50 % alpha.
Knob face gradient (top to bottom): dark `#3A4049` to `#23272D`, light `#FFFFFF` to `#D5D9DF`.
Black piano key: dark `#0C0E11` to `#1A1D22`, border `#000`, text `#9AA3AE`; light `#2A2F38`
to `#3D4450`, text `#C9CED6`; its keycap face `#15181D`, border `#2B3038`, text `#9AA3AE`.

## 3. Type, spacing, radii, borders, shadows

Type: the system UI stack (egui's proportional face; weights 400 / 500 / 600 where the font
has them, otherwise 400 and bold); tabular numerals on every value.

| Style | Size / weight | Case / tracking | Colour | Use |
|---|---|---|---|---|
| title | 12 / 600 | UPPER, +0.06em | text | window + dock headers, side-column head, group heads |
| label | 12 / 500 | UPPER, +0.04em | text-2 | knob + group labels, column heads, "Len", "Oct" |
| button | 12 / 600 | UPPER, +0.04em | text (on-accent on primary) | every button, tab, chip |
| value | 13 / 400 | tabular | text | knob values, fields, list rows, position |
| body | 13 / 400 | | text | menus, browser rows, project name (600) |
| display | 18 / 600 | tabular | text | BPM number (min width 34, centred) |
| small | 11 / 500 | | text-2 | tags, insert numbers (600), step note labels, strip names (600), mini-insert chips |
| hint | 10 / 600 | UPPER, +0.02em | text-3 | keycaps |
| section | 11 / 600 | UPPER, +0.06..0.08em | text-3 | rack section heads, synth section rules, side-column group label, rail vertical text |
| micro | 10 / 600 | | text-2 / text-3 | ruler numbers, roll key names, slot pills (UPPER), timeline sub-line (500) |

- Spacing scale: 4 / 8 / 12 / 16 / 24. Rhythm 8: control height 28, row pitch 32 (28 + 4),
  frame padding 4, dock gap 4 (= splitter width; splitter hit 8), dock padding 8, group gap 16,
  toolbar 40, window header 28, window toolbar 36 (28 control + 4 + 4), tab strip 28.
- Radii: 4 on buttons, windows, fields, dropdowns, groups, note bars, menus, tooltips, tags'
  parents; 3 on step cells, keycaps, small buttons, segmented items, slot pills, pattern tiles,
  insert boxes, fader / slider thumbs, timeline blocks; 2 on meter columns, colour bars, chips;
  switch = full pill (11 of 22); LEDs and knobs 50 %.
- Borders: always 1 px. `border` on surfaces and separators between surfaces, `border-2` on
  interactive controls. Separators in toolbars: 1 x 20 border-2.
- Shadows: only the `shadow` token, only on menus, tooltips and a floating drawer. Depth
  everywhere else = the four panel tones + 1 px borders. Simple vertical two-stop gradients are
  allowed on knob faces, fader / slider tracks and thumbs, and piano keys (egui mesh); a flat
  fill of the first stop is an acceptable fallback.

## 4. Control sizes and states

Hit target >= 28 x 28 always; where a control is drawn smaller (22 pt small buttons, 24 pt
step cells) the gap belongs to the cell and the hit area is the full row / pitch.

**Buttons.** 28 h, min 28 w, padding 0 10, radius 4, 12/600 caps, inner gap 6.
- secondary: panel-3 face, border-2; hover panel-4; pressed panel-2 + `border`; disabled whole
  control at 45 % alpha; focus (keyboard only) 2 px accent outline 1 px outside.
- primary: accent face, accent-press border, on-accent text; hover accent-hover; pressed accent-press.
- toggle ON: accent-wash face, accent border + text. Solo ON: ok @ 16 % face, ok border + text.
  Record-armed ON: rec @ 16 % face, rec border + glyph.
- icon: 28 x 28, padding 0, glyph in a 16 box, stroke 1.5 round caps / joins (filled glyphs for
  play / stop / rec / dots).
- ghost: transparent face and border; hover panel-3 + `border`; same ON states as toggle.
- small: 22 h, min 22 w, padding 0 5, 11, radius 3; small icon 22 x 22 with a 12 glyph. xs: 24 h,
  padding 0 6, 11 (browser PROJECT buttons). Window-header and rail icon buttons: 24 x 24 with
  a 12 (header) / 14 (rail) glyph.

**Segmented.** 28 h, panel-2, border-2, radius 4, padding 2, item gap 2; items 24 h, padding
0 8, radius 3, 12/500 text-2, hover text; selected = panel-4 face + text + 2 px accent underline
(inset 4 each side, radius 1). Small: 22 h, items 18 h, padding 0 6, 11.

**Dropdown.** 28 h, padding 0 6 0 8, panel-2, border-2, radius 4, 13 value, chevron 6 x 6 at
1.5 text-2 in a 10 box, gap 6. Small: 22 h, 12, padding 0 4 0 6. Track variant: a 3 px track-
colour bar inside the left edge. Menu (also the project menu): min 180 w, panel-2, border-2,
radius 4, `shadow`, padding 4 0; items 28 h, padding 0 10, 13 body, gap 8, keycap right-
aligned; hover panel-4; disabled text-3; separator 1 px `border`, margin 4 0.

**Switch.** 40 x 22, radius 11, panel-2, border-2, knob 16 circle text-2 at (2, 2); ON:
accent-wash track, accent border, knob accent at x 20. Small (side column): 34 x 20, knob 14.

**Text field.** 28 h, padding 0 8, panel-2, border-2, radius 4, 13 text, gap 6, leading icon
14 text-2, placeholder text-3; focus: accent border + 1 px accent ring outside (2 px visual);
caret 1 x 14 text. Shortcuts never fire while a field has focus.

**Keycap.** 16 h, padding 0 4, panel-2, border-2, radius 3, hint type. **LED.** 10 circle,
1 px border: on = ok + (ok 60 % black); off = panel-4 + border-2; accent = accent + accent-press
(limiter, MODIFIED); rec = rec + (rec 60 % black). Unsaved dot: 7 circle accent, 6 left of it.
**Tag.** 16 h, padding 0 5, radius 3, border-2, 11/500 text-2; `vinyl` / `drone` in track 7 /
track 4 with the border at 50 %. **Chip.** 10 x 10, radius 2, track colour.

**Knobs.** Size s: standard 32, mini 22, large 40. c = s / 2, r = s / 2 - 3 (13 / 8 / 17).
Track arc: 270 degrees from 135 deg (7:30) clockwise to 405 deg (4:30), border-2, stroke 2.5
(mini 2), butt caps. Value arc: accent, same stroke, from 135 deg to the value angle
135 + 270 v; bipolar knobs draw from 270 deg (12 o'clock) towards the value in either
direction. Face: circle radius r - 4 (9 / 4 / 13), vertical gradient (section 2), 1 px
border-2. Pointer: a line from radius max(r - 8, 3) to r at the value angle, text, stroke 2
round (mini 1.5). Block: label above, value below, gap 3, min width 56 (46 in the mixer side
column); mini knobs have no label or value (tooltip "Vol 85 %"); inline variant (swing) puts
the mini knob and value in a row, gap 6. Disabled: whole block at 45 % alpha. Drag 150 pt =
full range (Shift fine), wheel steps, double-click resets, click the value to type.

**Faders.** 24 x 120; track 8 wide centred, radius 4, horizontal gradient panel-2 to panel,
border-2; cap 24 x 12, radius 3, gradient panel-4 to panel-3, border-2, 1 px text line 5 from
its top inset 4; travel 108; unity (0 dB) mark 1 px text-3 at 75 % + 6. Double-click resets to
0 dB. **GR meter** 4 x 120, panel-2, `border`, radius 2, accent filling from the top by the
reduction. **Meters.** Stereo = 2 columns 5 wide, gap 2, 120 tall, radius 2, 1 px `border`;
body a vertical gradient ok 0..70 %, accent 70..88 %, clip 88..100 %, masked by panel-2 above
the level, 1 px panel ticks every 4 pt (3 lit, 1 dark); clip = level in the top band (no extra
latch). Horizontal = 2 rows 72 x 5, gap 2, 16 total (toolbar master, drawer bar). Limiter
GR square: 40 x 40, radius 3.

**Sliders.** Vertical 28 x 72: track 6 wide at x 11, radius 3, gradient panel-2 to panel,
border-2; fill 4 wide accent from the bottom; thumb 20 x 12, radius 3, gradient panel-4 to
panel-3, border-2, 1 px text-2 line; travel 60. Horizontal 72 x 28: track 72 x 6 at y 11, fill
4 h accent, thumb 12 x 20 at y 4 with a vertical text-2 line; travel 60. 24 h variant in the
keyboard header.

**Step cells.** 24 x 24, radius 3, 1 px border; gap 2 inside a beat, 6 between beats; hit
= the 28 pitch. off: step-off (beats 2 + 4) / step-off-alt (beats 1 + 3), `border`; hover:
border text-2; on: accent, accent-press border; accent (Shift-click): accent-hover, accent-
press border, 2 px accent-press inset ring, a 2 pt on-accent notch at y 4 inset 8 each side;
playhead column: 1 px text-2 ring outside every cell in the column (kept on accent cells) +
a 6 x 6 accent triangle under the ruler cell; note cell (track lanes): track colour, border
colour 70 % towards black, label 10/700 on-track.

**Note bars (roll).** Row 13, bar 12, radius 3, track colour at alpha 0.5 + 0.5 x velocity,
1 px outline colour 60 % towards black, a 2 pt resize handle (colour 50 % black) inset 2 at the
right; selected: 1 px text outline 1 pt outside; ghost: colour @ 35 % fill, 1 px colour @ 60 %
border, not interactive. Playhead: 1 px accent full height. Velocity lane bars: 3 wide, track
colour, height = velocity; velocity >= 0.85 carries a 7 x 3 text marker above; selected =
1 px text ring.

**Tabs.** 28 h strip, panel-2, `border` below; items padding 0 12 (8 at narrow), button type,
text-2, hover text, active text + 2 px accent underline inset 6; optional keycap 6 after the
text. **List rows.** 28 h, padding 0 8 0 12, 13 body, gap 6; hover panel-3; selected
accent-wash + 2 px accent bar at the left edge; trailing tags / LED / meta (11 text-3).
**Tooltips.** panel-4 face, border-2, radius 4, 26 h, padding 0 8, 12/400 text, keycap after
an 8 gap, `shadow`, 300 ms delay; every control with a shortcut names it in a keycap.
**Scrollbars.** 10 wide track panel-2 with a 1 px `border` on its inner edge; thumb 6 wide,
radius 3, inset 2, text-3, hover text-2, drag text; ALWAYS drawn; when nothing scrolls the
thumb spans the track at 50 % alpha; a vertical bar stops 10 short of a horizontal one.
**Window.** panel fill, 1 px `border`, radius 4, clipped. Header 28: panel-2, `border`
below, padding 0 4 0 8, gap 8: title, sub (12 text-2, ellipsis; the track part in text/500),
spacer, keycap, collapse (minus glyph; chevron-down/up on the rack and keyboard) and close as
24 ghost icon buttons. Collapsed = header only, 28, no bottom border. Toolbar 36: padding
0 8, gap 8, `border` below. Body clips; scrollbars sit inside it.
**Insert number box.** 22 x 22, radius 3, panel-2, border-2, 11/600 text centred; click opens
the insert dropdown (0 = master, 1..16), Ctrl+L links a free insert. **Name button.** 80 x 24
(76 in the rack, 64 at narrow), radius 4, panel-3, border-2, 12/600 left-aligned, padding
0 6 0 10, ellipsis, a 4 px track-colour bar at the left edge (text-3 for drums); hover
panel-4; muted row: text-2. Pattern tile: 24 x 24, radius 3, panel-2, border-2, 12/600 text-2;
has-notes = text; current = accent / accent-press / on-accent.

## 5. Layout

Frame: bg, padding 4, gap 4, column: TOOLBAR 40 | BODY (flex) | DRAWER BAR 28 or MIXER 440 |
KEYBOARD 96 or 28 collapsed. Every split is a 4 pt gutter with an 8 pt hit, draggable and
persisted. At 1280 x 800 the body is 614 tall.

**Toolbar (40, panel, `border`, radius 4, padding 0 6, gap 5), left to right:** project menu
(ghost icon) · project name 13/600 + unsaved dot · sep · undo · redo (ghost icons) · sep ·
PLAY (primary icon, toggles to a stop glyph while playing) · STOP · REC (toggle, rec) ·
segmented Pattern | Song · sep · BPM "-" icon · display number (drag / type / tooltip) · "+" ·
TAP · sep · "Swing" label + inline mini knob + value · sep · position 13 text-2 ("A · bar 1/2"
or "bar 5/25") · sep · master h-slider 72 + h-meter 72 · HINT panel (flex, min 40, 12 text-2 +
keycaps, ellipsised; this is the status / hint bar, there is no separate one) · MIDI lamp +
port name 12 text-2 · sep · VIEWS: five icon buttons (roll F5, mixer F6, synth F7, song F8,
browser F9; the active one is a toggle ON, the rest ghost) · sep · theme (ghost icon).

**BROWSER column (208 wide; F9 toggles it; collapsed rail 28).** Header "BROWSER" + F9. Search
toolbar 36 with a flex text field. Three sections, each a 28 h head (padding 0 8, triangle 5 x 8
text-2 rotated when open, section type in text-2, count text-3 at the right, hover panel-3):
PRESETS (category chips wrapping, gap 4, padding 4 8 8, chip 22 h padding 0 7, 11/600 caps,
border-2, ON accent-wash; a scrolling row list with vertical scrollbar; footer 36: prev small
icon · current name 12 ellipsised + modified dot · next small icon), PROJECT (file row with
icon + unsaved dot; New / Open / Save / Save as xs buttons wrapping, gap 4; "Export" label +
small segmented x1 x2 x4 x8; one 11 text-2 info line; full-width primary EXPORT WAV), RECENT
(rows with a file icon, name, age in 11 text-3). Rail: 28 wide, padding 4 0, gap 4, icon
buttons 24 (browser ON, presets, project, recent) and the word BROWSER vertical in section type.

**CHANNEL RACK window (664 wide default, 400 at narrow; left of the main window; F5 expands
it).** Header: "CHANNEL RACK" · sub "Pattern A · 2 bars · Keys · Dusty Rhodes" · F5 · chevron ·
close. Toolbar: pattern tiles A..H (gap 2) · "Len" + small segmented 1 | 2 | 4 · "Bar" + small
segmented 1 | 2 · FOLLOW small toggle · spacer · copy / paste / clear small ghost icons. Body
(padding-top 4, vertical + horizontal scrollbars): ruler row 20 h (cells 24 x 16, micro type,
beat numbers text-2, dots text-3, playhead triangle) aligned over the steps; section head
"DRUMS" 24 h (section type, column legends VOL TUNE / INS at 10 right-aligned over their
columns); five drum rows; section head "TRACKS 5 / 8" (legend VOL PAN); up to eight track rows;
an add row 28 h (small plus icon · "Add track" 12 text-2 · keycap Ctrl+T). Row: 28 h, pitch 32,
padding 0 8, gap 6, columns LED 10 · S 22 · knob 22 · knob 22 · INS 22 · NAME 76 · STEPS 426
(4 beats x (4 x 24 + 3 x 2) + 3 x 6) = 652 content + 10 scrollbar + 2 border = 664. Selected
row (the edit track): accent-wash + 2 px accent bar; muted row: LED off + name text-2.

**PIANO ROLL window (main window, flex; F5).** Header "PIANO ROLL" · sub "Keys · Dusty Rhodes"
· F5. Toolbar (gap 6): track dropdown small with colour bar · small segmented Draw | Select ·
snap dropdown small (1/16) · zoom - / + small icons · GHOSTS small toggle. Body: ruler 16 h
panel-2 (key column 40 blank; bar.beat labels "1.1" micro text-2 at x 4; playhead triangle
8 x 8 accent); key column 40 wide panel-2, rows 13, black-key rows panel-4 with the right 40 %
panel-2, C labels micro text-2, 1 px `border` between rows; grid: step lines grid-beat, beat
and bar lines grid-bar, row dividers grid-beat (C rows grid-bar), black-key rows panel-2 @ 60 %;
about three octaves visible (36 rows = 468) scrolling vertically; velocity lane 44 h panel-2
with `border` above, "VEL" micro label in the key column; vertical scrollbar right, horizontal
bottom. Ctrl+wheel zooms time, wheel scrolls pitch.

**MIXER drawer (F6).** Collapsed bar 28 h: panel-2, `border`, radius 4, padding 0 8, gap 8:
"MIXER" title · "16 inserts · master" 12 text-2 · mini-insert chips (20 h, padding 0 6, 11
text-2, number text-3/600, `border`, radius 3, panel) for inserts 1..5 and "... 16" · spacer ·
h-meter 72 · F6 keycap · chevron-up small ghost. Open: a 440 h window that pushes the body up
(may also float over it with `shadow`); drag its header gutter to resize. Header "MIXER" · sub
"16 inserts · master · sources on the rack rows" · F6. Toolbar: "Selected" label · dropdown
small "1 · Keys" · LINK small button (link glyph + keycap Ctrl+L) · sep · "Plugins" label · EQ
small · REVERB small · spacer · small segmented Wide | Narrow. Body padding 8 8 18 8, gap 8:
strips scroller (flex, horizontal scrollbar) · MASTER pinned · side column 276. Insert strip:
54 wide, padding 4 0, gap 4, panel, `border`, radius 4 (selected: accent-wash + accent border);
head 30 (number small/600 text-2, name small/600 ellipsised, empty inserts text-3); colour bar
44 x 3; three slot pills 46 x 16 (micro caps, panel-2, border-2, text-3 "OFF"; ON panel-4 +
text; click opens the effect menu); fader zone gap 3 (fader 24 + stereo meter + GR 4); M / S
small buttons gap 2; route "-> M" small/600 text-2 with the target in text. Narrow strip mode:
hides the slot pills and route (strip 40 wide). MASTER: 80 wide, panel-2, accent colour bar,
slots 70 wide (LIMITER in accent, "EQ · open", REVERB), level readout small text-2, M + limiter
LED. Side column (the selected insert): head 24 (chip · "INSERT 1 · KEYS" title · spacer · tag
"track Keys"); three slot rows 28 (number 16 wide small/600 text-2 · effect dropdown flex ·
mix mini knob · small switch); group label "EQ3 · SENDS · ROUTE"; knob row gap 4 (LOW / MID /
HIGH bipolar, DLY, REV); route row ("Route" label · dropdown flex · "Pan" label · mini bipolar
knob). At narrow the drawer stays a bar and F6 selects the MIXER tab of the main window
(strips + master; the side column is hidden below 1000 wide and a slot pill opens its
parameters in a popup, the M11 "pop-out"; decision, not drawn in the mock).

**SYNTH & FX window (F7; replaces the roll in the main slot, the rack collapses to its
header).** Header "SYNTH & FX" · sub "Keys · Dusty Rhodes" · F7. Toolbar: chip · track
dropdown small · sep · prev small icon · preset dropdown small (min 150) · next small icon ·
MODIFIED LED + label · sep · COPY TO... small · RESET small · spacer · hint text-3 "drag · wheel
· double-click resets · type a value". Body padding 8 8 0 8, vertical scrollbar; a 4-column
grid, gap 8 (3 columns under 1000 wide, 2 under 700), groups of `border` / radius 4 / panel /
padding 6 8 8 / gap 6 with a 24 h head (title + segmented / tag / xs button at the right) and
knob rows (gap 4, wrap, bottom-aligned). Section rules span all columns (section type, 16 h,
hairline). Order: **SYNTH · <track> · <preset>**: OSC 1 (Sine Saw Sqr Tri; Detune bipolar, Mix,
Pulse [disabled unless Sqr], Unison, Spread) · OSC 2 (same) · MIX & FM (Sub, Noise, FM amt, FM
ratio) · MODE (Synth | Pluck; Damping, Bright [disabled in Synth], Glide) · FILTER (LP BP HP;
Cutoff large 40, Res, Env bipolar, Key trk) · FILTER ENV (A D S R vertical sliders, gap 10,
label below, 12 value) · AMP ENV (same) · LFO & PITCH (Pitch | Cutoff; Rate, Depth, P-env
bipolar, Decay). **TRACK INSERTS · <track>**: VELOCITY (-> Amp, -> Cutoff) · DRIVE & SIDECHAIN
(Drive, SC amt, SC rel) · LO-FI (spans 2 columns; tag "slot n"; Bits, Rate, Wobble, Noise,
Tone, Mix) · CHORUS (Rate, Depth, Mix). **GLOBAL FX · MASTER**: DELAY (1/16 1/8 1/4 1/2;
Feedback, Mix) · REVERB (MORE... xs; Size, Damping, Mix) · MASTER EQ (OPEN EQ xs; curve well
44 h panel-2 border-2 radius 4, accent 1.5 stroke, grid-beat lines; caption 11) · LIMITER
(switch; Gain knob, GR square, limiter LED). The preset browser is the BROWSER column's PRESETS
section, open beside it.

**SONG window (F8; main slot, rack collapsed) + VISUALIZER (340 wide, right of it).** Song
header "SONG" · sub "6 entries · 25 bars · 1:05" · F8. Toolbar: "Add" label · pattern dropdown
small · primary small "+ ADD" · sep · LOOP small toggle · PLAY FROM small · spacer · total text-3.
Body padding 8, gap 8: timeline 48 h (panel-2, `border`, radius 4, padding 4, gap 2; blocks
flex-weighted by bars, panel-4, border-2, radius 3, 12/600 letter over a 10/500 text-2 "x2 · 4
bars"; selected accent-wash / accent border / accent text; 1 px accent playhead); table (panel,
`border`, radius 4; head row 28 panel-2 section-type text-2; grid 32 / 72 / 56 / 110 / 72 /
flex, gap 8, padding 0 8; rows 28 with `border` below, hover panel-3, selected accent-wash +
bar; cells: index text-2 · 10 x 10 swatch + "Pattern B" 600 · "2 bars" · repeats (- small ·
"x2" min 20 centred · + small) · "bar 5–8" text-2 · move up / down / remove small ghost icons;
vertical scrollbar). Visualizer: header "VISUALIZER" · "scope"; toolbar segmented Scope | Bars |
Radial + full-screen small icon (F11); body padding 8, scope well panel-2 border-2 radius 4,
accent 1.5 polyline, grid-beat centre line. F11 covers the body with the visualizer until
Escape.

**KEYBOARD strip (96 = header 28 + body 68; collapsible to 28).** Header: "KEYBOARD" · sub
"A W S E D F T G Y H U J K O L P ; · Z / X octave" · sep · "Oct" · - small · value · + small ·
sep · "Vel" · h-slider (24 h) · value · spacer · "48 kHz · 256 · 5.3 ms" text-3 · chevron-down ·
close. Body: 24 keys (2 octaves) centred, white 56 x 64, black 32 x 38 overlapping at the white
boundary minus 16 (784 wide); generic key size for other uses 40 x 64 / 24 x 38. Key: border-2,
radius 0 0 4 4, gradient panel-3 to panel-4, note name micro text-3 and a keycap (16 h) stacked
at the bottom (gap 2, padding-bottom 4); black keys per section 2; held: accent / on-accent /
accent-press, keycap transparent. Collapsed header adds "· oct 4 · vel 80 %" to the sub.

**View switcher.** F5 piano roll (and re-expands the rack), F6 mixer drawer, F7 synth & fx,
F8 song, F9 browser, F11 visualizer full. The main slot shows one of roll / synth / song; at
narrow it grows a 28 h tab strip PIANO ROLL F5 | MIXER F6 | SYNTH & FX F7 | SONG F8 under the
header and the toolbar icons mirror it.

**Narrow (850 x 600) and minimum.** Minimum inner size 840 x 600; everything below is the
state at or under 1000 wide: the browser becomes the 28 rail; the toolbar drops the project
name, undo / redo, TAP, swing, the h-meter, the hint text and the MIDI port name ("Pattern"
reads "Pat"); the rack is 400 wide, drops its two knob columns (values live in the row
tooltip), name 64, and scrolls its 16 steps in a 210 viewport with the horizontal scrollbar
drawn; the rack toolbar becomes dropdowns "Pattern A" / "2 bars" + Bar segmented + FOLLOW +
a dots menu (Copy · Paste · Clear); the main window gets the tab strip and drops zoom
buttons; the mixer and keyboard stay bars; the keyboard opens over the body when expanded.
Nothing is ever clipped: every window body scrolls instead.

**Window sizing.** Logical = physical / scale factor. First launch: default inner size =
clamp(0.7 x work-area width, 1280, 1920) x clamp(0.7 x work-area height, 800, 1200), then
reduced to the work area minus (40, 60) if that is smaller, floored at 840 x 600, centred on
the monitor; persisted geometry is restored only if it still fits the current work area,
otherwise the rule above runs again. Verify the first frame's actual inner size against the
request (the eframe quirk at 125 / 150 %) and resize once if it differs.

## 6. Interaction notes

- Step cell: click toggles; Shift-click sets / clears accent; right-click clears; drag across
  cells paints the first cell's new state. Row LED: click mutes / unmutes, right-click solos.
- Double-click a name button, insert strip name or track row to rename inline (text field in
  place, Enter commits, Escape reverts). Double-click a knob, fader or slider resets it.
- Every tooltip carries the shortcut in a keycap; the toolbar hint panel repeats the hovered
  control's hint. Tooltips appear after 300 ms and never cover the hovered control.
- Escape closes the open menu, popup, rename, visualizer full-screen or the floating drawer,
  in that order. Enter confirms. Shortcuts are suppressed while a text field has focus.
- The mixer drawer bar's chevron and F6 pull it up; its top gutter drags its height; dragging
  it to the bar height collapses it. Window chevrons collapse to the header; the close icon
  hides the window (F-key brings it back). Rack / roll gutter trades width between them.
- Scrollbars are always drawn, including a full-length faint thumb when nothing scrolls, so
  the user can see that an area is a scroller.
- Selected faders move together with Shift-select; Ctrl+L links the selected track to a free
  insert; Ctrl+PgUp / PgDn steps presets; Ctrl+T / Ctrl+Shift+D / Alt+Up / Alt+Down manage
  tracks; Space play / stop; R arm; Ctrl+Z / Ctrl+Y; Ctrl+N / O / S / Shift+S / E.
- The FOLLOW toggle (on the rack) is global: the rack pages bars and the roll scrolls time to
  the playhead while playing.

## 7. Implementation notes for `neu`

`Theme` is rebuilt around the table in section 2: `bg, panel, panel_2, panel_3, panel_4,
border, border_2, text, text_2, text_3, accent, accent_hover, accent_press, on_accent,
accent_wash, rec, ok, clip, grid_beat, grid_bar, step_off, step_off_alt, on_track,
track[8], shadow` (a `Shadow { offset: (0, 8), blur: 24, color }`), plus the knob-face and
piano-key gradient pairs, the spacing scale, the radii (4 / 3 / 2) and the type scale.
`Theme::apply` sets egui's `Style`: `text_styles` (Small 11, Body 13, Button 12, Heading 12
tracked caps via the kit's `TextKind`, Monospace 13), `spacing.item_spacing = (4, 4)`,
`button_padding = (10, 0)`, `interaction.tooltip_delay = 0.3`, `scroll` = always visible,
bar width 10, handle 6, inner margin 2, `bar_outer_margin 0`, floating false, colours per
section 4; `visuals.window_fill / panel_fill = panel`, popup / menu corner radius 4, popup
shadow = `shadow`, widget strokes = border-2, selection = accent-wash with accent stroke.
`contrast_ratio` and the AA tests stay, re-pointed at the new pairs (text-2 on panel and
panel-2; on-accent on accent and accent-hover; accent on panel; track colours on panel and
on-track on each swatch; ok / rec on panel at 3:1).

New or changed widgets: `panel` becomes a flat frame (fill, 1 px border, radius 4) with a
`window` variant (header + sub + keycap + collapse / close, toolbar, clipped body) and a
`group` variant (24 h head + knob rows); `toolbar` (40 h row with separators); `button` grows
variants `Secondary / Primary / Toggle / Solo / Rec / Icon / Ghost / Small / Xs` with the
state table of section 4; `segmented` (underline selection, small size); `dropdown` + the
single `menu` style; `switch`; `text_field` styling; `led`; `tag`; `chip`; `keycap`; `knob`
(32 / 22 / 40, bipolar, disabled, inline; arc geometry of section 4); `fader`, `meter`
(stereo / horizontal / GR); `slider` (vertical / horizontal with thumb); `step_cell`
(off / alt / hover / on / accent / playhead / note); `piano_roll` (ruler, key column, grid,
note bars by velocity, selection, ghosts, velocity lane); `tabs`; `list_row`; `rail`;
`drawer` (bar + open window + gutter); `insert_box`; `name_button`; `pattern_tile`;
`scrollbar` styling via egui `Style` (no custom painter); `piano_keys` restyled to the strip
keys; `visualizer` restyled (panel-2 well, accent stroke).

Retired: `shadow.rs` (the paired soft-shadow painter, `Depth`, `paint_soft_*`, the focus
circle), the pill shapes and `radius_pill`, `stepper` circles, the concentric `dial` and
`DialSize`, the playhead tint, the on-glow treatment, `value_edit`'s inset pill (an inline text
field replaces it), and the 24 pt panel radius. The focus ring becomes a 2 px accent rectangle
1 px outside the control, drawn only for keyboard focus.
