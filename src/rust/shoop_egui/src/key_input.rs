use std::collections::BTreeMap;

use crate::{KeyEvent, KeyEventType};

pub fn translate_events(
    events: &[egui::Event],
    text_entry_active: bool,
    pressed: &mut BTreeMap<egui::Key, (i64, i64)>,
) -> Vec<KeyEvent> {
    let mut translated = Vec::new();
    for event in events {
        match event {
            egui::Event::Key {
                key,
                pressed: is_pressed,
                repeat,
                modifiers,
                ..
            } => {
                let Some(key_value) = script_key(*key) else {
                    continue;
                };
                if *is_pressed {
                    if text_entry_active || *repeat || pressed.contains_key(key) {
                        continue;
                    }
                    let modifiers = script_modifiers(*modifiers);
                    pressed.insert(*key, (key_value, modifiers));
                    translated.push(KeyEvent {
                        event_type: KeyEventType::Pressed,
                        key: key_value,
                        modifiers,
                    });
                } else if pressed.remove(key).is_some() {
                    translated.push(KeyEvent {
                        event_type: KeyEventType::Released,
                        key: key_value,
                        modifiers: script_modifiers(*modifiers),
                    });
                }
            }
            egui::Event::WindowFocused(false) => {
                for (_, (key, modifiers)) in std::mem::take(pressed) {
                    translated.push(KeyEvent {
                        event_type: KeyEventType::Released,
                        key,
                        modifiers,
                    });
                }
            }
            _ => {}
        }
    }
    translated
}

fn script_modifiers(modifiers: egui::Modifiers) -> i64 {
    let mut value = 0;
    if modifiers.shift {
        value |= 33_554_432;
    }
    if modifiers.ctrl {
        value |= 67_108_864;
    }
    if modifiers.alt {
        value |= 134_217_728;
    }
    if modifiers.mac_cmd {
        value |= 268_435_456;
    }
    value
}

fn script_key(key: egui::Key) -> Option<i64> {
    Some(match key {
        egui::Key::Space => 32,
        egui::Key::Period => 46,
        egui::Key::Num0 => 48,
        egui::Key::Num1 => 49,
        egui::Key::Num2 => 50,
        egui::Key::Num3 => 51,
        egui::Key::Num4 => 52,
        egui::Key::Num5 => 53,
        egui::Key::Num6 => 54,
        egui::Key::Num7 => 55,
        egui::Key::Num8 => 56,
        egui::Key::Num9 => 57,
        egui::Key::A => 65,
        egui::Key::B => 66,
        egui::Key::C => 67,
        egui::Key::D => 68,
        egui::Key::E => 69,
        egui::Key::F => 70,
        egui::Key::G => 71,
        egui::Key::H => 72,
        egui::Key::I => 73,
        egui::Key::J => 74,
        egui::Key::K => 75,
        egui::Key::L => 76,
        egui::Key::M => 77,
        egui::Key::N => 78,
        egui::Key::O => 79,
        egui::Key::P => 80,
        egui::Key::Q => 81,
        egui::Key::R => 82,
        egui::Key::S => 83,
        egui::Key::T => 84,
        egui::Key::U => 85,
        egui::Key::V => 86,
        egui::Key::W => 87,
        egui::Key::X => 88,
        egui::Key::Y => 89,
        egui::Key::Z => 90,
        egui::Key::Escape => 16_777_216,
        egui::Key::Tab => 16_777_217,
        egui::Key::Backspace => 16_777_219,
        egui::Key::Enter => 16_777_220,
        egui::Key::Insert => 16_777_222,
        egui::Key::Delete => 16_777_223,
        egui::Key::Home => 16_777_232,
        egui::Key::End => 16_777_233,
        egui::Key::ArrowLeft => 16_777_234,
        egui::Key::ArrowUp => 16_777_235,
        egui::Key::ArrowRight => 16_777_236,
        egui::Key::ArrowDown => 16_777_237,
        egui::Key::PageUp => 16_777_238,
        egui::Key::PageDown => 16_777_239,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: egui::Key, pressed: bool, repeat: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers,
        }
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn translates_script_values_modifiers_and_suppresses_repeats() {
        let mut pressed = BTreeMap::new();
        let modifiers = egui::Modifiers {
            shift: true,
            ctrl: true,
            ..Default::default()
        };
        let events = translate_events(
            &[
                key(egui::Key::Space, true, false, modifiers),
                key(egui::Key::Space, true, true, modifiers),
                key(egui::Key::Space, false, false, modifiers),
            ],
            false,
            &mut pressed,
        );
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].key, 32);
        assert_eq!(events[0].modifiers, 33_554_432 | 67_108_864);
        assert_eq!(events[0].event_type, KeyEventType::Pressed);
        assert_eq!(events[1].event_type, KeyEventType::Released);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn text_entry_suppresses_new_keys_and_focus_loss_releases_held_keys() {
        let mut pressed = BTreeMap::new();
        assert!(translate_events(
            &[key(egui::Key::R, true, false, egui::Modifiers::NONE)],
            true,
            &mut pressed,
        )
        .is_empty());
        translate_events(
            &[key(egui::Key::R, true, false, egui::Modifiers::NONE)],
            false,
            &mut pressed,
        );
        let released = translate_events(&[egui::Event::WindowFocused(false)], false, &mut pressed);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].key, 82);
        assert_eq!(released[0].event_type, KeyEventType::Released);
        assert!(pressed.is_empty());
    }
}
