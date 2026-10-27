//! M9 preset library tests: every `LIBRARY` preset is in range, applies without panic,
//! renders a finite, non-silent, limited note and decays after release (to silence, or to
//! its own vinyl-noise bed, unless tagged "drone"); the browser helpers behave; a handful of
//! flagship presets have the measurable character their names promise.

mod common;

use std::f64::consts::PI;

use common::*;
use engine::presets::{self, ALL, Category, LIBRARY, LIBRARY_COUNT, Preset};
use engine::{Engine, ParamId, new_engine};

/// Middle C and C2 (65.4 Hz).
const C4: u8 = 60;
const C2: u8 = 36;

/// Anything below this RMS over a one-second window counts as silence.
const SILENCE_RMS: f32 = 1.0e-4;
/// A note must be at least this loud (RMS over its first second).
const AUDIBLE_RMS: f32 = 1.0e-3;
/// Release tail budget, seconds.
const TAIL_SECONDS: usize = 8;

fn half_second() -> usize {
    ms_to_frames(500.0)
}

/// A fresh engine with `preset` applied (track ids on the edit track 0, globals too) and
/// its smoothers settled by a short silent render.
fn engine_with(preset: &Preset) -> Engine {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    preset.apply(&mut engine);
    let warm = render(&mut engine, ms_to_frames(200.0));
    assert!(all_finite(&warm), "{}: warm-up not finite", preset.name);
    engine
}

/// RMS of the preset's steady noise bed with no note playing (the lo-fi vinyl hiss is not
/// gated by the voices), measured over three seconds after a settle.
fn noise_bed(preset: &Preset) -> f32 {
    if !preset.has_vinyl_noise() {
        return 0.0;
    }
    let mut engine = engine_with(preset);
    let _ = render(&mut engine, ms_to_frames(500.0));
    rms(&render(&mut engine, ms_to_frames(3000.0)))
}

/// Plays `note` for `hold_ms`, releases it, then renders half-second blocks until a full
/// one-second window falls below `threshold` (returns the seconds after release that took)
/// or `TAIL_SECONDS` run out (`None`). Also returns the held note's stereo render and the
/// last one-second window's RMS.
struct Note {
    held: Vec<f32>,
    tail_seconds: Option<f32>,
    last_window_rms: f32,
    peak: f32,
}

fn play(preset: &Preset, note: u8, hold_ms: f32, threshold: f32) -> Note {
    let mut engine = engine_with(preset);
    engine.note_on(note, 1.0);
    let held = render(&mut engine, ms_to_frames(hold_ms));
    assert!(all_finite(&held), "{}: held note not finite", preset.name);
    let mut peak_level = peak(&held);
    engine.note_off(note);
    let mut prev = render(&mut engine, half_second());
    assert!(all_finite(&prev), "{}: tail not finite", preset.name);
    peak_level = peak_level.max(peak(&prev));
    let mut tail_seconds = None;
    let mut last_window_rms = f32::MAX;
    for block in 1..TAIL_SECONDS * 2 {
        let cur = render(&mut engine, half_second());
        assert!(all_finite(&cur), "{}: tail not finite", preset.name);
        peak_level = peak_level.max(peak(&cur));
        let window: Vec<f32> = prev.iter().chain(&cur).copied().collect();
        last_window_rms = rms(&window);
        if last_window_rms < threshold {
            tail_seconds = Some((block + 1) as f32 * 0.5);
            break;
        }
        prev = cur;
    }
    Note {
        held,
        tail_seconds,
        last_window_rms,
        peak: peak_level,
    }
}

/// Silence threshold for `preset`: strict silence, or twice its vinyl bed.
fn threshold_for(preset: &Preset) -> f32 {
    SILENCE_RMS.max(2.0 * noise_bed(preset))
}

