use crate::{model::TextFont, theme};
use eframe::egui::{self, FontData, FontFamily};
use fontdb::{Source, Style, Weight};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone)]
pub struct AvailableFont {
    pub label: String,
    pub spec: TextFont,
}

pub struct FontCatalog {
    pub available: Vec<AvailableFont>,
    definitions: egui::FontDefinitions,
    registered: HashSet<String>,
}

impl FontCatalog {
    pub fn discover() -> Self {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();

        let mut available = vec![AvailableFont {
            label: "Archivo Black (bundled)".to_owned(),
            spec: TextFont::default(),
        }];
        for face in database.faces() {
            let path = match &face.source {
                Source::File(path) | Source::SharedFile(path, _) => path.clone(),
                Source::Binary(_) => continue,
            };
            let family = face
                .families
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| face.post_script_name.clone());
            available.push(AvailableFont {
                label: face_label(&family, face.weight, face.style),
                spec: TextFont {
                    family,
                    face: face.post_script_name.clone(),
                    path: Some(path),
                    index: face.index,
                },
            });
        }

        available.sort_by_key(|font| font.label.to_lowercase());
        available.dedup_by(|left, right| {
            left.spec.path == right.spec.path && left.spec.index == right.spec.index
        });

        Self {
            available,
            definitions: theme::base_font_definitions(),
            registered: HashSet::from([TextFont::default().registration_key()]),
        }
    }

    pub fn ensure_registered(
        &mut self,
        context: &egui::Context,
        font: &TextFont,
    ) -> Result<(), String> {
        let key = font.registration_key();
        if self.registered.contains(&key) {
            return Ok(());
        }
        let path = font
            .path
            .as_ref()
            .ok_or_else(|| format!("Font file for {} is unavailable", font.family))?;
        let bytes = std::fs::read(path)
            .map_err(|error| format!("Could not load font {}: {error}", font.family))?;
        let mut data = FontData::from_owned(bytes);
        data.index = font.index;
        self.definitions
            .font_data
            .insert(key.clone(), Arc::new(data));

        let mut fallbacks = self
            .definitions
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        fallbacks.retain(|fallback| fallback != &key);
        fallbacks.insert(0, key.clone());
        self.definitions
            .families
            .insert(FontFamily::Name(Arc::from(key.as_str())), fallbacks);
        self.registered.insert(key);
        context.set_fonts(self.definitions.clone());
        Ok(())
    }

    pub fn register_design_fonts(
        &mut self,
        context: &egui::Context,
        fonts: &[TextFont],
    ) -> Result<(), String> {
        for font in fonts {
            self.ensure_registered(context, font)?;
        }
        Ok(())
    }
}

fn face_label(family: &str, weight: Weight, style: Style) -> String {
    let weight = match weight.0 {
        0..=149 => "Thin",
        150..=249 => "Extra Light",
        250..=349 => "Light",
        350..=449 => "Regular",
        450..=549 => "Medium",
        550..=649 => "Semibold",
        650..=749 => "Bold",
        750..=849 => "Extra Bold",
        _ => "Black",
    };
    let style = match style {
        Style::Normal => "",
        Style::Italic => " Italic",
        Style::Oblique => " Oblique",
    };
    format!("{family} · {weight}{style}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_always_contains_the_bundled_font() {
        let catalog = FontCatalog::discover();

        assert!(
            catalog
                .available
                .iter()
                .any(|font| font.spec == TextFont::default())
        );
    }

    #[test]
    fn discovered_system_face_can_be_registered_with_egui() {
        let context = egui::Context::default();
        let mut catalog = FontCatalog::discover();
        let Some(font) = catalog
            .available
            .iter()
            .find(|font| font.spec.path.is_some())
            .map(|font| font.spec.clone())
        else {
            return;
        };

        catalog
            .ensure_registered(&context, &font)
            .expect("register installed font");
        let key = font.registration_key();
        let _ = context.run_ui(Default::default(), |ui| {
            let galley = ui.painter().layout_no_wrap(
                "Installed font".to_owned(),
                egui::FontId::new(18.0, egui::FontFamily::Name(Arc::from(key.as_str()))),
                egui::Color32::WHITE,
            );
            assert!(galley.size().x > 0.0);
        });

        assert!(catalog.registered.contains(&font.registration_key()));
    }

    #[test]
    fn discovered_system_face_renders_in_product_preview() {
        let catalog = FontCatalog::discover();
        let Some(font) = catalog
            .available
            .iter()
            .find(|font| font.spec.path.is_some())
            .map(|font| font.spec.clone())
        else {
            return;
        };
        let mut design = crate::model::Design::default();
        let crate::model::ElementKind::Text {
            font: layer_font, ..
        } = &mut design.elements[1].kind
        else {
            panic!("expected default text layer");
        };
        *layer_font = font;

        let image = crate::render::render_preview_image(&design, 192)
            .expect("render installed font in preview");

        assert!(image.pixels.iter().any(|pixel| pixel.a() > 0));
    }
}
