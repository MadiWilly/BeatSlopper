//! Generates `src/presets/pack.rs`, the Sound Pack: 1 000 presets in the ten categories after
//! `Category::Risers`, 100 per category.
//!
//! ```text
//! cargo run --release -p engine --example gen_preset_pack
//! ```
//!
//! How it works (docs/PRESET_PACK.md has the research and licensing notes):
//! - Every category has a **template** (the settings its sounds share) and a dozen or more
//!   **archetypes**: one documented sound-design recipe each (a brass section is detuned saws
//!   behind a filter envelope with a slow attack; a tubular bell is a sine phase-modulated at
//!   an inharmonic 3.5:1 ratio with a long decay; ...). No third-party patch data is used: the
//!   recipes are the textbook techniques, translated to this engine's parameters.
//! - **Variations** ("Dusty", "Wide", "Cathedral", "Driven", ...) each change several
//!   parameters in a musically meaningful way, and only apply where they make sense (no
//!   unison on a plucked string, no glide on a bell).
//! - Candidates are taken round-robin (every archetype's plain version first, then every
//!   archetype's first fitting variation, ...) and each one is **rendered through the
//!   engine**: its level is normalised into a loudness window by scaling the oscillator
//!   mixes, and it is kept only when it passes the contract `tests/presets.rs` enforces
//!   (in range, audible, peak <= 1.0, silent within 8 s of release with a 2 s margin), has
//!   a unique name and differs from every other preset in at least four track parameters.
//! - The output is deterministic: the same source always writes the same file.

use std::fmt::Write as _;
use std::path::PathBuf;

use engine::presets::{ALL, Category, LIBRARY, Preset};
use engine::{Engine, ParamId, new_engine};

use ParamId as P;

const SR: f32 = 48_000.0;
/// Presets per pack category.
const PER_CATEGORY: usize = 100;
/// Loudness window (dBFS, loudest 200 ms RMS of a held note at velocity 0.8) and its centre.
const LOUD_MIN: f32 = -27.0;
const LOUD_MAX: f32 = -11.0;
const LOUD_TARGET: f32 = -18.0;
/// The test suite's limits (tests/presets.rs) and the margin the generator keeps.
const AUDIBLE_RMS: f32 = 1.0e-3;
const SILENCE_RMS: f32 = 1.0e-4;
const TAIL_BUDGET_SECONDS: f32 = 6.0;
/// Track parameters every pair of presets must differ in.
const MIN_DIFFERENCES: usize = 4;

/// A full parameter set, indexed by `ParamId::index`.
type Patch = [f32; ParamId::COUNT];

fn get(p: &Patch, id: P) -> f32 {
    p[id.index()]
}

fn set(p: &mut Patch, id: P, v: f32) {
    let r = id.range();
    p[id.index()] = v.clamp(*r.start(), *r.end());
}

fn scale(p: &mut Patch, id: P, k: f32) {
    set(p, id, get(p, id) * k);
}

fn add(p: &mut Patch, id: P, d: f32) {
    set(p, id, get(p, id) + d);
}

fn is_pluck(p: &Patch) -> bool {
    get(p, P::OscMode) >= 0.5
}

/// `vals![Osc1Wave: 1.0, ...]` -> `&[(P::Osc1Wave, 1.0), ...]`.
macro_rules! vals {
    ($($id:ident : $v:expr),* $(,)?) => { &[$((P::$id, $v)),*] };
}

// ---------------------------------------------------------------------------------------------
// Templates: the settings every sound of a category starts from.

/// Neutral start: ParamId defaults with every modulation, insert and send switched off.
fn neutral() -> Patch {
    let mut p = [0.0; ParamId::COUNT];
    for &id in ParamId::ALL {
        p[id.index()] = id.default_value();
    }
    let base: &[(P, f32)] = vals![
        Osc1Wave: 1.0, Osc1Detune: 0.0, Osc1Mix: 0.8, Osc2Wave: 1.0, Osc2Detune: 0.0, Osc2Mix: 0.0,
        Osc1Pulse: 0.5, Osc2Pulse: 0.5, Osc1Unison: 1.0, Osc1Spread: 10.0, SubLevel: 0.0,
        NoiseLevel: 0.0, NoiseColor: 0.0, FmAmount: 0.0, FmRatio: 1.0, OscMode: 0.0,
        PluckDamping: 0.5, PluckBright: 0.7,
        FilterType: 0.0, FilterCutoff: 4000.0, FilterResonance: 0.1, FilterEnvAmount: 0.0,
        FilterAttack: 0.002, FilterDecay: 0.3, FilterSustain: 0.5, FilterRelease: 0.3,
        FilterKeyTrack: 0.5,
        AmpAttack: 0.005, AmpDecay: 0.3, AmpSustain: 0.8, AmpRelease: 0.3,
        LfoRate: 5.0, LfoDepth: 0.0, LfoTarget: 1.0, Glide: 0.0,
        PitchEnvAmount: 0.0, PitchEnvDecay: 0.05, VelToCutoff: 0.3, VelToAmp: 0.8,
        SidechainAmount: 0.0, SidechainRelease: 0.25, DriveAmount: 0.2, DriveMix: 0.0,
        DelayTime: 1.0, DelayFeedback: 0.3, DelayMix: 0.0,
        ReverbSize: 0.5, ReverbDamping: 0.5, ReverbMix: 0.15,
        LofiBits: 16.0, LofiRate: 1.0, LofiWobble: 0.0, LofiNoise: 0.0, LofiTone: 20000.0,
        LofiMix: 0.0, ChorusRate: 0.6, ChorusDepth: 0.3, ChorusMix: 0.0,
    ];
    for &(id, v) in base {
        set(&mut p, id, v);
    }
    p
}

fn template(cat: Category) -> &'static [(P, f32)] {
    match cat {
        // Struck keys: fast attack, a long decay into a low sustain, a filter envelope that
        // mellows the tone as the note dies, velocity on brightness.
        Category::Keys => vals![
            AmpAttack: 0.002, AmpDecay: 1.4, AmpSustain: 0.2, AmpRelease: 0.4,
            FilterCutoff: 2800.0, FilterEnvAmount: 0.3, FilterDecay: 0.6, FilterSustain: 0.2,
            VelToCutoff: 0.6, ReverbMix: 0.2,
        ],
        // Organs: gate-shaped amp (instant on, full sustain, quick off), no filter motion.
        Category::Organs => vals![
            AmpAttack: 0.006, AmpDecay: 0.1, AmpSustain: 1.0, AmpRelease: 0.08,
            FilterCutoff: 6000.0, VelToCutoff: 0.0, VelToAmp: 0.2, ReverbMix: 0.2,
        ],
        // Brass and winds: the filter opens with the breath (filter attack) and settles.
        Category::BrassWinds => vals![
            AmpAttack: 0.04, AmpDecay: 0.3, AmpSustain: 0.85, AmpRelease: 0.2,
            FilterCutoff: 900.0, FilterEnvAmount: 0.45, FilterAttack: 0.06, FilterDecay: 0.4,
            FilterSustain: 0.5, VelToCutoff: 0.5, ReverbMix: 0.2,
        ],
        // Bowed strings: a saw (the bowed-string waveform), slow attack, long release, a
        // little ensemble chorus and a room.
        Category::Strings => vals![
            AmpAttack: 0.3, AmpDecay: 0.5, AmpSustain: 1.0, AmpRelease: 0.8,
            FilterCutoff: 2600.0, ChorusMix: 0.3, ChorusDepth: 0.35, ChorusRate: 0.5,
            ReverbSize: 0.7, ReverbMix: 0.3,
        ],
        // Bells and mallets: sine carriers phase-modulated by the FM operator, no sustain.
        Category::BellsMallets => vals![
            Osc1Wave: 0.0, AmpAttack: 0.001, AmpDecay: 1.5, AmpSustain: 0.0, AmpRelease: 1.2,
            FilterCutoff: 9000.0, VelToCutoff: 0.2, ReverbSize: 0.6, ReverbMix: 0.25,
        ],
        // Guitars: the Karplus-Strong string; the amp only gates it.
        Category::Guitars => vals![
            OscMode: 1.0, Osc1Mix: 1.0, AmpAttack: 0.001, AmpDecay: 0.3, AmpSustain: 1.0,
            AmpRelease: 0.35, FilterCutoff: 5000.0, VelToCutoff: 0.4, VelToAmp: 0.9,
            ReverbMix: 0.2,
        ],
        // Leads: full sustain, a touch of filter envelope.
        Category::SynthLeads => vals![
            AmpAttack: 0.004, AmpDecay: 0.3, AmpSustain: 0.9, AmpRelease: 0.2,
            FilterCutoff: 3200.0, FilterResonance: 0.25, FilterEnvAmount: 0.3, FilterDecay: 0.3,
            DelayMix: 0.12, DelayTime: 1.0, DelayFeedback: 0.3,
        ],
        // Basses: low cutoff with an envelope pluck, a little drive, a tight release.
        Category::SynthBass => vals![
            AmpAttack: 0.003, AmpDecay: 0.3, AmpSustain: 0.75, AmpRelease: 0.1,
            FilterCutoff: 600.0, FilterResonance: 0.3, FilterEnvAmount: 0.5, FilterDecay: 0.22,
            FilterSustain: 0.3, FilterKeyTrack: 0.3, DriveAmount: 0.3, DriveMix: 0.3,
            ReverbMix: 0.05,
        ],
        // Pads: slow in, slow out, wide, a big room.
        Category::PadsAtmos => vals![
            Osc1Unison: 3.0, Osc1Spread: 12.0, AmpAttack: 0.8, AmpDecay: 1.0, AmpSustain: 0.9,
            AmpRelease: 2.0, FilterCutoff: 1600.0, ChorusMix: 0.35, ChorusDepth: 0.4,
            ChorusRate: 0.4, ReverbSize: 0.8, ReverbDamping: 0.45, ReverbMix: 0.4,
        ],
        // Arps and stabs: no sustain, an enveloped filter, an eighth-note echo.
        Category::ArpsStabs => vals![
            AmpAttack: 0.001, AmpDecay: 0.25, AmpSustain: 0.0, AmpRelease: 0.2,
            FilterCutoff: 1200.0, FilterEnvAmount: 0.6, FilterDecay: 0.2, FilterSustain: 0.0,
            DelayMix: 0.2, DelayFeedback: 0.35, DelayTime: 1.0,
        ],
        _ => vals![],
    }
}

/// Note the loudness is measured on (basses an octave and a half lower).
fn level_note(cat: Category) -> u8 {
    match cat {
        Category::SynthBass => 36,
        Category::Guitars => 52,
        _ => 60,
    }
}

// ---------------------------------------------------------------------------------------------
// Archetypes: one documented recipe each. Waves: 0 sine, 1 saw, 2 square, 3 triangle.
// Filter types: 0 LP, 1 BP, 2 HP. LFO target: 0 pitch (depth 1 = +-1 semitone), 1 cutoff
// (depth 1 = +-2 octaves). FM amount 0..1 maps to index 0..8 on osc 1.

struct Arch {
    cat: Category,
    name: &'static str,
    tags: &'static [&'static str],
    set: &'static [(P, f32)],
}