/// Renders every preset of `category` through a C4 note and checks the M9 contract.
fn check_category(category: Category) {
    let presets: Vec<&Preset> = presets::by_category(category).collect();
    assert!(
        presets.len() >= 10,
        "{} has only {} presets",
        category.label(),
        presets.len()
    );
    for p in presets {
        let threshold = threshold_for(p);
        let n = play(p, C4, 1000.0, threshold);
        assert!(all_finite(&n.held), "{}", p.name);
        let level = rms(&left(&n.held));
        assert!(
            level > AUDIBLE_RMS,
            "{}: note is silent (rms {level})",
            p.name
        );
        assert!(
            n.peak <= 1.0,
            "{}: peak {} after the limiter",
            p.name,
            n.peak
        );
        if p.is_drone() {
            assert!(
                n.last_window_rms >= SILENCE_RMS,
                "{}: tagged drone but silent after {TAIL_SECONDS} s (rms {})",
                p.name,
                n.last_window_rms
            );
        } else {
            assert!(
                n.tail_seconds.is_some(),
                "{}: still {} rms {TAIL_SECONDS} s after release (threshold {threshold})",
                p.name,
                n.last_window_rms
            );
        }
    }
}

#[test]
fn lofi_keys_render_and_decay() {
    check_category(Category::LofiKeys);
}

#[test]
fn lofi_guitars_render_and_decay() {
    check_category(Category::LofiGuitars);
}

#[test]
fn lofi_pads_render_and_decay() {
    check_category(Category::LofiPads);
}

#[test]
fn edm_leads_render_and_decay() {
    check_category(Category::EdmLeads);
}

#[test]
fn edm_bass_render_and_decay() {
    check_category(Category::EdmBass);
}

#[test]
fn edm_plucks_render_and_decay() {
    check_category(Category::EdmPlucks);
}

#[test]
fn edm_pads_render_and_decay() {
    check_category(Category::EdmPads);
}

#[test]
fn risers_render_and_decay() {
    check_category(Category::Risers);
}

// The Sound Pack (generated by examples/gen_preset_pack.rs; it checks the same contract with
// a margin, these tests hold the checked-in file to it).

#[test]
fn pack_keys_render_and_decay() {
    check_category(Category::Keys);
}

#[test]
fn pack_organs_render_and_decay() {
    check_category(Category::Organs);
}

#[test]
fn pack_brass_winds_render_and_decay() {
    check_category(Category::BrassWinds);
}

#[test]
fn pack_strings_render_and_decay() {
    check_category(Category::Strings);
}

#[test]
fn pack_bells_mallets_render_and_decay() {
    check_category(Category::BellsMallets);
}

#[test]
fn pack_guitars_render_and_decay() {
    check_category(Category::Guitars);
}

#[test]
fn pack_synth_leads_render_and_decay() {
    check_category(Category::SynthLeads);
}

#[test]
fn pack_synth_bass_render_and_decay() {
    check_category(Category::SynthBass);
}

#[test]
fn pack_pads_atmos_render_and_decay() {
    check_category(Category::PadsAtmos);
}

#[test]
fn pack_arps_stabs_render_and_decay() {
    check_category(Category::ArpsStabs);
}

#[test]
fn the_pack_is_a_thousand_presets_a_hundred_per_category_after_the_core() {
    assert_eq!(presets::PACK.len(), 1000);
    let pack_categories = &Category::ALL[Category::Risers.index() + 1..];
    assert_eq!(pack_categories.len(), 10);
    for &c in pack_categories {
        assert_eq!(presets::by_category(c).count(), 100, "{}", c.label());
    }
    assert!(
        presets::PACK
            .iter()
            .all(|p| p.category.index() > Category::Risers.index())
    );
    // The pack follows the hand-made core in LIBRARY.
    let core = LIBRARY_COUNT - presets::PACK.len();
    assert_eq!(core, 91);
    for (i, p) in presets::PACK.iter().enumerate() {
        assert_eq!(LIBRARY[core + i].name, p.name);
        assert!(p.tags.len() >= 3, "{} has too few tags", p.name);
        assert!(!p.is_drone(), "{}: the pack has no drones", p.name);
    }
}

/// Loudest 200 ms RMS (dBFS, windows 50 ms apart) of `note` held 1.5 s at velocity 0.8 (the
/// generator's measure).
fn loudness_db(preset: &Preset, note: u8) -> f32 {
    let mut engine = engine_with(preset);
    engine.note_on(note, 0.8);
    loudest_200ms_db(&left(&render(&mut engine, ms_to_frames(1500.0))))
}

