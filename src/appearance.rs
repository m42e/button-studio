use crate::model::DesignColor;
use serde::{Deserialize, Serialize};

/// Per-element visual refinements. Missing values retain the legacy element style.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub fill: Option<DesignColor>,
    pub outline: Option<DesignColor>,
    pub outline_width: f32,
    pub corner_radius: f32,
    pub text_alignment: TextAlignment,
    /// Multiplier applied to the font's natural line height (1.0 is default).
    pub line_spacing: f32,
    /// Extra spacing in pixels at the 100px base font size.
    pub letter_spacing: f32,
    pub image_fit: ImageFit,
    /// Optional image frame aspect ratio (width / height), independent from its source image.
    pub image_frame_aspect: Option<f32>,
    /// Image crop offset, in normalized frame coordinates, clamped to [-1, 1].
    pub crop_pan: [f32; 2],
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            fill: None,
            outline: None,
            outline_width: 0.0,
            corner_radius: 0.0,
            text_alignment: TextAlignment::Center,
            line_spacing: 1.0,
            letter_spacing: 0.0,
            image_fit: ImageFit::Contain,
            image_frame_aspect: None,
            crop_pan: [0.0, 0.0],
        }
    }
}

impl Appearance {
    pub fn normalize(&mut self) {
        self.outline_width = if self.outline_width.is_finite() {
            self.outline_width.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.corner_radius = if self.corner_radius.is_finite() {
            self.corner_radius.clamp(0.0, 0.5)
        } else {
            0.0
        };
        self.line_spacing = if self.line_spacing.is_finite() {
            self.line_spacing.clamp(0.25, 4.0)
        } else {
            1.0
        };
        self.letter_spacing = if self.letter_spacing.is_finite() {
            self.letter_spacing.clamp(-50.0, 100.0)
        } else {
            0.0
        };
        for offset in &mut self.crop_pan {
            *offset = if offset.is_finite() {
                offset.clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }
        if let Some(aspect) = &mut self.image_frame_aspect {
            if aspect.is_finite() && *aspect > 0.0 {
                *aspect = aspect.clamp(0.01, 100.0);
            } else {
                self.image_frame_aspect = None;
            }
        }
    }
}

/// Appearance controls shared by the selected-element panel. `artwork_mm` is
/// the full artwork diameter including bleed and is used for physical-width labels.
pub fn controls(
    ui: &mut eframe::egui::Ui,
    element: &mut crate::model::Element,
    artwork_mm: f32,
) -> bool {
    use eframe::egui::{ComboBox, Slider};
    let mut changed = false;
    let body = eframe::egui::CollapsingHeader::new("Appearance")
        .default_open(false)
        .show(ui, |ui| {
            let appearance = &mut element.appearance;
            let mut outlined = appearance.outline.is_some();
            if ui.checkbox(&mut outlined, "Outline").changed() {
                appearance.outline = outlined.then_some(DesignColor::INK);
                if outlined && appearance.outline_width <= 0.0 {
                    appearance.outline_width = 0.003;
                } else if !outlined {
                    appearance.outline_width = 0.0;
                }
                changed = true;
            }
            if outlined {
                if let Some(color) = &mut appearance.outline {
                    changed |= crate::color::edit_design_color(ui, color);
                }
                let mut outline_mm = appearance.outline_width * artwork_mm;
                if ui
                    .add(
                        Slider::new(&mut outline_mm, 0.0..=5.0)
                            .suffix(" mm")
                            .text("Outline width"),
                    )
                    .changed()
                {
                    appearance.outline_width = (outline_mm / artwork_mm.max(1.0)).clamp(0.0, 0.25);
                    changed = true;
                }
            }
            match &element.kind {
                crate::model::ElementKind::Shape {
                    shape: crate::model::ShapeKind::Rectangle,
                    ..
                } => {
                    let mut corner_mm = appearance.corner_radius * artwork_mm;
                    if ui
                        .add(
                            Slider::new(&mut corner_mm, 0.0..=artwork_mm.min(30.0))
                                .suffix(" mm")
                                .text("Corner radius"),
                        )
                        .changed()
                    {
                        appearance.corner_radius =
                            (corner_mm / artwork_mm.max(1.0)).clamp(0.0, 0.5);
                        changed = true;
                    }
                }
                crate::model::ElementKind::Text { curvature, .. }
                    if curvature.abs() <= f32::EPSILON =>
                {
                    ComboBox::from_id_salt(("text-align", element.id))
                        .selected_text(format!("{:?}", appearance.text_alignment))
                        .show_ui(ui, |ui| {
                            for alignment in [
                                TextAlignment::Start,
                                TextAlignment::Center,
                                TextAlignment::End,
                            ] {
                                changed |= ui
                                    .selectable_value(
                                        &mut appearance.text_alignment,
                                        alignment,
                                        format!("{alignment:?}"),
                                    )
                                    .changed();
                            }
                        });
                    changed |= ui
                        .add(
                            Slider::new(&mut appearance.line_spacing, 0.5..=2.0)
                                .text("Line spacing"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            Slider::new(&mut appearance.letter_spacing, -20.0..=50.0)
                                .text("Letter spacing"),
                        )
                        .changed();
                }
                crate::model::ElementKind::Image { .. } => {
                    ComboBox::from_id_salt(("image-fit", element.id))
                        .selected_text(format!("{:?}", appearance.image_fit))
                        .show_ui(ui, |ui| {
                            for fit in [ImageFit::Contain, ImageFit::Cover] {
                                changed |= ui
                                    .selectable_value(
                                        &mut appearance.image_fit,
                                        fit,
                                        format!("{fit:?}"),
                                    )
                                    .changed();
                            }
                        });
                    let mut custom_frame = appearance.image_frame_aspect.is_some();
                    if ui
                        .checkbox(&mut custom_frame, "Custom image frame")
                        .changed()
                    {
                        appearance.image_frame_aspect = custom_frame.then_some(1.0);
                        changed = true;
                    }
                    if let Some(aspect) = &mut appearance.image_frame_aspect {
                        changed |= ui
                            .add(Slider::new(aspect, 0.25..=4.0).text("Frame aspect"))
                            .changed();
                    }
                    if appearance.image_fit == ImageFit::Cover {
                        changed |= ui
                            .add(
                                Slider::new(&mut appearance.crop_pan[0], -1.0..=1.0)
                                    .text("Crop horizontal"),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                Slider::new(&mut appearance.crop_pan[1], -1.0..=1.0)
                                    .text("Crop vertical"),
                            )
                            .changed();
                        if ui.button("Reset crop").clicked() {
                            appearance.crop_pan = [0.0, 0.0];
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
            if artwork_mm <= 0.0 {
                ui.label("Physical sizing unavailable");
            }
        });
    let _ = body;
    changed
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextAlignment {
    Start,
    #[default]
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
}