macro_rules! arch {
    ($cat:ident, $name:expr, [$($tag:expr),*], $($id:ident : $v:expr),* $(,)?) => {
        Arch { cat: Category::$cat, name: $name, tags: &[$($tag),*], set: vals![$($id: $v),*] }
    };
}

#[rustfmt::skip]
static ARCHETYPES: &[Arch] = &[
    // ----- Keys ------------------------------------------------------------------------------
    // Felt piano: triangle body plus a slightly detuned sine, dark and soft-hammered.
    arch!(Keys, "Felt Piano", ["piano", "soft", "warm"], Osc1Wave: 3.0, Osc2Wave: 0.0, Osc2Mix: 0.4, Osc2Detune: 4.0, FilterCutoff: 1600.0, FilterEnvAmount: 0.35, FilterDecay: 0.5, AmpDecay: 1.6, AmpSustain: 0.1, AmpRelease: 0.5),
    // FM electric piano: 1:1 sine FM (the tine), a pitch blip for the hammer, chorus.
    arch!(Keys, "Tine Piano", ["ep", "fm", "electric piano"], Osc1Wave: 0.0, FmAmount: 0.2, FmRatio: 1.0, PitchEnvAmount: 1.5, PitchEnvDecay: 0.01, FilterCutoff: 6000.0, FilterEnvAmount: 0.0, AmpSustain: 0.3, ChorusMix: 0.25),
    // Reed piano: narrow pulse plus triangle with amp drive (a reed through a small amp).
    arch!(Keys, "Reed Piano", ["ep", "reed", "vintage"], Osc1Wave: 2.0, Osc1Pulse: 0.35, Osc2Wave: 3.0, Osc2Mix: 0.5, FilterCutoff: 2200.0, FilterEnvAmount: 0.4, FilterDecay: 0.4, AmpDecay: 1.0, AmpSustain: 0.3, DriveAmount: 0.3, DriveMix: 0.4),
    // Clav: very narrow pulse into a resonant enveloped filter, short and velocity-bright.
    arch!(Keys, "Funk Clav", ["clav", "funk", "percussive"], Osc1Wave: 2.0, Osc1Pulse: 0.2, FilterCutoff: 2000.0, FilterResonance: 0.35, FilterEnvAmount: 0.5, FilterDecay: 0.15, AmpDecay: 0.4, AmpSustain: 0.2, AmpRelease: 0.08, VelToCutoff: 0.7),
    // Harpsichord: saw plus a thin pulse, bright, plucked envelope with no sustain.
    arch!(Keys, "Harpsichord", ["baroque", "bright", "plucked"], Osc1Wave: 1.0, Osc2Wave: 2.0, Osc2Pulse: 0.15, Osc2Mix: 0.4, FilterCutoff: 5000.0, FilterEnvAmount: 0.3, FilterDecay: 0.3, AmpDecay: 1.2, AmpSustain: 0.0, AmpRelease: 0.3),
    // Celesta: sine with 4:1 FM shimmer, bell-like decay.
    arch!(Keys, "Celesta", ["celesta", "bell", "sweet"], Osc1Wave: 0.0, FmAmount: 0.15, FmRatio: 4.0, FilterCutoff: 8000.0, FilterEnvAmount: 0.0, AmpDecay: 1.5, AmpSustain: 0.0, AmpRelease: 0.6, ReverbMix: 0.3),
    // Toy keys: triangle plus a quarter pulse, short, a little crushed.
    arch!(Keys, "Toy Keys", ["toy", "cute", "lofi"], Osc1Wave: 3.0, Osc2Wave: 2.0, Osc2Pulse: 0.25, Osc2Mix: 0.4, AmpDecay: 0.5, AmpSustain: 0.0, AmpRelease: 0.2, LofiBits: 10.0, LofiMix: 0.5),
    // Upright: saw and triangle under a filter that closes as the note decays.
    arch!(Keys, "Upright Keys", ["piano", "upright", "warm"], Osc1Mix: 0.5, Osc2Wave: 3.0, Osc2Mix: 0.7, FilterCutoff: 1400.0, FilterEnvAmount: 0.45, FilterDecay: 0.7, AmpDecay: 1.8, AmpSustain: 0.15),
    // Bright grand: triangle plus quiet saw, open filter with strong velocity tracking.
    arch!(Keys, "Bright Grand", ["piano", "bright", "pop"], Osc1Wave: 3.0, Osc2Wave: 1.0, Osc2Mix: 0.3, FilterCutoff: 3500.0, AmpDecay: 1.6, AmpSustain: 0.15, VelToCutoff: 0.7),
    // Mellow EP: sine and triangle with gentle FM and a lazy chorus.
    arch!(Keys, "Mellow EP", ["ep", "mellow", "soul"], Osc1Wave: 0.0, Osc2Wave: 3.0, Osc2Mix: 0.3, Osc2Detune: 5.0, FmAmount: 0.1, FilterCutoff: 2500.0, AmpDecay: 1.2, AmpSustain: 0.4, ChorusMix: 0.4, ChorusRate: 0.4),
    // Glass keys: 3:1 FM for a glassy, hollow tine.
    arch!(Keys, "Glass Keys", ["glassy", "fm", "ep"], Osc1Wave: 0.0, FmAmount: 0.25, FmRatio: 3.0, FilterCutoff: 7000.0, AmpDecay: 1.0, AmpSustain: 0.2),
    // Stage keys: saw plus square through amp drive, a punchy filter envelope.
    arch!(Keys, "Stage Keys", ["stage", "driven", "rock"], Osc2Wave: 2.0, Osc2Mix: 0.4, FilterCutoff: 1800.0, FilterEnvAmount: 0.5, FilterDecay: 0.4, AmpDecay: 1.0, AmpSustain: 0.35, DriveAmount: 0.35, DriveMix: 0.45),
    // Hammer dulcimer: hard pitch blip plus 2:1 FM, quick decay.
    arch!(Keys, "Hammer Dulcimer", ["dulcimer", "folk", "metallic"], Osc1Wave: 3.0, FmAmount: 0.18, FmRatio: 2.0, PitchEnvAmount: 2.0, PitchEnvDecay: 0.008, AmpDecay: 0.9, AmpSustain: 0.0, AmpRelease: 0.5),
    // Lounge EP: square and sine, slow chorus and vibrato-like cutoff sway.
    arch!(Keys, "Lounge EP", ["ep", "lounge", "jazz"], Osc1Wave: 0.0, Osc2Wave: 2.0, Osc2Mix: 0.25, FmAmount: 0.08, FilterCutoff: 3000.0, AmpDecay: 1.3, AmpSustain: 0.35, ChorusMix: 0.35, LfoTarget: 1.0, LfoRate: 3.5, LfoDepth: 0.08),

    // ----- Organs ----------------------------------------------------------------------------
    // Drawbar organ: sine foundation, a 16' sub, a triangle partial, rotary-style chorus.
    arch!(Organs, "Drawbar Organ", ["organ", "tonewheel", "classic"], Osc1Wave: 0.0, SubLevel: 0.5, Osc2Wave: 3.0, Osc2Mix: 0.4, Osc2Detune: 2.0, ChorusMix: 0.4, ChorusRate: 4.5, ChorusDepth: 0.25),
    // Rock organ: square plus sub through hard drive and a fast rotary.
    arch!(Organs, "Rock Organ", ["organ", "rock", "driven"], Osc1Wave: 2.0, SubLevel: 0.4, DriveAmount: 0.5, DriveMix: 0.6, ChorusMix: 0.45, ChorusRate: 5.0, FilterCutoff: 4500.0),
    // Church organ: saw, square and sub ranks with a slow speak and a cathedral.
    arch!(Organs, "Church Organ", ["organ", "pipe", "church"], Osc1Mix: 0.4, Osc2Wave: 2.0, Osc2Mix: 0.5, SubLevel: 0.6, FilterCutoff: 3000.0, FilterKeyTrack: 0.7, AmpAttack: 0.06, AmpRelease: 0.5, ReverbSize: 0.88, ReverbMix: 0.45, ReverbDamping: 0.35),
    // Harmonium: pulse plus saw, slow bellows attack, faint vibrato.
    arch!(Organs, "Harmonium", ["reed", "folk", "organ"], Osc1Wave: 2.0, Osc1Pulse: 0.3, Osc2Mix: 0.4, FilterCutoff: 1600.0, AmpAttack: 0.08, AmpRelease: 0.2, LfoTarget: 0.0, LfoRate: 5.0, LfoDepth: 0.05),
    // Combo organ: thin square and triangle with a quick vibrato (60s transistor organs).
    arch!(Organs, "Combo Organ", ["organ", "sixties", "vibrato"], Osc1Wave: 2.0, Osc1Pulse: 0.45, Osc2Wave: 3.0, Osc2Mix: 0.4, FilterCutoff: 4000.0, LfoTarget: 0.0, LfoRate: 6.5, LfoDepth: 0.15),
    // Gospel organ: sine, sub and a square partial, driven, fast rotary.
    arch!(Organs, "Gospel Organ", ["organ", "gospel", "soul"], Osc1Wave: 0.0, SubLevel: 0.6, Osc2Wave: 2.0, Osc2Mix: 0.25, DriveAmount: 0.3, DriveMix: 0.4, ChorusMix: 0.4, ChorusRate: 4.8),
    // Jazz organ: sine and triangle with a percussive filter click on the attack.
    arch!(Organs, "Jazz Organ", ["organ", "jazz", "percussive"], Osc1Wave: 0.0, Osc2Wave: 3.0, Osc2Mix: 0.5, FilterCutoff: 2500.0, FilterEnvAmount: 0.4, FilterDecay: 0.1, FilterSustain: 0.0, ChorusMix: 0.3, ChorusRate: 1.0),
    // Pipe flute: a sine rank with a breath of pink noise and a slow speak.
    arch!(Organs, "Pipe Flute", ["pipe", "flute", "breathy"], Osc1Wave: 0.0, NoiseLevel: 0.04, NoiseColor: 1.0, FilterCutoff: 3000.0, AmpAttack: 0.08, AmpRelease: 0.25, ReverbMix: 0.35, ReverbSize: 0.75),
    // Theatre organ: saw, square and sub with a deep tremulant vibrato.
    arch!(Organs, "Theatre Organ", ["organ", "theatre", "vibrato"], Osc1Mix: 0.5, Osc2Wave: 2.0, Osc2Mix: 0.4, SubLevel: 0.5, FilterCutoff: 2500.0, LfoTarget: 0.0, LfoRate: 6.0, LfoDepth: 0.2, ReverbMix: 0.3),
    // Calliope: narrow pulse plus steam noise and a wide, wobbly vibrato.
    arch!(Organs, "Calliope", ["calliope", "circus", "vintage"], Osc1Wave: 2.0, Osc1Pulse: 0.3, NoiseLevel: 0.05, FilterCutoff: 3500.0, AmpAttack: 0.03, LfoTarget: 0.0, LfoRate: 6.0, LfoDepth: 0.3),
    // Musette: two pulse reeds detuned apart (the accordion's wet tuning).
    arch!(Organs, "Musette", ["accordion", "musette", "folk"], Osc1Wave: 2.0, Osc1Pulse: 0.4, Osc2Wave: 2.0, Osc2Pulse: 0.35, Osc2Mix: 0.7, Osc2Detune: 14.0, FilterCutoff: 2200.0, AmpAttack: 0.03),
    // Percussive organ: sine plus square with a decaying second partial (a percussion tab).
    arch!(Organs, "Percussive Organ", ["organ", "percussive", "pop"], Osc1Wave: 0.0, Osc2Wave: 2.0, Osc2Mix: 0.3, AmpDecay: 0.6, AmpSustain: 0.6, FilterEnvAmount: 0.5, FilterDecay: 0.08, FilterSustain: 0.2, FilterCutoff: 2500.0),
    // Bass pedal organ: sine plus a strong sub for the 16' and 32' pedals.
    arch!(Organs, "Pedal Organ", ["organ", "pedal", "low"], Osc1Wave: 0.0, SubLevel: 0.8, Osc2Wave: 3.0, Osc2Mix: 0.3, FilterCutoff: 1200.0, ReverbMix: 0.3),

    // ----- Brass & Winds ---------------------------------------------------------------------
    // Brass section: two saws a few cents apart behind the breath envelope.
    arch!(BrassWinds, "Brass Section", ["brass", "section", "fanfare"], Osc2Mix: 0.6, Osc2Detune: 8.0, FilterEnvAmount: 0.5, FilterAttack: 0.06),
    // Trumpet: one bright saw, quick attack, a singer's vibrato.
    arch!(BrassWinds, "Solo Trumpet", ["brass", "trumpet", "solo"], FilterCutoff: 1100.0, FilterEnvAmount: 0.55, FilterAttack: 0.03, FilterDecay: 0.3, AmpAttack: 0.03, LfoTarget: 0.0, LfoRate: 5.5, LfoDepth: 0.1),
    // French horn: saw plus triangle, dark, a slow round attack and a hall.
    arch!(BrassWinds, "French Horn", ["brass", "horn", "mellow"], Osc2Wave: 3.0, Osc2Mix: 0.6, FilterCutoff: 600.0, FilterEnvAmount: 0.3, FilterAttack: 0.1, AmpAttack: 0.08, AmpRelease: 0.3, ReverbMix: 0.3, ReverbSize: 0.7),
    // Trombone: dark saw with glide between notes.
    arch!(BrassWinds, "Trombone", ["brass", "trombone", "slide"], FilterCutoff: 700.0, FilterEnvAmount: 0.45, FilterAttack: 0.08, AmpAttack: 0.06, Glide: 0.06),
    // Tuba: saw plus sub, very dark.
    arch!(BrassWinds, "Tuba", ["brass", "tuba", "low"], SubLevel: 0.5, FilterCutoff: 400.0, FilterEnvAmount: 0.4, AmpAttack: 0.05),
    // Synth brass: five-voice unison saw, fast, with a pronounced filter blip.
    arch!(BrassWinds, "Synth Brass", ["brass", "synth", "eighties"], Osc1Unison: 5.0, Osc1Spread: 15.0, AmpAttack: 0.01, AmpSustain: 0.6, FilterCutoff: 1200.0, FilterEnvAmount: 0.6, FilterAttack: 0.01, FilterDecay: 0.25),
    // Flute: sine plus a little triangle, pink breath noise, slow vibrato.
    arch!(BrassWinds, "Flute", ["flute", "woodwind", "breathy"], Osc1Wave: 0.0, Osc2Wave: 3.0, Osc2Mix: 0.3, NoiseLevel: 0.06, NoiseColor: 1.0, FilterCutoff: 3500.0, FilterEnvAmount: 0.1, AmpAttack: 0.08, LfoTarget: 0.0, LfoRate: 5.0, LfoDepth: 0.12),
    // Clarinet: a square wave (odd harmonics, like the closed bore).
    arch!(BrassWinds, "Clarinet", ["clarinet", "woodwind", "reed"], Osc1Wave: 2.0, Osc1Pulse: 0.5, FilterCutoff: 1800.0, FilterEnvAmount: 0.1, AmpAttack: 0.04, LfoTarget: 0.0, LfoRate: 4.5, LfoDepth: 0.05),
    // Oboe: a narrow pulse through a resonant band-pass (the nasal double reed).
    arch!(BrassWinds, "Oboe", ["oboe", "woodwind", "nasal"], Osc1Wave: 2.0, Osc1Pulse: 0.15, FilterType: 1.0, FilterCutoff: 1500.0, FilterResonance: 0.3, FilterEnvAmount: 0.15, AmpAttack: 0.04),
    // Saxophone: saw plus pulse with breath, a growl of drive and vibrato.
    arch!(BrassWinds, "Saxophone", ["sax", "woodwind", "jazz"], Osc2Wave: 2.0, Osc2Pulse: 0.3, Osc2Mix: 0.5, NoiseLevel: 0.04, FilterCutoff: 1500.0, FilterResonance: 0.2, FilterEnvAmount: 0.35, FilterAttack: 0.05, DriveAmount: 0.3, DriveMix: 0.3, LfoTarget: 0.0, LfoRate: 5.0, LfoDepth: 0.1),
    // Pan pipes: sine with lots of breath and a little chiff from the pitch envelope.
    arch!(BrassWinds, "Pan Pipes", ["pan flute", "breathy", "world"], Osc1Wave: 0.0, NoiseLevel: 0.12, NoiseColor: 1.0, FilterCutoff: 2500.0, FilterEnvAmount: 0.1, PitchEnvAmount: 0.5, PitchEnvDecay: 0.03, AmpAttack: 0.05, ReverbMix: 0.35),
    // Bassoon: narrow pulse and saw, low cutoff.
    arch!(BrassWinds, "Bassoon", ["bassoon", "woodwind", "low"], Osc1Wave: 2.0, Osc1Pulse: 0.25, Osc2Mix: 0.4, FilterCutoff: 600.0, FilterEnvAmount: 0.25, AmpAttack: 0.05),
    // Harmonica: pulse plus a detuned saw through a band-pass, a fast vibrato.
    arch!(BrassWinds, "Harmonica", ["harmonica", "blues", "reed"], Osc1Wave: 2.0, Osc1Pulse: 0.35, Osc2Mix: 0.4, Osc2Detune: 6.0, FilterType: 1.0, FilterCutoff: 1800.0, FilterResonance: 0.25, FilterEnvAmount: 0.2, AmpAttack: 0.02, LfoTarget: 0.0, LfoRate: 6.0, LfoDepth: 0.1),
    // Recorder: triangle with a breath of noise.
    arch!(BrassWinds, "Recorder", ["recorder", "woodwind", "soft"], Osc1Wave: 3.0, NoiseLevel: 0.03, NoiseColor: 1.0, FilterCutoff: 3000.0, FilterEnvAmount: 0.05, AmpAttack: 0.04),
    // Muted trumpet: band-passed saw (the cup mute) with a short wah from the envelope.
    arch!(BrassWinds, "Muted Trumpet", ["brass", "muted", "jazz"], FilterType: 1.0, FilterCutoff: 1300.0, FilterResonance: 0.4, FilterEnvAmount: 0.3, FilterAttack: 0.04, FilterDecay: 0.25, AmpAttack: 0.03),

    // ----- Strings ---------------------------------------------------------------------------
    // Ensemble: five-voice unison saw, slow attack, ensemble chorus.
    arch!(Strings, "String Ensemble", ["strings", "ensemble", "orchestral"], Osc1Unison: 5.0, Osc1Spread: 14.0, ChorusMix: 0.4, ChorusDepth: 0.4, AmpAttack: 0.35, AmpRelease: 0.9),
    // Solo violin: one saw with a bowing attack and a real vibrato.
    arch!(Strings, "Solo Violin", ["violin", "solo", "expressive"], FilterCutoff: 3500.0, FilterResonance: 0.15, AmpAttack: 0.12, AmpRelease: 0.3, LfoTarget: 0.0, LfoRate: 5.5, LfoDepth: 0.15, ChorusMix: 0.0),
    // Cello: two saws, darker, slower vibrato.
    arch!(Strings, "Cello", ["cello", "solo", "warm"], Osc2Mix: 0.5, Osc2Detune: 4.0, FilterCutoff: 1500.0, AmpAttack: 0.15, AmpRelease: 0.5, LfoTarget: 0.0, LfoRate: 5.0, LfoDepth: 0.1),
    // Contrabass arco: saw plus sub, very dark.
    arch!(Strings, "Contrabass", ["bass", "orchestral", "low"], SubLevel: 0.5, FilterCutoff: 700.0, AmpAttack: 0.2, AmpRelease: 0.5),
    // Pizzicato: plucked envelope with no sustain.
    arch!(Strings, "Pizzicato Strings", ["pizzicato", "plucked", "orchestral"], Osc1Unison: 3.0, AmpAttack: 0.001, AmpDecay: 0.35, AmpSustain: 0.0, AmpRelease: 0.25, FilterCutoff: 1800.0, FilterEnvAmount: 0.4, FilterDecay: 0.15, FilterSustain: 0.0, ChorusMix: 0.15),
    // Tremolo strings: a fast cutoff LFO chops the ensemble like bowed tremolo.
    arch!(Strings, "Tremolo Strings", ["strings", "tremolo", "tension"], Osc1Unison: 3.0, LfoTarget: 1.0, LfoRate: 8.0, LfoDepth: 0.4, FilterCutoff: 1800.0, AmpAttack: 0.2),
    // Chamber strings: saw plus a detuned pulse, a gentle chorus.
    arch!(Strings, "Chamber Strings", ["strings", "chamber", "soft"], Osc2Wave: 2.0, Osc2Pulse: 0.4, Osc2Mix: 0.5, Osc2Detune: -6.0, FilterCutoff: 2200.0, AmpAttack: 0.4, ChorusMix: 0.25),
    // Analog strings: the string-machine sound, unison saws and a deep ensemble chorus.
    arch!(Strings, "String Machine", ["string machine", "analog", "seventies"], Osc1Unison: 3.0, Osc1Spread: 9.0, FilterCutoff: 3500.0, ChorusMix: 0.6, ChorusDepth: 0.55, ChorusRate: 0.7, AmpAttack: 0.15),
    // Spiccato: bounced bow, very short.
    arch!(Strings, "Spiccato", ["strings", "short", "rhythmic"], Osc1Unison: 3.0, AmpAttack: 0.005, AmpDecay: 0.15, AmpSustain: 0.1, AmpRelease: 0.1, FilterCutoff: 2500.0, FilterEnvAmount: 0.3, FilterDecay: 0.08),
    // Legato strings: slow attack with glide between notes.
    arch!(Strings, "Legato Strings", ["strings", "legato", "cinematic"], Osc1Unison: 3.0, AmpAttack: 0.5, Glide: 0.05, FilterCutoff: 2400.0),
    // Swell strings: the longest attack and release, a hall.
    arch!(Strings, "Swell Strings", ["strings", "swell", "cinematic"], Osc1Unison: 5.0, AmpAttack: 1.4, AmpRelease: 1.5, ReverbSize: 0.85, ReverbMix: 0.4, FilterCutoff: 2000.0),
    // Octave strings: unison saws doubled an octave down by the sub.
    arch!(Strings, "Octave Strings", ["strings", "octave", "big"], Osc1Unison: 3.0, SubLevel: 0.5, FilterCutoff: 2800.0, AmpAttack: 0.3),
    // Dark strings: closed filter, low and moody.
    arch!(Strings, "Dark Strings", ["strings", "dark", "moody"], Osc1Unison: 3.0, FilterCutoff: 900.0, AmpAttack: 0.4, AmpRelease: 1.0),
    // Viola: saw plus triangle, mid-dark, a soft vibrato.
    arch!(Strings, "Viola", ["viola", "solo", "warm"], Osc2Wave: 3.0, Osc2Mix: 0.5, FilterCutoff: 2000.0, AmpAttack: 0.12, AmpRelease: 0.4, LfoTarget: 0.0, LfoRate: 5.2, LfoDepth: 0.12),

    // ----- Bells & Mallets -------------------------------------------------------------------
    // Tubular bell: inharmonic 3.5:1 FM, long decay.
    arch!(BellsMallets, "Tubular Bell", ["bell", "tubular", "fm"], FmAmount: 0.35, FmRatio: 3.5, AmpDecay: 2.0, AmpRelease: 2.0),
    // Church bell: 1.41:1 FM (strongly inharmonic), very long.
    arch!(BellsMallets, "Church Bell", ["bell", "church", "big"], FmAmount: 0.5, FmRatio: 1.41, AmpDecay: 2.0, AmpRelease: 2.5, AmpAttack: 0.003, FilterCutoff: 5000.0, VelToCutoff: 0.5, Osc2Wave: 0.0, Osc2Mix: 0.3, Osc2Detune: -12.0, ReverbSize: 0.85, ReverbMix: 0.35),
    // Glockenspiel: bright 3:1 FM, shorter.
    arch!(BellsMallets, "Glockenspiel", ["glockenspiel", "mallet", "bright"], FmAmount: 0.2, FmRatio: 3.0, AmpDecay: 0.9, AmpRelease: 0.8),
    // Vibraphone: sine and triangle with faint 4:1 FM and a motor-speed chorus.
    arch!(BellsMallets, "Vibraphone", ["vibes", "mallet", "jazz"], Osc2Wave: 3.0, Osc2Mix: 0.2, FmAmount: 0.08, FmRatio: 4.0, AmpDecay: 1.8, AmpRelease: 1.0, ChorusMix: 0.3, ChorusRate: 4.5, ChorusDepth: 0.2),
    // Marimba: 4:1 FM wood tone with a tiny pitch blip, short.
    arch!(BellsMallets, "Wood Marimba", ["marimba", "mallet", "wood"], Osc2Wave: 3.0, Osc2Mix: 0.3, FmAmount: 0.1, FmRatio: 4.0, AmpDecay: 0.5, AmpRelease: 0.3, PitchEnvAmount: 0.5, PitchEnvDecay: 0.005),
    // Xylophone: triangle with 3:1 FM, very short.
    arch!(BellsMallets, "Xylophone", ["xylophone", "mallet", "short"], Osc1Wave: 3.0, FmAmount: 0.15, FmRatio: 3.0, AmpDecay: 0.25, AmpRelease: 0.2),
    // Thumb piano: 5:1 FM with a pitch blip (the tine's bend).
    arch!(BellsMallets, "Thumb Piano", ["kalimba", "mbira", "world"], FmAmount: 0.15, FmRatio: 5.0, AmpDecay: 0.6, AmpRelease: 0.4, PitchEnvAmount: 0.7, PitchEnvDecay: 0.01),
    // Steel pan: 1.5:1 FM with a pitch dip on the strike.
    arch!(BellsMallets, "Steel Pan", ["steel drum", "caribbean", "world"], FmAmount: 0.25, FmRatio: 1.5, AmpDecay: 0.8, AmpRelease: 0.5, PitchEnvAmount: 1.0, PitchEnvDecay: 0.02),
    // Crystal bell: 7:1 FM, high shimmer.
    arch!(BellsMallets, "Crystal Bell", ["bell", "crystal", "shimmer"], FmAmount: 0.4, FmRatio: 7.0, AmpDecay: 1.6, AmpRelease: 1.2),
    // Gamelan: 2.76:1 FM (a bonang-like inharmonic partial), long.
    arch!(BellsMallets, "Gamelan", ["gamelan", "world", "metallic"], FmAmount: 0.45, FmRatio: 2.76, AmpDecay: 1.8, AmpRelease: 1.4),
    // Wind chime: 5.4:1 FM, high, airy hall.
    arch!(BellsMallets, "Wind Chime", ["chime", "airy", "ambient"], FmAmount: 0.3, FmRatio: 5.4, AmpDecay: 2.0, AmpRelease: 2.0, FilterType: 2.0, FilterCutoff: 900.0, ChorusMix: 0.3, ChorusRate: 0.3, VelToAmp: 1.0, ReverbSize: 0.85, ReverbMix: 0.4),
    // Handbell: 2.4:1 FM, medium decay.
    arch!(BellsMallets, "Handbell", ["bell", "handbell", "choir"], FmAmount: 0.25, FmRatio: 2.4, AmpAttack: 0.002, AmpDecay: 1.4, AmpRelease: 1.0),
    // Celeste mallet: triangle with 6:1 FM sparkle.
    arch!(BellsMallets, "Sparkle Mallet", ["mallet", "sparkle", "sweet"], Osc1Wave: 3.0, FmAmount: 0.1, FmRatio: 6.0, AmpDecay: 1.0, AmpRelease: 0.6),
    // Temple bowl: 2.1:1 FM, slow bloom, very long.
    arch!(BellsMallets, "Singing Bowl", ["bowl", "meditation", "ambient"], FmAmount: 0.2, FmRatio: 2.1, AmpAttack: 0.05, AmpDecay: 2.0, AmpRelease: 2.5, LfoTarget: 0.0, LfoRate: 4.0, LfoDepth: 0.04),

    // ----- Guitars (Karplus-Strong) ----------------------------------------------------------
    // Steel-string acoustic: bright, long ring, open filter.
    arch!(Guitars, "Steel Acoustic", ["guitar", "acoustic", "steel"], PluckDamping: 0.35, PluckBright: 0.7, FilterCutoff: 7000.0, ChorusMix: 0.1),
    // Classical nylon: darker pick, medium ring.
    arch!(Guitars, "Classical Nylon", ["guitar", "nylon", "classical"], PluckDamping: 0.45, PluckBright: 0.45, FilterCutoff: 4500.0),
    // Clean electric: a touch of amp, a chorus.
    arch!(Guitars, "Clean Electric", ["guitar", "electric", "clean"], PluckDamping: 0.3, PluckBright: 0.6, FilterCutoff: 4000.0, DriveAmount: 0.15, DriveMix: 0.2, ChorusMix: 0.25),
    // Overdrive: a cranked amp on a long-ringing string.
    arch!(Guitars, "Overdrive Guitar", ["guitar", "overdrive", "rock"], PluckDamping: 0.25, PluckBright: 0.6, FilterCutoff: 3000.0, DriveAmount: 0.65, DriveMix: 0.8),
    // Fuzz: full drive, resonant cab-ish lowpass.
    arch!(Guitars, "Fuzz Guitar", ["guitar", "fuzz", "lead"], PluckDamping: 0.15, PluckBright: 0.7, FilterCutoff: 2500.0, FilterResonance: 0.2, DriveAmount: 1.0, DriveMix: 0.9),
    // Palm-muted chug: heavy damping into drive.
    arch!(Guitars, "Palm Mute", ["guitar", "muted", "chug"], PluckDamping: 0.85, PluckBright: 0.45, FilterCutoff: 2200.0, DriveAmount: 0.55, DriveMix: 0.7, AmpRelease: 0.15),
    // Twelve-string: bright and doubled by a fast shallow chorus.
    arch!(Guitars, "Twelve String", ["guitar", "12-string", "jangle"], PluckDamping: 0.3, PluckBright: 0.8, FilterCutoff: 8000.0, ChorusMix: 0.5, ChorusDepth: 0.2, ChorusRate: 1.2),
    // Banjo: the brightest, shortest string through a high-pass (the drum head's thin body).
    arch!(Guitars, "Banjo", ["banjo", "bluegrass", "twang"], PluckDamping: 0.65, PluckBright: 0.95, FilterType: 2.0, FilterCutoff: 250.0),
    // Mandolin: bright, medium-short, a quick shimmer.
    arch!(Guitars, "Mandolin", ["mandolin", "folk", "bright"], PluckDamping: 0.55, PluckBright: 0.9, FilterCutoff: 6000.0, ChorusMix: 0.2, ChorusRate: 1.5, ChorusDepth: 0.15),
    // Sitar-like: resonant band-pass buzz, a little drive and a pitch blip (the jawari bridge).
    arch!(Guitars, "Sitar Pluck", ["sitar", "india", "world"], PluckDamping: 0.3, PluckBright: 0.9, FilterType: 1.0, FilterCutoff: 2000.0, FilterResonance: 0.5, DriveAmount: 0.3, DriveMix: 0.3, PitchEnvAmount: 0.5, PitchEnvDecay: 0.05),
    // Concert harp: long ring that sings on after release, a hall.
    arch!(Guitars, "Concert Harp", ["harp", "orchestral", "long"], PluckDamping: 0.2, PluckBright: 0.55, AmpRelease: 1.5, ReverbMix: 0.4, ReverbSize: 0.75),
    // Koto: raw-noise pick and a semitone bend in.
    arch!(Guitars, "Koto", ["koto", "japan", "world"], PluckDamping: 0.45, PluckBright: 0.95, FilterCutoff: 7000.0, PitchEnvAmount: 1.0, PitchEnvDecay: 0.03),
    // Finger bass: dark, medium damping.
    arch!(Guitars, "Finger Bass", ["bass guitar", "fingerstyle", "warm"], PluckDamping: 0.55, PluckBright: 0.35, FilterCutoff: 1500.0, DriveAmount: 0.2, DriveMix: 0.25),
    // Slap bass: raw-noise pop with a filter snap.
    arch!(Guitars, "Slap Bass", ["bass guitar", "slap", "funk"], PluckDamping: 0.5, PluckBright: 1.0, FilterCutoff: 3000.0, FilterEnvAmount: 0.3, FilterDecay: 0.1, FilterSustain: 0.2, DriveAmount: 0.3, DriveMix: 0.3),
    // Jazz archtop: dark pick into a 1.5 kHz lowpass.
    arch!(Guitars, "Jazz Archtop", ["guitar", "jazz", "dark"], PluckDamping: 0.5, PluckBright: 0.3, FilterCutoff: 1500.0),
    // Surf: bright string, slapback echo and a spring-ish room.
    arch!(Guitars, "Surf Guitar", ["guitar", "surf", "twang"], PluckDamping: 0.3, PluckBright: 0.75, FilterCutoff: 6000.0, FilterResonance: 0.2, ChorusMix: 0.2, ChorusRate: 4.0, ChorusDepth: 0.15, DriveAmount: 0.15, DriveMix: 0.2, DelayTime: 0.0, DelayFeedback: 0.3, DelayMix: 0.3, ReverbMix: 0.45, ReverbDamping: 0.3),
    // Oud: bright, resonant mid body, fretless glide.
    arch!(Guitars, "Oud", ["oud", "middle east", "world"], PluckDamping: 0.55, PluckBright: 0.8, FilterCutoff: 3500.0, FilterResonance: 0.2, Glide: 0.03),

    // ----- Synth Leads -----------------------------------------------------------------------
    // Classic mono lead: two saws 7 cents apart, resonant filter, glide.
    arch!(SynthLeads, "Mono Saw Lead", ["lead", "mono", "classic"], Osc2Mix: 0.6, Osc2Detune: 7.0, FilterCutoff: 2500.0, FilterResonance: 0.35, Glide: 0.05),
    // Square lead with a sub.
    arch!(SynthLeads, "Square Wave Lead", ["lead", "square", "retro"], Osc1Wave: 2.0, SubLevel: 0.3, FilterCutoff: 3000.0),
    // Thin pulse lead.
    arch!(SynthLeads, "Thin Pulse Lead", ["lead", "pulse", "nasal"], Osc1Wave: 2.0, Osc1Pulse: 0.12, FilterCutoff: 4000.0, FilterResonance: 0.2),
    // Filter-sweep lead: a big envelope on a closed resonant filter.
    arch!(SynthLeads, "Sweep Lead", ["lead", "sweep", "squelch"], FilterCutoff: 800.0, FilterEnvAmount: 0.8, FilterDecay: 0.3, FilterResonance: 0.5),
    // Whistle lead: sine with vibrato and glide.
    arch!(SynthLeads, "Whistle Lead", ["lead", "sine", "whistle"], Osc1Wave: 0.0, FilterCutoff: 6000.0, FilterEnvAmount: 0.0, LfoTarget: 0.0, LfoRate: 5.5, LfoDepth: 0.2, Glide: 0.04),
    // Saw stack: five-voice unison.
    arch!(SynthLeads, "Saw Stack Lead", ["lead", "unison", "big"], Osc1Unison: 5.0, Osc1Spread: 18.0, FilterCutoff: 4000.0),
    // Chip lead: quarter pulse through a 6-bit crusher.
    arch!(SynthLeads, "Chip Lead", ["lead", "chiptune", "8bit"], Osc1Wave: 2.0, Osc1Pulse: 0.25, FilterCutoff: 20000.0, FilterEnvAmount: 0.0, LofiBits: 6.0, LofiRate: 3.0, LofiMix: 0.8),
    // Detuned lead: a second saw a quarter-tone up for a sour, wide sound.
    arch!(SynthLeads, "Detuned Lead", ["lead", "detuned", "wide"], Osc2Mix: 0.7, Osc2Detune: 25.0, FilterCutoff: 3000.0),
    // FM lead: sine at 2:1 FM, bright and vocal.
    arch!(SynthLeads, "FM Lead", ["lead", "fm", "digital"], Osc1Wave: 0.0, FmAmount: 0.45, FmRatio: 2.0, FilterCutoff: 6000.0, FilterEnvAmount: 0.1),
    // Acid lead: saw into a screaming resonant filter with drive.
    arch!(SynthLeads, "Acid Lead", ["lead", "acid", "303"], FilterCutoff: 500.0, FilterResonance: 0.8, FilterEnvAmount: 0.7, FilterDecay: 0.2, FilterSustain: 0.1, DriveAmount: 0.4, DriveMix: 0.5),
    // Soft lead: triangle and sine, slow attack.
    arch!(SynthLeads, "Soft Lead", ["lead", "soft", "mellow"], Osc1Wave: 3.0, Osc2Wave: 0.0, Osc2Mix: 0.5, FilterCutoff: 2000.0, AmpAttack: 0.03),
    // Screamer: saw and square through heavy drive and resonance.
    arch!(SynthLeads, "Screamer", ["lead", "distorted", "aggressive"], Osc2Wave: 2.0, Osc2Mix: 0.5, FilterCutoff: 1800.0, FilterResonance: 0.5, DriveAmount: 0.8, DriveMix: 0.8),
    // Talking lead: band-passed saw with a wah LFO.
    arch!(SynthLeads, "Talk Lead", ["lead", "wah", "vocal"], FilterType: 1.0, FilterCutoff: 900.0, FilterResonance: 0.6, LfoTarget: 1.0, LfoRate: 3.0, LfoDepth: 0.4),
    // Pulse whistle: pulse plus breath.
    arch!(SynthLeads, "Breath Lead", ["lead", "breathy", "pulse"], Osc1Wave: 2.0, Osc1Pulse: 0.3, NoiseLevel: 0.06, NoiseColor: 1.0, FilterCutoff: 2800.0),

    // ----- Synth Bass ------------------------------------------------------------------------
    // Analog mono bass: saw plus a square 7 cents flat, enveloped filter.
    arch!(SynthBass, "Analog Bass", ["bass", "analog", "mono"], Osc2Wave: 2.0, Osc2Mix: 0.5, Osc2Detune: -7.0),
    // Sub sine bass.
    arch!(SynthBass, "Sine Sub Bass", ["bass", "sub", "sine"], Osc1Wave: 0.0, SubLevel: 0.4, FilterCutoff: 300.0, FilterEnvAmount: 0.0, DriveMix: 0.1),
    // Pluck bass: no sustain, a hard envelope.
    arch!(SynthBass, "Pluck Bass", ["bass", "pluck", "short"], AmpSustain: 0.0, AmpDecay: 0.3, FilterCutoff: 400.0, FilterEnvAmount: 0.7, FilterDecay: 0.15),
    // Rubber bass: triangle and sine with a downward pitch blip.
    arch!(SynthBass, "Rubber Bass", ["bass", "rubber", "funky"], Osc1Wave: 3.0, Osc2Wave: 0.0, Osc2Mix: 0.4, FilterCutoff: 600.0, FilterEnvAmount: 0.4, PitchEnvAmount: 2.0, PitchEnvDecay: 0.03),
    // Square bass plus sub.
    arch!(SynthBass, "Square Bass", ["bass", "square", "retro"], Osc1Wave: 2.0, SubLevel: 0.4, FilterCutoff: 900.0),
    // FM bass: 1:1 FM with an envelope on the filter.
    arch!(SynthBass, "Digital FM Bass", ["bass", "fm", "digital"], Osc1Wave: 0.0, FmAmount: 0.45, FmRatio: 1.0, FilterCutoff: 1200.0, FilterEnvAmount: 0.3),
    // Reese: two saws 18 cents apart plus unison, driven.
    arch!(SynthBass, "Reese", ["bass", "reese", "dnb"], Osc2Mix: 0.7, Osc2Detune: 18.0, Osc1Unison: 3.0, Osc1Spread: 8.0, FilterCutoff: 700.0, FilterEnvAmount: 0.2, DriveAmount: 0.4, DriveMix: 0.5),
    // Acid bass: the squelch with glide.
    arch!(SynthBass, "Acid Bass", ["bass", "acid", "303"], FilterCutoff: 350.0, FilterResonance: 0.8, FilterEnvAmount: 0.7, FilterDecay: 0.18, FilterSustain: 0.1, Glide: 0.05, DriveAmount: 0.35, DriveMix: 0.5),
    // Funk bass: pulse with a snappy filter.
    arch!(SynthBass, "Funk Bass", ["bass", "funk", "snappy"], Osc1Wave: 2.0, Osc1Pulse: 0.3, FilterCutoff: 700.0, FilterEnvAmount: 0.6, FilterDecay: 0.12, AmpSustain: 0.3),
    // Deep pulse bass.
    arch!(SynthBass, "Deep Pulse Bass", ["bass", "pulse", "deep"], Osc1Wave: 2.0, Osc1Pulse: 0.2, SubLevel: 0.5, FilterCutoff: 400.0),
    // Growl: saw and square under a fast cutoff LFO with drive.
    arch!(SynthBass, "Growl Bass", ["bass", "growl", "dubstep"], Osc2Wave: 2.0, Osc2Mix: 0.5, FilterCutoff: 500.0, LfoTarget: 1.0, LfoRate: 4.0, LfoDepth: 0.5, DriveAmount: 0.6, DriveMix: 0.6),
    // Ladder bass: two saws and a sub through a warm resonant lowpass.
    arch!(SynthBass, "Ladder Bass", ["bass", "warm", "analog"], Osc2Mix: 0.6, Osc2Detune: -5.0, SubLevel: 0.3, FilterCutoff: 400.0, FilterResonance: 0.45, FilterEnvAmount: 0.45),
    // Synth-pop bass: unison saw with a bouncy envelope.
    arch!(SynthBass, "Synthpop Bass", ["bass", "synthpop", "eighties"], Osc1Unison: 3.0, Osc1Spread: 6.0, FilterCutoff: 1200.0, FilterEnvAmount: 0.4, FilterDecay: 0.15, AmpSustain: 0.5),
    // Wobble bass: a slower LFO on a resonant filter.
    arch!(SynthBass, "Wobble", ["bass", "wobble", "dubstep"], Osc2Wave: 2.0, Osc2Mix: 0.5, FilterCutoff: 400.0, FilterResonance: 0.5, LfoTarget: 1.0, LfoRate: 2.0, LfoDepth: 0.6, DriveAmount: 0.5, DriveMix: 0.5),

    // ----- Pads & Atmos ----------------------------------------------------------------------
    // Warm analog pad: unison saw plus triangle.
    arch!(PadsAtmos, "Warm Analog Pad", ["pad", "warm", "analog"], Osc2Wave: 3.0, Osc2Mix: 0.4, FilterCutoff: 1400.0),
    // Glass pad: 3:1 FM sines.
    arch!(PadsAtmos, "Glass Pad", ["pad", "glassy", "fm"], Osc1Wave: 0.0, FmAmount: 0.2, FmRatio: 3.0, FilterCutoff: 5000.0, AmpAttack: 0.6),
    // Aah choir: unison saw through a resonant band-pass formant.
    arch!(PadsAtmos, "Aah Voices", ["choir", "vocal", "pad"], Osc1Unison: 5.0, FilterType: 1.0, FilterCutoff: 900.0, FilterResonance: 0.35, AmpAttack: 0.5),
    // Dark pad: saw and sub, low cutoff with a very slow sway.
    arch!(PadsAtmos, "Dark Matter", ["pad", "dark", "cinematic"], SubLevel: 0.4, FilterCutoff: 500.0, LfoTarget: 1.0, LfoRate: 0.1, LfoDepth: 0.3),
    // Breathing pad: saw with pink noise and a slow filter breath.
    arch!(PadsAtmos, "Breathing Pad", ["pad", "breathy", "evolving"], NoiseLevel: 0.1, NoiseColor: 1.0, LfoTarget: 1.0, LfoRate: 0.2, LfoDepth: 0.4),
    // Sweep pad: a two-second filter-envelope sweep.
    arch!(PadsAtmos, "Sweep Pad", ["pad", "sweep", "evolving"], Osc1Unison: 5.0, FilterCutoff: 400.0, FilterEnvAmount: 0.6, FilterAttack: 2.0, FilterDecay: 2.0, FilterSustain: 0.6),
    // Shimmer air: sines with 2:1 FM and an echo into the hall.
    arch!(PadsAtmos, "Shimmer Air", ["pad", "shimmer", "ambient"], Osc1Wave: 0.0, Osc2Wave: 3.0, Osc2Mix: 0.3, FmAmount: 0.1, FmRatio: 2.0, FilterCutoff: 6000.0, DelayTime: 2.0, DelayFeedback: 0.4, DelayMix: 0.2),
    // Pulse pad: two pulses of opposite width, detuned.
    arch!(PadsAtmos, "Pulse Pad", ["pad", "pulse", "retro"], Osc1Wave: 2.0, Osc1Pulse: 0.3, Osc2Wave: 2.0, Osc2Pulse: 0.7, Osc2Mix: 0.6, Osc2Detune: 6.0, ChorusMix: 0.5),
    // Ice pad: triangle and 4:1 FM through a high-pass.
    arch!(PadsAtmos, "Ice Pad", ["pad", "cold", "airy"], Osc1Wave: 3.0, FmAmount: 0.15, FmRatio: 4.0, FilterType: 2.0, FilterCutoff: 300.0),
    // Cloud pad: seven-voice unison spread wide.
    arch!(PadsAtmos, "Cloud Pad", ["pad", "wide", "lush"], Osc1Unison: 7.0, Osc1Spread: 25.0, FilterCutoff: 2000.0, ChorusMix: 0.5),
    // Noise wash: band-passed pink noise over a sine, slowly swept.
    arch!(PadsAtmos, "Noise Wash", ["texture", "noise", "ambient"], Osc1Wave: 0.0, Osc1Mix: 0.3, Osc1Unison: 1.0, NoiseLevel: 0.6, NoiseColor: 1.0, FilterType: 1.0, FilterCutoff: 1500.0, LfoTarget: 1.0, LfoRate: 0.15, LfoDepth: 0.5),
    // Organ pad: sine, square and sub with a slow attack.
    arch!(PadsAtmos, "Drawbar Pad", ["pad", "organ", "soft"], Osc1Wave: 0.0, Osc1Unison: 1.0, Osc2Wave: 2.0, Osc2Mix: 0.2, SubLevel: 0.4, AmpAttack: 0.7),
    // Hollow pad: square and triangle, a little resonance.
    arch!(PadsAtmos, "Hollow Square Pad", ["pad", "hollow", "square"], Osc1Wave: 2.0, Osc2Wave: 3.0, Osc2Mix: 0.5, FilterCutoff: 1200.0, FilterResonance: 0.2),
    // Space strings: unison saw into a quarter-note echo.
    arch!(PadsAtmos, "Space Strings", ["pad", "strings", "space"], Osc1Unison: 5.0, Osc1Spread: 20.0, FilterCutoff: 2200.0, DelayTime: 2.0, DelayFeedback: 0.4, DelayMix: 0.25),

    // ----- Arps & Stabs ----------------------------------------------------------------------
    // Saw arp: the classic enveloped saw with a sixteenth echo.
    arch!(ArpsStabs, "Saw Arp", ["arp", "saw", "trance"], FilterCutoff: 1500.0, FilterEnvAmount: 0.5, FilterDecay: 0.15, DelayTime: 0.0),
    // Square arp.
    arch!(ArpsStabs, "Square Arp", ["arp", "square", "retro"], Osc1Wave: 2.0, AmpDecay: 0.18, FilterCutoff: 2000.0),
    // Pulse arp.
    arch!(ArpsStabs, "Pulse Arp", ["arp", "pulse", "bright"], Osc1Wave: 2.0, Osc1Pulse: 0.2, FilterCutoff: 2500.0),
    // FM arp.
    arch!(ArpsStabs, "FM Arp", ["arp", "fm", "digital"], Osc1Wave: 0.0, FmAmount: 0.4, FmRatio: 2.0, FilterCutoff: 5000.0, FilterEnvAmount: 0.2),
    // Chord stab: five-voice unison, short.
    arch!(ArpsStabs, "Chord Stab", ["stab", "chord", "house"], Osc1Unison: 5.0, Osc1Spread: 14.0, AmpDecay: 0.3, FilterCutoff: 2000.0, FilterEnvAmount: 0.4),
    // Rave stab: detuned saw and square, resonant, driven.
    arch!(ArpsStabs, "Rave Stab", ["stab", "rave", "nineties"], Osc2Wave: 2.0, Osc2Mix: 0.6, Osc2Detune: 12.0, Osc1Unison: 3.0, FilterCutoff: 1800.0, FilterResonance: 0.3, FilterEnvAmount: 0.5, DriveAmount: 0.3, DriveMix: 0.4),
    // Bell arp: 3.5:1 FM sine.
    arch!(ArpsStabs, "Bell Arp", ["arp", "bell", "fm"], Osc1Wave: 0.0, FmAmount: 0.3, FmRatio: 3.5, AmpDecay: 0.4, FilterCutoff: 8000.0, FilterEnvAmount: 0.0),
    // Sequencer pluck: saw and sub with a resonant envelope (the Berlin-school sequence).
    arch!(ArpsStabs, "Sequencer Pluck", ["arp", "sequence", "berlin"], SubLevel: 0.3, FilterCutoff: 600.0, FilterEnvAmount: 0.7, FilterDecay: 0.12, FilterResonance: 0.4),
    // Dub stab: chord with a long quarter-note echo.
    arch!(ArpsStabs, "Dub Stab", ["stab", "dub", "echo"], FilterCutoff: 900.0, FilterEnvAmount: 0.4, FilterResonance: 0.3, DelayTime: 2.0, DelayFeedback: 0.45, DelayMix: 0.35, ReverbMix: 0.3),
    // Organ stab: square, sine and sub, short.
    arch!(ArpsStabs, "Organ Stab", ["stab", "organ", "house"], Osc1Wave: 2.0, Osc2Wave: 0.0, Osc2Mix: 0.5, SubLevel: 0.3, AmpDecay: 0.25, AmpSustain: 0.1, FilterCutoff: 3000.0, FilterEnvAmount: 0.2),
    // Glass arp: triangle with 4:1 FM.
    arch!(ArpsStabs, "Glass Arp", ["arp", "glassy", "fm"], Osc1Wave: 3.0, FmAmount: 0.2, FmRatio: 4.0, FilterCutoff: 6000.0, FilterEnvAmount: 0.2),
    // Chip arp: an eighth-width pulse, 6-bit.
    arch!(ArpsStabs, "Chip Arp", ["arp", "chiptune", "8bit"], Osc1Wave: 2.0, Osc1Pulse: 0.125, AmpDecay: 0.12, FilterCutoff: 20000.0, FilterEnvAmount: 0.0, LofiBits: 6.0, LofiRate: 2.0, LofiMix: 0.8),
    // Brass stab: unison saw, a hard filter blip.
    arch!(ArpsStabs, "Brass Stab", ["stab", "brass", "funk"], Osc1Unison: 3.0, FilterCutoff: 800.0, FilterEnvAmount: 0.7, FilterDecay: 0.2, AmpDecay: 0.3, AmpSustain: 0.2),
    // Synthwave arp: two saws 8 cents apart, chorus and an eighth echo.
    arch!(ArpsStabs, "Synthwave Arp", ["arp", "synthwave", "retro"], Osc2Mix: 0.6, Osc2Detune: 8.0, FilterCutoff: 1200.0, FilterEnvAmount: 0.45, ChorusMix: 0.4),
];

