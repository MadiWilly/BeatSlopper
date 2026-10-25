//! M7 polyphonic-note tests: exact note-on/note-off samples, chords, notes that outlive their
//! pattern, the length-1 rule (and swing), the voice-pool bound, the grid lane as a derived
//! view, note messages through the queue, and the version-4 -> 5 migration.
//!
//! Everything renders at 48 kHz / 120 BPM (6000 frames per step, 96 000 per bar) with the
//! click-fast dry sine patch from `tests/common` (1 ms attack and release, sustain 1). A
//! note-off is located exactly by rendering the same pattern twice, once with the note under
//! test and once with that note lengthened, and finding the first sample where the two
//! outputs differ: the envelope leaves its sustain level on the very first release sample.

mod common;

use common::*;
use engine::pattern::{ACCENT_VELOCITY, MAX_NOTES, STEP_VELOCITY};
use engine::project::FORMAT_VERSION;
use engine::{Engine, MAX_VOICES, Message, Note, Pattern, PatternLength, Project, SynthStep};

/// Step length at 120 BPM / 48 kHz.
const SPP: usize = 6000;
/// One 16-step bar.
const BAR: usize = SPP * 16;
/// MIDI A4.
const A4: u8 = 69;

/// Index of the first sample where `a` and `b` differ (or where the shorter one ends).
fn first_difference(a: &[f32], b: &[f32]) -> Option<usize> {
    let n = a.len().min(b.len());
    (0..n)
        .find(|&i| a[i] != b[i])
        .or((a.len() != b.len()).then_some(n))
}

/// Index of the last audible sample.
fn last_sound(mono: &[f32]) -> Option<usize> {
    mono.iter().rposition(|x| x.abs() > ONSET_EPS)
}

/// A fresh sequencer engine whose edit pattern (A) holds exactly `notes`.
fn engine_with(notes: &[Note], length: PatternLength) -> Engine {
    let (_h, mut engine) = seq_engine();
    let mut p = Pattern::empty();
    p.set_length(length);
    for &n in notes {
        p.add_note(n).expect("valid note");
    }
    engine.set_pattern(&p);
    engine
}

/// Pitches of the voices currently held (not idle, not releasing), sorted.
fn held_pitches(engine: &Engine) -> Vec<u8> {
    let mut v: Vec<u8> = engine
        .voices()
        .voices()
        .iter()
        .filter(|v| !v.env().is_idle() && !v.env().is_releasing())
        .map(|v| v.note())
        .collect();
    v.sort_unstable();
    v
}

// (1) A note of length 3 on step 2: on at 2 x spp, off at 5 x spp - 1.
#[test]
fn note_of_length_three_at_step_two_sounds_from_2spp_to_5spp_minus_1() {
    let mut engine = engine_with(&[Note::new(2, 3, A4, 0.8)], PatternLength::Bar1);
    let mono = play_and_render(&mut engine, BAR);
    assert!(all_finite(&mono));
    assert_onsets(&onsets(&mono), &[2 * SPP]);
    assert!(is_silent(&mono[..2 * SPP]), "nothing before the note");

    // The same note one step longer is identical up to the shorter note's release sample.
    let mut longer = engine_with(&[Note::new(2, 4, A4, 0.8)], PatternLength::Bar1);
    let reference = play_and_render(&mut longer, BAR);
    assert_eq!(
        first_difference(&mono, &reference),
        Some(5 * SPP - 1),
        "note-off must land on the trigger sample of step 5 minus one"
    );
    // The 1 ms release tail starts there and the lane is silent soon after.
    let end = last_sound(&mono).unwrap();
    assert!(
        (5 * SPP - 1..5 * SPP + ms_to_frames(5.0)).contains(&end),
        "last audible sample {end}"
    );
    assert!(is_silent(&mono[5 * SPP + ms_to_frames(5.0)..]));

    // Held for the whole note, released afterwards: the release happens while sample index
    // 5 x spp - 1 is rendered, so it is visible once that many samples plus one are out.
    let mut engine = engine_with(&[Note::new(2, 3, A4, 0.8)], PatternLength::Bar1);
    engine.play();
    let _ = render(&mut engine, 5 * SPP - 1);
    assert_eq!(held_pitches(&engine), vec![A4]);
    let _ = render(&mut engine, 1);
    assert!(held_pitches(&engine).is_empty(), "released at 5 x spp - 1");
    assert_eq!(engine.active_voices(), 1, "still in its release tail");
}

