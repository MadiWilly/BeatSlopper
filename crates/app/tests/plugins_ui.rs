//! Plugins milestone: the EQ and reverb window contents render headless in both themes
//! (as plain panels from a `PluginShared`), clicking a preset pill applies the preset through
//! the app state, dragging an EQ node moves the band, and the `AppState` setters keep the
//! engine, the undo history and the two reverb views in sync.

use app::plugins::{PluginEdit, PluginHost, PluginKind, PluginShared};
use app::ui::plugins::{eq_graph, eq_window, reverb_window};
use app::{AppState, BeatSlopperApp};
use egui::{Vec2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use engine::fx::{eq, reverb};
use engine::{EqParam, EqState, Message, ParamId, ReverbParam, ReverbState};
use neu::{Theme, ThemeMode};

const EQ_SIZE: Vec2 = Vec2::new(760.0, 480.0);
const REVERB_SIZE: Vec2 = Vec2::new(660.0, 420.0);

/// Delivers everything the state has queued to the engine (the app does this frame by frame;
/// an undo walk sends more than one queue's worth).
fn pump(state: &mut AppState, engine: &mut engine::Engine) {
    loop {
        state.flush_outbox();
        engine.process_messages();
        if state.outbox.is_empty() {
            break;
        }
    }
}

/// A primary-button press and release at `pos` without moving the pointer away (a click;
/// two in quick succession are a double-click).
fn click_at(harness: &Harness<'_, PluginShared>, pos: egui::Pos2) {
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
}

fn eq_harness(mode: ThemeMode) -> Harness<'static, PluginShared> {
    eq_harness_dt(mode, 0.25)
}

/// The EQ window with an explicit simulated frame time (double-clicks need frames closer
/// together than egui's 0.3 s double-click window).
fn eq_harness_dt(mode: ThemeMode, step_dt: f32) -> Harness<'static, PluginShared> {
    let shared = PluginShared::new(Theme::for_mode(mode));
    Harness::builder()
        .with_size(EQ_SIZE)
        .with_step_dt(step_dt)
        .build_ui_state(
            |ui, shared: &mut PluginShared| eq_window::show(ui, shared),
            shared,
        )
}

fn reverb_harness(mode: ThemeMode) -> Harness<'static, PluginShared> {
    let mut shared = PluginShared::new(Theme::for_mode(mode));
    shared.compute_envelope_now();
    Harness::builder().with_size(REVERB_SIZE).build_ui_state(
        |ui, shared: &mut PluginShared| reverb_window::show(ui, shared),
        shared,
    )
}

#[test]
fn eq_window_renders_in_both_themes_without_edits() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut harness = eq_harness(mode);
        harness.run_steps(4);
        let shared = harness.state();
        assert_eq!(shared.theme.mode, mode);
        assert!(shared.edits.is_empty(), "rendering must not edit");
        assert_eq!(shared.eq, EqState::FLAT);
        let graph = shared.eq_graph_rect.expect("the graph was drawn");
        assert!(graph.width() > 300.0 && graph.height() >= eq_window::MIN_GRAPH_HEIGHT);
        assert!(shared.spectrum_updated.is_some(), "the analyser ran");
    }
}

#[test]
fn reverb_window_renders_in_both_themes_without_edits() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut harness = reverb_harness(mode);
        harness.run_steps(4);
        let shared = harness.state();
        assert_eq!(shared.theme.mode, mode);
        assert!(shared.edits.is_empty());
        assert_eq!(shared.reverb, ReverbState::DEFAULT);
        let env = shared.envelope.as_ref().expect("envelope present");
        assert!(!env.bins.is_empty());
    }
}

#[test]
fn clicking_an_eq_preset_pill_applies_it_through_the_app_state() {
    let mut harness = eq_harness(ThemeMode::Dark);
    harness.run_steps(2);
    harness.get_by_label("Bass Boost").click();
    harness.step();
    let edits = harness.state().edits.clone();
    assert_eq!(edits, vec![PluginEdit::EqPreset(1)]);
    assert_eq!(
        harness.state().eq,
        eq::PRESETS[1].state,
        "optimistic mirror"
    );

    // The main frame routes the edit through the app state: engine message, dirty, undo.
    let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
    assert!(!state.dirty);
    for edit in edits {
        state.apply_plugin_edit(edit);
    }
    assert_eq!(state.eq, eq::PRESETS[1].state);
    assert_eq!(state.eq_preset, Some(1));
    assert!(state.dirty);
    assert_eq!(state.undo.len(), 1);
    pump(&mut state, &mut engine);
    assert_eq!(engine.eq(), &eq::PRESETS[1].state);
    assert!(state.undo());
    assert_eq!(state.eq, EqState::FLAT);
}