// ---------------------------------------------------------------------------------------------
// Variations.

struct Var {
    word: &'static str,
    tags: &'static [&'static str],
    fits: fn(&Patch, Category) -> bool,
    apply: fn(&mut Patch),
}

fn any(_: &Patch, _: Category) -> bool {
    true
}

fn not_bass(_: &Patch, c: Category) -> bool {
    c != Category::SynthBass
}

#[rustfmt::skip]
static VARIATIONS: &[Var] = &[
    Var { word: "", tags: &[], fits: any, apply: |_| {} },
    // Lo-fi: 11-bit, half rate, tape wobble, vinyl, a darker top.
    Var { word: "Dusty", tags: &["lofi", "vinyl", "dusty"], fits: any, apply: |p| {
        set(p, P::LofiBits, 11.0); set(p, P::LofiRate, 2.0); set(p, P::LofiWobble, 0.25);
        set(p, P::LofiNoise, 0.2); set(p, P::LofiTone, 6500.0); set(p, P::LofiMix, 0.6);
    } },
    // Wide: ensemble chorus, and unison where the oscillator has none.
    Var { word: "Wide", tags: &["wide", "stereo"], fits: |p, c| !is_pluck(p) && c != Category::SynthBass, apply: |p| {
        set(p, P::ChorusMix, 0.5); set(p, P::ChorusDepth, 0.45); set(p, P::ChorusRate, 0.6);
        if get(p, P::Osc1Unison) < 2.0 { set(p, P::Osc1Unison, 3.0); set(p, P::Osc1Spread, 12.0); }
        add(p, P::Osc2Detune, 6.0);
    } },
    // Bright: open filter, more resonance and velocity bite; a brighter pick on strings.
    Var { word: "Bright", tags: &["bright"], fits: any, apply: |p| {
        scale(p, P::FilterCutoff, 1.9); add(p, P::FilterResonance, 0.1); set(p, P::VelToCutoff, 0.6);
        add(p, P::PluckBright, 0.2); add(p, P::PluckDamping, -0.1); scale(p, P::LofiTone, 1.5);
    } },
    // Velvet: closed filter, softer velocity, a darker pick.
    Var { word: "Velvet", tags: &["dark", "warm", "soft"], fits: any, apply: |p| {
        scale(p, P::FilterCutoff, 0.5); scale(p, P::FilterResonance, 0.5); set(p, P::VelToCutoff, 0.2);
        add(p, P::PluckBright, -0.2); set(p, P::VelToAmp, 0.6);
    } },
    // Cathedral: a big hall, slower attack and release, a gentle chorus bloom.
    Var { word: "Cathedral", tags: &["hall", "ambient", "big"], fits: not_bass, apply: |p| {
        set(p, P::ReverbSize, 0.88); set(p, P::ReverbDamping, 0.35); set(p, P::ReverbMix, 0.45);
        scale(p, P::AmpAttack, 1.5); scale(p, P::AmpRelease, 1.6); set(p, P::ChorusMix, 0.3);
        set(p, P::ChorusDepth, 0.3); set(p, P::ChorusRate, 0.3);
    } },
    // Tape: wow and flutter, a little saturation, a soft top.
    Var { word: "Tape", tags: &["tape", "vintage"], fits: any, apply: |p| {
        set(p, P::LofiWobble, 0.5); set(p, P::LofiBits, 14.0); set(p, P::LofiNoise, 0.05);
        set(p, P::LofiTone, 9000.0); set(p, P::LofiMix, 0.45); set(p, P::DriveMix, get(p, P::DriveMix).max(0.2));
    } },
    // Driven: amp drive, the filter pulled back so it growls instead of fizzing.
    Var { word: "Driven", tags: &["driven", "dirty"], fits: |p, _| get(p, P::DriveMix) < 0.5, apply: |p| {
        set(p, P::DriveAmount, 0.55); set(p, P::DriveMix, 0.65); scale(p, P::FilterCutoff, 0.8);
        add(p, P::FilterResonance, 0.1);
    } },
    // Echoing: an eighth-note echo, a longer tail and a softer top.
    Var { word: "Echoing", tags: &["echo", "delay"], fits: |p, c| c != Category::SynthBass && get(p, P::DelayMix) < 0.2, apply: |p| {
        set(p, P::DelayTime, 1.0); set(p, P::DelayFeedback, 0.45); set(p, P::DelayMix, 0.3);
        scale(p, P::AmpRelease, 1.3); scale(p, P::FilterCutoff, 0.85); set(p, P::ChorusMix, 0.2);
        set(p, P::ChorusDepth, 0.25); set(p, P::VelToCutoff, 0.5);
    } },
    // Soft: a slower, velocity-sensitive attack and a mellower filter.
    Var { word: "Soft", tags: &["soft", "gentle"], fits: |p, _| get(p, P::AmpAttack) < 0.2 && !is_pluck(p), apply: |p| {
        set(p, P::AmpAttack, (get(p, P::AmpAttack) * 4.0).max(0.03)); set(p, P::VelToAmp, 1.0);
        scale(p, P::FilterCutoff, 0.7); scale(p, P::FilterResonance, 0.5); scale(p, P::FilterEnvAmount, 0.6);
    } },
    // Pumping: kick-triggered sidechain ducking, an EDM staple.
    Var { word: "Pumping", tags: &["sidechain", "edm", "pumping"], fits: |_, c| matches!(c, Category::PadsAtmos | Category::Strings | Category::SynthLeads | Category::ArpsStabs | Category::Keys | Category::SynthBass | Category::Organs), apply: |p| {
        set(p, P::SidechainAmount, 0.65); set(p, P::SidechainRelease, 0.2); set(p, P::ChorusMix, get(p, P::ChorusMix).max(0.2));
        scale(p, P::AmpRelease, 0.8);
    } },
    // Vintage: a drifting second oscillator, slight vibrato, a rolled-off top.
    Var { word: "Vintage", tags: &["vintage", "analog"], fits: |p, _| !is_pluck(p) && get(p, P::LfoDepth) == 0.0, apply: |p| {
        add(p, P::Osc2Detune, -9.0); add(p, P::Osc1Detune, 4.0); set(p, P::LfoTarget, 0.0);
        set(p, P::LfoRate, 4.5); set(p, P::LfoDepth, 0.04); set(p, P::LofiTone, 11000.0); set(p, P::LofiMix, 0.3);
    } },
    // Crystal: FM sparkle on a plain oscillator.
    Var { word: "Crystal", tags: &["glassy", "fm", "sparkle"], fits: |p, _| !is_pluck(p) && get(p, P::FmAmount) == 0.0, apply: |p| {
        set(p, P::FmAmount, 0.12); set(p, P::FmRatio, 3.0); scale(p, P::FilterCutoff, 1.5);
        set(p, P::Osc1Wave, 0.0);
    } },
    // Gliding: portamento between notes, legato-style.
    Var { word: "Gliding", tags: &["glide", "mono"], fits: |p, c| matches!(c, Category::SynthLeads | Category::SynthBass | Category::BrassWinds | Category::Strings) && get(p, P::Glide) == 0.0, apply: |p| {
        set(p, P::Glide, 0.09); scale(p, P::AmpRelease, 0.8); add(p, P::FilterResonance, 0.08);
        set(p, P::VelToAmp, 0.5);
    } },
    // Staccato: short decay, no sustain.
    Var { word: "Staccato", tags: &["short", "staccato"], fits: |p, _| get(p, P::AmpSustain) > 0.3 && !is_pluck(p), apply: |p| {
        set(p, P::AmpDecay, (get(p, P::AmpDecay) * 0.5).max(0.15)); set(p, P::AmpSustain, 0.0);
        scale(p, P::AmpRelease, 0.4); set(p, P::AmpAttack, get(p, P::AmpAttack).min(0.01));
    } },
    // Bitcrushed: 7-bit, quarter rate.
    Var { word: "Bitcrushed", tags: &["crushed", "8bit", "lofi"], fits: |_, c| matches!(c, Category::SynthLeads | Category::ArpsStabs | Category::SynthBass | Category::Keys | Category::BellsMallets | Category::Guitars), apply: |p| {
        set(p, P::LofiBits, 7.0); set(p, P::LofiRate, 4.0); set(p, P::LofiMix, 0.7); set(p, P::LofiTone, 7000.0);
    } },
    // Drifting: a slow filter LFO keeps the tone moving.
    Var { word: "Drifting", tags: &["evolving", "movement"], fits: |p, _| get(p, P::LfoDepth) == 0.0, apply: |p| {
        set(p, P::LfoTarget, 1.0); set(p, P::LfoRate, 0.25); set(p, P::LfoDepth, 0.35);
        scale(p, P::FilterCutoff, 0.8); add(p, P::FilterResonance, 0.1);
    } },
    // Airy: a breath of pink noise and a brighter top.
    Var { word: "Airy", tags: &["airy", "breathy"], fits: |p, c| !is_pluck(p) && matches!(c, Category::PadsAtmos | Category::BrassWinds | Category::Strings | Category::Keys | Category::Organs) && get(p, P::NoiseLevel) < 0.05, apply: |p| {
        set(p, P::NoiseLevel, 0.07); set(p, P::NoiseColor, 1.0); scale(p, P::FilterCutoff, 1.3);
        set(p, P::ReverbMix, (get(p, P::ReverbMix) + 0.1).min(0.5));
    } },
    // Singing: a vocal vibrato.
    Var { word: "Singing", tags: &["vibrato", "expressive"], fits: |p, c| get(p, P::LfoDepth) == 0.0 && matches!(c, Category::SynthLeads | Category::BrassWinds | Category::Strings | Category::Organs | Category::PadsAtmos), apply: |p| {
        set(p, P::LfoTarget, 0.0); set(p, P::LfoRate, 5.5); set(p, P::LfoDepth, 0.15); set(p, P::AmpAttack, get(p, P::AmpAttack).max(0.02));
    } },
    // Deep: a sub octave and a lower filter.
    Var { word: "Deep", tags: &["deep", "sub"], fits: |p, c| !is_pluck(p) && matches!(c, Category::SynthBass | Category::SynthLeads | Category::PadsAtmos | Category::ArpsStabs) && get(p, P::SubLevel) < 0.3, apply: |p| {
        set(p, P::SubLevel, 0.55); scale(p, P::FilterCutoff, 0.75); set(p, P::DriveMix, get(p, P::DriveMix).max(0.2));
    } },
    // Plucked on the string voice: fingerpicked, softer and shorter ring.
    Var { word: "Fingerpicked", tags: &["fingerstyle", "soft"], fits: |p, _| is_pluck(p), apply: |p| {
        add(p, P::PluckBright, -0.25); add(p, P::PluckDamping, 0.15); set(p, P::VelToAmp, 1.0);
        scale(p, P::FilterCutoff, 0.8); set(p, P::ChorusMix, get(p, P::ChorusMix).max(0.15));
    } },
    // Ringing strings: long sustain, a hall, a slow release.
    Var { word: "Ringing", tags: &["long", "ambient"], fits: |p, _| is_pluck(p) && get(p, P::PluckDamping) > 0.2, apply: |p| {
        add(p, P::PluckDamping, -0.2); scale(p, P::AmpRelease, 2.0); set(p, P::ReverbMix, 0.35);
        set(p, P::ReverbSize, 0.75); set(p, P::ChorusMix, 0.25); set(p, P::ChorusDepth, 0.3);
    } },
];

