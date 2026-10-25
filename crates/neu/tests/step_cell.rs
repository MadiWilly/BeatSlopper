//! egui_kittest tests for the M3 sequencer pieces: every step-cell state renders in both
//! themes (including hover and press), a click is reported through the response and the row
//! helper, cells carry accessible names and states, and the row geometry matches the mock.

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use neu::{StepCellSpec, Theme, ThemeMode};

/// Every combination of the four flags, with and without a label.
fn all_specs() -> Vec<StepCellSpec<'static>> {
    let mut specs = Vec::new();
    for bits in 0..16u8 {
        let flags = StepCellSpec {
            on: bits & 1 != 0,
            accent: bits & 2 != 0,
            current: bits & 4 != 0,
            selected: bits & 8 != 0,
            label: None,
        };
        specs.push(flags);
        specs.push(StepCellSpec {
            label: Some(if flags.on { "D#2" } else { "–" }),
            ..flags
        });
    }
    specs
}

#[test]
fn renders_all_states_in_both_themes() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let specs = all_specs();
        let mut harness = Harness::builder()
            .with_size(vec2(900.0, 400.0))
            .build_ui(move |ui| {
                let theme = Theme::for_mode(mode);
                theme.apply(ui.ctx());
                ui.add_space(20.0);
                for (c, chunk) in specs.chunks(16).enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        for (i, spec) in chunk.iter().enumerate() {
                            neu::step_cell_named(ui, &theme, spec, &format!("Cell {}", c * 16 + i));
                        }
                    });
                }
                neu::beat_numbers(ui, &theme, 16);
                neu::playhead_row(ui, &theme, 16, Some(5));
                neu::playhead_row(ui, &theme, 16, None);
                neu::playhead_dot(ui, &theme, true);
                neu::playhead_dot(ui, &theme, false);
            });
        harness.run();
        // Hover and press an off cell and an on cell to paint the hover / pressed paths.
        harness.get_by_label("Cell 0").hover();
        harness.run();
        harness.get_by_label("Cell 0").click();
        harness.run();
        harness.get_by_label("Cell 2").hover();
        harness.run();
        harness.get_by_label("Cell 2").click();
        harness.run();
    }
}

#[test]
fn click_is_reported_and_state_is_exposed() {
    let mut harness = Harness::builder().build_ui_state(
        |ui, s: &mut (bool, u32)| {
            let theme = Theme::dark();
            theme.apply(ui.ctx());
            let spec = StepCellSpec {
                on: s.0,
                ..StepCellSpec::default()
            };
            if neu::step_cell_named(ui, &theme, &spec, "Kick step 1").clicked() {
                s.0 = !s.0;
                s.1 += 1;
            }
            // The plain `step_cell` is named after its label.
            neu::step_cell(
                ui,
                &theme,
                &StepCellSpec {
                    on: true,
                    label: Some("C2"),
                    ..StepCellSpec::default()
                },
            );
        },
        (false, 0),
    );
    harness.run();
    assert_eq!(*harness.state(), (false, 0));

    let cell = harness.get_by_label("Kick step 1");
    assert_eq!(cell.rect().size(), vec2(24.0, 24.0));
    cell.click();
    harness.run();
    assert_eq!(*harness.state(), (true, 1));
    harness.run();
    assert_eq!(
        *harness.state(),
        (true, 1),
        "clicked() only on the click frame"
    );

    harness.get_by_label("Kick step 1").click();
    harness.run();
    assert_eq!(*harness.state(), (false, 2));

    // The labelled cell is reachable by its note name.
    assert_eq!(harness.get_by_label("C2").rect().size(), vec2(24.0, 24.0));
}

/// The full-state cell: every beat tone, the note colour and the playhead ring render in
/// both themes, and the coloured row names its cells like the plain one.
#[test]
fn coloured_cells_and_beat_alternation_render() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let mut harness = Harness::builder()
            .with_size(vec2(600.0, 200.0))
            .build_ui(move |ui| {
                let theme = Theme::for_mode(mode);
                theme.apply(ui.ctx());
                let specs: Vec<StepCellSpec<'static>> = (0..16)
                    .map(|i| StepCellSpec {
                        on: i % 3 == 0,
                        accent: i % 6 == 0,
                        current: i == 5,
                        selected: i == 9,
                        label: if i % 3 == 0 { Some("C3") } else { None },
                    })
                    .collect();
                neu::step_row_coloured(ui, &theme, "Keys", &specs, Some(theme.track_colour(4)));
                neu::beat_numbers(ui, &theme, 16);
                neu::playhead_row(ui, &theme, 16, Some(5));
            });
        harness.run();
        let first = harness.get_by_label("Keys step 1").rect();
        let last = harness.get_by_label("Keys step 16").rect();
        assert_eq!(last.max.x - first.min.x, neu::step_row_width(16));
        assert_eq!(neu::step_row_width(16), 426.0);
        harness.get_by_label("Keys step 6").hover();
        harness.run();
        harness.get_by_label("Keys step 1").hover();
        harness.run();
        assert!(neu::is_alt_beat(0) && !neu::is_alt_beat(4));
    }
}

#[test]
fn step_row_reports_the_clicked_index_and_lays_cells_out_with_gaps() {
    let mut harness = Harness::builder()
        .with_size(vec2(760.0, 120.0))
        .build_ui_state(
            |ui, s: &mut ([bool; 16], Option<usize>)| {
                let theme = Theme::light();
                theme.apply(ui.ctx());
                let specs: Vec<StepCellSpec<'_>> =
                    s.0.iter()
                        .map(|&on| StepCellSpec {
                            on,
                            ..StepCellSpec::default()
                        })
                        .collect();
                let out = neu::step_row(ui, &theme, "Snare", &specs);
                if let Some(i) = out.clicked {
                    s.0[i] = !s.0[i];
                    s.1 = Some(i);
                }
            },
            ([false; 16], None),
        );
    harness.run();

    let first = harness.get_by_label("Snare step 1").rect();
    let fifth = harness.get_by_label("Snare step 5").rect();
    let last = harness.get_by_label("Snare step 16").rect();
    assert_eq!(fifth.min.x - first.min.x, neu::step_x_offset(4));
    assert_eq!(last.max.x - first.min.x, neu::step_row_width(16));

    harness.get_by_label("Snare step 5").click();
    harness.run();
    assert_eq!(harness.state().1, Some(4));
    assert!(harness.state().0[4]);
}

#[test]
fn row_geometry_matches_the_mock() {
    assert_eq!(
        neu::step_row_width(16),
        16.0 * 24.0 + 12.0 * 2.0 + 3.0 * 6.0
    );
    assert_eq!(neu::step_row_width(16), 426.0);
    assert_eq!(neu::step_x_offset(4), 4.0 * 24.0 + 3.0 * 2.0 + 6.0);
    assert_eq!(neu::step_x_offset(4), 108.0);
    assert_eq!(neu::STEP_CELL, 24.0);
    assert_eq!(neu::STEP_PITCH, 28.0);
    assert_eq!(
        neu::step_x_offset(15) + neu::STEP_CELL,
        neu::step_row_width(16)
    );
    assert_eq!(neu::step_gap_after(3), neu::STEP_GROUP_GAP);
    assert_eq!(neu::step_gap_after(2), neu::STEP_GAP);
}