#[test]
fn clicking_a_reverb_preset_pill_applies_it() {
    let mut harness = reverb_harness(ThemeMode::Light);
    harness.run_steps(2);
    harness.get_by_label("Cathedral").click();
    harness.step();
    let edits = harness.state().edits.clone();
    assert_eq!(edits, vec![PluginEdit::ReverbPreset(4)]);
    assert!(!harness.state().reverb_modified());

    let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
    for edit in edits {
        state.apply_plugin_edit(edit);
    }
    assert_eq!(state.reverb, reverb::PRESETS[4].state);
    assert_eq!(
        state.param(ParamId::ReverbSize),
        reverb::PRESETS[4].state.size
    );
    pump(&mut state, &mut engine);
    assert_eq!(engine.reverb(), &reverb::PRESETS[4].state);
    assert_eq!(
        engine.param(ParamId::ReverbMix),
        reverb::PRESETS[4].state.mix
    );
}

#[test]
fn dragging_an_eq_node_moves_the_band() {
    let mut harness = eq_harness(ThemeMode::Dark);
    harness.run_steps(2);
    let graph = harness.state().eq_graph_rect.unwrap();
    let plot = eq_graph::plot_rect(graph);
    let band = 3;
    let start = eq_graph::node_pos(plot, &harness.state().eq.bands[band]);
    let target = start + vec2(60.0, -40.0);
    let expected_freq = eq_graph::x_to_freq(plot, target.x);
    let expected_db = eq_graph::y_to_db(plot, target.y);
    assert!(expected_freq > 1000.0 && expected_db > 0.0);

    harness.hover_at(start);
    harness.step();
    harness.drag_at(start);
    harness.step();
    harness.hover_at(start + vec2(20.0, -10.0));
    harness.step();
    harness.hover_at(target);
    harness.step();
    harness.drop_at(target);
    harness.step();

    let shared = harness.state();
    assert_eq!(shared.eq_selected, band, "the dragged band is selected");
    assert!(shared.eq_drag.is_none(), "the drag ended");
    let moved = shared.eq.bands[band];
    assert!(
        (moved.freq - expected_freq).abs() / expected_freq < 0.02,
        "freq {} vs {expected_freq}",
        moved.freq
    );
    assert!(
        (moved.gain_db - expected_db).abs() < 0.5,
        "gain {} vs {expected_db}",
        moved.gain_db
    );
    let edits = shared.edits.clone();
    assert!(edits.iter().any(|e| matches!(
        e,
        PluginEdit::EqBand {
            band: 3,
            param: EqParam::Freq,
            drag_started: true,
            ..
        }
    )));
    // Through the app state: one undo step for the whole drag.
    let (mut state, _engine) = AppState::headless(ThemeMode::Dark);
    for edit in edits {
        state.apply_plugin_edit(edit);
    }
    assert_eq!(state.undo.len(), 1, "a drag coalesces into one undo step");
    assert!((state.eq.bands[band].freq - moved.freq).abs() < 1e-3);
    let untouched = eq_graph::node_pos(plot, &EqState::FLAT.bands[2]);
    assert_eq!(eq_graph::node_pos(plot, &state.eq.bands[2]), untouched);
}

#[test]
fn double_click_toggles_a_band_and_wheel_changes_q() {
    let mut harness = eq_harness_dt(ThemeMode::Dark, 1.0 / 60.0);
    harness.run_steps(2);
    let graph = harness.state().eq_graph_rect.unwrap();
    let plot = eq_graph::plot_rect(graph);
    let node = eq_graph::node_pos(plot, &harness.state().eq.bands[4]);
    harness.hover_at(node);
    harness.step();
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, 120.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(12);
    let q = harness.state().eq.bands[4].q;
    assert!(q > 1.0, "wheel up doubles Q: {q}");
    click_at(&harness, node);
    harness.step();
    click_at(&harness, node);
    harness.step();
    assert!(
        !harness.state().eq.bands[4].on,
        "double-click turned band 5 off"
    );
}