// ---------------------------------------------------------------------------------------------
// Rendering and checks (mirrors tests/presets.rs).

fn render(engine: &mut Engine, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    engine.render(&mut out, frames);
    out
}

fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f64 = buf.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / buf.len() as f64).sqrt() as f32
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter()
        .map(|&s| {
            if s.is_finite() {
                s.abs()
            } else {
                f32::INFINITY
            }
        })
        .fold(0.0, f32::max)
}

fn left(stereo: &[f32]) -> Vec<f32> {
    stereo.iter().step_by(2).copied().collect()
}

fn frames(ms: f32) -> usize {
    (SR * ms * 0.001).round() as usize
}

fn engine_with(preset: &Preset) -> Engine {
    let (_handle, mut engine) = new_engine(SR);
    preset.apply(&mut engine);
    let _ = render(&mut engine, frames(200.0));
    engine
}

/// Loudest 200 ms RMS (dBFS, windows 50 ms apart) of `note` held 1.5 s at velocity 0.8:
/// about the time the ear integrates loudness over, so a pick transient does not count
/// for more than it sounds.
fn loudness(preset: &Preset, note: u8) -> f32 {
    let mut engine = engine_with(preset);
    engine.note_on(note, 0.8);
    let held = left(&render(&mut engine, frames(1500.0)));
    let loudest = (0..=held.len() - frames(200.0))
        .step_by(frames(50.0))
        .map(|i| rms(&held[i..i + frames(200.0)]))
        .fold(0.0, f32::max);
    20.0 * loudest.max(1.0e-9).log10()
}

