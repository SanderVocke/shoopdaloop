use std::sync::Arc;

const ROBOTO_REGULAR: &str = "Roboto Regular";
const ROBOTO_ITALIC: &str = "Roboto Italic";
const ROBOTO_BOLD: &str = "Roboto Bold";
const ROBOTO_BOLD_ITALIC: &str = "Roboto Bold Italic";

const ROBOTO_REGULAR_BYTES: &[u8] =
    include_bytes!("../../../../resources/fonts/roboto/Roboto-Regular.ttf");
const ROBOTO_ITALIC_BYTES: &[u8] =
    include_bytes!("../../../../resources/fonts/roboto/Roboto-Italic.ttf");
const ROBOTO_BOLD_BYTES: &[u8] =
    include_bytes!("../../../../resources/fonts/roboto/Roboto-Bold.ttf");
const ROBOTO_BOLD_ITALIC_BYTES: &[u8] =
    include_bytes!("../../../../resources/fonts/roboto/Roboto-BoldItalic.ttf");

pub fn initialize(context: &egui::Context) {
    context.set_fonts(shoop_font_definitions());
    context.all_styles_mut(|style| {
        if let Some(heading) = style.text_styles.get_mut(&egui::TextStyle::Heading) {
            heading.family = named_family(ROBOTO_BOLD);
        }
    });
}

fn shoop_font_definitions() -> egui::FontDefinitions {
    let mut definitions = egui::FontDefinitions::default();
    let proportional_fallbacks = definitions
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    insert_font(&mut definitions, ROBOTO_REGULAR, ROBOTO_REGULAR_BYTES);
    insert_font(&mut definitions, ROBOTO_ITALIC, ROBOTO_ITALIC_BYTES);
    insert_font(&mut definitions, ROBOTO_BOLD, ROBOTO_BOLD_BYTES);
    insert_font(
        &mut definitions,
        ROBOTO_BOLD_ITALIC,
        ROBOTO_BOLD_ITALIC_BYTES,
    );

    definitions
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, ROBOTO_REGULAR.to_owned());
    definitions
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, ROBOTO_REGULAR.to_owned());
    insert_family(&mut definitions, ROBOTO_ITALIC, &proportional_fallbacks);
    insert_family(&mut definitions, ROBOTO_BOLD, &proportional_fallbacks);
    insert_family(
        &mut definitions,
        ROBOTO_BOLD_ITALIC,
        &proportional_fallbacks,
    );

    insert_font_insert(&mut definitions, egui_material_icons::font_insert());
    let material_family = definitions
        .families
        .entry(egui::FontFamily::Name(
            egui_material_icons::FONT_FAMILY.into(),
        ))
        .or_default();
    for fallback in std::iter::once(ROBOTO_REGULAR.to_owned()).chain(proportional_fallbacks) {
        if !material_family.contains(&fallback) {
            material_family.push(fallback);
        }
    }

    definitions
}

pub fn bold_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text)
        .family(named_family(ROBOTO_BOLD))
        .strong()
}

pub fn bold_italic_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text)
        .family(named_family(ROBOTO_BOLD_ITALIC))
        .strong()
}

fn insert_font(definitions: &mut egui::FontDefinitions, name: &str, bytes: &'static [u8]) {
    definitions.font_data.insert(
        name.to_owned(),
        Arc::new(egui::FontData::from_static(bytes)),
    );
}

fn insert_font_insert(
    definitions: &mut egui::FontDefinitions,
    insertion: egui::epaint::text::FontInsert,
) {
    let egui::epaint::text::FontInsert {
        name,
        data,
        families,
    } = insertion;
    definitions.font_data.insert(name.clone(), Arc::new(data));
    for insertion in families {
        let family = definitions.families.entry(insertion.family).or_default();
        match insertion.priority {
            egui::epaint::text::FontPriority::Highest => family.insert(0, name.clone()),
            egui::epaint::text::FontPriority::Lowest => family.push(name.clone()),
        }
    }
}

fn insert_family(
    definitions: &mut egui::FontDefinitions,
    name: &str,
    proportional_fallbacks: &[String],
) {
    let mut fonts = Vec::with_capacity(proportional_fallbacks.len() + 1);
    fonts.push(name.to_owned());
    fonts.extend_from_slice(proportional_fallbacks);
    definitions.families.insert(named_family(name), fonts);
}

fn named_family(name: &str) -> egui::FontFamily {
    egui::FontFamily::Name(name.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn material_icon_family_has_a_text_fallback() {
        let context = egui::Context::default();
        initialize(&context);
        let mut ignored_output_0 = context.run_ui(egui::RawInput::default(), |ui| {
            let font_id = egui::FontId::new(
                18.0,
                egui::FontFamily::Name(egui_material_icons::FONT_FAMILY.into()),
            );
            ui.fonts_mut(|fonts| {
                assert!(fonts.has_glyph(&font_id, '?'));
                assert!(fonts.has_glyph(
                    &font_id,
                    egui_material_icons::icons::ICON_PLAY_ARROW
                        .codepoint
                        .chars()
                        .next()
                        .unwrap()
                ));
            });
        });
        ignored_output_0.textures_delta.clear();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bundled_roboto_faces_are_registered() {
        let definitions = shoop_font_definitions();
        for name in [
            ROBOTO_REGULAR,
            ROBOTO_ITALIC,
            ROBOTO_BOLD,
            ROBOTO_BOLD_ITALIC,
        ] {
            let data = definitions.font_data.get(name).unwrap();
            assert_eq!(&data.font[..4], b"\0\x01\0\0");
        }
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            assert_eq!(definitions.families[&family][0], ROBOTO_REGULAR);
        }

        let material_family = egui::FontFamily::Name(egui_material_icons::FONT_FAMILY.into());
        assert_eq!(
            definitions.families[&material_family][0],
            egui_material_icons::FONT_FAMILY
        );
        assert!(definitions.families[&material_family]
            .iter()
            .any(|name| name == ROBOTO_REGULAR));
    }
}