#[test]
fn app_state_setters_sync_engine_undo_and_both_reverb_views() {
    let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
    // EQ band edits: a drag coalesces, on / kind are discrete, nothing changes on no-ops.
    assert_eq!(state.set_eq_band(3, EqParam::Gain, 6.0, true), 6.0);
    assert_eq!(state.set_eq_band(3, EqParam::Gain, 7.0, false), 7.0);
    assert_eq!(
        state.set_eq_band(3, EqParam::Freq, 99_999.0, true),
        20_000.0
    );
    assert_eq!(state.undo.len(), 2, "two drags on different params");
    state.set_eq_band(0, EqParam::On, 1.0, false);
    state.set_eq_band(0, EqParam::On, 1.0, false);
    state.set_eq_band(0, EqParam::Kind, 4.0, false);
    assert_eq!(state.undo.len(), 4);
    state.set_eq_on(false);
    state.set_eq_on(false);
    assert_eq!(state.set_eq_gain(-3.0, true), -3.0);
    assert_eq!(state.undo.len(), 6);
    pump(&mut state, &mut engine);
    assert_eq!(engine.eq(), &state.eq);
    assert!(!engine.eq().on);
    assert_eq!(engine.eq().bands[3].gain_db, 7.0);

    // Reverb: both views agree whichever side writes.
    assert_eq!(state.set_reverb(ReverbParam::Size, 0.9, true), 0.9);
    assert_eq!(state.param(ParamId::ReverbSize), 0.9);
    state.set_param(ParamId::ReverbMix, 0.75);
    assert_eq!(state.reverb.mix, 0.75);
    assert_eq!(state.set_reverb(ReverbParam::PreDelay, 500.0, true), 200.0);
    pump(&mut state, &mut engine);
    assert_eq!(engine.reverb(), &state.reverb);
    assert_eq!(engine.param(ParamId::ReverbSize), 0.9);
    assert_eq!(engine.param(ParamId::ReverbMix), 0.75);

    // Synth presets carry reverb values and keep the mirror in step.
    state.apply_preset(3);
    assert_eq!(state.reverb.mix, state.param(ParamId::ReverbMix));
    assert_eq!(state.reverb.size, state.param(ParamId::ReverbSize));

    // Undo walks back through everything and resends.
    while state.undo() {}
    assert_eq!(state.eq, EqState::FLAT);
    assert_eq!(state.reverb, ReverbState::DEFAULT);
    assert_eq!(state.param(ParamId::ReverbSize), ReverbState::DEFAULT.size);
    pump(&mut state, &mut engine);
    assert_eq!(engine.eq(), &EqState::FLAT);
    assert_eq!(engine.reverb(), &ReverbState::DEFAULT);

    // Project round trip through the app state.
    state.apply_eq_preset(7);
    state.apply_reverb_preset(2);
    let project = state.build_project();
    assert_eq!(project.eq, eq::PRESETS[7].state);
    assert_eq!(project.reverb, reverb::PRESETS[2].state);
    let back = engine::Project::from_json(&project.to_json_pretty()).unwrap();
    assert_eq!(back, project);
    let (mut loaded, _e) = AppState::headless(ThemeMode::Light);
    loaded.apply_project(&back);
    assert_eq!(loaded.eq, eq::PRESETS[7].state);
    assert_eq!(loaded.reverb, reverb::PRESETS[2].state);
    assert_eq!(
        loaded.param(ParamId::ReverbSize),
        reverb::PRESETS[2].state.size
    );
}