/// `true` when the preset meets the tests/presets.rs contract with a margin.
fn passes_contract(preset: &Preset) -> Result<(), String> {
    let bed = if preset.has_vinyl_noise() {
        let mut engine = engine_with(preset);
        let _ = render(&mut engine, frames(500.0));
        rms(&render(&mut engine, frames(3000.0)))
    } else {
        0.0
    };
    let threshold = SILENCE_RMS.max(2.0 * bed);
    let mut engine = engine_with(preset);
    engine.note_on(60, 1.0);
    let held = render(&mut engine, frames(1000.0));
    if !held.iter().all(|s| s.is_finite()) {
        return Err("not finite".into());
    }
    if rms(&left(&held)) < AUDIBLE_RMS * 3.0 {
        return Err(format!("too quiet at C4 ({})", rms(&left(&held))));
    }
    let mut top = peak(&held);
    engine.note_off(60);
    let mut prev = render(&mut engine, frames(500.0));
    top = top.max(peak(&prev));
    let mut tail = None;
    for block in 1..16 {
        let cur = render(&mut engine, frames(500.0));
        top = top.max(peak(&cur));
        let window: Vec<f32> = prev.iter().chain(&cur).copied().collect();
        if rms(&window) < threshold {
            tail = Some((block + 1) as f32 * 0.5);
            break;
        }
        prev = cur;
    }
    if top > 0.98 {
        return Err(format!("peak {top}"));
    }
    match tail {
        Some(t) if t <= TAIL_BUDGET_SECONDS => Ok(()),
        Some(t) => Err(format!("tail {t} s")),
        None => Err("never decays".into()),
    }
}

