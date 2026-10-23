use crate::{
    appearance::Appearance,
    model::{Design, DesignColor, Element, ElementKind, ShapeKind, TextFont},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPalette {
    pub name: String,
    pub colors: Vec<DesignColor>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedStyle {
    pub name: String,
    pub color: DesignColor,
    pub font: Option<TextFont>,
    pub font_size: Option<f32>,
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    #[serde(default)]
    pub appearance: Appearance,
}
fn default_opacity() -> u8 {
    255
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignTemplate {
    pub name: String,
    pub description: String,
    pub design: Design,
}
fn element(id: u64, pos: [f32; 2], color: DesignColor, kind: ElementKind) -> Element {
    Element {
        id,
        name_override: None,
        locked: false,
        group_id: None,
        flip_x: false,
        flip_y: false,
        appearance: Appearance::default(),
        position: pos,
        rotation: 0.0,
        color,
        opacity: 255,
        visible: true,
        kind,
    }
}
pub fn builtin_templates() -> Vec<DesignTemplate> {
    let mut centered = Design::default();
    centered.elements.clear();
    centered.background_style = crate::model::BackgroundStyle::Solid;
    centered.elements.push(element(
        1,
        [0.0, -0.08],
        DesignColor::WHITE,
        ElementKind::Text {
            text: "YOUR TITLE".into(),
            font_size: 0.11,
            font: TextFont::default(),
            curvature: 0.0,
        },
    ));
    centered.elements.push(element(
        2,
        [0.0, 0.27],
        DesignColor::rgb(255, 207, 77),
        ElementKind::Shape {
            shape: ShapeKind::Star,
            size: 0.22,
            width: None,
            height: None,
        },
    ));
    let mut arc = Design::default();
    arc.elements.clear();
    arc.background_style = crate::model::BackgroundStyle::Solid;
    arc.elements.push(element(
        1,
        [0.0, -0.25],
        DesignColor::WHITE,
        ElementKind::Text {
            text: "YOUR BADGE".into(),
            font_size: 0.09,
            font: TextFont::default(),
            curvature: 0.65,
        },
    ));
    arc.elements.push(element(
        2,
        [0.0, 0.18],
        DesignColor::rgb(255, 207, 77),
        ElementKind::Shape {
            shape: ShapeKind::Star,
            size: 0.27,
            width: None,
            height: None,
        },
    ));
    let mut name = Design::default();
    name.elements.clear();
    name.background_style = crate::model::BackgroundStyle::Solid;
    name.elements.push(element(
        1,
        [0.0, 0.0],
        DesignColor::WHITE,
        ElementKind::Text {
            text: "{{name}}".into(),
            font_size: 0.13,
            font: TextFont::default(),
            curvature: 0.0,
        },
    ));
    centered.normalize_after_load();
    arc.normalize_after_load();
    name.normalize_after_load();
    vec![
        DesignTemplate {
            name: "Centered title + mark".into(),
            description: "A centered headline with a simple logo placeholder.".into(),
            design: centered,
        },
        DesignTemplate {
            name: "Arc badge".into(),
            description: "Curved title above a centered emblem.".into(),
            design: arc,
        },
        DesignTemplate {
            name: "Name badge".into(),
            description: "Safe-area centered variable name for CSV personalization.".into(),
            design: name,
        },
    ]
}
pub fn save_template(path: &Path, template: &DesignTemplate) -> Result<(), String> {
    let name = template.name.trim();
    if name.is_empty() {
        return Err("Template name cannot be empty".into());
    }
    validate_design_fonts(&template.design)?;
    let bytes = template.design.to_project_json_with_preview(None)?;
    let mut canonical = Design::from_project_json(&bytes, None)?;
    canonical.normalize_after_load();
    validate_design_fonts(&canonical)?;
    let mut stored = template.clone();
    stored.name = name.chars().take(120).collect();
    stored.description = stored.description.chars().take(500).collect();
    stored.design = canonical;
    write_json(path, &stored)
}
pub fn load_template(path: &Path) -> Result<DesignTemplate, String> {
    let value: serde_json::Value = read_json(path)?;
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Template name is missing")?;
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let design_value = value.get("design").ok_or("Template design is missing")?;
    let bytes = serde_json::to_vec(design_value).map_err(|e| e.to_string())?;
    let mut design = Design::from_project_json(&bytes, Some(path))?;
    design.normalize_after_load();
    validate_design_fonts(&design)?;
    if name.trim().is_empty() {
        return Err("Template name cannot be empty".into());
    }
    Ok(DesignTemplate {
        name: name.chars().take(120).collect(),
        description: description.chars().take(500).collect(),
        design,
    })
}
pub fn save_palette(path: &Path, palette: &SavedPalette) -> Result<(), String> {
    let mut normalized = normalize_palette(palette.clone())?;
    normalized.name = normalized.name.trim().chars().take(120).collect();
    write_json(path, &normalized)
}
pub fn load_palette(path: &Path) -> Result<SavedPalette, String> {
    normalize_palette(read_json(path)?)
}
pub fn save_style(path: &Path, style: &SavedStyle) -> Result<(), String> {
    let mut normalized = style.clone();
    normalized.name = normalized.name.trim().chars().take(120).collect();
    if normalized.name.is_empty() {
        return Err("Style name cannot be empty".into());
    }
    normalized.appearance.normalize();
    validate_style_font(normalized.font.as_ref())?;
    normalized.font_size = normalized
        .font_size
        .filter(|size| size.is_finite())
        .map(|size| size.clamp(0.025, 0.24));
    write_json(path, &normalized)
}
pub fn load_style(path: &Path) -> Result<SavedStyle, String> {
    let mut style: SavedStyle = read_json(path)?;
    style.name = style.name.trim().chars().take(120).collect();
    if style.name.is_empty() {
        return Err("Style name cannot be empty".into());
    }
    style.appearance.normalize();
    validate_style_font(style.font.as_ref())?;
    style.font_size = style
        .font_size
        .filter(|size| size.is_finite())
        .map(|size| size.clamp(0.025, 0.24));
    Ok(style)
}
fn normalize_palette(mut palette: SavedPalette) -> Result<SavedPalette, String> {
    palette.name = palette.name.trim().chars().take(120).collect();
    palette.colors.dedup();
    let mut unique = Vec::new();
    for color in palette.colors {
        if !unique.contains(&color) && unique.len() < 32 {
            unique.push(color);
        }
    }
    palette.colors = unique;
    if palette.name.is_empty() {
        return Err("Palette name cannot be empty".into());
    }
    if palette.colors.is_empty() {
        return Err("Palette must include at least one color".into());
    }
    Ok(palette)
}
fn validate_design_fonts(design: &Design) -> Result<(), String> {
    for element in &design.elements {
        if let ElementKind::Text { font, .. } = &element.kind {
            if let Some(path) = &font.path {
                if !path.is_file() {
                    return Err(format!(
                        "Font file for '{}' is unavailable: {}",
                        font.family,
                        path.display()
                    ));
                }
            } else if font.family != "Archivo Black" {
                return Err(format!(
                    "Font '{}' has no bundled or installed font file",
                    font.family
                ));
            }
        }
    }
    Ok(())
}
fn validate_style_font(font: Option<&TextFont>) -> Result<(), String> {
    if let Some(font) = font {
        if let Some(path) = &font.path {
            if !path.is_file() {
                return Err(format!(
                    "Font file for '{}' is unavailable: {}",
                    font.family,
                    path.display()
                ));
            }
        } else if font.family != "Archivo Black" {
            return Err(format!(
                "Font '{}' has no bundled or installed font file",
                font.family
            ));
        }
    }
    Ok(())
}
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("saved-design");
    let (tmp, mut file) = loop {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), id));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Could not create temporary file beside {}: {error}",
                    path.display()
                ));
            }
        }
    };
    if let Err(error) = file.write_all(&bytes) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Could not write {}: {error}", tmp.display()));
    }
    drop(file);
    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Could not save {}: {error}", path.display()));
    }
    Ok(())
}
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid saved design data: {e}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtins_roundtrip_through_serialized_design() {
        for t in builtin_templates() {
            let bytes = t.design.to_project_json().unwrap();
            let reopened = Design::from_project_json(&bytes, None).unwrap();
            assert_eq!(reopened, t.design);
        }
    }
    #[test]
    fn palette_and_template_file_roundtrip() {
        let path = std::env::temp_dir().join(format!(
            "button-studio-template-{}.json",
            std::process::id()
        ));
        let p = SavedPalette {
            name: "Brand".into(),
            colors: vec![DesignColor::WHITE],
        };
        save_palette(&path, &p).unwrap();
        assert_eq!(load_palette(&path).unwrap(), p);
        fs::remove_file(&path).unwrap();
        let template_path = std::env::temp_dir().join(format!(
            "button-studio-template-design-{}.json",
            std::process::id()
        ));
        let template = builtin_templates().remove(0);
        save_template(&template_path, &template).unwrap();
        assert_eq!(load_template(&template_path).unwrap(), template);
        fs::remove_file(template_path).unwrap();
        let style_path =
            std::env::temp_dir().join(format!("button-studio-style-{}.json", std::process::id()));
        let style = SavedStyle {
            name: "Brand heading".into(),
            color: DesignColor::INK,
            font: Some(TextFont::default()),
            font_size: Some(0.12),
            opacity: 196,
            appearance: Appearance {
                outline: Some(DesignColor::WHITE),
                outline_width: 0.2,
                ..Appearance::default()
            },
        };
        save_style(&style_path, &style).unwrap();
        assert_eq!(load_style(&style_path).unwrap(), style);
        fs::remove_file(style_path).unwrap();
    }
}
