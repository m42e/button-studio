use crate::model::{Design, ElementKind, ShapeKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvgExport {
    pub svg: String,
    pub warnings: Vec<String>,
}

/// Export artwork as SVG. Unsupported effects trigger a high-resolution embedded PNG fallback.
pub fn export_svg(design: &Design) -> Result<SvgExport, String> {
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
    visible_design.validate_assets()?;
    let reasons = fallback_reasons(design);
    let unsupported = !reasons.is_empty();
    if unsupported {
        let raster = crate::render::render_trimmed_badge(design, 2400, 1)?;
        let mut bytes = std::io::Cursor::new(Vec::new());
        raster
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| format!("Could not encode SVG fallback: {e}"))?;
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes.into_inner());
        return Ok(SvgExport {
            svg: format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 2400 2400\"><image width=\"2400\" height=\"2400\" href=\"data:image/png;base64,{data}\"/></svg>",
                design.artwork_diameter_mm(),
                design.artwork_diameter_mm()
            ),
            warnings: {
                let mut warnings = reasons;
                warnings.push("Artwork was rasterized at 2400 × 2400 px to preserve those features. SVG text and shapes are not editable.".into());
                warnings
            },
        });
    }
    let d = design.artwork_diameter_mm();
    let center = d / 2.0;
    let radius = d / 2.0;
    let mut body = String::new();
    let mut warnings = Vec::new();
    if design.background_style == crate::model::BackgroundStyle::Solid {
        body.push_str(&format!(
            "<circle cx=\"{center}\" cy=\"{center}\" r=\"{radius}\" fill=\"{}\" fill-opacity=\"{}\"/>",
            color(design.background), design.background.alpha as f32 / 255.0
        ));
    }
    for el in design.elements.iter().filter(|e| e.visible) {
        let x = center + el.position[0] * radius;
        let y = center + el.position[1] * radius;
        let opacity = el.opacity as f32 * el.color.alpha as f32 / (255.0 * 255.0);
        let transform = format!(
            "translate({x} {y}) rotate({}) scale({} {})",
            el.rotation,
            if el.flip_x { -1 } else { 1 },
            if el.flip_y { -1 } else { 1 }
        );
        let fill = color(el.color);
        match &el.kind {
            ElementKind::Text {
                text,
                font_size,
                font,
                ..
            } => {
                warnings.push(format!("SVG text uses the '{}' font family; install it in the viewing application to avoid substitution", font.family));
                let size = font_size * design.artwork_diameter_mm();
                body.push_str(&format!("<text transform=\"{transform}\" text-anchor=\"middle\" dominant-baseline=\"central\" font-family=\"{}\" font-size=\"{size}\" fill=\"{fill}\" fill-opacity=\"{opacity}\">{}</text>", xml(&font.family), xml(text)));
            }
            ElementKind::Shape {
                shape,
                size,
                width,
                height,
            } => {
                let default_height = if *shape == ShapeKind::Rectangle {
                    *size * 2.0 / 3.0
                } else {
                    *size
                };
                let w = width.unwrap_or(*size) * design.artwork_diameter_mm();
                let h = height.unwrap_or(default_height) * design.artwork_diameter_mm();
                let geom = match shape {
                    ShapeKind::Circle => format!(
                        "<ellipse cx=\"0\" cy=\"0\" rx=\"{}\" ry=\"{}\" fill=\"{fill}\" fill-opacity=\"{opacity}\"/>",
                        w / 2.0,
                        h / 2.0
                    ),
                    ShapeKind::Rectangle => format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{h}\" fill=\"{fill}\" fill-opacity=\"{opacity}\"/>",
                        -w / 2.0,
                        -h / 2.0
                    ),
                    _ => format!(
                        "<path d=\"{}\" transform=\"scale({} {})\" fill=\"{fill}\" fill-opacity=\"{opacity}\"/>",
                        shape_path(*shape),
                        w / 2.0,
                        h / 2.0
                    ),
                };
                body.push_str(&format!("<g transform=\"{transform}\">{geom}</g>"));
            }
            ElementKind::Image {
                asset_id,
                width,
                aspect_ratio,
                ..
            } => {
                let asset = design
                    .assets
                    .get(asset_id)
                    .ok_or_else(|| format!("Missing image asset {asset_id}"))?;
                let mime = match asset.format.as_str() {
                    "jpg" | "jpeg" => "image/jpeg",
                    "gif" => "image/gif",
                    "webp" => "image/webp",
                    _ => "image/png",
                };
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&asset.data);
                let w = width * design.artwork_diameter_mm();
                let h = w / aspect_ratio.max(0.01);
                body.push_str(&format!("<image transform=\"{transform}\" x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{h}\" opacity=\"{opacity}\" href=\"data:{mime};base64,{encoded}\"/>",-w/2.0,-h/2.0));
            }
            ElementKind::QrCode { .. } => unreachable!(),
        }
    }
    Ok(SvgExport {
        svg: format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{d}mm\" height=\"{d}mm\" viewBox=\"0 0 {d} {d}\"><defs><clipPath id=\"artwork-circle\"><circle cx=\"{center}\" cy=\"{center}\" r=\"{radius}\"/></clipPath></defs><g clip-path=\"url(#artwork-circle)\">{body}</g></svg>",
        ),
        warnings,
    })
}
/// Lists fidelity limitations that make this artwork use the raster SVG fallback.
pub fn fallback_reasons(design: &Design) -> Vec<String> {
    let mut reasons = Vec::new();
    if design.background_style != crate::model::BackgroundStyle::Solid
        && design.background_style != crate::model::BackgroundStyle::None
    {
        reasons.push(format!(
            "Background style '{}' is flattened",
            design.background_style.label()
        ));
    }
    if design.background_image.is_some() {
        reasons.push("Background photo is flattened".into());
    }
    for element in design.elements.iter().filter(|element| element.visible) {
        match &element.kind {
            ElementKind::QrCode { .. } => {
                reasons.push(format!("QR element '{}' is flattened", element.name()))
            }
            ElementKind::Text {
                text,
                curvature,
                font,
                ..
            } => {
                if curvature.abs() > f32::EPSILON {
                    reasons.push(format!("Curved text '{}' is flattened", element.name()));
                }
                if text.contains('\n') {
                    reasons.push(format!("Multiline text '{}' is flattened", element.name()));
                }
                if font.path.is_some()
                    || font.family != "Archivo Black"
                    || font.face != "ArchivoBlack-Regular"
                    || font.index != 0
                {
                    reasons.push(format!(
                        "Text '{}' uses a font face that cannot be guaranteed in other SVG viewers",
                        element.name()
                    ));
                }
            }
            ElementKind::Image {
                bleed_to_square: true,
                ..
            } => reasons.push(format!(
                "Edge-bleed image '{}' is flattened",
                element.name()
            )),
            ElementKind::Shape { shape, .. }
                if !matches!(shape, ShapeKind::Circle | ShapeKind::Rectangle) =>
            {
                reasons.push(format!(
                    "{} shape '{}' is flattened to preserve its exact geometry",
                    shape.label(),
                    element.name()
                ))
            }
            _ => {}
        }
        if element.appearance != crate::appearance::Appearance::default() {
            reasons.push(format!(
                "Appearance effects on '{}' are flattened",
                element.name()
            ));
        }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}
fn color(c: crate::model::DesignColor) -> String {
    format!("#{:02x}{:02x}{:02x}", c.red, c.green, c.blue)
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn shape_path(shape: ShapeKind) -> &'static str {
    match shape {
        ShapeKind::Star => {
            "M0 -1 L0.22 -0.3 L0.95 -0.3 L0.36 0.15 L0.59 0.85 L0 0.42 L-0.59 0.85 L-0.36 0.15 L-0.95 -0.3 L-0.22 -0.3 Z"
        }
        ShapeKind::Diamond => "M0 -1 L1 0 L0 1 L-1 0 Z",
        ShapeKind::Bolt => "M0.1 -1 L-0.8 0.1 L-0.1 0.1 L-0.3 1 L0.8 -0.2 L0.1 -0.2 Z",
        ShapeKind::Heart => {
            "M0 0.85 C-1 0.2 -1 -0.7 -0.4 -0.8 C-0.1 -0.85 0 -0.6 0 -0.6 C0 -0.6 0.2 -0.9 0.55 -0.8 C1 -0.65 1 0.1 0 0.85 Z"
        }
        ShapeKind::Circle | ShapeKind::Rectangle => "",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_export_keeps_text_and_curved_text_uses_raster_fallback() {
        let mut d = Design::default();
        d.background_style = crate::model::BackgroundStyle::Solid;
        d.elements
            .retain(|e| matches!(e.kind, ElementKind::Text { .. }));
        let out = export_svg(&d).unwrap();
        assert!(out.svg.contains("<text"));
        assert!(out.svg.contains("clipPath"));
        assert!(
            out.svg
                .contains(&format!("width=\"{}mm\"", d.artwork_diameter_mm()))
        );
        assert!(!out.svg.contains("stroke-dasharray"));
        d.elements[0].kind = ElementKind::Text {
            text: "Arc".into(),
            font_size: 0.1,
            font: Default::default(),
            curvature: 0.3,
        };
        let out = export_svg(&d).unwrap();
        assert!(out.svg.contains("data:image/png;base64"));
        assert!(!out.warnings.is_empty());
    }
}