// (2) Two notes with the same start (a chord) both sound, on two held voices.
#[test]
fn chord_notes_at_the_same_start_both_sound() {
    let notes = [Note::new(0, 4, 60, 0.8), Note::new(0, 4, 64, 1.0)];
    let mut engine = engine_with(&notes, PatternLength::Bar1);
    assert_eq!(
        engine.pattern().synth[0].note,
        Some(60),
        "the lane shows the lowest"
    );
    engine.play();
    let out = left(&render(&mut engine, 4 * SPP - 1));
    assert!(!is_silent(&out));
    assert_eq!(held_pitches(&engine), vec![60, 64], "two held voices");
    assert_eq!(engine.voices().held_voices(), 2);
    let _ = render(&mut engine, 1);
    assert!(
        held_pitches(&engine).is_empty(),
        "both released at 4 x spp - 1"
    );

    // A chord is louder than one of its notes alone: the output really is the sum.
    let mut single = engine_with(&notes[..1], PatternLength::Bar1);
    let one = play_and_render(&mut single, SPP);
    let mut chord = engine_with(&notes, PatternLength::Bar1);
    let two = play_and_render(&mut chord, SPP);
    assert!(rms(&two[100..]) > rms(&one[100..]) * 1.3);
}

// (3) A note that runs past the pattern end is released at the wrap; a queued pattern switch
// releases a note that would still be running.
#[test]
fn a_note_longer_than_the_pattern_is_released_at_the_wrap() {
    // Length 8 from step 14 of a one-bar pattern: cut at the pattern's last sample (BAR - 1),
    // exactly like a note that ends on the pattern end, then retriggered one bar later.
    let mut long = engine_with(&[Note::new(14, 8, A4, 0.8)], PatternLength::Bar1);
    let out = play_and_render(&mut long, 2 * BAR);
    let mut fitted = engine_with(&[Note::new(14, 2, A4, 0.8)], PatternLength::Bar1);
    let reference = play_and_render(&mut fitted, 2 * BAR);
    assert_eq!(
        out, reference,
        "a note past the end behaves like one ending at the end"
    );
    assert_onsets(&onsets(&out), &[14 * SPP, BAR + 14 * SPP]);

    // Against a two-bar pattern (where the note is not cut) the outputs diverge at BAR - 1.
    let mut room = engine_with(&[Note::new(14, 8, A4, 0.8)], PatternLength::Bar2);
    let uncut = play_and_render(&mut room, 2 * BAR);
    assert_eq!(first_difference(&out, &uncut), Some(BAR - 1));
    let mut long = engine_with(&[Note::new(14, 8, A4, 0.8)], PatternLength::Bar1);
    long.play();
    let _ = render(&mut long, BAR - 1);
    assert_eq!(held_pitches(&long), vec![A4]);
    let _ = render(&mut long, 1);
    assert!(held_pitches(&long).is_empty(), "released on the wrap");

    // A 32-step pattern with a note spanning all of it; B (empty) is queued during bar 0, so
    // the switch at the bar boundary releases the note.
    let (mut handle, mut engine) = seq_engine();
    let mut a = Pattern::empty();
    a.set_length(PatternLength::Bar2);
    a.add_note(Note::new(0, 32, A4, 0.8)).unwrap();
    engine.set_pattern(&a);
    engine.play();
    let _ = render(&mut engine, 100);
    assert!(handle.send(Message::SelectPattern(1)));
    let _ = render(&mut engine, BAR - 101);
    assert_eq!(
        held_pitches(&engine),
        vec![A4],
        "still held one sample before the switch"
    );
    let _ = render(&mut engine, 1);
    assert!(
        held_pitches(&engine).is_empty(),
        "released by the pattern switch"
    );
    assert_eq!(handle.position().pattern, 1);
    let tail = left(&render(&mut engine, BAR));
    assert!(is_silent(&tail[ms_to_frames(5.0)..]), "B is empty");
}