/// Loudest 200 ms RMS of a mono buffer, dBFS, windows 50 ms apart.
fn loudest_200ms_db(buf: &[f32]) -> f32 {
    let window = ms_to_frames(200.0);
    let loudest = (0..=buf.len() - window)
        .step_by(ms_to_frames(50.0))
        .map(|i| rms(&buf[i..i + window]))
        .fold(0.0, f32::max);
    20.0 * loudest.max(1.0e-9).log10()
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

/// Guitars used to come out 14 dB under the keys (the plucked string's burst lost most of
/// its energy in the loop filter): the lo-fi guitars must not be quieter than the lo-fi keys
/// and sit at the pack's level, the pack guitars with the pack keys.
#[test]
fn guitars_are_as_loud_as_keys() {
    let level = |c: Category, note: u8| {
        median(
            presets::by_category(c)
                .step_by(3)
                .map(|p| loudness_db(p, note))
                .collect(),
        )
    };
    let keys = level(Category::LofiKeys, C4);
    let lofi_guitars = level(Category::LofiGuitars, C4);
    assert!(
        lofi_guitars >= keys - 3.0 && (lofi_guitars + 18.0).abs() <= 4.0,
        "lofi guitars {lofi_guitars:.1} dB (lofi keys {keys:.1} dB, pack level -18 dB)"
    );
    let pack_keys = level(Category::Keys, C4);
    let pack_guitars = level(Category::Guitars, C4);
    assert!(
        (pack_guitars - pack_keys).abs() <= 3.0,
        "pack guitars {pack_guitars:.1} dB vs pack keys {pack_keys:.1} dB"
    );
    // A guitar holds its level across the neck (C2 .. C5).
    let nylon = presets::find("Nylon Guitar").unwrap();
    let levels: Vec<f32> = [36, 48, 60, 72]
        .iter()
        .map(|&n| loudness_db(nylon, n))
        .collect();
    let spread = levels.iter().copied().fold(f32::MIN, f32::max)
        - levels.iter().copied().fold(f32::MAX, f32::min);
    assert!(spread <= 9.0, "Nylon Guitar across C2..C5: {levels:?}");
}

/// The sequencer re-plucks a string that is still ringing (16ths on a long-ringing guitar):
/// the pick damps the string first, so repeated notes stay near a single hit's level instead
/// of piling energy into the loop and pushing the limiter.
#[test]
fn repeated_plucks_do_not_pile_up() {
    for name in [
        "Concert Harp",
        "Steel Acoustic",
        "Harp Dust",
        "Overdrive Guitar",
        "Nylon Guitar",
    ] {
        let p = presets::find(name).expect(name);
        let mut engine = engine_with(p);
        engine.note_on(52, 0.8);
        let single = loudest_200ms_db(&left(&render(&mut engine, ms_to_frames(2000.0))));
        // 32 sixteenths at 120 BPM with a 50 % gate.
        let mut engine = engine_with(p);
        let mut out = Vec::new();
        for _ in 0..32 {
            engine.note_on(52, 0.8);
            out.extend(left(&render(&mut engine, ms_to_frames(62.5))));
            engine.note_off(52);
            out.extend(left(&render(&mut engine, ms_to_frames(62.5))));
        }
        assert!(all_finite(&out), "{name}");
        let tail = &out[out.len() - ms_to_frames(2000.0)..];
        let repeated = loudest_200ms_db(tail);
        assert!(
            repeated - single <= 6.0,
            "{name}: repeated 16ths {repeated:.1} dB vs one hit {single:.1} dB"
        );
        assert!(peak(tail) <= 1.0, "{name}: peak {}", peak(tail));
    }
}

/// Every pack category is levelled into the generator's window (-27 .. -11 dB, centred on
/// -18); sampled, the medians sit within a few dB of each other.
#[test]
fn pack_categories_are_levelled() {
    for &c in &Category::ALL[Category::Risers.index() + 1..] {
        let note = if c == Category::SynthBass { C2 } else { C4 };
        let levels: Vec<f32> = presets::by_category(c)
            .step_by(7)
            .map(|p| loudness_db(p, note))
            .collect();
        for l in &levels {
            assert!((-30.0..=-8.0).contains(l), "{}: {l:.1} dB", c.label());
        }
        let m = median(levels);
        assert!(
            (-23.0..=-13.0).contains(&m),
            "{} median {m:.1} dB",
            c.label()
        );
    }
}

#[test]
fn library_has_at_least_eighty_presets_grouped_by_category() {
    assert!(LIBRARY.len() >= 80, "{} presets", LIBRARY.len());
    assert_eq!(LIBRARY_COUNT, LIBRARY.len());
    let mut total = 0;
    for c in Category::ALL {
        let in_category: Vec<&Preset> = presets::by_category(c).collect();
        assert!(
            in_category.len() >= 10,
            "{}: {}",
            c.label(),
            in_category.len()
        );
        assert!(in_category.iter().all(|p| p.category == c));
        total += in_category.len();
        // Grouped: the category's presets are contiguous in LIBRARY, in library order.
        let first = LIBRARY.iter().position(|p| p.category == c).unwrap();
        for (i, p) in in_category.iter().enumerate() {
            assert!(
                std::ptr::eq(LIBRARY[first + i].name, p.name),
                "{} not grouped",
                p.name
            );
        }
    }
    assert_eq!(total, LIBRARY_COUNT);
    let mut last = 0;
    for p in LIBRARY {
        assert!(
            p.category.index() >= last,
            "{} breaks category order",
            p.name
        );
        last = p.category.index();
        assert!(!p.tags.is_empty(), "{} has no tags", p.name);
    }
}

#[test]
fn values_are_in_range_finite_and_track_complete() {
    for p in LIBRARY {
        for (i, (id, v)) in p.values.iter().enumerate() {
            assert!(v.is_finite(), "{} {id:?} = {v}", p.name);
            assert!(id.range().contains(v), "{} {id:?} = {v}", p.name);
            assert!(
                p.values[..i].iter().all(|(other, _)| other != id),
                "{} lists {id:?} twice",
                p.name
            );
            assert!(
                !matches!(id, ParamId::MasterGain | ParamId::LimiterOn),
                "{} must not set the master stage",
                p.name
            );
        }
        for id in ParamId::ALL {
            let v = p.value(*id);
            assert!(
                v.is_finite() && id.range().contains(&v),
                "{} {id:?} = {v}",
                p.name
            );
        }
        // Every library preset decides the insert explicitly (lo-fi, chorus, sidechain, drive).
        for id in [
            ParamId::DriveMix,
            ParamId::LofiMix,
            ParamId::ChorusMix,
            ParamId::SidechainAmount,
        ] {
            assert!(p.lists(id), "{} does not set {id:?}", p.name);
        }
    }
}

#[test]
fn names_are_unique_across_library_and_all() {
    let mut names: Vec<String> = LIBRARY
        .iter()
        .chain(ALL.iter())
        .map(|p| p.name.to_ascii_lowercase())
        .collect();
    let count = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), count, "duplicate preset names");
    assert_eq!(ALL.len(), 8);
    assert_eq!(
        ALL.iter().map(|p| p.name).collect::<Vec<_>>(),
        [
            "Bass", "Sub", "Lead", "Pluck", "Pad", "Keys", "Acid", "Bell"
        ]
    );
}

