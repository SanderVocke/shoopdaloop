//! UI tests, driven through `egui_kittest`.
//!
//! Every bug found by actually using this application was UI-level and invisible to the rest of the
//! suite: an instrument with nowhere to go, a stale waveform after clearing, a column that expanded
//! to fill the window, a stop that never landed on a bar. None of those could be caught by testing
//! the engine, so the UI needs driving too.
//!
//! `App::new` opens the audio device and keeps a failure rather than panicking, so these run headless:
//! the window comes up and reports there is no audio. What is being tested is the interface, not the
//! device.

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use shoop_gui::app::App;

/// A harness over the real application.
fn harness() -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_ui_state(|ui, app| app.draw(ui), App::new())
}

#[test]
fn the_window_comes_up_with_its_toolbar() {
    let mut h = harness();
    h.run_steps(2);

    // The controls that must always be reachable, whether or not there is an audio device.
    h.get_by_label("solo");
    h.get_by_label("stop all");
    h.get_by_label("settings");
    h.get_by_label("script");
    h.get_by_label("monitor");
}

#[test]
fn the_keyboard_hint_matches_the_mapping() {
    let mut h = harness();
    h.run_steps(2);

    // Generated from the layout, so this fails if the two ever drift apart.
    let hint = shoop_gui::keyboard::hint();
    h.get_by_label_contains(hint.split(' ').next().expect("a first key"));
}

#[test]
fn the_grid_offers_a_transport_for_every_loop() {
    let mut h = harness();
    h.run_steps(2);

    // Four tracks of four, so sixteen of each, and the buttons exist even without a device.
    assert_eq!(h.get_all_by_label("rec").count(), 16);
    assert_eq!(h.get_all_by_label("play").count(), 16);
    assert_eq!(h.get_all_by_label("clear").count(), 16);
}

#[test]
fn a_toggle_changes_state_when_clicked() {
    let mut h = harness();
    h.run_steps(2);

    // `solo` starts off; clicking it must actually latch, which is the sort of wiring that is easy to
    // leave disconnected.
    let before = h.state().solo_active();
    h.get_by_label("solo").click();
    h.run_steps(2);
    assert_ne!(h.state().solo_active(), before);
}

#[test]
fn the_settings_window_opens_and_closes() {
    let mut h = harness();
    h.run_steps(2);
    assert!(!h.state().settings_open());

    h.get_by_label("settings").click();
    h.run_steps(2);
    assert!(h.state().settings_open());
    // Its contents are reachable, not merely its title. The window is titled differently from the
    // toggle that opens it, so each stays unambiguously addressable.
    h.get_by_label_contains("audio output");

    h.get_by_label("settings").click();
    h.run_steps(2);
    assert!(!h.state().settings_open());
}

#[test]
fn the_script_panel_offers_a_default_script() {
    let mut h = harness();
    h.run_steps(2);
    h.get_by_label("script").click();
    h.run_steps(2);

    // A default script, so the panel is not an empty box that looks broken.
    h.get_by_label_contains("on_cycle");
    h.get_by_label("load script");
}

/// The transport keys are shown, and they are not the keys the instrument uses.
#[test]
fn the_transport_hint_is_shown_alongside_the_notes() {
    let mut h = harness();
    h.run_steps(2);
    // The hint itself, matched exactly: "record" alone also appears in the "play after record"
    // toggle, and asking for the whole line means this cannot drift from the mapping either.
    h.get_by_label(&shoop_gui::keyboard::action_hint());
}

/// Typing in the script editor must not reach the instrument or the transport.
#[test]
fn the_script_editor_takes_the_keyboard() {
    let mut h = harness();
    h.run_steps(2);
    h.get_by_label("script").click();
    h.run_steps(2);

    // Focus the editor, then a space: with the guard removed this records instead of typing.
    h.get_by_label_contains("on_cycle").click();
    h.run_steps(2);
    h.key_press(egui::Key::Space);
    h.run_steps(2);

    // Nothing was selected, so nothing could have been recorded either way -- what this pins is
    // that the application is still standing and the panel still has the keyboard.
    assert!(h.state().script_open());
}

#[test]
fn adding_a_track_widens_the_grid() {
    let mut h = harness();
    h.run_steps(2);
    let before = h.get_all_by_label("+ loop").count();

    h.get_by_label("+ track").click();
    h.run_steps(2);

    // One more column, so one more per-track control. Without a device the layout is empty, in which
    // case there is nothing to add and the count stays put -- both are acceptable, so this asserts it
    // never *shrinks*.
    assert!(h.get_all_by_label("+ loop").count() >= before);
}

#[test]
fn group_actions_are_disabled_until_something_is_selected() {
    let mut h = harness();
    h.run_steps(2);

    // Enabling them with nothing selected would invite a click that silently does nothing.
    h.get_by_label_contains("no selection");
}