// (4) A length-1 note ends one sample before the next step's trigger, swing included, and a
// repeated pitch on the next step retriggers the same voice.
#[test]
fn length_one_note_ends_one_sample_before_the_next_trigger() {
    let mut one = engine_with(&[Note::new(0, 1, A4, 0.8)], PatternLength::Bar1);
    let short = play_and_render(&mut one, 2 * SPP);
    let mut two = engine_with(
        &[Note::new(0, 1, A4, 0.8), Note::new(1, 1, A4, 0.8)],
        PatternLength::Bar1,
    );
    let repeated = play_and_render(&mut two, 2 * SPP);
    let mut held = engine_with(&[Note::new(0, 2, A4, 0.8)], PatternLength::Bar1);
    let long = play_and_render(&mut held, 2 * SPP);
    assert_eq!(
        first_difference(&short, &long),
        Some(SPP - 1),
        "off at 1 x spp - 1"
    );
    assert_eq!(
        first_difference(&repeated, &long),
        Some(SPP - 1),
        "the repeated note is released for one sample before it retriggers"
    );
    assert!(
        !is_silent(&repeated[SPP..SPP + 100]),
        "the retrigger is click-free: no silence at the step boundary"
    );
    assert_eq!(two.active_voices(), 1, "same pitch, same voice");
    // The second note itself ends at 2 x spp - 1 (one engine, two steps rendered).
    let mut held2 = engine_with(
        &[Note::new(0, 1, A4, 0.8), Note::new(1, 2, A4, 0.8)],
        PatternLength::Bar1,
    );
    let longer2 = play_and_render(&mut held2, 3 * SPP);
    let mut two = engine_with(
        &[Note::new(0, 1, A4, 0.8), Note::new(1, 1, A4, 0.8)],
        PatternLength::Bar1,
    );
    let repeated = play_and_render(&mut two, 3 * SPP);
    assert_eq!(first_difference(&repeated, &longer2), Some(2 * SPP - 1));

    // With swing 0.5, step 1 triggers at 1.5 x spp: a note on step 0 ends at 1.5 x spp - 1 and
    // a note on step 1 ends at 2 x spp - 1 (the following even step is not swung).
    let mut e = engine_with(&[Note::new(0, 1, A4, 0.8)], PatternLength::Bar1);
    e.set_swing(0.5);
    let swung = play_and_render(&mut e, 3 * SPP);
    let mut r = engine_with(&[Note::new(0, 3, A4, 0.8)], PatternLength::Bar1);
    r.set_swing(0.5);
    let reference = play_and_render(&mut r, 3 * SPP);
    assert_eq!(
        first_difference(&swung, &reference),
        Some(SPP + SPP / 2 - 1)
    );
    let mut e = engine_with(&[Note::new(1, 1, A4, 0.8)], PatternLength::Bar1);
    e.set_swing(0.5);
    let swung = play_and_render(&mut e, 3 * SPP);
    assert_onsets(&onsets(&swung), &[SPP + SPP / 2]);
    let mut r = engine_with(&[Note::new(1, 2, A4, 0.8)], PatternLength::Bar1);
    r.set_swing(0.5);
    let reference = play_and_render(&mut r, 3 * SPP);
    assert_eq!(first_difference(&swung, &reference), Some(2 * SPP - 1));
}

// (5) Polyphony is bounded by the voice pool: a 17-note chord holds 16 voices, and every
// note is released again.
#[test]
fn polyphony_is_limited_by_the_voice_pool() {
    let (_h, mut engine) = seq_engine();
    let mut p = Pattern::empty();
    for i in 0..(MAX_VOICES as u8 + 1) {
        p.add_note(Note::new(0, 2, 48 + i, 0.8)).unwrap();
    }
    assert_eq!(p.notes().len(), MAX_VOICES + 1);
    engine.set_pattern(&p);
    engine.play();
    let _ = render(&mut engine, 10);
    assert_eq!(engine.voices().held_voices(), MAX_VOICES);
    assert_eq!(engine.active_voices(), MAX_VOICES);
    let _ = render(&mut engine, 2 * SPP);
    assert_eq!(
        engine.voices().held_voices(),
        0,
        "all released at 2 x spp - 1"
    );
    let tail = left(&render(&mut engine, ms_to_frames(20.0)));
    assert!(is_silent(&tail[ms_to_frames(10.0)..]));
    assert_eq!(MAX_NOTES, 64);
}