/// Rounds a value to a tidy number of digits for the generated file.
fn tidy(id: P, v: f32) -> f32 {
    let r = id.range();
    let v = v.clamp(*r.start(), *r.end());
    let step = if v.abs() >= 100.0 {
        1.0
    } else if v.abs() >= 10.0 {
        0.1
    } else {
        0.001
    };
    let t = (v / step).round() * step;
    // Keep the step's precision only (avoid 0.30000001-style noise).
    let s = format!("{t:.3}");
    s.parse::<f32>().unwrap_or(t).clamp(*r.start(), *r.end())
}

/// The ids a pack preset lists: everything except the master stage.
fn listed_ids() -> Vec<P> {
    ParamId::ALL
        .iter()
        .copied()
        .filter(|id| !matches!(id, P::MasterGain | P::LimiterOn))
        .collect()
}

fn to_preset(name: &str, cat: Category, tags: &[&str], p: &Patch) -> Preset {
    let values: Vec<(P, f32)> = listed_ids()
        .into_iter()
        .map(|id| (id, get(p, id)))
        .collect();
    let tags: Vec<&'static str> = tags
        .iter()
        .map(|t| &*Box::leak(t.to_string().into_boxed_str()))
        .collect();
    Preset {
        name: Box::leak(name.to_string().into_boxed_str()),
        category: cat,
        tags: Box::leak(tags.into_boxed_slice()),
        values: Box::leak(values.into_boxed_slice()),
    }
}