#[test]
fn plugin_pills_toggle_windows_and_the_host_round_trips() {
    let (app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 1080.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    harness.run_steps(3);
    assert_eq!(harness.state().state().plugin_open, [false, false]);
    harness.get_by_label("EQ").click();
    harness.step();
    assert!(harness.state().state().is_plugin_open(PluginKind::Eq));
    harness.get_by_label("REVERB").click();
    harness.step();
    assert_eq!(harness.state().state().plugin_open, [true, true]);
    // Not an edit: nothing dirty, no undo step.
    assert!(!harness.state().state().dirty);
    assert!(harness.state().state().undo.is_empty());

    // The host mirrors the app state (on the next tick) and routes a window's edits back.
    harness.step();
    {
        let host: &PluginHost = harness.state().plugins();
        let shared = host.lock();
        assert_eq!(shared.windows.open_flags(), [true, true]);
        assert_eq!(shared.eq, EqState::FLAT);
    }
    harness
        .state_mut()
        .plugins_mut()
        .lock()
        .push_edit(PluginEdit::Close(PluginKind::Reverb));
    harness
        .state_mut()
        .plugins_mut()
        .lock()
        .push_edit(PluginEdit::EqGain {
            db: 4.0,
            drag_started: true,
        });
    harness.step();
    assert_eq!(harness.state().state().plugin_open, [true, false]);
    assert_eq!(harness.state().state().eq.gain_db, 4.0);
    assert!(harness.state().state().dirty);
    harness.get_by_label("EQ").click();
    harness.step();
    assert_eq!(harness.state().state().plugin_open, [false, false]);
}

#[test]
fn eq_messages_reach_the_engine_through_the_queue() {
    let (mut handle, mut engine) = engine::new_engine(48_000.0);
    let mut state = eq::PRESETS[5].state;
    state.set_gain_db(2.0);
    for msg in app::app::eq_messages(&state) {
        assert!(handle.send(msg));
    }
    for msg in app::app::reverb_messages(&reverb::PRESETS[6].state) {
        assert!(handle.send(msg));
    }
    assert!(handle.send(Message::SetEqOn(true)));
    engine.process_messages();
    assert_eq!(engine.eq(), &state);
    assert_eq!(engine.reverb(), &reverb::PRESETS[6].state);
    let _ = pos2(0.0, 0.0);
}

// ---------------------------------------------------------------------------------------------
// Reverb audit: the FX rack's three knobs (`ParamId::Reverb*`) and the REVERB window
// (`ReverbState`) are two views of one state, on the app side as well as in the engine.

/// Asserts the app's two reverb views agree with each other.
fn assert_app_views_agree(state: &AppState) {
    assert_eq!(state.param(ParamId::ReverbSize), state.reverb.size);
    assert_eq!(state.param(ParamId::ReverbDamping), state.reverb.damping);
    assert_eq!(state.param(ParamId::ReverbMix), state.reverb.mix);
}

/// Delivers the app's queued messages and asserts the engine agrees with both views.
fn assert_engine_follows(state: &mut AppState, engine: &mut engine::Engine) {
    pump(state, engine);
    assert_eq!(engine.reverb(), &state.reverb);
    assert_eq!(
        engine.param(ParamId::ReverbMix),
        state.param(ParamId::ReverbMix)
    );
    assert_eq!(
        engine.param(ParamId::ReverbSize),
        state.param(ParamId::ReverbSize)
    );
    assert_eq!(
        engine.param(ParamId::ReverbDamping),
        state.param(ParamId::ReverbDamping)
    );
}

/// Queues `edit` as the REVERB window would and runs one main frame.
fn push(harness: &mut Harness<'_, BeatSlopperApp>, edit: PluginEdit) {
    harness.state_mut().plugins_mut().lock().push_edit(edit);
    harness.step();
}

#[test]
fn rack_knob_and_window_knob_drive_one_reverb_state() {
    let (app, mut engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 1080.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    harness.run_steps(3);

    // FX rack MIX knob -> the window's mirror follows on the next tick.
    harness
        .state_mut()
        .state_mut()
        .edit_param(ParamId::ReverbMix, 0.75, true);
    harness.step();
    assert_eq!(harness.state().state().reverb.mix, 0.75);
    assert_eq!(harness.state().plugins().lock().reverb.mix, 0.75);
    assert_app_views_agree(harness.state().state());

    // Window MIX knob -> the rack's param follows.
    push(
        &mut harness,
        PluginEdit::Reverb {
            param: ReverbParam::Mix,
            value: 0.35,
            drag_started: true,
        },
    );
    assert_eq!(harness.state().state().param(ParamId::ReverbMix), 0.35);
    assert_eq!(harness.state().plugins().lock().reverb.mix, 0.35);
    assert_app_views_agree(harness.state().state());
    push(
        &mut harness,
        PluginEdit::Reverb {
            param: ReverbParam::PreDelay,
            value: 40.0,
            drag_started: true,
        },
    );
    assert_eq!(harness.state().state().reverb.pre_delay_ms, 40.0);
    let steps_before_preset = harness.state().state().undo.len();

    // A synth preset writes size / damping / mix: the window follows, the extras stay.
    let synth = app::presets::ALL[4];
    harness.state_mut().state_mut().apply_preset(4);
    harness.step();
    {
        let shared = harness.state().plugins().lock();
        assert_eq!(shared.reverb.size, synth.value(ParamId::ReverbSize));
        assert_eq!(shared.reverb.damping, synth.value(ParamId::ReverbDamping));
        assert_eq!(shared.reverb.mix, synth.value(ParamId::ReverbMix));
        assert_eq!(
            shared.reverb.pre_delay_ms, 40.0,
            "the room's extras survive"
        );
    }
    assert_app_views_agree(harness.state().state());
    assert_eq!(harness.state().state().undo.len(), steps_before_preset + 1);

    // A reverb preset from the window moves the rack knobs.
    push(&mut harness, PluginEdit::ReverbPreset(4));
    let cathedral = reverb::PRESETS[4].state;
    assert_eq!(harness.state().state().reverb, cathedral);
    assert_eq!(
        harness.state().state().param(ParamId::ReverbSize),
        cathedral.size
    );
    assert_eq!(
        harness.state().state().param(ParamId::ReverbMix),
        cathedral.mix
    );
    assert_eq!(harness.state().state().reverb_preset, Some(4));

    // One undo step restores both views (and the extras) together.
    assert!(harness.state_mut().state_mut().undo());
    harness.step();
    {
        let state = harness.state().state();
        assert_app_views_agree(state);
        assert_eq!(state.reverb.pre_delay_ms, 40.0);
        assert_eq!(state.reverb.size, synth.value(ParamId::ReverbSize));
        assert_eq!(
            state.param(ParamId::ReverbMix),
            synth.value(ParamId::ReverbMix)
        );
        assert_eq!(harness.state().plugins().lock().reverb, state.reverb);
    }
    assert!(harness.state_mut().state_mut().undo());
    {
        let state = harness.state().state();
        assert_app_views_agree(state);
        assert_eq!(state.reverb.mix, 0.35);
        assert_eq!(state.reverb.pre_delay_ms, 40.0);
    }
    while harness.state_mut().state_mut().undo() {}
    assert_eq!(harness.state().state().reverb, ReverbState::DEFAULT);
    assert_app_views_agree(harness.state().state());

    // The engine saw every step and ends where the views are.
    assert_engine_follows(harness.state_mut().state_mut(), &mut engine);
    // Redo lands on the same agreement.
    assert!(harness.state_mut().state_mut().redo());
    assert_app_views_agree(harness.state().state());
    assert_engine_follows(harness.state_mut().state_mut(), &mut engine);
}

#[test]
fn project_round_trip_keeps_window_state_and_the_last_rack_edit() {
    let (mut state, mut engine) = AppState::headless(ThemeMode::Dark);
    // The window's extras first, then the rack's knobs last.
    state.set_reverb(ReverbParam::PreDelay, 60.0, true);
    state.set_reverb(ReverbParam::Width, 0.5, true);
    state.set_reverb(ReverbParam::LowCut, 150.0, true);
    state.set_reverb(ReverbParam::HighCut, 9000.0, true);
    state.set_param(ParamId::ReverbSize, 0.77);
    state.set_param(ParamId::ReverbMix, 0.42);
    assert_app_views_agree(&state);
    assert_engine_follows(&mut state, &mut engine);
    let heard = *engine.reverb();
    assert_eq!(heard.pre_delay_ms, 60.0);
    assert_eq!(heard.mix, 0.42);

    // The app builds the file from its mirrors; the engine would build the same one.
    let project = state.build_project();
    let from_engine = engine::Project::from_engine(&engine, 0, 4, 0.8);
    assert_eq!(project.reverb, from_engine.reverb);
    assert_eq!(project.params, from_engine.params);
    assert_eq!(project.reverb, heard);
    assert_eq!(project.param(ParamId::ReverbMix), 0.42);
    let back = engine::Project::from_json(&project.to_json_pretty()).expect("round trip");
    assert_eq!(back, project);

    // Loading sends the full state exactly once and reproduces what was heard.
    let (mut loaded, mut other) = AppState::headless(ThemeMode::Light);
    let free_before = loaded.engine.free_slots();
    loaded.apply_project(&back);
    let expected = ParamId::COUNT
        + 2
        + app::app::mixer_messages(&loaded.mixer).len()
        + app::app::eq_messages(&loaded.eq).len()
        + app::app::reverb_messages(&loaded.reverb).len()
        + app::app::bank_messages(&loaded.bank, None).len();
    assert_eq!(
        free_before - loaded.engine.free_slots() + loaded.outbox.len(),
        expected,
        "one full resend, no second reverb write"
    );
    assert_eq!(loaded.reverb, heard);
    assert_app_views_agree(&loaded);
    assert_engine_follows(&mut loaded, &mut other);
    assert_eq!(other.reverb(), &heard);
    // The DSP follows once audio runs (size / damping arrive through the chain's smoothers;
    // the chain drives the reverb's own mix at 1.0 and applies the return level itself).
    let mut buf = vec![0.0_f32; 9600 * 2];
    other.render(&mut buf, 9600);
    let dsp = other.fx().reverb().state();
    assert!((dsp.size - heard.size).abs() < 1e-3, "size {}", dsp.size);
    assert!((dsp.damping - heard.damping).abs() < 1e-3);
    assert_eq!(dsp.pre_delay_ms, heard.pre_delay_ms);
    assert_eq!(dsp.width, heard.width);
    assert_eq!(dsp.low_cut_hz, heard.low_cut_hz);
    assert_eq!(dsp.high_cut_hz, heard.high_cut_hz);

    // Undoing the load brings the previous project back in both views.
    assert!(loaded.undo());
    assert_eq!(loaded.reverb, ReverbState::DEFAULT);
    assert_app_views_agree(&loaded);

    // A version-5 file has no reverb state: it is built from the params on both sides.
    let mut v5 = serde_json::to_value(engine::Project::default()).unwrap();
    v5["version"] = serde_json::json!(5);
    let obj = v5.as_object_mut().unwrap();
    obj.remove("eq");
    obj.remove("reverb");
    v5["params"]["reverb_size"] = serde_json::json!(0.33);
    v5["params"]["reverb_damping"] = serde_json::json!(0.66);
    v5["params"]["reverb_mix"] = serde_json::json!(0.55);
    let old = engine::Project::from_json(&v5.to_string()).expect("v5 loads");
    loaded.apply_project(&old);
    assert_eq!(
        loaded.reverb,
        ReverbState::new(0.33, 0.66, 0.0, 1.0, 20.0, 20_000.0, 0.55)
    );
    assert_app_views_agree(&loaded);
    assert_engine_follows(&mut loaded, &mut other);

    // A hand-edited version-6 file whose params disagree with its state: the state wins
    // everywhere (file, app mirrors, engine).
    let mut v6 = serde_json::to_value(&project).unwrap();
    v6["params"]["reverb_mix"] = serde_json::json!(0.99);
    let fixed = engine::Project::from_json(&v6.to_string()).unwrap();
    assert_eq!(fixed.param(ParamId::ReverbMix), 0.42);
    loaded.apply_project(&fixed);
    assert_eq!(loaded.param(ParamId::ReverbMix), 0.42);
    assert_app_views_agree(&loaded);
    assert_engine_follows(&mut loaded, &mut other);
}

#[test]
fn the_racks_more_pill_opens_the_reverb_window_and_the_window_hint_renders() {
    let (app, _engine) = BeatSlopperApp::headless(ThemeMode::Dark, false);
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 1080.0))
        .build_ui_state(
            |ui, app: &mut BeatSlopperApp| {
                app.tick(ui.ctx());
                app.show(ui);
            },
            app,
        );
    harness.run_steps(3);
    assert!(!harness.state().state().is_plugin_open(PluginKind::Reverb));
    harness
        .get_by_label(app::ui::fx_rack::REVERB_MORE_LABEL)
        .click();
    harness.step();
    assert!(harness.state().state().is_plugin_open(PluginKind::Reverb));
    // Not an edit: nothing dirty, no undo step, no parameter moved.
    assert!(!harness.state().state().dirty);
    assert!(harness.state().state().undo.is_empty());
    assert_eq!(harness.state().state().reverb, ReverbState::DEFAULT);
    harness
        .get_by_label(app::ui::fx_rack::REVERB_MORE_LABEL)
        .click();
    harness.step();
    assert!(!harness.state().state().is_plugin_open(PluginKind::Reverb));

    // The window body says the three values are shared with the rack.
    let mut window = reverb_harness(ThemeMode::Light);
    window.run_steps(2);
    window.get_by_label(reverb_window::SHARED_HINT);
    assert!(window.state().edits.is_empty(), "rendering must not edit");
}