// (6) set_synth_step upserts a length-1 note and the lane view matches.
#[test]
fn set_synth_step_upserts_a_length_one_note_and_the_lane_follows() {
    let (_h, mut engine) = seq_engine();
    engine.set_synth_step(3, Some(60), true);
    let p = engine.pattern();
    assert_eq!(p.notes(), &[Note::new(3, 1, 60, ACCENT_VELOCITY)]);
    assert_eq!(
        p.synth[3],
        SynthStep {
            note: Some(60),
            accent: true
        }
    );

    // Changing the pitch replaces the note rather than stacking a second one.
    engine.set_synth_step(3, Some(62), false);
    let p = engine.pattern();
    assert_eq!(p.notes(), &[Note::new(3, 1, 62, STEP_VELOCITY)]);
    assert_eq!(p.synth[3].note, Some(62));
    assert!(!p.synth[3].accent);

    // A chord added through the note API shows its lowest pitch in the lane; the grid upsert
    // then removes every note at that step.
    engine.set_note(0, 1, Note::new(3, 2, 50, 0.8));
    engine.set_note(0, 2, Note::new(3, 2, 70, 0.8));
    assert_eq!(engine.pattern().notes().len(), 3);
    assert_eq!(engine.pattern().synth[3].note, Some(50));
    engine.set_synth_step(3, Some(64), false);
    assert_eq!(
        engine.pattern().notes(),
        &[Note::new(3, 1, 64, STEP_VELOCITY)]
    );
    assert_eq!(engine.pattern().synth[3].note, Some(64));
    engine.set_synth_step(3, None, false);
    assert!(engine.pattern().notes().is_empty());
    assert_eq!(engine.pattern().synth[3], SynthStep::default());

    // Every lane cell agrees with the notes for a busy pattern.
    engine.set_pattern(&Pattern::demo());
    let p = engine.pattern();
    for s in 0..p.len() {
        let lowest = p
            .notes()
            .iter()
            .filter(|n| usize::from(n.start) == s)
            .map(|n| n.pitch)
            .min();
        assert_eq!(p.synth[s].note, lowest, "step {s}");
    }
    assert_eq!(p.notes().len(), 9);
    assert!(p.notes().iter().all(|n| n.length == 1));
}

// (7) rebuild_lane picks the lowest pitch per step, and its velocity decides the accent.
#[test]
fn rebuild_lane_picks_the_lowest_pitch() {
    let mut p = Pattern::empty();
    p.add_note(Note::new(5, 1, 72, 1.0)).unwrap();
    p.add_note(Note::new(5, 2, 60, 0.8)).unwrap();
    p.add_note(Note::new(5, 1, 67, 1.0)).unwrap();
    assert_eq!(
        p.synth[5],
        SynthStep {
            note: Some(60),
            accent: false
        }
    );
    assert_eq!(
        p.synth[6],
        SynthStep::default(),
        "a held step is a rest in the lane"
    );
    p.remove_note(p.note_at(5, 60).unwrap());
    assert_eq!(
        p.synth[5],
        SynthStep {
            note: Some(67),
            accent: true
        }
    );
    // By-hand edits need an explicit rebuild, which does not rely on the order.
    p.notes[p.notes().len()] = Note::new(5, 1, 40, 0.5);
    p.note_count += 1;
    assert_eq!(p.synth[5].note, Some(67), "not yet rebuilt");
    p.rebuild_lane();
    assert_eq!(
        p.synth[5].note,
        Some(40),
        "appended out of order, still lowest"
    );
    assert!(!p.synth[5].accent);
    // normalise sorts as well, so the order no longer matters.
    p.normalise();
    assert_eq!(p.notes()[0], Note::new(5, 1, 40, 0.5));
    assert_eq!(p.notes()[1], Note::new(5, 1, 67, 1.0));
}

/// A literal version-4 project (as the M6 build wrote it) with a 16-step lane in pattern A:
/// an accented C3 on step 0, an unaccented G3 on step 7.
fn v4_json() -> String {
    let empty_drum = r#"{ "on": false, "accent": false }"#;
    let row = vec![empty_drum; 16].join(", ");
    let synth: Vec<String> = (0..16)
        .map(|i| match i {
            0 => r#"{ "note": 48, "accent": true }"#.to_owned(),
            7 => r#"{ "note": 55, "accent": false }"#.to_owned(),
            _ => r#"{ "note": null, "accent": false }"#.to_owned(),
        })
        .collect();
    let pattern_a = format!(
        r#"{{ "length": 16, "drums": [[{row}], [{row}], [{row}], [{row}], [{row}]], "synth": [{synth}],
            "drum_volume": [0.0, 0.0, 0.0, 0.0, 0.0], "drum_tune": [0.0, 0.0, 0.0, 0.0, 0.0] }}"#,
        synth = synth.join(", ")
    );
    format!(
        r#"{{
  "version": 4,
  "bpm": 110.0,
  "swing": 0.2,
  "bank": {{
    "patterns": [{pattern_a}],
    "chain": [ {{ "pattern": 0, "repeats": 1 }} ],
    "mode": "pattern",
    "current": 0
  }},
  "params": {{ "osc1_wave": 2.0 }},
  "preset_index": 1,
  "octave": 3,
  "velocity": 0.7
}}
"#
    )
}