#[test]
fn presets_differ_pairwise_in_at_least_four_track_parameters() {
    let everything: Vec<&Preset> = LIBRARY.iter().chain(ALL.iter()).collect();
    for (i, a) in everything.iter().enumerate() {
        for b in &everything[i + 1..] {
            let differing = ParamId::ALL
                .iter()
                .filter(|id| id.is_track() && a.value(**id) != b.value(**id))
                .count();
            assert!(
                differing >= 4,
                "{} vs {} differ in only {differing} track parameters",
                a.name,
                b.name
            );
        }
    }
}

#[test]
fn find_and_index_of_agree_and_search_works() {
    for (i, p) in LIBRARY.iter().enumerate() {
        assert_eq!(presets::index_of(p.name), Some(i), "{}", p.name);
        assert_eq!(presets::index_of(&p.name.to_ascii_uppercase()), Some(i));
        let found = presets::find(p.name).expect(p.name);
        assert!(std::ptr::eq(found, &LIBRARY[i]), "{}", p.name);
        assert!(std::ptr::eq(
            presets::find(&p.name.to_ascii_lowercase()).unwrap(),
            &LIBRARY[i]
        ));
    }
    for p in ALL {
        assert!(presets::find(p.name).is_some());
        assert!(presets::index_of(p.name).is_none());
    }
    assert!(presets::find("No Such Preset").is_none());
    assert!(presets::index_of("No Such Preset").is_none());

    assert_eq!(presets::search("").len(), LIBRARY_COUNT);
    assert_eq!(presets::search("  ").len(), LIBRARY_COUNT);
    let pianos = presets::search("piano");
    let names: Vec<&str> = pianos.iter().map(|p| p.name).collect();
    assert!(names.contains(&"Lofi Piano"), "{names:?}");
    assert!(names.contains(&"Toy Piano"), "{names:?}");
    for p in &pianos {
        assert!(
            p.name.to_ascii_lowercase().contains("piano") || p.has_tag("piano"),
            "{}",
            p.name
        );
    }
    assert_eq!(presets::search("PIANO").len(), pianos.len());
    let lofi = presets::search("lofi");
    assert!(lofi.len() >= 30, "{}", lofi.len());
    assert!(
        lofi.iter()
            .all(|p| p.has_tag("lofi") || p.name.to_ascii_lowercase().contains("lofi"))
    );
    let edm = presets::search("edm");
    assert!(edm.len() >= 40, "{}", edm.len());
    let drones = presets::search("drone");
    assert!(!drones.is_empty());
    assert!(drones.iter().all(|p| p.is_drone()));
    assert!(presets::search("zzzz-nothing").is_empty());
    // Library order is kept.
    let indices: Vec<usize> = presets::search("pad")
        .iter()
        .map(|p| presets::index_of(p.name).unwrap())
        .collect();
    assert!(indices.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn apply_to_track_sets_every_track_parameter_and_leaves_globals_alone() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_param(ParamId::DelayMix, 0.77);
    engine.set_param(ParamId::ReverbSize, 0.11);
    for p in LIBRARY {
        p.apply_to_track(&mut engine, 2);
        for id in ParamId::ALL {
            if id.is_track() {
                let got = engine.track_param(2, *id);
                assert!(
                    (got - p.value(*id)).abs() < 1.0e-6,
                    "{} {id:?}: track has {got}, preset says {}",
                    p.name,
                    p.value(*id)
                );
            }
        }
        assert_eq!(engine.param(ParamId::DelayMix), 0.77, "{}", p.name);
        assert_eq!(engine.param(ParamId::ReverbSize), 0.11, "{}", p.name);
        // Track 0 (the edit track) is untouched by a track-2 apply.
        for id in ParamId::ALL {
            if id.is_track() {
                assert_eq!(engine.track_param(0, *id), id.default_value(), "{}", p.name);
            }
        }
    }
    // A note on the track still renders after the last apply.
    engine.set_track_active(2, true);
    engine.track_note_on(2, C4, 1.0);
    let out = render(&mut engine, ms_to_frames(500.0));
    assert!(all_finite(&out));
    assert!(rms(&out) > AUDIBLE_RMS);
}

// --- flagship character checks -------------------------------------------------------------

/// Magnitude spectrum (Hann window) of the first `n` samples of `mono` at every DFT bin up
/// to `max_hz`: `(hz, magnitude)` pairs. Naive DFT; fine for a few thousand samples.
fn spectrum(mono: &[f32], n: usize, max_hz: f32) -> Vec<(f32, f64)> {
    let n = n.min(mono.len());
    let bins = ((max_hz / SAMPLE_RATE) * n as f32).ceil() as usize;
    let windowed: Vec<f64> = (0..n)
        .map(|i| {
            let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos();
            f64::from(mono[i]) * w
        })
        .collect();
    (1..bins.min(n / 2))
        .map(|k| {
            let (mut re, mut im) = (0.0_f64, 0.0_f64);
            let step = 2.0 * PI * k as f64 / n as f64;
            for (i, x) in windowed.iter().enumerate() {
                let a = step * i as f64;
                re += x * a.cos();
                im -= x * a.sin();
            }
            (
                k as f32 * SAMPLE_RATE / n as f32,
                (re * re + im * im).sqrt(),
            )
        })
        .collect()
}

fn lofi_preset(name: &str) -> &'static Preset {
    presets::find(name).unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn nylon_guitar_is_a_pluck_whose_tail_is_shorter_than_a_pads() {
    let nylon = lofi_preset("Nylon Guitar");
    assert_eq!(nylon.value(ParamId::OscMode), 1.0, "pluck mode");
    assert!(nylon.value(ParamId::PluckBright) >= 0.7);
    assert!(nylon.value(ParamId::ChorusMix) > 0.0);
    assert_eq!(nylon.category, Category::LofiGuitars);
    let pad = lofi_preset("Night Drive Pad");
    assert_eq!(pad.category, Category::LofiPads);
    let n_tail = play(nylon, C4, 1000.0, threshold_for(nylon))
        .tail_seconds
        .expect("nylon decays");
    let p_tail = play(pad, C4, 1000.0, threshold_for(pad))
        .tail_seconds
        .expect("pad decays");
    assert!(
        n_tail < p_tail,
        "nylon tail {n_tail} s, pad tail {p_tail} s"
    );
    assert!(n_tail <= 1.5, "nylon tail {n_tail} s");
}

#[test]
fn trap_808_spectrum_sits_below_two_hundred_hertz() {
    let p = lofi_preset("Trap 808");
    assert_eq!(p.value(ParamId::Osc1Wave), 0.0, "sine");
    assert!(p.value(ParamId::SubLevel) > 0.0);
    assert!(p.value(ParamId::PitchEnvAmount) >= 12.0);
    assert!(p.value(ParamId::DriveMix) > 0.0);
    let mut engine = engine_with(p);
    engine.note_on(C2, 1.0);
    // Skip the pitch drop (80 ms to 95 %), then look at a steady 170 ms.
    let _ = render(&mut engine, ms_to_frames(300.0));
    let out = left(&render(&mut engine, ms_to_frames(400.0)));
    assert!(peak(&out) <= 1.0);
    let spec = spectrum(&out, 8192, 20_000.0);
    let low: f64 = spec
        .iter()
        .filter(|(hz, _)| *hz < 200.0)
        .map(|(_, m)| m * m)
        .sum();
    let high: f64 = spec
        .iter()
        .filter(|(hz, _)| *hz >= 200.0)
        .map(|(_, m)| m * m)
        .sum();
    assert!(low > 0.0);
    assert!(
        high < low * 0.05,
        "energy above 200 Hz is {:.1} % of the energy below",
        100.0 * high / low
    );
}

#[test]
fn supersaw_lead_is_seven_voices_and_brighter_than_a_lofi_pad() {
    for name in ["Festival Saw", "Supersaw Lead"] {
        let p = lofi_preset(name);
        assert_eq!(p.value(ParamId::Osc1Unison), 7.0, "{name}");
        assert!(p.value(ParamId::Osc1Spread) >= 20.0, "{name}");
        assert_eq!(p.value(ParamId::Osc1Wave), 1.0, "{name} saw");
        assert_ne!(p.value(ParamId::FilterType), 2.0, "{name}: not a highpass");
        assert!(p.value(ParamId::ChorusMix) > 0.0, "{name}");
        assert!(p.value(ParamId::SidechainAmount) >= 0.4, "{name}");
    }
    let lead = lofi_preset("Festival Saw");
    let pad = lofi_preset("Night Drive Pad");
    let bright_of = |p: &Preset| {
        let mut engine = engine_with(p);
        engine.note_on(C4, 1.0);
        let _ = render(&mut engine, ms_to_frames(2000.0));
        brightness(&left(&render(&mut engine, ms_to_frames(1000.0))))
    };
    let (l, p) = (bright_of(lead), bright_of(pad));
    assert!(l > p * 2.0, "lead brightness {l}, pad {p}");
}

#[test]
fn lofi_keys_use_the_lofi_insert() {
    for p in presets::by_category(Category::LofiKeys) {
        if p.name == "Keys" {
            continue;
        }
        assert!(p.value(ParamId::LofiMix) > 0.0, "{}", p.name);
        assert!(p.value(ParamId::LofiBits) < 16.0, "{}", p.name);
        assert!(p.has_vinyl_noise(), "{}", p.name);
        assert!(p.value(ParamId::ReverbMix) > 0.0, "{}", p.name);
    }
    let piano = lofi_preset("Lofi Piano");
    assert!(piano.value(ParamId::FmAmount) > 0.0, "FM EP edge");
    assert!(piano.value(ParamId::PitchEnvAmount) > 0.0, "hammer click");
    assert!(piano.value(ParamId::PitchEnvDecay) <= 0.02);
    assert!((10.0..=12.0).contains(&piano.value(ParamId::LofiBits)));
    assert!((2.0..=3.0).contains(&piano.value(ParamId::LofiRate)));
    assert!(piano.value(ParamId::LofiWobble) >= 0.3);
    assert!(piano.value(ParamId::LofiNoise) >= 0.3);

    // The lo-fi stage audibly changes the note: compare with the insert bypassed.
    let rhodes = lofi_preset("Dusty Rhodes");
    let render_with = |bypass: bool| {
        let mut engine = engine_with(rhodes);
        if bypass {
            engine.set_param(ParamId::LofiMix, 0.0);
            let _ = render(&mut engine, ms_to_frames(100.0));
        }
        engine.note_on(C4, 1.0);
        left(&render(&mut engine, ms_to_frames(500.0)))
    };
    let (wet, dry) = (render_with(false), render_with(true));
    let diff: Vec<f32> = wet.iter().zip(&dry).map(|(a, b)| a - b).collect();
    assert!(rms(&diff) > 0.02 * rms(&dry), "lo-fi changes the sound");
}

#[test]
fn white_riser_is_noise_that_swells_and_drones_keep_sounding() {
    let riser = lofi_preset("White Riser");
    assert!(riser.value(ParamId::NoiseLevel) >= 0.9);
    assert_eq!(riser.value(ParamId::Osc1Mix), 0.0);
    assert!(riser.value(ParamId::AmpAttack) >= 1.0);
    assert!(riser.value(ParamId::FilterAttack) >= 1.0);
    assert!(riser.value(ParamId::FilterEnvAmount) > 0.5);
    assert!(!riser.is_drone());
    let mut engine = engine_with(riser);
    engine.note_on(C4, 1.0);
    let out = left(&render(&mut engine, ms_to_frames(2000.0)));
    let early = rms(&out[..ms_to_frames(500.0)]);
    let late = rms(&out[ms_to_frames(1500.0)..]);
    assert!(
        late > early * 2.0,
        "riser swells: early {early} late {late}"
    );
    assert!(
        brightness(&out[ms_to_frames(1500.0)..]) > brightness(&out[..ms_to_frames(500.0)]),
        "the filter opens"
    );

    for name in ["Dub Echo", "Infinite Hall", "Vinyl Crackle"] {
        let p = lofi_preset(name);
        assert!(p.is_drone(), "{name}");
        let n = play(p, C4, 1000.0, SILENCE_RMS);
        assert!(
            n.tail_seconds.is_none(),
            "{name} is still sounding after 8 s"
        );
        assert!(
            n.last_window_rms >= SILENCE_RMS,
            "{name}: {}",
            n.last_window_rms
        );
    }
    // Only the three above are drones.
    assert_eq!(LIBRARY.iter().filter(|p| p.is_drone()).count(), 3);
}

#[test]
fn the_original_eight_still_apply_and_render() {
    for p in ALL {
        let threshold = threshold_for(p);
        let n = play(p, C4, 500.0, threshold);
        assert!(rms(&left(&n.held)) > AUDIBLE_RMS, "{}", p.name);
        assert!(n.peak <= 1.0, "{}", p.name);
        assert!(n.tail_seconds.is_some(), "{} decays", p.name);
    }
}