/// Track-parameter vector used by the distance check.
fn track_vector(preset: &Preset) -> Vec<f32> {
    ParamId::ALL
        .iter()
        .filter(|id| id.is_track())
        .map(|id| preset.value(*id))
        .collect()
}

fn differences(a: &[f32], b: &[f32]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// Normalises the oscillator mixes so the loudness lands near [`LOUD_TARGET`]; returns the
/// final loudness, or `None` when it cannot be brought into the window.
fn normalise(p: &mut Patch, name: &str, cat: Category, tags: &[&str]) -> Option<f32> {
    let note = level_note(cat);
    let mut level = loudness(&to_preset(name, cat, tags, p), note);
    for _ in 0..6 {
        if (LOUD_TARGET - level).abs() < 2.0 {
            break;
        }
        // Osc 1's mix is also the string's level in pluck mode.
        let mixes = [P::Osc1Mix, P::Osc2Mix, P::SubLevel, P::NoiseLevel];
        let largest = mixes.iter().map(|&m| get(p, m)).fold(0.0, f32::max);
        if largest <= 0.0 {
            break;
        }
        let k = (10.0_f32.powf((LOUD_TARGET - level) / 20.0)).min(1.0 / largest);
        if (k - 1.0).abs() < 0.02 {
            // The mixes are at full and it is still too quiet: make up the level with the
            // drive stage (gain-compensated tanh, mostly clean at these settings).
            if level >= LOUD_TARGET || get(p, P::DriveMix) >= 1.0 {
                break;
            }
            set(p, P::DriveAmount, get(p, P::DriveAmount).max(0.15) + 0.1);
            set(p, P::DriveMix, get(p, P::DriveMix) + 0.3);
        } else {
            for m in mixes {
                set(p, m, get(p, m) * k);
            }
        }
        for id in listed_ids() {
            p[id.index()] = tidy(id, get(p, id));
        }
        level = loudness(&to_preset(name, cat, tags, p), note);
    }
    if !(LOUD_MIN..=LOUD_MAX).contains(&level) {
        eprintln!("  level {name}: {level:.1} dB");
    }
    (LOUD_MIN..=LOUD_MAX).contains(&level).then_some(level)
}

struct Built {
    name: String,
    cat: Category,
    tags: Vec<&'static str>,
    patch: Patch,
    level: f32,
}

/// Builds one category: round-robin over its archetypes, one variation level at a time.
fn build_category(cat: Category, taken_names: &[String], taken: &[Vec<f32>]) -> Vec<Built> {
    let archs: Vec<&Arch> = ARCHETYPES.iter().filter(|a| a.cat == cat).collect();
    let mut out: Vec<Built> = Vec::new();
    let mut vectors: Vec<Vec<f32>> = Vec::new();
    let mut rejected = 0;
    // Each archetype gets its plain version first, then the variations that fit it in an
    // order rotated by its position, so every variation shows up across the category.
    let base_patch = |a: &Arch| {
        let mut p = neutral();
        for &(id, v) in template(cat).iter().chain(a.set) {
            set(&mut p, id, v);
        }
        p
    };
    let sequences: Vec<Vec<&Var>> = archs
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let p = base_patch(a);
            let fitting: Vec<&Var> = VARIATIONS[1..]
                .iter()
                .filter(|v| (v.fits)(&p, cat))
                .collect();
            let shift = if fitting.is_empty() {
                0
            } else {
                (i * 3) % fitting.len()
            };
            let mut seq = vec![&VARIATIONS[0]];
            seq.extend(fitting[shift..].iter().chain(&fitting[..shift]));
            seq
        })
        .collect();
    let levels = sequences.iter().map(Vec::len).max().unwrap_or(0);
    'outer: for level in 0..levels {
        for (a, seq) in archs.iter().zip(&sequences) {
            if out.len() == PER_CATEGORY {
                break 'outer;
            }
            let Some(var) = seq.get(level) else {
                continue;
            };
            let mut p = base_patch(a);
            (var.apply)(&mut p);
            for id in listed_ids() {
                p[id.index()] = tidy(id, get(&p, id));
            }
            let name = if var.word.is_empty() {
                a.name.to_string()
            } else {
                format!("{} {}", var.word, a.name)
            };
            let lower = name.to_ascii_lowercase();
            if taken_names.contains(&lower)
                || out.iter().any(|b| b.name.eq_ignore_ascii_case(&name))
            {
                eprintln!("  NAME TAKEN {name}");
                continue;
            }
            let mut tags: Vec<&'static str> = a.tags.to_vec();
            for t in var.tags {
                if !tags.contains(t) {
                    tags.push(t);
                }
            }
            let Some(level) = normalise(&mut p, &name, cat, &tags) else {
                eprintln!("  skip {name}: loudness out of window");
                rejected += 1;
                continue;
            };
            let preset = to_preset(&name, cat, &tags, &p);
            let v = track_vector(&preset);
            if let Some(other) = taken
                .iter()
                .chain(&vectors)
                .position(|o| differences(o, &v) < MIN_DIFFERENCES)
            {
                eprintln!("  skip {name}: too close to preset #{other}");
                rejected += 1;
                continue;
            }
            if let Err(why) = passes_contract(&preset) {
                eprintln!("  skip {name}: {why}");
                rejected += 1;
                continue;
            }
            vectors.push(v);
            out.push(Built {
                name,
                cat,
                tags,
                patch: p,
                level,
            });
        }
    }
    eprintln!(
        "{}: {} presets, {rejected} rejected",
        cat.label(),
        out.len()
    );
    out
}

