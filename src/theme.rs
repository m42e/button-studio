use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};
use iconflow::{Pack, Size, Style as IconStyle, fonts, try_icon};
use std::sync::Arc;

pub const INK: Color32 = Color32::from_rgb(29, 31, 36);
pub const PAPER: Color32 = Color32::from_rgb(244, 243, 238);
pub const SURFACE: Color32 = Color32::from_rgb(255, 255, 252);
pub const ACCENT: Color32 = Color32::from_rgb(225, 75, 52);
pub const MINT: Color32 = Color32::from_rgb(37, 128, 113);
pub const MUTED: Color32 = Color32::from_rgb(112, 112, 108);
pub const DISPLAY_FAMILY: &str = "Archivo Black";

const DISPLAY_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/ArchivoBlack-Regular.ttf");

pub fn install(context: &egui::Context) {
    context.set_fonts(base_font_definitions());

    let mut style = egui::Style {
        visuals: egui::Visuals::light(),
        ..Default::default()
    };
    style.visuals.panel_fill = PAPER;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = Color32::from_rgb(235, 234, 229);
    style.visuals.faint_bg_color = Color32::from_rgb(248, 247, 243);
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(239, 238, 233);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, INK);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(229, 227, 221);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, INK);
    style.visuals.widgets.active.bg_fill = ACCENT;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    style.spacing.item_spacing = egui::vec2(9.0, 9.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size.y = 36.0;
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(22.0, FontFamily::Name(Arc::from(DISPLAY_FAMILY))),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    context.set_global_style(style);
}

pub fn icon(name: &str) -> String {
    try_icon(Pack::Lucide, name, IconStyle::Regular, Size::Regular)
        .ok()
        .and_then(|icon| char::from_u32(icon.codepoint))
        .unwrap_or('•')
        .to_string()
}

pub fn icon_text(name: &str, label: &str) -> String {
    format!("{}  {label}", icon(name))
}

pub fn display_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(Arc::from(DISPLAY_FAMILY)))
}

pub fn base_font_definitions() -> FontDefinitions {
    let mut definitions = FontDefinitions::default();
    let fallback_fonts = definitions.font_data.keys().cloned().collect::<Vec<_>>();

    definitions.font_data.insert(
        DISPLAY_FAMILY.to_owned(),
        Arc::new(FontData::from_static(DISPLAY_FONT_BYTES)),
    );
    let display_family = definitions
        .families
        .entry(FontFamily::Name(Arc::from(DISPLAY_FAMILY)))
        .or_default();
    display_family.push(DISPLAY_FAMILY.to_owned());
    display_family.extend(fallback_fonts.iter().cloned());

    for font in fonts() {
        definitions.font_data.insert(
            font.family.to_owned(),
            Arc::new(FontData::from_static(font.bytes)),
        );
        let family = definitions
            .families
            .entry(FontFamily::Name(Arc::from(font.family)))
            .or_default();
        family.push(font.family.to_owned());
        family.extend(fallback_fonts.iter().cloned());

        if let Some(proportional) = definitions.families.get_mut(&FontFamily::Proportional) {
            proportional.insert(0, font.family.to_owned());
        }
    }

    definitions
}
