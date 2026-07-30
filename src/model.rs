use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

pub const PROJECT_VERSION: u32 = 8;
pub const ELEMENT_POSITION_LIMIT: f32 = 1.0;
pub const IMAGE_WIDTH_LIMIT: f32 = 4.0;
const HISTORY_LIMIT: usize = 80;
const BLEED_PER_SIDE_MM: f32 = 5.4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageAsset {
    pub format: String,
    #[serde(with = "base64_bytes")]
    pub data: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<PathBuf>,
}

mod base64_bytes {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        STANDARD.decode(encoded).map_err(serde::de::Error::custom)
    }
}

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
        asset_id: String,
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
    pub asset_id: String,
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
            ElementKind::Image { .. } => "Image".to_owned(),
            ElementKind::QrCode { content, .. } => {
                format!("QR · {}", content)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Design {
    pub version: u32,
    #[serde(default)]
    pub assets: HashMap<String, ImageAsset>,
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
            assets: HashMap::new(),
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

    pub fn import_asset(
        &mut self,
        data: Vec<u8>,
        source_path: Option<PathBuf>,
    ) -> Result<(String, f32), String> {
        let decoded = image::load_from_memory(&data)
            .map_err(|error| format!("Unsupported or corrupt image: {error}"))?;
        let format = image::guess_format(&data)
            .map_err(|error| format!("Unsupported image format: {error}"))?;
        let asset_id = asset_digest(&data);
        let aspect_ratio = decoded.width() as f32 / decoded.height().max(1) as f32;
        self.assets.entry(asset_id.clone()).or_insert(ImageAsset {
            format: format.extensions_str()[0].to_owned(),
            data,
            source_path,
        });
        Ok((asset_id, aspect_ratio))
    }

    pub fn import_asset_path(&mut self, path: &Path) -> Result<(String, f32), String> {
        let data = fs::read(path)
            .map_err(|error| format!("Could not read image {}: {error}", path.display()))?;
        self.import_asset(data, Some(path.to_owned()))
    }

    pub fn add_image(&mut self, asset_id: String, aspect_ratio: f32) -> u64 {
        let id = self.take_id();
        self.elements.push(Element {
            id,
            position: [0.0, 0.0],
            rotation: 0.0,
            color: DesignColor::WHITE,
            opacity: 255,
            visible: true,
            kind: ElementKind::Image {
                asset_id,
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

    pub fn set_background_image(&mut self, asset_id: String, aspect_ratio: f32) -> u64 {
        let id = self.take_id();
        self.background_image = Some(BackgroundImage {
            id,
            asset_id,
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
                    *width = width.clamp(0.05, IMAGE_WIDTH_LIMIT);
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

    pub fn asset(&self, asset_id: &str) -> Result<&ImageAsset, String> {
        self.assets
            .get(asset_id)
            .ok_or_else(|| format!("Project references missing image asset {asset_id}"))
    }

    pub fn validate_assets(&self) -> Result<(), String> {
        for (asset_id, asset) in &self.assets {
            if asset_digest(&asset.data) != asset_id.as_str() {
                return Err(format!(
                    "Embedded image asset {asset_id} is corrupt (digest mismatch)"
                ));
            }
            image::load_from_memory(&asset.data)
                .map_err(|error| format!("Embedded image asset {asset_id} is invalid: {error}"))?;
        }
        for asset_id in self.referenced_asset_ids() {
            self.asset(asset_id)?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn to_project_json(&self) -> Result<Vec<u8>, String> {
        self.to_project_json_with_preview(None)
    }

    pub fn to_project_json_with_preview(
        &self,
        preview: Option<(&[u8], u32, u32)>,
    ) -> Result<Vec<u8>, String> {
        let mut project = self.clone();
        let referenced = project
            .referenced_asset_ids()
            .into_iter()
            .map(str::to_owned)
            .collect::<HashSet<_>>();
        project
            .assets
            .retain(|asset_id, _| referenced.contains(asset_id));
        project.validate_assets()?;
        let mut value = serde_json::to_value(project).map_err(|error| error.to_string())?;
        if let Some((data, width, height)) = preview {
            value["preview"] = serde_json::json!({
                "format": "png",
                "width": width,
                "height": height,
                "data": base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    data,
                ),
            });
        }
        serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())
    }

    pub fn from_project_json(bytes: &[u8], project_path: Option<&Path>) -> Result<Self, String> {
        let mut value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|error| format!("Invalid project JSON: {error}"))?;
        migrate_legacy_images(&mut value, project_path)?;
        let mut design: Self = serde_json::from_value(value)
            .map_err(|error| format!("Invalid project data: {error}"))?;
        if design.version > PROJECT_VERSION {
            return Err(format!(
                "Project version {} is newer than supported version {PROJECT_VERSION}",
                design.version
            ));
        }
        design.validate_assets()?;
        design.normalize_after_load();
        Ok(design)
    }

    fn referenced_asset_ids(&self) -> Vec<&str> {
        self.elements
            .iter()
            .filter_map(|element| match &element.kind {
                ElementKind::Image { asset_id, .. } => Some(asset_id.as_str()),
                _ => None,
            })
            .chain(
                self.background_image
                    .iter()
                    .map(|image| image.asset_id.as_str()),
            )
            .collect()
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

fn asset_digest(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn migrate_legacy_images(
    value: &mut serde_json::Value,
    project_path: Option<&Path>,
) -> Result<(), String> {
    let Some(project) = value.as_object_mut() else {
        return Err("Project root must be a JSON object".to_owned());
    };
    let mut legacy_paths = Vec::new();
    if let Some(elements) = project
        .get("elements")
        .and_then(serde_json::Value::as_array)
    {
        for element in elements {
            if element["kind"]["type"] == "image"
                && let Some(path) = element["kind"]["path"].as_str()
            {
                legacy_paths.push(path.to_owned());
            }
        }
    }
    if let Some(path) = project
        .get("background_image")
        .and_then(|image| image.get("path"))
        .and_then(serde_json::Value::as_str)
    {
        legacy_paths.push(path.to_owned());
    }
    if legacy_paths.is_empty() {
        return Ok(());
    }

    let mut migrated_assets = serde_json::Map::new();
    let mut path_to_asset = HashMap::new();
    for legacy_path in legacy_paths {
        if path_to_asset.contains_key(&legacy_path) {
            continue;
        }
        let path = resolve_legacy_path(&legacy_path, project_path);
        let data = fs::read(&path)
            .map_err(|error| format!("Could not load legacy image {}: {error}", path.display()))?;
        let mut temporary = Design::default();
        let (asset_id, _) = temporary.import_asset(data, Some(PathBuf::from(&legacy_path)))?;
        let asset = temporary.assets.remove(&asset_id).expect("inserted asset");
        migrated_assets.insert(
            asset_id.clone(),
            serde_json::to_value(asset).map_err(|error| error.to_string())?,
        );
        path_to_asset.insert(legacy_path, asset_id);
    }
    if let Some(elements) = project
        .get_mut("elements")
        .and_then(serde_json::Value::as_array_mut)
    {
        for element in elements {
            let kind = &mut element["kind"];
            if kind["type"] == "image"
                && let Some(path) = kind["path"].as_str().map(str::to_owned)
            {
                let asset_id = path_to_asset[&path].clone();
                kind.as_object_mut()
                    .expect("image kind object")
                    .remove("path");
                kind["asset_id"] = asset_id.into();
            }
        }
    }
    if let Some(background) = project
        .get_mut("background_image")
        .and_then(serde_json::Value::as_object_mut)
        && let Some(path) = background
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    {
        background.remove("path");
        background.insert("asset_id".to_owned(), path_to_asset[&path].clone().into());
    }
    project.insert("assets".to_owned(), migrated_assets.into());
    Ok(())
}

fn resolve_legacy_path(path: &str, project_path: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() || path.exists() {
        path
    } else {
        project_path
            .and_then(Path::parent)
            .map_or(path.clone(), |parent| parent.join(path))
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

    fn test_png(color: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(2, 1, image::Rgba(color));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("encode PNG");
        bytes.into_inner()
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "button-studio-{name}-{}-{}.png",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    #[test]
    fn project_json_round_trip_preserves_design() {
        let mut design = Design::default();
        design.add_shape(ShapeKind::Heart);
        design.add_qr_code("https://example.com/buttons".to_owned());

        let json = serde_json::to_string(&design).expect("serialize design");
        let restored: Design = serde_json::from_str(&json).expect("deserialize design");

        assert_eq!(restored, design);
    }

    #[test]
    fn project_preview_is_embedded_without_becoming_design_state() {
        let design = Design::default();
        let preview = test_png([10, 20, 30, 255]);

        let json = design
            .to_project_json_with_preview(Some((&preview, 2, 1)))
            .expect("serialize project with preview");
        let value: serde_json::Value = serde_json::from_slice(&json).expect("parse project");
        let restored = Design::from_project_json(&json, None).expect("load project");

        assert_eq!(value["preview"]["format"], "png");
        assert_eq!(value["preview"]["width"], 2);
        assert_eq!(value["preview"]["height"], 1);
        assert_eq!(
            value["preview"]["data"],
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &preview)
        );
        assert_eq!(restored, design);
    }

    #[test]
    fn normalize_preserves_image_width_needed_to_cover_artwork() {
        let mut design = Design::default();
        let image_id = design.add_image("asset".to_owned(), 5.0 / 3.0);
        let ElementKind::Image { width, .. } =
            &mut design.element_mut(image_id).expect("image element").kind
        else {
            panic!("expected image element");
        };
        *width = 5.0 / 3.0;

        design.normalize_after_load();

        let ElementKind::Image { width, .. } =
            &design.element(image_id).expect("image element").kind
        else {
            panic!("expected image element");
        };
        assert!((*width - 5.0 / 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn embedded_foreground_round_trip_deduplicates_by_content_not_path() {
        let mut design = Design::default();
        let bytes = test_png([10, 20, 30, 255]);
        let (first, ratio) = design
            .import_asset(bytes.clone(), Some(PathBuf::from("first.png")))
            .expect("import first image");
        let (second, _) = design
            .import_asset(bytes, Some(PathBuf::from("elsewhere/second.png")))
            .expect("import duplicate image");
        design.add_image(first.clone(), ratio);
        design.add_image(second.clone(), ratio);

        let json = design.to_project_json().expect("serialize project");
        let restored = Design::from_project_json(&json, None).expect("load project");

        assert_eq!(first, second);
        assert_eq!(restored.assets.len(), 1);
        assert_eq!(restored.elements.len(), 4);
    }

    #[test]
    fn background_and_foreground_share_one_embedded_asset() {
        let mut design = Design::default();
        let (asset_id, ratio) = design
            .import_asset(
                test_png([40, 50, 60, 255]),
                Some(PathBuf::from("shared.png")),
            )
            .expect("import image");
        design.add_image(asset_id.clone(), ratio);
        design.set_background_image(asset_id.clone(), ratio);

        let json = design.to_project_json().expect("serialize project");
        let restored = Design::from_project_json(&json, None).expect("load project");

        assert_eq!(restored.assets.len(), 1);
        assert_eq!(
            restored.background_image.expect("background").asset_id,
            asset_id
        );
    }

    #[test]
    fn legacy_path_project_loads_and_migrates_to_embedded_asset() {
        let image_path = temp_path("legacy");
        fs::write(&image_path, test_png([70, 80, 90, 255])).expect("write legacy image");
        let mut value = serde_json::to_value(Design::default()).expect("serialize design");
        value["version"] = 7.into();
        value["elements"]
            .as_array_mut()
            .expect("elements")
            .push(serde_json::json!({
                "id": 99,
                "position": [0.0, 0.0],
                "rotation": 0.0,
                "color": DesignColor::WHITE,
                "opacity": 255,
                "visible": true,
                "kind": { "type": "image", "path": image_path, "width": 0.5, "aspect_ratio": 2.0 }
            }));
        value.as_object_mut().expect("project").remove("assets");
        let bytes = serde_json::to_vec(&value).expect("serialize legacy project");

        let migrated = Design::from_project_json(&bytes, None).expect("migrate project");
        fs::remove_file(&image_path).expect("remove source image");
        let saved = migrated.to_project_json().expect("save migrated project");
        let reopened = Design::from_project_json(&saved, None).expect("reopen without source");

        assert_eq!(reopened.version, PROJECT_VERSION);
        assert_eq!(reopened.assets.len(), 1);
        assert!(matches!(
            reopened.elements.last().expect("image").kind,
            ElementKind::Image { .. }
        ));
    }

    #[test]
    fn missing_legacy_image_reports_its_path() {
        let missing = temp_path("missing-legacy");
        let mut value = serde_json::to_value(Design::default()).expect("serialize design");
        value["background_image"] = serde_json::json!({
            "id": 99,
            "path": missing,
            "aspect_ratio": 1.0
        });
        let bytes = serde_json::to_vec(&value).expect("serialize legacy project");

        let error = Design::from_project_json(&bytes, None).expect_err("missing image must fail");

        assert!(error.contains("Could not load legacy image"));
        assert!(error.contains("missing-legacy"));
    }

    #[test]
    fn corrupt_or_missing_embedded_assets_are_rejected() {
        let mut design = Design::default();
        let (asset_id, ratio) = design
            .import_asset(test_png([100, 110, 120, 255]), None)
            .expect("import image");
        design.add_image(asset_id.clone(), ratio);
        let mut corrupt = serde_json::to_value(&design).expect("serialize design");
        corrupt["assets"][&asset_id]["data"] = "AAAA".into();
        let corrupt_bytes = serde_json::to_vec(&corrupt).expect("serialize corruption");
        let corrupt_error =
            Design::from_project_json(&corrupt_bytes, None).expect_err("corrupt asset must fail");
        assert!(corrupt_error.contains("digest mismatch"));

        let mut missing = serde_json::to_value(&design).expect("serialize design");
        missing["assets"] = serde_json::json!({});
        let missing_bytes = serde_json::to_vec(&missing).expect("serialize missing asset");
        let missing_error =
            Design::from_project_json(&missing_bytes, None).expect_err("missing asset must fail");
        assert!(missing_error.contains("missing image asset"));
    }

    #[test]
    fn save_prunes_unreferenced_assets_and_image_free_projects_stay_compatible() {
        let mut design = Design::default();
        design
            .import_asset(test_png([130, 140, 150, 255]), None)
            .expect("import unused image");

        let json = design.to_project_json().expect("serialize project");
        let restored = Design::from_project_json(&json, None).expect("load project");

        assert!(restored.assets.is_empty());
        assert_eq!(restored.elements, design.elements);
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
