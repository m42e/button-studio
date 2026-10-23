use crate::model::{Design, ElementKind};
use eframe::egui::{self, Color32, Stroke, StrokeKind, pos2, vec2};
use qrcode::QrCode;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SheetPreviewSummary {
    pub buttons_per_page: usize,
    pub pages: Option<usize>,
}

/// Paint a true-scale-proportioned A4 print-sheet preview using the same layout as PDF export.
/// `button_count=None` shows a full page; a count shows only the first page of a series.
pub fn paint_sheet_preview(
    ui: &mut egui::Ui,
    design: &Design,
    button_count: Option<usize>,
) -> SheetPreviewSummary {
    let layout = crate::render::pdf_sheet_layout(design.artwork_diameter_mm());
    let shown = button_count.map_or(layout.len(), |count| count.min(layout.len()));
    let pages = button_count.map(|count| {
        if count == 0 || layout.is_empty() {
            0
        } else {
            count.div_ceil(layout.len())
        }
    });

    let max_width = ui.available_width().clamp(80.0, 420.0);
    let scale = max_width / crate::render::A4_WIDTH_MM;
    let page_size = vec2(
        crate::render::A4_WIDTH_MM * scale,
        crate::render::A4_HEIGHT_MM * scale,
    );
    let (rect, _) = ui.allocate_exact_size(page_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::WHITE);
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, Color32::from_gray(165)),
        StrokeKind::Inside,
    );

    let diameter = design.artwork_diameter_mm() * scale;
    let trim_diameter = diameter * design.trim_ratio();
    for [x_mm, y_mm] in layout.iter().take(shown) {
        let center = pos2(
            rect.left() + (x_mm + design.artwork_diameter_mm() / 2.0) * scale,
            rect.bottom() - (y_mm + design.artwork_diameter_mm() / 2.0) * scale,
        );
        painter.circle_filled(center, diameter / 2.0, Color32::from_rgb(250, 250, 250));
        painter.circle_stroke(
            center,
            diameter / 2.0,
            Stroke::new(0.8, Color32::from_gray(110)),
        );
        painter.circle_stroke(
            center,
            trim_diameter / 2.0,
            Stroke::new(0.7, Color32::from_rgb(212, 86, 78)),
        );
    }
    ui.label(format!(
        "A4 · 210 × 297 mm · {} buttons per page · print at 100%{}",
        layout.len(),
        pages
            .map(|n| format!(
                " · {n} page{} for this selection",
                if n == 1 { "" } else { "s" }
            ))
            .unwrap_or_default()
    ));
    ui.horizontal(|ui| {
        ui.colored_label(Color32::from_gray(110), "Outer circle: artwork with bleed");
        ui.colored_label(Color32::from_rgb(212, 86, 78), "Inner circle: trim");
    });
    SheetPreviewSummary {
        buttons_per_page: layout.len(),
        pages,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueSeverity {
    Warning,
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreflightIssue {
    pub severity: IssueSeverity,
    pub code: &'static str,
    pub element_id: Option<u64>,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowPreflight {
    pub row_number: usize,
    pub issues: Vec<PreflightIssue>,
}

pub fn preflight_design(design: &Design) -> Vec<PreflightIssue> {
    let mut out = Vec::new();
    let mut issue = |severity, code, id, message: String| {
        out.push(PreflightIssue {
            severity,
            code,
            element_id: id,
            message,
        })
    };
    for element in design.elements.iter().filter(|element| element.visible) {
        if let ElementKind::Image { asset_id, .. } = &element.kind
            && !design.assets.contains_key(asset_id)
        {
            issue(
                IssueSeverity::Error,
                "missing_asset",
                Some(element.id),
                format!("Image asset for '{}' is missing", element.name()),
            );
        }
    }
    let mut visible_design = design.clone();
    visible_design.elements.retain(|element| element.visible);
    let referenced = visible_design
        .elements
        .iter()
        .filter_map(|element| match &element.kind {
            ElementKind::Image { asset_id, .. } => Some(asset_id.clone()),
            _ => None,
        })
        .chain(
            visible_design
                .background_image
                .iter()
                .map(|image| image.asset_id.clone()),
        )
        .collect::<std::collections::HashSet<_>>();
    visible_design
        .assets
        .retain(|id, _| referenced.contains(id));
    if let Err(error) = visible_design.validate_assets()
        && !error.starts_with("Project references missing image asset")
    {
        issue(IssueSeverity::Error, "assets", None, error);
    }
    if design
        .background_image
        .as_ref()
        .is_some_and(|i| !design.assets.contains_key(&i.asset_id))
    {
        issue(
            IssueSeverity::Error,
            "missing_asset",
            None,
            "Background image asset is missing".into(),
        );
    }
    let artwork_diameter = design.artwork_diameter_mm();
    if let Some(background) = &design.background_image
        && let Some(asset) = design.assets.get(&background.asset_id)
        && let Ok(image) = image::load_from_memory(&asset.data)
    {
        let pixels = image.width().min(image.height()) as f32;
        let dpi = pixels * 25.4 / artwork_diameter.max(0.01);
        if dpi < 300.0 {
            issue(
                IssueSeverity::Warning,
                "low_dpi_background",
                None,
                format!(
                    "Background image is about {dpi:.0} DPI at the artwork size (300 DPI recommended)"
                ),
            );
        }
    }
    let trim = design.trim_radius(1.0);
    let safe_radius = ((design.diameter_mm as f32 / 2.0 - 2.0).max(0.0)) / (artwork_diameter / 2.0);
    for el in &design.elements {
        if !el.visible {
            issue(
                IssueSeverity::Warning,
                "hidden",
                Some(el.id),
                format!("'{}' is hidden and will not appear in export", el.name()),
            );
            continue;
        }
        let bounds = crate::render::normalized_element_bounds(el);
        let extent = [
            bounds.left_top(),
            bounds.right_top(),
            bounds.left_bottom(),
            bounds.right_bottom(),
        ]
        .into_iter()
        .map(|point| (point.x * point.x + point.y * point.y).sqrt())
        .fold(0.0_f32, f32::max);
        if extent > 1.0 {
            issue(
                IssueSeverity::Warning,
                "outside_artwork",
                Some(el.id),
                format!(
                    "'{}' may extend outside the artwork boundary and be clipped",
                    el.name()
                ),
            );
        } else if extent > trim {
            issue(
                IssueSeverity::Warning,
                "outside_trim",
                Some(el.id),
                format!(
                    "'{}' extends beyond the trim edge and may be cut",
                    el.name()
                ),
            );
        } else if extent > safe_radius {
            issue(
                IssueSeverity::Warning,
                "outside_safe_area",
                Some(el.id),
                format!("'{}' extends outside the recommended safe area", el.name()),
            );
        }
        match &el.kind {
            ElementKind::Image {
                asset_id,
                width,
                aspect_ratio,
                bleed_to_square,
            } if let Some(asset) = design.assets.get(asset_id)
                && let Ok(img) = image::load_from_memory(&asset.data) =>
            {
                let frame_width = design.artwork_diameter_mm() * width;
                let frame_aspect = el
                    .appearance
                    .image_frame_aspect
                    .unwrap_or_else(|| {
                        if *bleed_to_square
                            || el.appearance.image_fit == crate::appearance::ImageFit::Cover
                        {
                            1.0
                        } else {
                            *aspect_ratio
                        }
                    })
                    .max(0.01);
                let frame_height = frame_width / frame_aspect;
                let (physical_width, physical_height) = match el.appearance.image_fit {
                    crate::appearance::ImageFit::Cover => {
                        if *aspect_ratio > frame_aspect {
                            (frame_height * *aspect_ratio, frame_height)
                        } else {
                            (frame_width, frame_width / (*aspect_ratio).max(0.01))
                        }
                    }
                    crate::appearance::ImageFit::Contain => {
                        if *bleed_to_square {
                            if *aspect_ratio >= 1.0 {
                                (frame_width, frame_width / *aspect_ratio)
                            } else {
                                (frame_width * *aspect_ratio, frame_width)
                            }
                        } else if el.appearance.image_frame_aspect.is_some() {
                            let scale = (frame_width / img.width().max(1) as f32)
                                .min(frame_height / img.height().max(1) as f32);
                            (img.width() as f32 * scale, img.height() as f32 * scale)
                        } else {
                            (frame_width, frame_width / aspect_ratio.max(0.01))
                        }
                    }
                };
                let dpi = (img.width() as f32 * 25.4 / physical_width.max(0.01))
                    .min(img.height() as f32 * 25.4 / physical_height.max(0.01));
                if dpi < 300.0 {
                    issue(
                        IssueSeverity::Warning,
                        "low_dpi",
                        Some(el.id),
                        format!(
                            "'{}' is about {:.0} DPI at this placement (300 DPI recommended)",
                            el.name(),
                            dpi
                        ),
                    );
                }
            }
            ElementKind::Text { text, font, .. } => {
                if (font.path.as_ref().is_some_and(|p| !p.exists()))
                    || (font.path.is_none() && font.family != "Archivo Black")
                {
                    issue(
                        IssueSeverity::Error,
                        "missing_font",
                        Some(el.id),
                        format!("Font file for '{}' is unavailable", font.family),
                    );
                }
                for var in variables(text) {
                    issue(
                        IssueSeverity::Warning,
                        "unresolved_variable",
                        Some(el.id),
                        format!("'{}' contains unresolved variable {{{{{var}}}}}", el.name()),
                    );
                }
            }
            ElementKind::QrCode { content, .. } => {
                for var in variables(content) {
                    issue(
                        IssueSeverity::Warning,
                        "unresolved_variable",
                        Some(el.id),
                        format!("QR code contains unresolved variable {{{{{var}}}}}"),
                    );
                }
                if QrCode::new(content.as_bytes()).is_err() {
                    issue(
                        IssueSeverity::Error,
                        "invalid_qr",
                        Some(el.id),
                        format!("QR content for '{}' cannot be encoded", el.name()),
                    );
                }
            }
            _ => {}
        }
    }
    out
}
fn variables(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("}}") {
            let key = rest[..end].trim();
            if !key.is_empty() && !result.iter().any(|existing| existing == key) {
                result.push(key.to_owned())
            };
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }
    result
}
pub fn preflight_series(
    design: &Design,
    rows: &[HashMap<String, String>],
    first_row_number: usize,
) -> Vec<RowPreflight> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let personalized = design.personalized(row);
            let mut issues = preflight_design(&personalized);
            for problem in &mut issues {
                if problem.code == "unresolved_variable" {
                    problem.severity = IssueSeverity::Error;
                }
            }
            RowPreflight {
                row_number: first_row_number + i,
                issues,
            }
        })
        .collect()
}
/// Selects inclusive, one-based source rows and repeats each row `copies` times.
pub fn select_rows<T: Clone>(
    rows: &[T],
    start_1based: usize,
    end_1based: usize,
    copies: usize,
) -> Result<Vec<T>, String> {
    if copies == 0 {
        return Err("Copies must be at least one".into());
    }
    if start_1based == 0 || end_1based < start_1based || end_1based > rows.len() {
        return Err(format!("Select a row range from 1 to {}", rows.len()));
    }
    let selected_count = end_1based - start_1based + 1;
    let total = selected_count
        .checked_mul(copies)
        .ok_or("Selected copy count is too large")?;
    if total > 10_000 {
        return Err("A single batch export is limited to 10,000 buttons".into());
    }
    let mut out = Vec::with_capacity(total);
    for row in &rows[start_1based - 1..end_1based] {
        for _ in 0..copies {
            out.push(row.clone());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Design, ElementKind};
    use std::collections::HashMap;

    #[test]
    fn hidden_objects_only_report_hidden_notice_and_do_not_block_preflight() {
        let mut design = Design::default();
        design.elements[0].visible = false;
        design.elements[0].kind = ElementKind::Image {
            asset_id: "missing-hidden-asset".into(),
            width: 0.5,
            aspect_ratio: 1.0,
            bleed_to_square: false,
        };
        let issues = preflight_design(&design);
        let element_issues = issues
            .iter()
            .filter(|i| i.element_id == Some(design.elements[0].id))
            .collect::<Vec<_>>();
        assert_eq!(element_issues.len(), 1);
        assert_eq!(element_issues[0].code, "hidden");
        assert_eq!(element_issues[0].severity, IssueSeverity::Warning);
    }

    #[test]
    fn measured_bounds_include_rotated_shape_and_trim_safe_radius_scales_with_size() {
        let mut design = Design::default();
        design.elements.clear();
        let id = design.add_shape(crate::model::ShapeKind::Rectangle);
        let element = design.element_mut(id).unwrap();
        element.position = [0.3, 0.0];
        element.rotation = 45.0;
        let bounds = crate::render::normalized_element_bounds(element);
        assert!(bounds.right() > 0.3 + 0.1);
        let mut small = design.clone();
        small.diameter_mm = 25;
        let mut large = design.clone();
        large.diameter_mm = 75;
        let small_issues = preflight_design(&small);
        let large_issues = preflight_design(&large);
        assert!(
            small_issues.iter().any(|i| i.code == "outside_safe_area"),
            "issues: {small_issues:?}"
        );
        assert!(!large_issues.iter().any(|i| i.code == "outside_safe_area"));
    }

    #[test]
    fn positioning_findings_are_advisory_and_unresolved_series_values_are_errors() {
        let mut design = Design::default();
        design.elements[1].position = [1.0, 0.0];
        if let ElementKind::Text { text, .. } = &mut design.elements[1].kind {
            *text = "{{name}}".into();
        }
        let ordinary = preflight_design(&design);
        assert!(
            ordinary
                .iter()
                .any(|i| i.code == "outside_artwork" && i.severity == IssueSeverity::Warning)
        );
        let rows = [HashMap::new()];
        let row = &preflight_series(&design, &rows, 2)[0];
        assert!(
            row.issues
                .iter()
                .any(|i| i.code == "unresolved_variable" && i.severity == IssueSeverity::Error)
        );
    }

    #[test]
    fn text_bounds_use_renderer_metrics_not_character_count_estimates() {
        let design = Design::default();
        let bounds = crate::render::normalized_element_bounds(&design.elements[1]);
        let width_mm = bounds.width() * design.artwork_diameter_mm() / 2.0;
        assert!(
            width_mm > 50.0 && width_mm < 65.0,
            "measured text width: {width_mm}"
        );
    }

    #[test]
    fn series_selection_duplicates_requested_rows_and_reports_source_numbers() {
        let rows = vec![
            HashMap::from([("name".into(), "Ada".into())]),
            HashMap::from([("name".into(), "Lin".into())]),
        ];
        let chosen = select_rows(&rows, 2, 2, 3).unwrap();
        assert_eq!(chosen.len(), 3);
        assert!(chosen.iter().all(|r| r["name"] == "Lin"));
        assert_eq!(
            preflight_series(&Design::default(), &rows, 4)
                .iter()
                .map(|r| r.row_number)
                .collect::<Vec<_>>(),
            [4, 5]
        );
        assert!(select_rows(&rows, 0, 2, 1).is_err());
        assert!(select_rows(&rows, 1, 2, 10_000).is_err());
    }

    #[test]
    fn warns_for_low_effective_dpi_and_invalid_qr() {
        let mut design = Design::default();
        design.elements.clear();
        let mut data = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4, 4)
            .write_to(&mut data, image::ImageFormat::Png)
            .unwrap();
        let (asset, ratio) = design.import_asset(data.into_inner(), None).unwrap();
        let image_id = design.add_image(asset, ratio);
        if let ElementKind::Image { width, .. } = &mut design.element_mut(image_id).unwrap().kind {
            *width = 0.9;
        }
        design.set_background_image(
            design
                .elements
                .iter()
                .find_map(|e| {
                    if let ElementKind::Image { asset_id, .. } = &e.kind {
                        Some(asset_id.clone())
                    } else {
                        None
                    }
                })
                .unwrap(),
            ratio,
        );
        let qr_id = design.add_qr_code("x".repeat(5000));
        if let ElementKind::QrCode { content, .. } = &mut design.element_mut(qr_id).unwrap().kind {
            *content = "x".repeat(5000);
        }
        let issues = preflight_design(&design);
        assert!(issues.iter().any(|i| i.code == "low_dpi"));
        assert!(issues.iter().any(|i| i.code == "low_dpi_background"));
        assert!(issues.iter().any(|i| i.code == "invalid_qr"));
    }

    #[test]
    fn cover_dpi_uses_the_actual_non_square_frame() {
        let mut design = Design::default();
        design.elements.clear();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(240, 240)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let (asset, ratio) = design.import_asset(bytes.into_inner(), None).unwrap();
        let id = design.add_image(asset, ratio);
        let image = design.element_mut(id).unwrap();
        if let ElementKind::Image { width, .. } = &mut image.kind {
            *width = 0.6;
        }
        image.appearance.image_fit = crate::appearance::ImageFit::Cover;
        image.appearance.image_frame_aspect = Some(2.0);
        let issues = preflight_design(&design);
        assert!(issues.iter().any(|issue| issue.code == "low_dpi"));
    }
}
