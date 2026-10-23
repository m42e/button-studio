use crate::model::TextFont;
use fontdb::{Source, Style, Weight};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontWeight {
    Thin,
    ExtraLight,
    Light,
    Regular,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
}

impl FontWeight {
    pub fn from_weight(weight: Weight) -> Self {
        match weight.0 {
            0..=149 => Self::Thin,
            150..=249 => Self::ExtraLight,
            250..=349 => Self::Light,
            350..=449 => Self::Regular,
            450..=549 => Self::Medium,
            550..=649 => Self::SemiBold,
            650..=749 => Self::Bold,
            750..=849 => Self::ExtraBold,
            _ => Self::Black,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Thin => "Thin",
            Self::ExtraLight => "Extra Light",
            Self::Light => "Light",
            Self::Regular => "Regular",
            Self::Medium => "Medium",
            Self::SemiBold => "Semibold",
            Self::Bold => "Bold",
            Self::ExtraBold => "Extra Bold",
            Self::Black => "Black",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontSlant {
    Normal,
    Italic,
    Oblique,
}

impl FontSlant {
    pub fn from_style(style: Style) -> Self {
        match style {
            Style::Normal => Self::Normal,
            Style::Italic => Self::Italic,
            Style::Oblique => Self::Oblique,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Regular",
            Self::Italic => "Italic",
            Self::Oblique => "Oblique",
        }
    }
}

#[derive(Clone)]
pub struct AvailableFont {
    pub label: String,
    pub spec: TextFont,
    pub weight: FontWeight,
    pub slant: FontSlant,
}

pub struct FontCatalog {
    pub available: Vec<AvailableFont>,
}

impl FontCatalog {
    pub fn discover() -> Self {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();

        let mut available = vec![AvailableFont {
            label: "Archivo Black (bundled)".to_owned(),
            spec: TextFont::default(),
            weight: FontWeight::Black,
            slant: FontSlant::Normal,
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
            let weight = FontWeight::from_weight(face.weight);
            let slant = FontSlant::from_style(face.style);
            available.push(AvailableFont {
                label: face_label(&family, weight, slant),
                spec: TextFont {
                    family,
                    face: face.post_script_name.clone(),
                    path: Some(path),
                    index: face.index,
                },
                weight,
                slant,
            });
        }

        available.sort_by_key(|font| font.label.to_lowercase());
        available.dedup_by(|left, right| {
            left.spec.path == right.spec.path && left.spec.index == right.spec.index
        });

        Self { available }
    }
}

fn face_label(family: &str, weight: FontWeight, slant: FontSlant) -> String {
    if slant == FontSlant::Normal {
        format!("{family} · {}", weight.label())
    } else {
        format!("{family} · {} {}", weight.label(), slant.label())
    }
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
    fn discovered_system_face_has_weight_and_style_metadata() {
        let catalog = FontCatalog::discover();
        let Some(font) = catalog
            .available
            .iter()
            .find(|font| font.spec.path.is_some())
        else {
            return;
        };
        assert!(!font.spec.family.is_empty());
        assert!(!font.label.is_empty());
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