fn fmt_value(v: f32) -> String {
    let s = format!("{v:?}");
    if s.contains('.') || s.contains('e') {
        s
    } else {
        format!("{s}.0")
    }
}

fn main() {
    let pack_categories: Vec<Category> = Category::ALL
        .iter()
        .copied()
        .filter(|c| c.index() > Category::Risers.index())
        .collect();
    // The hand-made presets this pack must stay distinct from (any existing pack entries are
    // ignored: they are what is being regenerated).
    let core: Vec<&Preset> = LIBRARY
        .iter()
        .filter(|p| p.category.index() <= Category::Risers.index())
        .chain(ALL.iter())
        .collect();
    let taken_names: Vec<String> = core.iter().map(|p| p.name.to_ascii_lowercase()).collect();
    let taken: Vec<Vec<f32>> = core.iter().map(|p| track_vector(p)).collect();

    let built: Vec<Vec<Built>> = std::thread::scope(|s| {
        let handles: Vec<_> = pack_categories
            .iter()
            .map(|&cat| {
                let (names, taken) = (&taken_names, &taken);
                s.spawn(move || build_category(cat, names, taken))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("category thread"))
            .collect()
    });

    // Cross-category checks (each thread only saw its own category).
    let all: Vec<&Built> = built.iter().flatten().collect();
    let presets: Vec<Preset> = all
        .iter()
        .map(|b| to_preset(&b.name, b.cat, &b.tags, &b.patch))
        .collect();
    let vectors: Vec<Vec<f32>> = presets.iter().map(track_vector).collect();
    for (i, a) in vectors.iter().enumerate() {
        for (j, b) in vectors.iter().enumerate().skip(i + 1) {
            assert!(
                differences(a, b) >= MIN_DIFFERENCES,
                "{} vs {} too close",
                presets[i].name,
                presets[j].name
            );
        }
        assert!(
            !presets[i + 1..]
                .iter()
                .any(|q| q.name.eq_ignore_ascii_case(presets[i].name)),
            "duplicate {}",
            presets[i].name
        );
    }
    for (cat, list) in pack_categories.iter().zip(&built) {
        assert_eq!(
            list.len(),
            PER_CATEGORY,
            "{} has {} presets",
            cat.label(),
            list.len()
        );
    }

    let ids = listed_ids();
    let mut src = String::new();
    src.push_str(
        "//! The Sound Pack: 1 000 generated presets, 100 in each of the ten categories after\n\
         //! `Category::Risers` (see docs/PRESET_PACK.md).\n\
         //!\n\
         //! @generated by `cargo run --release -p engine --example gen_preset_pack`; edit the\n\
         //! archetypes and variations there and regenerate instead of editing this file. Every\n\
         //! entry lists every parameter except the master stage, in the order of the [`row!`]\n\
         //! macro.\n\n\
         // Normalised levels can land on digits clippy takes for constants (0.318 ~ 1/pi).\n\
         #![allow(clippy::approx_constant)]\n\n\
         use super::{Category, Preset};\n\
         use crate::params::ParamId as P;\n\n",
    );
    src.push_str("/// One preset's values, in the fixed parameter order below.\n#[rustfmt::skip]\nmacro_rules! row {\n    (");
    let args: Vec<String> = (0..ids.len()).map(|i| format!("$v{i}:expr")).collect();
    src.push_str(&args.join(", "));
    src.push_str(") => {\n        &[");
    let pairs: Vec<String> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| format!("(P::{id:?}, $v{i})"))
        .collect();
    src.push_str(&pairs.join(", "));
    src.push_str("]\n    };\n}\n\n");
    src.push_str("/// The generated Sound Pack, grouped by category in `Category::ALL` order.\n#[rustfmt::skip]\npub const PACK: &[Preset] = &[\n");
    for b in &all {
        let tags: Vec<String> = b.tags.iter().map(|t| format!("{t:?}")).collect();
        let vals: Vec<String> = ids.iter().map(|&id| fmt_value(get(&b.patch, id))).collect();
        let _ = writeln!(
            src,
            "    Preset {{ name: {:?}, category: Category::{:?}, tags: &[{}], values: row![{}] }}, // {:.1} dB",
            b.name,
            b.cat,
            tags.join(", "),
            vals.join(", "),
            b.level
        );
    }
    src.push_str("];\n");

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/presets/pack.rs");
    std::fs::write(&path, src).expect("write pack.rs");
    eprintln!("wrote {} presets to {}", all.len(), path.display());
}
