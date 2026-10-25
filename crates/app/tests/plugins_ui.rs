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
