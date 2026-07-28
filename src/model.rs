use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

pub const PROJECT_VERSION: u32 = 7;
pub const ELEMENT_POSITION_LIMIT: f32 = 1.0;
const HISTORY_LIMIT: usize = 80;
const BLEED_PER_SIDE_MM: f32 = 5.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl DesignColor {
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const INK: Self = Self::rgb(29, 31, 36);

    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    pub fn with_alpha(self, alpha: u8) -> Self {
        Self { alpha, ..self }
    }
}

impl From<DesignColor> for Color32 {
    fn from(color: DesignColor) -> Self {
        Self::from_rgba_unmultiplied(color.red, color.green, color.blue, color.alpha)
    }
}

impl From<Color32> for DesignColor {
    fn from(color: Color32) -> Self {
        Self {
            red: color.r(),
            green: color.g(),
            blue: color.b(),
            alpha: color.a(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackgroundStyle {
    #[default]
    Solid,
    None,
    Sunburst,
    Confetti,
    Grid,
}

impl BackgroundStyle {
    pub const ALL: [Self; 5] = [
        Self::Solid,
        Self::None,
        Self::Sunburst,
        Self::Confetti,
        Self::Grid,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::None => "No background",
            Self::Sunburst => "Sunburst",
            Self::Confetti => "Confetti",
            Self::Grid => "Grid",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeKind {
    #[default]
    Star,
    Heart,
    Circle,
    Rectangle,
    Diamond,
    Bolt,
}

impl ShapeKind {
    pub const ALL: [Self; 6] = [
        Self::Star,
        Self::Heart,
        Self::Circle,
        Self::Rectangle,
        Self::Diamond,
        Self::Bolt,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Star => "Star",
            Self::Heart => "Heart",
            Self::Circle => "Circle",
            Self::Rectangle => "Rectangle",
            Self::Diamond => "Diamond",
            Self::Bolt => "Bolt",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextFont {
    pub family: String,
    pub face: String,
    pub path: Option<PathBuf>,
    pub index: u32,
}

impl TextFont {
    pub fn registration_key(&self) -> String {
        if self.path.is_none() {
            self.family.clone()
        } else {
            format!("font::{}::{}", self.face, self.index)
        }
    }
}

impl Default for TextFont {
    fn default() -> Self {
        Self {
            family: "Archivo Black".to_owned(),
            face: "ArchivoBlack-Regular".to_owned(),
            path: None,
            index: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElementKind {
    Text {
        text: String,
        font_size: f32,
        #[serde(default)]
        font: TextFont,
        #[serde(default)]
        curvature: f32,
    },
    Shape {
        shape: ShapeKind,
        size: f32,
    },
    Image {
        path: PathBuf,
        width: f32,
        aspect_ratio: f32,
    },
    QrCode {
        content: String,
        size: f32,
        #[serde(default = "default_qr_background")]
        background: DesignColor,
        #[serde(default)]
        transparent_background: bool,
    },
}

fn default_qr_background() -> DesignColor {
    DesignColor::WHITE
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BackgroundImage {
    pub id: u64,
    pub path: PathBuf,
    pub aspect_ratio: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: u64,
    pub position: [f32; 2],
    #[serde(default)]
    pub rotation: f32,
    pub color: DesignColor,
    pub opacity: u8,
    pub visible: bool,
    pub kind: ElementKind,
}

impl Element {
    pub fn name(&self) -> String {
        match &self.kind {
            ElementKind::Text { text, .. } => text.clone(),
            ElementKind::Shape { shape, .. } => shape.label().to_owned(),
            ElementKind::Image { path, .. } => path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Image")
                .to_owned(),
            ElementKind::QrCode { content, .. } => {
                format!("QR · {}", content)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Design {
    pub version: u32,
    pub diameter_mm: u32,
    pub background: DesignColor,
    pub background_style: BackgroundStyle,
    #[serde(default)]
    pub background_image: Option<BackgroundImage>,
    pub elements: Vec<Element>,
    next_id: u64,
}

impl Default for Design {
    fn default() -> Self {
        Self {
            version: PROJECT_VERSION,
            diameter_mm: 59,
            background: DesignColor::rgb(27, 102, 91),
            background_style: BackgroundStyle::Sunburst,
            background_image: None,
            elements: vec![
                Element {
                    id: 1,
                    position: [0.0, -0.18],
                    rotation: 0.0,
                    color: DesignColor::rgb(255, 207, 77),
                    opacity: 255,
                    visible: true,
                    kind: ElementKind::Shape {
                        shape: ShapeKind::Star,
                        size: 0.28,
                    },
                },
                Element {
                    id: 2,
                    position: [0.0, 0.2],
                    rotation: 0.0,
                    color: DesignColor::WHITE,
                    opacity: 255,
                    visible: true,
                    kind: ElementKind::Text {
                        text: "MAKE IT YOURS".to_owned(),
                        font_size: 0.095,
                        font: TextFont::default(),
                        curvature: 0.0,
                    },
                },
            ],
            next_id: 3,
        }
    }
}

impl Design {
    pub fn personalized(&self, values: &HashMap<String, String>) -> Self {
        let mut personalized = self.clone();
        for element in &mut personalized.elements {
            let content = match &mut element.kind {
                ElementKind::Text { text, .. } => text,
                ElementKind::QrCode { content, .. } => content,
                _ => continue,
            };
            for (variable, value) in values {
                *content = content.replace(&format!("{{{{{variable}}}}}"), value);
            }
        }
        personalized
    }

    pub fn artwork_diameter_mm(&self) -> f32 {
        self.diameter_mm as f32 + BLEED_PER_SIDE_MM * 2.0
    }

    pub fn trim_ratio(&self) -> f32 {
        self.diameter_mm as f32 / self.artwork_diameter_mm()
    }

    pub fn trim_radius(&self, artwork_radius: f32) -> f32 {
        artwork_radius * self.trim_ratio()
    }

    pub fn artwork_radius(&self, trim_radius: f32) -> f32 {
        trim_radius / self.trim_ratio()
    }

    pub fn add_text(&mut self, text: String) -> u64 {
        let id = self.take_id();
        self.elements.push(Element {
            id,
            position: [0.0, 0.0],
            rotation: 0.0,
            color: DesignColor::WHITE,
            opacity: 255,
            visible: true,
            kind: ElementKind::Text {
                text,
                font_size: 0.09,
                font: TextFont::default(),
                curvature: 0.0,
            },
        });
        id
    }

    pub fn add_shape(&mut self, shape: ShapeKind) -> u64 {
        let id = self.take_id();
        self.elements.push(Element {
            id,
            position: [0.0, 0.0],
            rotation: 0.0,
            color: DesignColor::rgb(255, 207, 77),
            opacity: 255,
            visible: true,
            kind: ElementKind::Shape { shape, size: 0.27 },
        });
        id
    }

    pub fn add_image(&mut self, path: PathBuf, aspect_ratio: f32) -> u64 {
        let id = self.take_id();
        self.elements.push(Element {
            id,
            position: [0.0, 0.0],
            rotation: 0.0,
            color: DesignColor::WHITE,
            opacity: 255,
            visible: true,
            kind: ElementKind::Image {
                path,
                width: 0.5,
                aspect_ratio: aspect_ratio.max(0.01),
            },
        });
        id
    }

    pub fn add_qr_code(&mut self, content: String) -> u64 {
        let id = self.take_id();
        self.elements.push(Element {
            id,
            position: [0.0, 0.0],
            rotation: 0.0,
            color: DesignColor::INK,
            opacity: 255,
            visible: true,
            kind: ElementKind::QrCode {
                content,
                size: 0.46,
                background: default_qr_background(),
                transparent_background: false,
            },
        });
        id
    }

    pub fn set_background_image(&mut self, path: PathBuf, aspect_ratio: f32) -> u64 {
        let id = self.take_id();
        self.background_image = Some(BackgroundImage {
            id,
            path,
            aspect_ratio: aspect_ratio.max(0.01),
        });
        id
    }

    pub fn delete(&mut self, id: u64) -> bool {
        let original_len = self.elements.len();
        self.elements.retain(|element| element.id != id);
        self.elements.len() != original_len
    }

    pub fn duplicate(&mut self, id: u64) -> Option<u64> {
        let mut duplicate = self
            .elements
            .iter()
            .find(|element| element.id == id)?
            .clone();
        duplicate.id = self.take_id();
        duplicate.position[0] = (duplicate.position[0] + 0.08).min(0.72);
        duplicate.position[1] = (duplicate.position[1] + 0.08).min(0.72);
        let new_id = duplicate.id;
        self.elements.push(duplicate);
        Some(new_id)
    }

    pub fn move_layer(&mut self, id: u64, toward_front: bool) -> bool {
        let Some(index) = self.elements.iter().position(|element| element.id == id) else {
            return false;
        };
        let target = if toward_front {
            (index + 1).min(self.elements.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        if target == index {
            return false;
        }
        self.elements.swap(index, target);
        true
    }

    pub fn element(&self, id: u64) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }

    pub fn element_mut(&mut self, id: u64) -> Option<&mut Element> {
        self.elements.iter_mut().find(|element| element.id == id)
    }

    pub fn normalize_after_load(&mut self) {
        self.version = PROJECT_VERSION;
        if ![25, 32, 38, 50, 59, 75].contains(&self.diameter_mm) {
            self.diameter_mm = 59;
        }
        self.elements.truncate(200);
        for element in &mut self.elements {
            element.position[0] =
                element.position[0].clamp(-ELEMENT_POSITION_LIMIT, ELEMENT_POSITION_LIMIT);
            element.position[1] =
                element.position[1].clamp(-ELEMENT_POSITION_LIMIT, ELEMENT_POSITION_LIMIT);
            element.rotation = if element.rotation.is_finite() {
                (element.rotation + 180.0).rem_euclid(360.0) - 180.0
            } else {
                0.0
            };
            match &mut element.kind {
                ElementKind::Text {
                    text,
                    font_size,
                    curvature,
                    ..
                } => {
                    text.truncate(200);
                    *font_size = font_size.clamp(0.025, 0.24);
                    *curvature = curvature.clamp(-1.0, 1.0);
                }
                ElementKind::Shape { size, .. } => *size = size.clamp(0.05, 0.8),
                ElementKind::Image {
                    width,
                    aspect_ratio,
                    ..
                } => {
                    *width = width.clamp(0.05, 1.5);
                    *aspect_ratio = aspect_ratio.clamp(0.01, 100.0);
                }
                ElementKind::QrCode { content, size, .. } => {
                    content.truncate(2_048);
                    *size = size.clamp(0.08, 1.2);
                }
            }
        }
        if let Some(background_image) = &mut self.background_image {
            background_image.aspect_ratio = background_image.aspect_ratio.clamp(0.01, 100.0);
        }
        self.next_id = self
            .elements
            .iter()
            .map(|element| element.id)
            .chain(self.background_image.iter().map(|image| image.id))
            .max()
            .unwrap_or(0)
            + 1;
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<Design>,
    redo: Vec<Design>,
}

impl History {
    pub fn record(&mut self, previous: Design) {
        if self.undo.last() == Some(&previous) {
            return;
        }
        self.undo.push(previous);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, design: &mut Design) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(design, previous));
        true
    }

    pub fn redo(&mut self, design: &mut Design) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(design, next));
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_json_round_trip_preserves_design() {
        let mut design = Design::default();
        design.add_shape(ShapeKind::Heart);
        design.add_qr_code("https://example.com/buttons".to_owned());
        design.set_background_image(PathBuf::from("backdrop.png"), 1.5);

        let json = serde_json::to_string(&design).expect("serialize design");
        let restored: Design = serde_json::from_str(&json).expect("deserialize design");

        assert_eq!(restored, design);
    }

    #[test]
    fn legacy_project_without_background_image_still_loads() {
        let mut value = serde_json::to_value(Design::default()).expect("serialize design");
        value
            .as_object_mut()
            .expect("design object")
            .remove("background_image");

        let restored: Design = serde_json::from_value(value).expect("deserialize legacy design");

        assert!(restored.background_image.is_none());
    }

    #[test]
    fn legacy_text_without_font_defaults_to_bundled_face() {
        let mut value = serde_json::to_value(Design::default()).expect("serialize design");
        value["elements"][1]
            .as_object_mut()
            .expect("text element")
            .remove("font");

        let restored: Design = serde_json::from_value(value).expect("deserialize legacy design");
        let ElementKind::Text { font, .. } = &restored.elements[1].kind else {
            panic!("expected text element");
        };

        assert_eq!(font, &TextFont::default());
    }

    #[test]
    fn legacy_text_without_curvature_stays_straight() {
        let mut value = serde_json::to_value(Design::default()).expect("serialize design");
        value["elements"][1]
            .as_object_mut()
            .expect("text element")
            .remove("curvature");

        let restored: Design = serde_json::from_value(value).expect("deserialize legacy design");
        let ElementKind::Text { curvature, .. } = &restored.elements[1].kind else {
            panic!("expected text element");
        };

        assert_eq!(*curvature, 0.0);
    }

    #[test]
    fn legacy_element_without_rotation_defaults_to_zero() {
        let mut value = serde_json::to_value(Design::default()).expect("serialize design");
        value["elements"][0]
            .as_object_mut()
            .expect("element")
            .remove("rotation");

        let restored: Design = serde_json::from_value(value).expect("deserialize legacy design");

        assert_eq!(restored.elements[0].rotation, 0.0);
    }

    #[test]
    fn legacy_qr_code_defaults_to_white_background() {
        let mut design = Design::default();
        design.add_qr_code("https://example.com".to_owned());
        let mut value = serde_json::to_value(design).expect("serialize design");
        let qr_code = value["elements"][2]
            .as_object_mut()
            .expect("QR code element");
        qr_code.remove("background");
        qr_code.remove("transparent_background");

        let restored: Design = serde_json::from_value(value).expect("deserialize legacy design");
        let ElementKind::QrCode {
            background,
            transparent_background,
            ..
        } = &restored.elements[2].kind
        else {
            panic!("expected QR code element");
        };

        assert_eq!(*background, DesignColor::WHITE);
        assert!(!transparent_background);
    }

    #[test]
    fn undo_and_redo_restore_whole_document() {
        let mut design = Design::default();
        let original = design.clone();
        let mut history = History::default();

        history.record(design.clone());
        design.background = DesignColor::rgb(10, 20, 30);
        assert!(history.undo(&mut design));
        assert_eq!(design, original);
        assert!(history.redo(&mut design));
        assert_eq!(design.background, DesignColor::rgb(10, 20, 30));
    }

    #[test]
    fn trim_ratio_maps_artwork_to_the_physical_button_size() {
        let design = Design::default();

        assert!((design.artwork_diameter_mm() - 69.8).abs() < f32::EPSILON);
        assert!((design.trim_ratio() - 59.0 / 69.8).abs() < f32::EPSILON);
        assert!((design.trim_radius(349.0) - 295.0).abs() < f32::EPSILON);
        assert!((design.artwork_radius(295.0) - 349.0).abs() < f32::EPSILON);
    }

    #[test]
    fn personalization_replaces_text_and_qr_variables_without_mutating_source() {
        let mut design = Design::default();
        design.elements[1].kind = ElementKind::Text {
            text: "{{name}} · {{team}} · {{name}} · {{missing}}".to_owned(),
            font_size: 0.1,
            font: TextFont::default(),
            curvature: 0.0,
        };
        design.add_qr_code("https://example.com/{{id}}".to_owned());
        let values = HashMap::from([
            ("name".to_owned(), "Ada".to_owned()),
            ("team".to_owned(), "Research".to_owned()),
            ("id".to_owned(), "42".to_owned()),
        ]);

        let personalized = design.personalized(&values);

        let ElementKind::Text { text, .. } = &personalized.elements[1].kind else {
            panic!("expected text element");
        };
        let ElementKind::QrCode { content, .. } = &personalized.elements[2].kind else {
            panic!("expected QR code element");
        };
        assert_eq!(text, "Ada · Research · Ada · {{missing}}");
        assert_eq!(content, "https://example.com/42");
        assert!(matches!(
            &design.elements[1].kind,
            ElementKind::Text { text, .. } if text.contains("{{name}}")
        ));
    }
}