// (8) A version-4 file's lane becomes length-1 notes and re-saves as version 5 without `synth`.
#[test]
fn v4_json_with_a_lane_migrates_to_length_one_notes_and_round_trips_as_v5() {
    let p = Project::from_json(&v4_json()).expect("v4 file loads");
    assert_eq!(p.version, FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 5);
    assert_eq!(p.bpm, 110.0);
    let a = &p.bank.patterns[0];
    assert_eq!(
        a.notes(),
        &[
            Note::new(0, 1, 48, ACCENT_VELOCITY),
            Note::new(7, 1, 55, STEP_VELOCITY)
        ]
    );
    assert_eq!(
        a.synth[0],
        SynthStep {
            note: Some(48),
            accent: true
        }
    );
    assert_eq!(
        a.synth[7],
        SynthStep {
            note: Some(55),
            accent: false
        }
    );
    assert_eq!(a.synth.iter().filter(|s| s.note.is_some()).count(), 2);
    assert!(p.bank.patterns[1..].iter().all(Pattern::is_empty));

    let v5 = p.to_json_pretty();
    assert!(v5.starts_with("{\n  \"version\": 5,"));
    assert!(!v5.contains("\"synth\""));
    assert!(v5.contains("\"notes\": ["));
    assert!(v5.contains("\"pitch\": 48"));
    let value: serde_json::Value = serde_json::from_str(&v5).unwrap();
    assert_eq!(
        value["bank"]["patterns"][0]["notes"],
        serde_json::json!([
            { "start": 0, "length": 1, "pitch": 48, "velocity": 1.0 },
            { "start": 7, "length": 1, "pitch": 55, "velocity": 0.8 }
        ])
    );
    let back = Project::from_json(&v5).unwrap();
    assert_eq!(back, p);
    assert_eq!(back.to_json_pretty(), v5, "re-saving is byte-identical");

    // The migrated pattern plays the same two hits through the scheduler.
    let (_h, mut engine) = seq_engine();
    engine.set_bank(&p.bank);
    assert_eq!(engine.pattern().notes().len(), 2);
    let mono = play_and_render(&mut engine, BAR);
    assert_eq!(onsets(&mono).len(), 2);

    // A version-5 project with held notes and chords round-trips through the file format.
    let mut project = Project::default();
    project.bank.patterns[2].set_length(PatternLength::Bar2);
    project.bank.patterns[2]
        .add_note(Note::new(0, 8, 60, 0.6))
        .unwrap();
    project.bank.patterns[2]
        .add_note(Note::new(0, 8, 64, 0.6))
        .unwrap();
    project.bank.patterns[2]
        .add_note(Note::new(30, 4, 72, 1.0))
        .unwrap();
    let json = project.to_json_pretty();
    let back = Project::from_json(&json).unwrap();
    assert_eq!(back, project);
    assert_eq!(back.bank.patterns[2].notes().len(), 3);
    assert_eq!(back.bank.patterns[2].synth[0].note, Some(60));
    assert_eq!(back.bank.patterns[2].synth[30].note, Some(72));
}

