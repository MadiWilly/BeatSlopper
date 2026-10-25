//! M8 integration: the synth channel runs through the stereo insert (drive -> lo-fi ->
//! chorus -> sidechain ducker) inside the engine's render path, and the kick triggers the
//! ducker.

mod common;

use common::{SAMPLE_RATE, all_finite, left, render, right, rms, seq_engine};
use engine::{Channel, ChannelParam, DrumRow, ParamId};

/// Window RMS of the left channel over `range` (in frames).
fn window_rms(stereo: &[f32], range: std::ops::Range<usize>) -> f32 {
    rms(&left(stereo)[range])
}

/// Holds A4 from the keyboard, puts a kick on step 0 (muted in the mixer so only the synth
/// is heard), sets the sidechain `amount` and renders one second from `play`.
fn pump_render(amount: f32) -> Vec<f32> {
    let (_handle, mut engine) = seq_engine();
    engine.set_mixer(Channel::Kick, ChannelParam::Mute, 1.0);
    engine.set_drum_step(DrumRow::Kick, 0, true, true);
    engine.set_param(ParamId::SidechainAmount, amount);
    engine.set_param(ParamId::SidechainRelease, 0.25);
    engine.note_on(69, 1.0);
    // Let the mute smoother and the amp envelope settle before the transport starts.
    let _ = render(&mut engine, 4_800);
    engine.play();
    render(&mut engine, SAMPLE_RATE as usize)
}

#[test]
fn sidechain_pumps_the_synth_on_a_kick_step() {
    // 3..24 ms after the kick (past the 2 ms dip) versus 0.83..1 s later (the 250 ms release
    // has recovered about 96 % by then).
    let early = 150..1_150;
    let late = 40_000..48_000;

    let pumped = pump_render(1.0);
    assert!(all_finite(&pumped));
    let dip = window_rms(&pumped, early.clone());
    let recovered = window_rms(&pumped, late.clone());
    assert!(recovered > 0.05, "the synth must sound again: {recovered}");
    assert!(
        dip < recovered * 0.25,
        "full sidechain must dip the synth right after the kick: dip {dip}, later {recovered}"
    );

    let flat = pump_render(0.0);
    let a = window_rms(&flat, early);
    let b = window_rms(&flat, late);
    assert!(
        (a - b).abs() < b * 0.1,
        "amount 0 must not pump: early {a}, late {b}"
    );
}

/// A held saw note through the engine with the given insert settings.
fn held_note(set: impl FnOnce(&mut engine::Engine)) -> Vec<f32> {
    let (_handle, mut engine) = seq_engine();
    engine.set_param(ParamId::Osc1Wave, engine::Wave::Saw.index() as f32);
    set(&mut engine);
    engine.note_on(69, 1.0);
    let _ = render(&mut engine, 4_800);
    render(&mut engine, 9_600)
}

#[test]
fn lofi_mix_one_with_four_bits_changes_the_synth_output() {
    let clean = held_note(|_| {});
    let crushed = held_note(|e| {
        e.set_param(ParamId::LofiBits, 4.0);
        e.set_param(ParamId::LofiMix, 1.0);
    });
    assert!(all_finite(&crushed));
    let diff: Vec<f32> = clean.iter().zip(&crushed).map(|(a, b)| a - b).collect();
    let ratio = rms(&diff) / rms(&clean);
    assert!(
        ratio > 0.02,
        "4-bit lo-fi must add quantisation error to the synth: ratio {ratio}"
    );
    assert!(rms(&crushed) > 0.05, "the crushed note must still sound");
}

#[test]
fn synth_channel_is_mono_without_chorus_and_wide_with_it() {
    let dry = held_note(|_| {});
    let (l, r) = (left(&dry), right(&dry));
    let diff: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a - b).collect();
    assert!(
        rms(&diff) < 1.0e-6,
        "with every M8 mix at 0 the centre-panned synth stays mono: {}",
        rms(&diff)
    );

    let wide = held_note(|e| {
        e.set_param(ParamId::ChorusDepth, 1.0);
        e.set_param(ParamId::ChorusMix, 1.0);
    });
    assert!(all_finite(&wide));
    let (l, r) = (left(&wide), right(&wide));
    let diff: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a - b).collect();
    assert!(
        rms(&diff) > 0.01 * rms(&l),
        "chorus must make the synth channel stereo: diff {} vs {}",
        rms(&diff),
        rms(&l)
    );
}