// (9) SetNote / RemoveNote / ClearNotes through the queue, with the lane kept in step.
#[test]
fn note_messages_through_the_queue() {
    let (mut handle, mut engine) = seq_engine();
    let c = Note::new(0, 4, 60, 0.8);
    let e = Note::new(0, 4, 64, 0.8);
    let g = Note::new(2, 2, 67, 1.0);
    for m in [
        Message::SetNote {
            pattern: 1,
            index: 0,
            note: e,
        }, // append to empty
        Message::SetNote {
            pattern: 1,
            index: 1,
            note: c,
        }, // append; sorts before e
        Message::SetNote {
            pattern: 1,
            index: 2,
            note: g,
        }, // append
        Message::SetNote {
            pattern: 1,
            index: 7,
            note: g,
        }, // out of range: ignored
        Message::SetNote {
            pattern: 1,
            index: 3,
            note: Note {
                start: 20,
                length: 1,
                pitch: 50,
                velocity: 0.8,
            },
        }, // start beyond the length: ignored
    ] {
        assert!(handle.send(m));
    }
    let _ = render(&mut engine, 1);
    let b = engine.bank().pattern(1);
    assert_eq!(b.notes(), &[c, e, g]);
    assert_eq!(b.synth[0].note, Some(60));
    assert_eq!(
        b.synth[2],
        SynthStep {
            note: Some(67),
            accent: true
        }
    );
    assert!(engine.bank().pattern(0).notes().is_empty(), "A untouched");

    // Replace: the note moves in the sorted list and the lane follows.
    let moved = Note::new(5, 1, 60, 0.8);
    assert!(handle.send(Message::SetNote {
        pattern: 1,
        index: 0,
        note: moved
    }));
    let _ = render(&mut engine, 1);
    let b = engine.bank().pattern(1);
    assert_eq!(b.notes(), &[e, g, moved]);
    assert_eq!(b.synth[0].note, Some(64));
    assert_eq!(b.synth[5].note, Some(60));

    // Remove (and an ignored out-of-range remove), then clear.
    assert!(handle.send(Message::RemoveNote {
        pattern: 1,
        index: 1
    }));
    assert!(handle.send(Message::RemoveNote {
        pattern: 1,
        index: 9
    }));
    let _ = render(&mut engine, 1);
    let b = engine.bank().pattern(1);
    assert_eq!(b.notes(), &[e, moved]);
    assert_eq!(b.synth[2], SynthStep::default());
    assert!(handle.send(Message::ClearNotes(1)));
    let _ = render(&mut engine, 1);
    assert!(engine.bank().pattern(1).notes().is_empty());
    assert!(
        engine
            .bank()
            .pattern(1)
            .synth
            .iter()
            .all(|s| s.note.is_none())
    );

    // The direct API reports indices; the grid message still writes length-1 notes.
    assert_eq!(engine.set_note(1, 0, g), Some(0));
    assert_eq!(engine.set_note(1, 1, c), Some(0));
    assert_eq!(engine.set_note(1, 5, c), None);
    assert_eq!(engine.set_note(1, 0, Note::new(3, 1, 61, 0.8)), Some(1));
    assert_eq!(
        engine.bank().pattern(1).notes(),
        &[g, Note::new(3, 1, 61, 0.8)]
    );
    assert!(engine.remove_note(1, 0));
    assert!(!engine.remove_note(1, 4));
    engine.set_edit_pattern(1);
    assert!(handle.send(Message::SetSynthStep {
        step: 3,
        note: Some(40),
        accent: true
    }));
    let _ = render(&mut engine, 1);
    assert_eq!(
        engine.pattern().notes(),
        &[Note::new(3, 1, 40, ACCENT_VELOCITY)]
    );
    assert!(handle.send(Message::ClearSynth));
    let _ = render(&mut engine, 1);
    assert!(engine.pattern().notes().is_empty());
    engine.clear_notes(1);
    assert!(engine.pattern().is_empty());

    // Messages stay Copy.
    let m = Message::SetNote {
        pattern: 0,
        index: 0,
        note: c,
    };
    let copy = m;
    assert_eq!(m, copy);
}

// (10) Editing the playing pattern while a note sounds does not disturb that note, and a
// keyboard-held pitch survives a sequencer note of the same pitch ending.
#[test]
fn sounding_notes_survive_edits_and_keyboard_holds_survive_note_offs() {
    let mut engine = engine_with(&[Note::new(0, 4, A4, 0.8)], PatternLength::Bar1);
    engine.play();
    let _ = render(&mut engine, SPP);
    assert_eq!(held_pitches(&engine), vec![A4]);
    engine.clear_notes(0);
    let _ = render(&mut engine, 2 * SPP);
    assert_eq!(
        held_pitches(&engine),
        vec![A4],
        "the running note keeps its off time"
    );
    let _ = render(&mut engine, SPP);
    assert!(
        held_pitches(&engine).is_empty(),
        "and ends at 4 x spp - 1 as scheduled"
    );

    let mut engine = engine_with(
        &[Note::new(0, 1, A4, 0.8), Note::new(0, 1, 60, 0.8)],
        PatternLength::Bar1,
    );
    engine.note_on(A4, 1.0);
    engine.play();
    let _ = render(&mut engine, 2 * SPP);
    assert_eq!(
        held_pitches(&engine),
        vec![A4],
        "the key is still held, C4 was released"
    );
    engine.stop();
    assert_eq!(held_pitches(&engine), vec![A4]);
    engine.note_off(A4);
    assert!(held_pitches(&engine).is_empty());
}
