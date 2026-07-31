use crate::model::{
    BackgroundStyle, Design, DesignColor, Element, ElementKind, ShapeKind, TextFont,
};
use eframe::egui::{
    self, Align2, Color32, ColorImage, FontId, Pos2, Rect, Shape, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Vec2, pos2, vec2,
};
use eframe::epaint::{Mesh, Vertex};
use image::{Rgba, RgbaImage, imageops};
use imageproc::{
    drawing::{draw_filled_circle_mut, draw_line_segment_mut, draw_polygon_mut},
    geometric_transformations::{Border, Interpolation, rotate_about_center_no_crop},
    point::Point,
};
use printpdf::{
    Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Pt, RawImage, RawImageData, RawImageFormat,
    XObjectTransform,
};
use qrcode::{Color, QrCode};
use std::{cell::RefCell, collections::HashMap, fs, io::Cursor, path::Path};

const EXPORT_DPI: f32 = 300.0;
const EXPORT_SUPERSAMPLE: u32 = 2;
const A4_WIDTH_MM: f32 = 210.0;
const A4_HEIGHT_MM: f32 = 297.0;
const PDF_MARGIN_MM: f32 = 5.0;
const PDF_BUTTON_GAP_MM: f32 = 2.0;

pub type TextureMap = HashMap<String, TextureHandle>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfExportSummary {
    pub buttons: usize,
    pub pages: usize,
}

pub fn load_texture(
    context: &egui::Context,
    asset_id: &str,
    data: &[u8],
) -> Result<TextureHandle, String> {
    let decoded = image::load_from_memory(data)
        .map_err(|error| format!("Could not decode embedded image {asset_id}: {error}"))?
        .to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
    Ok(context.load_texture(format!("artwork-{asset_id}"), image, TextureOptions::LINEAR))
}

pub fn paint_design(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    design: &Design,
    textures: &TextureMap,
    selected: Option<u64>,
) {
    paint_background(painter, center, radius, design, textures);
    for element in design.elements.iter().filter(|element| element.visible) {
        paint_element(painter, center, radius, element, textures);
    }

    if let Some(element) = selected.and_then(|id| design.element(id)) {
        let bounds = selection_bounds(painter, center, radius, element);
        painter.rect_stroke(
            bounds,
            3.0,
            Stroke::new(1.5, Color32::WHITE),
            StrokeKind::Outside,
        );
        painter.rect_stroke(
            bounds.expand(2.0),
            4.0,
            Stroke::new(1.0, Color32::from_black_alpha(120)),
            StrokeKind::Outside,
        );
        for handle in selection_handles(bounds, painter.clip_rect()) {
            painter.circle_filled(handle, 4.5, Color32::WHITE);
            painter.circle_stroke(handle, 4.5, Stroke::new(1.0, Color32::from_gray(55)));
        }
    }
}

pub fn selection_handles(bounds: Rect, canvas_rect: Rect) -> [Pos2; 4] {
    let handle_rect = canvas_rect.shrink(8.0);
    [
        bounds.left_top(),
        bounds.right_top(),
        bounds.left_bottom(),
        bounds.right_bottom(),
    ]
    .map(|handle| {
        Pos2::new(
            handle.x.clamp(handle_rect.left(), handle_rect.right()),
            handle.y.clamp(handle_rect.top(), handle_rect.bottom()),
        )
    })
}

pub fn paint_preview(painter: &egui::Painter, rect: Rect, design: &Design, textures: &TextureMap) {
    let painter = painter.with_clip_rect(rect);
    let preview_background = Color32::from_rgb(232, 230, 224);
    painter.rect_filled(rect, 7.0, preview_background);
    let center = rect.center() - vec2(0.0, rect.height() * 0.025);
    let badge_radius = rect.width().min(rect.height()) * 0.37;

    paint_badge_shadow(&painter, center, badge_radius);
    paint_design(
        &painter,
        center,
        design.artwork_radius(badge_radius),
        design,
        textures,
        None,
    );
    paint_outside_circle(&painter, rect, center, badge_radius, preview_background);
    paint_badge_specular(&painter, center, badge_radius);
    painter.rect_stroke(
        rect,
        7.0,
        Stroke::new(1.0, Color32::from_gray(207)),
        StrokeKind::Inside,
    );
}

fn paint_outside_circle(
    painter: &egui::Painter,
    rect: Rect,
    center: Pos2,
    radius: f32,
    color: Color32,
) {
    const SEGMENTS: usize = 128;
    let outer_radius = rect.size().length();
    let mut mesh = Mesh::default();
    for index in 0..=SEGMENTS {
        let angle = index as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let direction = vec2(angle.cos(), angle.sin());
        mesh.vertices
            .push(Vertex::untextured(center + direction * radius, color));
        mesh.vertices
            .push(Vertex::untextured(center + direction * outer_radius, color));
    }
    for index in 0..SEGMENTS as u32 {
        let first = index * 2;
        mesh.indices.extend_from_slice(&[
            first,
            first + 1,
            first + 2,
            first + 1,
            first + 3,
            first + 2,
        ]);
    }
    painter.add(Shape::mesh(mesh));
}

pub fn paint_trim_boundary(
    painter: &egui::Painter,
    center: Pos2,
    artwork_radius: f32,
    design: &Design,
) {
    let trim_radius = design.trim_radius(artwork_radius);

    const DASH_SEGMENTS: usize = 64;
    for segment in (0..DASH_SEGMENTS).step_by(2) {
        let start = segment as f32 * std::f32::consts::TAU / DASH_SEGMENTS as f32;
        let end = (segment + 1) as f32 * std::f32::consts::TAU / DASH_SEGMENTS as f32;
        let points = (0..=3)
            .map(|point| {
                let angle = start + (end - start) * point as f32 / 3.0;
                center + vec2(angle.cos(), angle.sin()) * trim_radius
            })
            .collect();
        painter.add(Shape::line(points, Stroke::new(2.0, Color32::WHITE)));
    }

    let label_position = center - vec2(0.0, trim_radius - 14.0);
    let label = format!("FINISHED EDGE · {} mm", design.diameter_mm);
    let font = FontId::proportional(10.0);
    painter.text(
        label_position + vec2(1.0, 1.0),
        Align2::CENTER_CENTER,
        &label,
        font.clone(),
        Color32::from_black_alpha(210),
    );
    painter.text(
        label_position,
        Align2::CENTER_CENTER,
        label,
        font,
        Color32::WHITE,
    );
}

fn paint_badge_shadow(painter: &egui::Painter, center: Pos2, radius: f32) {
    const SEGMENTS: usize = 72;
    let shadow_center = center + vec2(3.0, 7.0);
    let inner_radius = radius - 1.0;
    let outer_radius = radius + 14.0;
    painter.circle_filled(shadow_center, inner_radius, Color32::from_black_alpha(24));

    let mut mesh = Mesh::default();
    for index in 0..=SEGMENTS {
        let angle = index as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let direction = vec2(angle.cos(), angle.sin());
        mesh.vertices.push(Vertex::untextured(
            shadow_center + direction * inner_radius,
            Color32::from_black_alpha(24),
        ));
        mesh.vertices.push(Vertex::untextured(
            shadow_center + direction * outer_radius,
            Color32::TRANSPARENT,
        ));
    }
    for index in 0..SEGMENTS as u32 {
        let first = index * 2;
        mesh.indices.extend_from_slice(&[
            first,
            first + 1,
            first + 2,
            first + 1,
            first + 3,
            first + 2,
        ]);
    }
    painter.add(Shape::mesh(mesh));
}

fn paint_badge_specular(painter: &egui::Painter, center: Pos2, radius: f32) {
    const SEGMENTS: usize = 64;
    let highlight_center = center - vec2(radius * 0.23, radius * 0.28);
    let highlight_radius = vec2(radius * 0.46, radius * 0.23);
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex::untextured(
        highlight_center,
        Color32::from_white_alpha(34),
    ));
    for index in 0..=SEGMENTS {
        let angle = index as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        mesh.vertices.push(Vertex::untextured(
            highlight_center
                + vec2(
                    angle.cos() * highlight_radius.x,
                    angle.sin() * highlight_radius.y,
                ),
            Color32::TRANSPARENT,
        ));
    }
    for index in 0..SEGMENTS as u32 {
        mesh.indices.extend_from_slice(&[0, index + 1, index + 2]);
    }
    painter.add(Shape::mesh(mesh));
}

pub fn selection_bounds(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    element: &Element,
) -> Rect {
    let margin = match &element.kind {
        ElementKind::Text { curvature, .. } if curvature.abs() > f32::EPSILON => 2.0,
        _ => 5.0,
    };
    element_bounds(painter, center, radius, element).expand(margin)
}

pub fn element_bounds(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    element: &Element,
) -> Rect {
    let bounds = unrotated_element_bounds(painter, center, radius, element);
    if element.rotation.abs() <= f32::EPSILON {
        return bounds;
    }
    let element_center = center + vec2(element.position[0], element.position[1]) * radius;
    Rect::from_points(&[
        rotate_point(bounds.left_top(), element_center, element.rotation),
        rotate_point(bounds.right_top(), element_center, element.rotation),
        rotate_point(bounds.right_bottom(), element_center, element.rotation),
        rotate_point(bounds.left_bottom(), element_center, element.rotation),
    ])
}

fn unrotated_element_bounds(
    _painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    element: &Element,
) -> Rect {
    let element_center = center + vec2(element.position[0], element.position[1]) * radius;
    match &element.kind {
        ElementKind::Text {
            text,
            font_size,
            font,
            curvature,
        } => {
            let scale = radius * 2.0 * *font_size;
            let color = Rgba([255, 255, 255, 255]);
            let composed = if curvature.abs() <= f32::EPSILON {
                compose_straight_text(text, scale, font, color)
            } else {
                compose_curved_text(text, scale, font, *curvature, color)
            };
            match composed {
                Some((image, origin)) => Rect::from_min_size(
                    element_center - origin,
                    vec2(image.width() as f32, image.height() as f32),
                ),
                None => Rect::from_center_size(element_center, Vec2::ZERO),
            }
        }
        ElementKind::Shape { shape, size } => Rect::from_center_size(
            element_center,
            if *shape == ShapeKind::Rectangle {
                vec2(radius * size * 2.0, radius * size * 4.0 / 3.0)
            } else {
                Vec2::splat(radius * size * 2.0)
            },
        ),
        ElementKind::Image {
            width,
            aspect_ratio,
            bleed_to_square,
            ..
        } => {
            let width = radius * 2.0 * width;
            let height = if *bleed_to_square {
                width
            } else {
                width / aspect_ratio
            };
            Rect::from_center_size(element_center, vec2(width, height))
        }
        ElementKind::QrCode { size, .. } => {
            Rect::from_center_size(element_center, Vec2::splat(radius * 2.0 * size))
        }
    }
}

pub fn hit_test(
    painter: &egui::Painter,
    pointer: Pos2,
    center: Pos2,
    radius: f32,
    design: &Design,
) -> Option<u64> {
    design
        .elements
        .iter()
        .rev()
        .filter(|element| element.visible)
        .find(|element| {
            element_bounds(painter, center, radius, element)
                .expand(10.0)
                .contains(pointer)
        })
        .map(|element| element.id)
}

pub fn export_png(design: &Design, path: &Path) -> Result<u32, String> {
    let output_size = export_pixel_size(design);
    let output = render_trimmed_badge(design, output_size, EXPORT_SUPERSAMPLE)?;
    output
        .save(path)
        .map_err(|error| format!("Could not save PNG: {error}"))?;
    Ok(output_size)
}

pub fn render_project_preview_png(design: &Design, output_size: u32) -> Result<Vec<u8>, String> {
    let output = render_trimmed_badge(design, output_size, 1)?;
    let mut bytes = Cursor::new(Vec::new());
    output
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| format!("Could not encode project preview: {error}"))?;
    Ok(bytes.into_inner())
}

pub fn export_pdf_sheet(design: &Design, path: &Path) -> Result<usize, String> {
    let output_size = export_pixel_size(design);
    let output = render_trimmed_badge(design, output_size, EXPORT_SUPERSAMPLE)?;
    let mut document = PdfDocument::new("Button Studio print sheet");
    let image_id = document.add_image(&RawImage {
        pixels: RawImageData::U8(output.into_raw()),
        width: output_size as usize,
        height: output_size as usize,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new(),
    });
    let layout = pdf_sheet_layout(design.artwork_diameter_mm());
    if layout.is_empty() {
        return Err("Button artwork is too large for an A4 page".to_owned());
    }

    let unrounded_pixels = design.artwork_diameter_mm() / 25.4 * EXPORT_DPI;
    let pixel_rounding_scale = unrounded_pixels / output_size as f32;
    let operations = layout
        .iter()
        .map(|position| Op::UseXobject {
            id: image_id.clone(),
            transform: XObjectTransform {
                translate_x: Some(Pt::from(Mm(position[0]))),
                translate_y: Some(Pt::from(Mm(position[1]))),
                scale_x: Some(pixel_rounding_scale),
                scale_y: Some(pixel_rounding_scale),
                dpi: Some(EXPORT_DPI),
                ..Default::default()
            },
        })
        .collect();
    let page = PdfPage::new(Mm(A4_WIDTH_MM), Mm(A4_HEIGHT_MM), operations);
    let bytes = document
        .with_pages(vec![page])
        .save(&PdfSaveOptions::default(), &mut Vec::new());
    fs::write(path, bytes).map_err(|error| format!("Could not save PDF: {error}"))?;
    Ok(layout.len())
}

pub fn export_pdf_series(
    design: &Design,
    rows: &[HashMap<String, String>],
    path: &Path,
) -> Result<PdfExportSummary, String> {
    if rows.is_empty() {
        return Err("Import at least one CSV entry before exporting a series".to_owned());
    }
    let layout = pdf_sheet_layout(design.artwork_diameter_mm());
    if layout.is_empty() {
        return Err("Button artwork is too large for an A4 page".to_owned());
    }

    let output_size = export_pixel_size(design);
    let unrounded_pixels = design.artwork_diameter_mm() / 25.4 * EXPORT_DPI;
    let pixel_rounding_scale = unrounded_pixels / output_size as f32;
    let mut document = PdfDocument::new("Button Studio series print");
    let mut pages = Vec::new();
    for page_rows in rows.chunks(layout.len()) {
        let mut operations = Vec::with_capacity(page_rows.len());
        for (values, position) in page_rows.iter().zip(&layout) {
            let personalized = design.personalized(values);
            let output = render_trimmed_badge(&personalized, output_size, EXPORT_SUPERSAMPLE)?;
            let image_id = document.add_image(&RawImage {
                pixels: RawImageData::U8(output.into_raw()),
                width: output_size as usize,
                height: output_size as usize,
                data_format: RawImageFormat::RGBA8,
                tag: Vec::new(),
            });
            operations.push(Op::UseXobject {
                id: image_id,
                transform: XObjectTransform {
                    translate_x: Some(Pt::from(Mm(position[0]))),
                    translate_y: Some(Pt::from(Mm(position[1]))),
                    scale_x: Some(pixel_rounding_scale),
                    scale_y: Some(pixel_rounding_scale),
                    dpi: Some(EXPORT_DPI),
                    ..Default::default()
                },
            });
        }
        pages.push(PdfPage::new(Mm(A4_WIDTH_MM), Mm(A4_HEIGHT_MM), operations));
    }
    let page_count = pages.len();
    let bytes = document
        .with_pages(pages)
        .save(&PdfSaveOptions::default(), &mut Vec::new());
    fs::write(path, bytes).map_err(|error| format!("Could not save PDF: {error}"))?;
    Ok(PdfExportSummary {
        buttons: rows.len(),
        pages: page_count,
    })
}

fn pdf_sheet_layout(artwork_diameter_mm: f32) -> Vec<[f32; 2]> {
    let available_width = A4_WIDTH_MM - PDF_MARGIN_MM * 2.0;
    let available_height = A4_HEIGHT_MM - PDF_MARGIN_MM * 2.0;
    let stride = artwork_diameter_mm + PDF_BUTTON_GAP_MM;
    let columns = ((available_width + PDF_BUTTON_GAP_MM) / stride).floor() as usize;
    let rows = ((available_height + PDF_BUTTON_GAP_MM) / stride).floor() as usize;
    let grid_width =
        columns as f32 * artwork_diameter_mm + columns.saturating_sub(1) as f32 * PDF_BUTTON_GAP_MM;
    let grid_height =
        rows as f32 * artwork_diameter_mm + rows.saturating_sub(1) as f32 * PDF_BUTTON_GAP_MM;
    let left = (A4_WIDTH_MM - grid_width) / 2.0;
    let bottom = (A4_HEIGHT_MM - grid_height) / 2.0;

    (0..rows)
        .flat_map(|row| {
            (0..columns)
                .map(move |column| [left + column as f32 * stride, bottom + row as f32 * stride])
        })
        .collect()
}

pub fn export_pixel_size(design: &Design) -> u32 {
    ((design.artwork_diameter_mm() / 25.4) * EXPORT_DPI).round() as u32
}

#[cfg(test)]
pub fn render_preview_image(design: &Design, output_size: u32) -> Result<egui::ColorImage, String> {
    let image = render_trimmed_badge(design, output_size, 1)?;
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

fn render_trimmed_badge(
    design: &Design,
    output_size: u32,
    supersample: u32,
) -> Result<RgbaImage, String> {
    let render_size = output_size * supersample;
    let center = render_size as f32 / 2.0;
    let artwork_radius = center;
    let mut canvas = RgbaImage::from_pixel(render_size, render_size, Rgba([0, 0, 0, 0]));
    let mut decoded_assets = HashMap::new();

    raster_background(
        &mut canvas,
        center,
        artwork_radius,
        design,
        &mut decoded_assets,
    )?;
    for element in design.elements.iter().filter(|element| element.visible) {
        raster_element(
            &mut canvas,
            center,
            artwork_radius,
            element,
            design,
            &mut decoded_assets,
        )?;
    }

    let radius_squared = artwork_radius * artwork_radius;
    for (x, y, pixel) in canvas.enumerate_pixels_mut() {
        let dx = x as f32 + 0.5 - center;
        let dy = y as f32 + 0.5 - center;
        if dx * dx + dy * dy > radius_squared {
            *pixel = Rgba([0, 0, 0, 0]);
        }
    }

    if supersample == 1 {
        Ok(canvas)
    } else {
        Ok(imageops::resize(
            &canvas,
            output_size,
            output_size,
            imageops::FilterType::Lanczos3,
        ))
    }
}

fn paint_background(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    design: &Design,
    textures: &TextureMap,
) {
    if design.background_style == BackgroundStyle::None {
        return;
    }

    let base: Color32 = design.background.into();
    painter.circle_filled(center, radius, base);

    if let Some(background_image) = &design.background_image
        && let Some(texture) = textures.get(&background_image.asset_id)
    {
        paint_textured_circle(
            painter,
            center,
            radius,
            texture,
            background_image.aspect_ratio,
        );
        return;
    }

    match design.background_style {
        BackgroundStyle::None => {}
        BackgroundStyle::Solid => {}
        BackgroundStyle::Sunburst => {
            let accent = mix_color(base, Color32::WHITE, 0.13);
            for wedge in (0..18).step_by(2) {
                let start = wedge as f32 * std::f32::consts::TAU / 18.0;
                let end = (wedge + 1) as f32 * std::f32::consts::TAU / 18.0;
                painter.add(Shape::convex_polygon(
                    vec![
                        center,
                        center + vec2(start.cos(), start.sin()) * radius,
                        center + vec2(end.cos(), end.sin()) * radius,
                    ],
                    accent,
                    Stroke::NONE,
                ));
            }
        }
        BackgroundStyle::Confetti => {
            for (index, (x, y)) in confetti_positions().iter().enumerate() {
                let position = center + vec2(*x, *y) * radius;
                let base = confetti_color(index);
                let color = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 145);
                if index % 3 == 0 {
                    painter.circle_filled(position, radius * 0.025, color);
                } else {
                    painter.line_segment(
                        [position - vec2(3.0, 5.0), position + vec2(3.0, 5.0)],
                        Stroke::new(radius * 0.018, color),
                    );
                }
            }
        }
        BackgroundStyle::Grid => {
            let line_color = mix_color(base, Color32::WHITE, 0.26).gamma_multiply(0.55);
            let step = radius * 0.2;
            for index in -4..=4 {
                let offset = index as f32 * step;
                let extent = (radius * radius - offset * offset).max(0.0).sqrt();
                painter.line_segment(
                    [
                        center + vec2(offset, -extent),
                        center + vec2(offset, extent),
                    ],
                    Stroke::new(1.0, line_color),
                );
                painter.line_segment(
                    [
                        center + vec2(-extent, offset),
                        center + vec2(extent, offset),
                    ],
                    Stroke::new(1.0, line_color),
                );
            }
        }
    }
}

fn paint_textured_circle(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    texture: &TextureHandle,
    aspect_ratio: f32,
) {
    const SEGMENTS: usize = 96;
    let (u_radius, v_radius) = cover_uv_radii(aspect_ratio);
    let mut mesh = Mesh::with_texture(texture.id());
    mesh.vertices.push(Vertex {
        pos: center,
        uv: pos2(0.5, 0.5),
        color: Color32::WHITE,
    });

    for segment in 0..=SEGMENTS {
        let angle = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let direction = vec2(angle.cos(), angle.sin());
        mesh.vertices.push(Vertex {
            pos: center + direction * radius,
            uv: pos2(0.5 + direction.x * u_radius, 0.5 + direction.y * v_radius),
            color: Color32::WHITE,
        });
    }
    for segment in 0..SEGMENTS as u32 {
        mesh.indices
            .extend_from_slice(&[0, segment + 1, segment + 2]);
    }
    painter.add(Shape::mesh(mesh));
}

fn cover_uv_radii(aspect_ratio: f32) -> (f32, f32) {
    if aspect_ratio >= 1.0 {
        (0.5 / aspect_ratio.max(0.01), 0.5)
    } else {
        (0.5, 0.5 * aspect_ratio.max(0.01))
    }
}

fn paint_element(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    element: &Element,
    textures: &TextureMap,
) {
    let position = center + vec2(element.position[0], element.position[1]) * radius;
    let color = element_color(element);
    match &element.kind {
        ElementKind::Text {
            text,
            font_size,
            font,
            curvature,
        } => {
            let ppp = painter.ctx().pixels_per_point();
            let scale = radius * 2.0 * *font_size * ppp;
            let raster_color = element_rgba(element);
            if let Some((texture_id, size, origin)) = text_texture(
                painter.ctx(),
                text,
                scale,
                font,
                *curvature,
                raster_color,
                ppp,
            ) {
                let rect = Rect::from_min_size(position - origin, size);
                paint_rotated_image(
                    painter,
                    texture_id,
                    rect,
                    position,
                    element.rotation,
                    Color32::WHITE,
                );
            }
        }
        ElementKind::Shape { shape, size } => match shape {
            ShapeKind::Circle => {
                painter.circle_filled(position, radius * size, color);
            }
            ShapeKind::Star => {
                let points =
                    rotated_shape_points(*shape, position, radius * size, element.rotation);
                paint_polygon_fan(painter, position, &points, color);
            }
            _ => {
                let points =
                    rotated_shape_points(*shape, position, radius * size, element.rotation);
                painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
            }
        },
        ElementKind::Image {
            asset_id,
            aspect_ratio,
            bleed_to_square,
            ..
        } => {
            let bounds = unrotated_element_bounds(painter, center, radius, element);
            if let Some(texture) = textures.get(asset_id) {
                let tint = Color32::WHITE.gamma_multiply(element.opacity as f32 / 255.0);
                if *bleed_to_square {
                    paint_edge_bleed_image(
                        painter,
                        texture.id(),
                        bounds,
                        position,
                        element.rotation,
                        *aspect_ratio,
                        tint,
                    );
                } else {
                    paint_rotated_image(
                        painter,
                        texture.id(),
                        bounds,
                        position,
                        element.rotation,
                        tint,
                    );
                }
            } else {
                let points = rotated_rect_points(bounds, position, element.rotation);
                painter.add(Shape::convex_polygon(
                    points.clone(),
                    Color32::from_white_alpha(35),
                    Stroke::NONE,
                ));
                painter.add(Shape::closed_line(
                    points,
                    Stroke::new(1.0, Color32::from_white_alpha(120)),
                ));
            }
        }
        ElementKind::QrCode {
            content,
            background,
            transparent_background,
            ..
        } => {
            let background = (!transparent_background)
                .then(|| Color32::from(*background).gamma_multiply(element.opacity as f32 / 255.0));
            paint_qr_code(
                painter,
                unrotated_element_bounds(painter, center, radius, element),
                content,
                color,
                background,
                element.rotation,
            );
        }
    }
}

fn paint_rotated_image(
    painter: &egui::Painter,
    texture_id: egui::TextureId,
    bounds: Rect,
    center: Pos2,
    rotation: f32,
    color: Color32,
) {
    paint_rotated_image_uv(
        painter,
        texture_id,
        bounds,
        center,
        rotation,
        Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
        color,
    );
}

fn paint_edge_bleed_image(
    painter: &egui::Painter,
    texture_id: egui::TextureId,
    bounds: Rect,
    center: Pos2,
    rotation: f32,
    aspect_ratio: f32,
    color: Color32,
) {
    let aspect_ratio = aspect_ratio.max(0.01);
    if aspect_ratio >= 1.0 {
        let image_height = bounds.height() / aspect_ratio;
        let image_bounds = Rect::from_center_size(center, vec2(bounds.width(), image_height));
        paint_rotated_image_uv(
            painter,
            texture_id,
            Rect::from_min_max(bounds.min, image_bounds.right_top()),
            center,
            rotation,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 0.0)),
            color,
        );
        paint_rotated_image(painter, texture_id, image_bounds, center, rotation, color);
        paint_rotated_image_uv(
            painter,
            texture_id,
            Rect::from_min_max(image_bounds.left_bottom(), bounds.max),
            center,
            rotation,
            Rect::from_min_max(pos2(0.0, 1.0), pos2(1.0, 1.0)),
            color,
        );
    } else {
        let image_width = bounds.width() * aspect_ratio;
        let image_bounds = Rect::from_center_size(center, vec2(image_width, bounds.height()));
        paint_rotated_image_uv(
            painter,
            texture_id,
            Rect::from_min_max(bounds.min, image_bounds.left_bottom()),
            center,
            rotation,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(0.0, 1.0)),
            color,
        );
        paint_rotated_image(painter, texture_id, image_bounds, center, rotation, color);
        paint_rotated_image_uv(
            painter,
            texture_id,
            Rect::from_min_max(image_bounds.right_top(), bounds.max),
            center,
            rotation,
            Rect::from_min_max(pos2(1.0, 0.0), pos2(1.0, 1.0)),
            color,
        );
    }
}

fn paint_rotated_image_uv(
    painter: &egui::Painter,
    texture_id: egui::TextureId,
    bounds: Rect,
    center: Pos2,
    rotation: f32,
    uv_bounds: Rect,
    color: Color32,
) {
    let points = rotated_rect_points(bounds, center, rotation);
    let mut mesh = Mesh::with_texture(texture_id);
    for (pos, uv) in points.into_iter().zip([
        uv_bounds.left_top(),
        uv_bounds.right_top(),
        uv_bounds.right_bottom(),
        uv_bounds.left_bottom(),
    ]) {
        mesh.vertices.push(Vertex { pos, uv, color });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
}

fn paint_qr_code(
    painter: &egui::Painter,
    bounds: Rect,
    content: &str,
    color: Color32,
    background: Option<Color32>,
    rotation: f32,
) {
    const QUIET_ZONE: usize = 4;
    let Ok(code) = QrCode::new(content.as_bytes()) else {
        if let Some(background) = background {
            painter.add(Shape::convex_polygon(
                rotated_rect_points(bounds, bounds.center(), rotation),
                background,
                Stroke::NONE,
            ));
        }
        painter.line_segment(
            [
                rotate_point(bounds.left_top(), bounds.center(), rotation),
                rotate_point(bounds.right_bottom(), bounds.center(), rotation),
            ],
            Stroke::new(2.0, color),
        );
        painter.line_segment(
            [
                rotate_point(bounds.right_top(), bounds.center(), rotation),
                rotate_point(bounds.left_bottom(), bounds.center(), rotation),
            ],
            Stroke::new(2.0, color),
        );
        return;
    };
    let module_count = code.width();
    let full_width = module_count + QUIET_ZONE * 2;
    let module_size = bounds.width() / full_width as f32;
    if let Some(background) = background {
        painter.add(Shape::convex_polygon(
            rotated_rect_points(bounds, bounds.center(), rotation),
            background,
            Stroke::NONE,
        ));
    }
    for y in 0..module_count {
        for x in 0..module_count {
            if code[(x, y)] == Color::Dark {
                let min = bounds.min
                    + vec2(
                        (x + QUIET_ZONE) as f32 * module_size,
                        (y + QUIET_ZONE) as f32 * module_size,
                    );
                painter.add(Shape::convex_polygon(
                    rotated_rect_points(
                        Rect::from_min_size(min, Vec2::splat(module_size)),
                        bounds.center(),
                        rotation,
                    ),
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }
}

struct TextTexture {
    handle: TextureHandle,
    size: Vec2,
    origin: Vec2,
}

thread_local! {
    static TEXT_TEXTURES: RefCell<HashMap<u64, TextTexture>> = RefCell::new(HashMap::new());
}

fn element_rgba(element: &Element) -> Rgba<u8> {
    let mut color = element.color;
    color.alpha = ((color.alpha as u16 * element.opacity as u16) / 255) as u8;
    rgba(color)
}

/// Rasterizes straight text into a tight RGBA image through the shared export
/// pipeline. The returned origin is the pixel offset that maps to the element
/// position (the image center).
fn compose_straight_text(
    text: &str,
    scale: f32,
    font: &TextFont,
    color: Rgba<u8>,
) -> Option<(RgbaImage, Vec2)> {
    let image = crate::text_raster::render_line(text, scale, font, color)?;
    let origin = vec2(image.width() as f32 / 2.0, image.height() as f32 / 2.0);
    Some((image, origin))
}

/// Rasterizes curved text into an RGBA image through the shared export pipeline.
/// The returned origin is the pixel offset that maps to the element position
/// (the arc center).
fn compose_curved_text(
    text: &str,
    scale: f32,
    font: &TextFont,
    curvature: f32,
    color: Rgba<u8>,
) -> Option<(RgbaImage, Vec2)> {
    let tiles = crate::text_raster::render_glyph_tiles(text, scale, font, color);
    let widths = tiles.iter().map(|(_, advance)| *advance).collect::<Vec<_>>();

    let mut placed = Vec::new();
    let mut min = vec2(f32::INFINITY, f32::INFINITY);
    let mut max = vec2(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for ((tile, _), placement) in tiles.iter().zip(arc_glyphs(&widths, curvature)) {
        let Some(tile) = tile else {
            continue;
        };
        let rotated = rotate_about_center_no_crop(
            tile,
            placement.angle,
            Interpolation::Bicubic,
            Border::Constant(Rgba([0, 0, 0, 0])),
        );
        let width = rotated.width() as f32;
        let height = rotated.height() as f32;
        let left = placement.offset.x - width / 2.0;
        let top = placement.offset.y - height / 2.0;
        min.x = min.x.min(left);
        min.y = min.y.min(top);
        max.x = max.x.max(left + width);
        max.y = max.y.max(top + height);
        placed.push((rotated, left, top));
    }
    if placed.is_empty() {
        return None;
    }

    let width = (max.x - min.x).ceil().max(1.0) as u32;
    let height = (max.y - min.y).ceil().max(1.0) as u32;
    let mut canvas = RgbaImage::new(width, height);
    for (image, left, top) in &placed {
        imageops::overlay(
            &mut canvas,
            image,
            (left - min.x).round() as i64,
            (top - min.y).round() as i64,
        );
    }
    Some((canvas, vec2(-min.x, -min.y)))
}

/// Rasterizes a text element through the shared export pipeline and caches the
/// result as an egui texture so the editor preview matches the exported artwork
/// pixel-for-pixel (including where glyphs fall relative to the trim edge).
/// Returns the texture id, its logical size, and the logical origin that maps to
/// the element position.
fn text_texture(
    ctx: &egui::Context,
    text: &str,
    scale: f32,
    font: &TextFont,
    curvature: f32,
    color: Rgba<u8>,
    pixels_per_point: f32,
) -> Option<(egui::TextureId, Vec2, Vec2)> {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    font.registration_key().hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    curvature.to_bits().hash(&mut hasher);
    color.0.hash(&mut hasher);
    let key = hasher.finish();

    TEXT_TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&key) {
            let (image, origin) = if curvature.abs() <= f32::EPSILON {
                compose_straight_text(text, scale, font, color)
            } else {
                compose_curved_text(text, scale, font, curvature, color)
            }?;
            let dimensions = [image.width() as usize, image.height() as usize];
            let color_image = ColorImage::from_rgba_unmultiplied(dimensions, image.as_raw());
            let handle = ctx.load_texture(
                format!("text-{key:016x}"),
                color_image,
                TextureOptions::LINEAR,
            );
            if cache.len() > 512 {
                cache.clear();
            }
            cache.insert(
                key,
                TextTexture {
                    handle,
                    size: vec2(
                        dimensions[0] as f32 / pixels_per_point,
                        dimensions[1] as f32 / pixels_per_point,
                    ),
                    origin: origin / pixels_per_point,
                },
            );
        }
        let entry = cache.get(&key)?;
        Some((entry.handle.id(), entry.size, entry.origin))
    })
}

#[derive(Clone, Copy, Debug)]
struct ArcGlyph {
    offset: Vec2,
    angle: f32,
}

fn arc_glyphs(widths: &[f32], curvature: f32) -> Vec<ArcGlyph> {
    let total_width = widths.iter().sum::<f32>();
    let total_angle = curvature * std::f32::consts::PI * 0.9;
    if total_width <= f32::EPSILON || total_angle.abs() <= f32::EPSILON {
        return widths
            .iter()
            .scan(-total_width / 2.0, |x, width| {
                let center = *x + width / 2.0;
                *x += width;
                Some(ArcGlyph {
                    offset: vec2(center, 0.0),
                    angle: 0.0,
                })
            })
            .collect();
    }

    let direction = curvature.signum();
    let arc_radius = total_width / total_angle.abs();
    widths
        .iter()
        .scan(-total_width / 2.0, |x, width| {
            let distance = *x + width / 2.0;
            *x += width;
            let angle = distance / arc_radius;
            Some(ArcGlyph {
                offset: vec2(
                    angle.sin() * arc_radius,
                    direction * arc_radius * (1.0 - angle.cos()),
                ),
                angle: direction * angle,
            })
        })
        .collect()
}

fn paint_polygon_fan(painter: &egui::Painter, center: Pos2, points: &[Pos2], color: Color32) {
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex::untextured(center, color));
    mesh.vertices
        .extend(points.iter().map(|point| Vertex::untextured(*point, color)));
    for index in 0..points.len() as u32 {
        mesh.indices
            .extend_from_slice(&[0, index + 1, (index + 1) % points.len() as u32 + 1]);
    }
    painter.add(Shape::mesh(mesh));
}

fn shape_points(kind: ShapeKind, center: Pos2, radius: f32) -> Vec<Pos2> {
    match kind {
        ShapeKind::Star => (0..10)
            .map(|index| {
                let angle =
                    index as f32 * std::f32::consts::TAU / 10.0 - std::f32::consts::FRAC_PI_2;
                let point_radius = radius * if index % 2 == 0 { 1.0 } else { 0.45 };
                center + vec2(angle.cos(), angle.sin()) * point_radius
            })
            .collect(),
        ShapeKind::Heart => (0..32)
            .map(|index| {
                let angle = index as f32 * std::f32::consts::TAU / 32.0;
                let x = 16.0 * angle.sin().powi(3) / 17.0;
                let y = -(13.0 * angle.cos()
                    - 5.0 * (2.0 * angle).cos()
                    - 2.0 * (3.0 * angle).cos()
                    - (4.0 * angle).cos())
                    / 17.0;
                center + vec2(x, y) * radius
            })
            .collect(),
        ShapeKind::Diamond => vec![
            center - vec2(0.0, radius),
            center + vec2(radius * 0.82, 0.0),
            center + vec2(0.0, radius),
            center - vec2(radius * 0.82, 0.0),
        ],
        ShapeKind::Rectangle => vec![
            center + vec2(-radius, -radius * 2.0 / 3.0),
            center + vec2(radius, -radius * 2.0 / 3.0),
            center + vec2(radius, radius * 2.0 / 3.0),
            center + vec2(-radius, radius * 2.0 / 3.0),
        ],
        ShapeKind::Bolt => [
            (-0.15, -1.0),
            (0.72, -1.0),
            (0.2, -0.12),
            (0.78, -0.12),
            (-0.55, 1.0),
            (-0.12, 0.12),
            (-0.7, 0.12),
        ]
        .into_iter()
        .map(|(x, y)| center + vec2(x, y) * radius)
        .collect(),
        ShapeKind::Circle => Vec::new(),
    }
}

fn rotated_shape_points(kind: ShapeKind, center: Pos2, radius: f32, rotation: f32) -> Vec<Pos2> {
    shape_points(kind, center, radius)
        .into_iter()
        .map(|point| rotate_point(point, center, rotation))
        .collect()
}

fn rotated_rect_points(bounds: Rect, center: Pos2, rotation: f32) -> Vec<Pos2> {
    [
        bounds.left_top(),
        bounds.right_top(),
        bounds.right_bottom(),
        bounds.left_bottom(),
    ]
    .into_iter()
    .map(|point| rotate_point(point, center, rotation))
    .collect()
}

fn rotate_point(point: Pos2, center: Pos2, rotation: f32) -> Pos2 {
    center + rotate_vector(point - center, rotation.to_radians())
}

fn rotate_vector(vector: Vec2, rotation: f32) -> Vec2 {
    let (sin, cos) = rotation.sin_cos();
    vec2(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    )
}

fn raster_background(
    canvas: &mut RgbaImage,
    center: f32,
    radius: f32,
    design: &Design,
    decoded_assets: &mut HashMap<String, RgbaImage>,
) -> Result<(), String> {
    if design.background_style == BackgroundStyle::None {
        return Ok(());
    }

    let base = rgba(design.background);
    draw_filled_circle_mut(canvas, (center as i32, center as i32), radius as i32, base);

    if let Some(background_image) = &design.background_image {
        let source = decoded_asset(design, decoded_assets, &background_image.asset_id)?;
        let target_size = (radius * 2.0).round().max(1.0) as u32;
        let covered = cover_square(source, target_size);
        imageops::overlay(
            canvas,
            &covered,
            (center - target_size as f32 / 2.0).round() as i64,
            (center - target_size as f32 / 2.0).round() as i64,
        );
        return Ok(());
    }

    match design.background_style {
        BackgroundStyle::None => {}
        BackgroundStyle::Solid => {}
        BackgroundStyle::Sunburst => {
            let accent = rgba(mix_design_color(
                design.background,
                DesignColor::WHITE,
                0.13,
            ));
            for wedge in (0..18).step_by(2) {
                let start = wedge as f32 * std::f32::consts::TAU / 18.0;
                let end = (wedge + 1) as f32 * std::f32::consts::TAU / 18.0;
                draw_polygon_mut(
                    canvas,
                    &[
                        Point::new(center as i32, center as i32),
                        Point::new(
                            (center + start.cos() * radius) as i32,
                            (center + start.sin() * radius) as i32,
                        ),
                        Point::new(
                            (center + end.cos() * radius) as i32,
                            (center + end.sin() * radius) as i32,
                        ),
                    ],
                    accent,
                );
            }
        }
        BackgroundStyle::Confetti => {
            for (index, (x, y)) in confetti_positions().iter().enumerate() {
                let x = center + x * radius;
                let y = center + y * radius;
                let color = rgba(DesignColor::from(confetti_color(index)).with_alpha(150));
                if index % 3 == 0 {
                    draw_filled_circle_mut(
                        canvas,
                        (x as i32, y as i32),
                        (radius * 0.025) as i32,
                        color,
                    );
                } else {
                    draw_line_segment_mut(
                        canvas,
                        (x - radius * 0.012, y - radius * 0.02),
                        (x + radius * 0.012, y + radius * 0.02),
                        color,
                    );
                }
            }
        }
        BackgroundStyle::Grid => {
            let color = rgba(mix_design_color(
                design.background,
                DesignColor::WHITE,
                0.18,
            ));
            let step = radius * 0.2;
            for index in -4..=4 {
                let offset = index as f32 * step;
                let extent = (radius * radius - offset * offset).max(0.0).sqrt();
                draw_line_segment_mut(
                    canvas,
                    (center + offset, center - extent),
                    (center + offset, center + extent),
                    color,
                );
                draw_line_segment_mut(
                    canvas,
                    (center - extent, center + offset),
                    (center + extent, center + offset),
                    color,
                );
            }
        }
    }
    Ok(())
}

fn cover_square(source: &RgbaImage, target_size: u32) -> RgbaImage {
    let scale = (target_size as f32 / source.width().max(1) as f32)
        .max(target_size as f32 / source.height().max(1) as f32);
    let width = (source.width() as f32 * scale).ceil().max(1.0) as u32;
    let height = (source.height() as f32 * scale).ceil().max(1.0) as u32;
    let resized = imageops::resize(source, width, height, imageops::FilterType::Lanczos3);
    let left = (width - target_size) / 2;
    let top = (height - target_size) / 2;
    imageops::crop_imm(&resized, left, top, target_size, target_size).to_image()
}

fn raster_element(
    canvas: &mut RgbaImage,
    center: f32,
    radius: f32,
    element: &Element,
    design: &Design,
    decoded_assets: &mut HashMap<String, RgbaImage>,
) -> Result<(), String> {
    if element.rotation.abs() <= f32::EPSILON {
        return raster_element_unrotated(canvas, center, radius, element, design, decoded_assets);
    }

    let mut layer = RgbaImage::from_pixel(canvas.width(), canvas.height(), Rgba([0, 0, 0, 0]));
    let mut local_element = element.clone();
    local_element.position = [0.0, 0.0];
    local_element.rotation = 0.0;
    raster_element_unrotated(
        &mut layer,
        center,
        radius,
        &local_element,
        design,
        decoded_assets,
    )?;
    let rotated = rotate_about_center_no_crop(
        &layer,
        element.rotation.to_radians(),
        Interpolation::Bicubic,
        Border::Constant(Rgba([0, 0, 0, 0])),
    );
    imageops::overlay(
        canvas,
        &rotated,
        (center + element.position[0] * radius - rotated.width() as f32 / 2.0).round() as i64,
        (center + element.position[1] * radius - rotated.height() as f32 / 2.0).round() as i64,
    );
    Ok(())
}

fn raster_element_unrotated(
    canvas: &mut RgbaImage,
    center: f32,
    radius: f32,
    element: &Element,
    design: &Design,
    decoded_assets: &mut HashMap<String, RgbaImage>,
) -> Result<(), String> {
    let x = center + element.position[0] * radius;
    let y = center + element.position[1] * radius;
    let mut color = element.color;
    color.alpha = ((color.alpha as u16 * element.opacity as u16) / 255) as u8;

    match &element.kind {
        ElementKind::Text {
            text,
            font_size,
            font,
            curvature,
        } => {
            let scale = radius * 2.0 * font_size;
            if curvature.abs() <= f32::EPSILON {
                raster_centered_text(canvas, pos2(x, y), text, scale, font, rgba(color));
            } else {
                raster_curved_text(
                    canvas,
                    pos2(x, y),
                    text,
                    scale,
                    font,
                    *curvature,
                    rgba(color),
                );
            }
        }
        ElementKind::Shape { shape, size } => {
            let shape_radius = radius * size;
            if *shape == ShapeKind::Circle {
                draw_filled_circle_mut(
                    canvas,
                    (x as i32, y as i32),
                    shape_radius as i32,
                    rgba(color),
                );
            } else {
                let points = shape_points(*shape, pos2(x, y), shape_radius)
                    .into_iter()
                    .map(|point| Point::new(point.x as i32, point.y as i32))
                    .collect::<Vec<_>>();
                draw_polygon_mut(canvas, &points, rgba(color));
            }
        }
        ElementKind::Image {
            asset_id,
            width,
            aspect_ratio,
            bleed_to_square,
        } => {
            let source = decoded_asset(design, decoded_assets, asset_id)?;
            let target_width = (radius * 2.0 * width).max(1.0) as u32;
            let mut resized = if *bleed_to_square {
                edge_bleed_square(source, target_width, *aspect_ratio)
            } else {
                let target_height = (target_width as f32 / aspect_ratio).max(1.0) as u32;
                imageops::resize(
                    source,
                    target_width,
                    target_height,
                    imageops::FilterType::Lanczos3,
                )
            };
            for pixel in resized.pixels_mut() {
                pixel[3] = ((pixel[3] as u16 * element.opacity as u16) / 255) as u8;
            }
            imageops::overlay(
                canvas,
                &resized,
                (x - target_width as f32 / 2.0) as i64,
                (y - resized.height() as f32 / 2.0) as i64,
            );
        }
        ElementKind::QrCode {
            content,
            size,
            background,
            transparent_background,
        } => {
            let mut background = *background;
            background.alpha = ((background.alpha as u16 * element.opacity as u16) / 255) as u8;
            raster_qr_code(
                canvas,
                x,
                y,
                radius * 2.0 * size,
                content,
                color,
                (!transparent_background).then_some(background),
            )?;
        }
    }
    Ok(())
}

fn edge_bleed_square(source: &RgbaImage, target_size: u32, aspect_ratio: f32) -> RgbaImage {
    let aspect_ratio = aspect_ratio.max(0.01);
    let (width, height) = if aspect_ratio >= 1.0 {
        (
            target_size,
            (target_size as f32 / aspect_ratio).round().max(1.0) as u32,
        )
    } else {
        (
            (target_size as f32 * aspect_ratio).round().max(1.0) as u32,
            target_size,
        )
    };
    let resized = imageops::resize(source, width, height, imageops::FilterType::Lanczos3);
    let left = (target_size - width) / 2;
    let top = (target_size - height) / 2;
    RgbaImage::from_fn(target_size, target_size, |x, y| {
        *resized.get_pixel(
            x.saturating_sub(left).min(width - 1),
            y.saturating_sub(top).min(height - 1),
        )
    })
}

fn decoded_asset<'a>(
    design: &Design,
    decoded_assets: &'a mut HashMap<String, RgbaImage>,
    asset_id: &str,
) -> Result<&'a RgbaImage, String> {
    if !decoded_assets.contains_key(asset_id) {
        let asset = design.asset(asset_id)?;
        let decoded = image::load_from_memory(&asset.data)
            .map_err(|error| format!("Could not decode embedded image {asset_id}: {error}"))?
            .to_rgba8();
        decoded_assets.insert(asset_id.to_owned(), decoded);
    }
    Ok(&decoded_assets[asset_id])
}

fn raster_centered_text(
    canvas: &mut RgbaImage,
    position: Pos2,
    text: &str,
    scale: f32,
    font: &TextFont,
    color: Rgba<u8>,
) {
    let Some((image, origin)) = compose_straight_text(text, scale, font, color) else {
        return;
    };
    imageops::overlay(
        canvas,
        &image,
        (position.x - origin.x).round() as i64,
        (position.y - origin.y).round() as i64,
    );
}

#[cfg(test)]
fn alpha_bounds(image: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] == 0 {
            continue;
        }
        bounds = Some(match bounds {
            Some((left, top, right, bottom)) => {
                (left.min(x), top.min(y), right.max(x), bottom.max(y))
            }
            None => (x, y, x, y),
        });
    }
    bounds
}

fn raster_qr_code(
    canvas: &mut RgbaImage,
    center_x: f32,
    center_y: f32,
    size: f32,
    content: &str,
    color: DesignColor,
    background: Option<DesignColor>,
) -> Result<(), String> {
    const QUIET_ZONE: usize = 4;
    let code = QrCode::new(content.as_bytes())
        .map_err(|error| format!("Could not generate QR code: {error}"))?;
    let module_count = code.width();
    let full_width = module_count + QUIET_ZONE * 2;
    let left = center_x - size / 2.0;
    let top = center_y - size / 2.0;
    let module_size = size / full_width as f32;
    let dark = rgba(color);

    let x_start = left.floor() as i32;
    let y_start = top.floor() as i32;
    let x_end = (left + size).ceil() as i32;
    let y_end = (top + size).ceil() as i32;
    for pixel_y in y_start..y_end {
        for pixel_x in x_start..x_end {
            let module_x = (((pixel_x as f32 + 0.5 - left) / module_size).floor() as isize)
                .clamp(0, full_width as isize - 1) as usize;
            let module_y = (((pixel_y as f32 + 0.5 - top) / module_size).floor() as isize)
                .clamp(0, full_width as isize - 1) as usize;
            let dark_module = module_x >= QUIET_ZONE
                && module_y >= QUIET_ZONE
                && module_x < module_count + QUIET_ZONE
                && module_y < module_count + QUIET_ZONE
                && code[(module_x - QUIET_ZONE, module_y - QUIET_ZONE)] == Color::Dark;
            if pixel_x >= 0
                && pixel_y >= 0
                && pixel_x < canvas.width() as i32
                && pixel_y < canvas.height() as i32
            {
                if dark_module {
                    canvas.put_pixel(pixel_x as u32, pixel_y as u32, dark);
                } else if let Some(background) = background {
                    canvas.put_pixel(pixel_x as u32, pixel_y as u32, rgba(background));
                }
            }
        }
    }
    Ok(())
}

fn raster_curved_text(
    canvas: &mut RgbaImage,
    position: Pos2,
    text: &str,
    scale: f32,
    font: &TextFont,
    curvature: f32,
    color: Rgba<u8>,
) {
    let Some((image, origin)) = compose_curved_text(text, scale, font, curvature, color) else {
        return;
    };
    imageops::overlay(
        canvas,
        &image,
        (position.x - origin.x).round() as i64,
        (position.y - origin.y).round() as i64,
    );
}

fn element_color(element: &Element) -> Color32 {
    let mut color = element.color;
    color.alpha = ((color.alpha as u16 * element.opacity as u16) / 255) as u8;
    color.into()
}

fn mix_color(from: Color32, to: Color32, amount: f32) -> Color32 {
    Color32::from_rgb(
        lerp(from.r(), to.r(), amount),
        lerp(from.g(), to.g(), amount),
        lerp(from.b(), to.b(), amount),
    )
}

fn mix_design_color(from: DesignColor, to: DesignColor, amount: f32) -> DesignColor {
    DesignColor::rgb(
        lerp(from.red, to.red, amount),
        lerp(from.green, to.green, amount),
        lerp(from.blue, to.blue, amount),
    )
}

fn lerp(from: u8, to: u8, amount: f32) -> u8 {
    (from as f32 + (to as f32 - from as f32) * amount.clamp(0.0, 1.0)).round() as u8
}

fn rgba(color: DesignColor) -> Rgba<u8> {
    Rgba([color.red, color.green, color.blue, color.alpha])
}

fn confetti_positions() -> &'static [(f32, f32)] {
    &[
        (-0.68, -0.35),
        (-0.43, -0.65),
        (-0.12, -0.73),
        (0.24, -0.65),
        (0.62, -0.42),
        (0.72, -0.08),
        (0.55, 0.28),
        (0.3, 0.67),
        (-0.05, 0.72),
        (-0.4, 0.61),
        (-0.68, 0.32),
        (-0.74, 0.0),
        (-0.36, -0.23),
        (0.38, -0.22),
        (0.39, 0.34),
        (-0.35, 0.28),
    ]
}

fn confetti_color(index: usize) -> Color32 {
    [
        Color32::from_rgb(255, 214, 93),
        Color32::from_rgb(236, 81, 57),
        Color32::from_rgb(100, 195, 178),
        Color32::WHITE,
    ][index % 4]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_bleed_square_repeats_outer_rows() {
        let source = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });

        let square = edge_bleed_square(&source, 4, 2.0);

        assert_eq!(square.dimensions(), (4, 4));
        for x in 0..4 {
            assert_eq!(square.get_pixel(x, 0), square.get_pixel(x, 1));
            assert_eq!(square.get_pixel(x, 3), square.get_pixel(x, 2));
        }
    }

    #[test]
    fn rectangle_shape_has_three_to_two_aspect_ratio() {
        let points = shape_points(ShapeKind::Rectangle, Pos2::ZERO, 30.0);
        let bounds = Rect::from_two_pos(points[0], points[2]);

        assert_eq!(points.len(), 4);
        assert!((bounds.width() / bounds.height() - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn curved_text_layout_mirrors_top_and_bottom_arcs() {
        let widths = [20.0, 24.0, 20.0];
        let top = arc_glyphs(&widths, 0.75);
        let bottom = arc_glyphs(&widths, -0.75);

        assert!(top[0].offset.y > 0.0);
        assert!(bottom[0].offset.y < 0.0);
        for (top_glyph, bottom_glyph) in top.iter().zip(bottom) {
            assert!((top_glyph.offset.x - bottom_glyph.offset.x).abs() < f32::EPSILON);
            assert!((top_glyph.offset.y + bottom_glyph.offset.y).abs() < f32::EPSILON);
            assert!((top_glyph.angle + bottom_glyph.angle).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn export_uses_physical_size_at_300_dpi() {
        let design = Design::default();
        let path = std::env::temp_dir().join(format!(
            "button-studio-export-{}-{}.png",
            std::process::id(),
            design.diameter_mm
        ));

        let size = export_png(&design, &path).expect("export PNG");
        let image = image::open(&path).expect("open exported PNG");
        std::fs::remove_file(&path).expect("remove exported PNG");

        assert_eq!(size, 824);
        assert_eq!(image.width(), size);
        assert_eq!(image.height(), size);
    }

    #[test]
    fn project_preview_is_a_valid_png_at_the_requested_size() {
        let bytes = render_project_preview_png(&Design::default(), 512)
            .expect("render embedded project preview");
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .expect("decode embedded project preview");

        assert_eq!(image.width(), 512);
        assert_eq!(image.height(), 512);
    }

    #[test]
    fn pdf_sheet_packs_full_size_artwork_inside_page_margins() {
        let design = Design::default();
        let diameter = design.artwork_diameter_mm();
        let layout = pdf_sheet_layout(diameter);

        assert_eq!(layout.len(), 8);
        assert!((layout[1][0] - layout[0][0] - diameter - PDF_BUTTON_GAP_MM).abs() < f32::EPSILON);
        assert!((layout[2][1] - layout[0][1] - diameter - PDF_BUTTON_GAP_MM).abs() < f32::EPSILON);
        for [x, y] in layout {
            assert!(x >= PDF_MARGIN_MM);
            assert!(y >= PDF_MARGIN_MM);
            assert!(x + diameter <= A4_WIDTH_MM - PDF_MARGIN_MM);
            assert!(y + diameter <= A4_HEIGHT_MM - PDF_MARGIN_MM);
        }
    }

    #[test]
    fn pdf_sheet_export_writes_a_pdf_with_every_packed_button() {
        let design = Design::default();
        let path = std::env::temp_dir().join(format!(
            "button-studio-sheet-{}-{}.pdf",
            std::process::id(),
            design.diameter_mm
        ));

        let count = export_pdf_sheet(&design, &path).expect("export PDF sheet");
        let bytes = std::fs::read(&path).expect("read exported PDF");
        std::fs::remove_file(&path).expect("remove exported PDF");

        assert_eq!(count, 8);
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn series_pdf_places_one_csv_entry_per_button_across_pages() {
        let mut design = Design::default();
        design.elements[1].kind = ElementKind::Text {
            text: "{{name}}".to_owned(),
            font_size: 0.1,
            font: TextFont::default(),
            curvature: 0.0,
        };
        let rows = (0..9)
            .map(|index| HashMap::from([("name".to_owned(), format!("Entry {index}"))]))
            .collect::<Vec<_>>();
        let path = std::env::temp_dir().join(format!(
            "button-studio-series-{}-{}.pdf",
            std::process::id(),
            design.diameter_mm
        ));

        let summary = export_pdf_series(&design, &rows, &path).expect("export series PDF");
        let bytes = std::fs::read(&path).expect("read exported PDF");
        std::fs::remove_file(&path).expect("remove exported PDF");

        assert_eq!(summary.buttons, 9);
        assert_eq!(summary.pages, 2);
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn no_background_export_is_transparent_and_ignores_backdrop_photo() {
        let mut design = Design::default();
        design.elements.clear();
        design.background_style = BackgroundStyle::None;
        design.set_background_image("missing-backdrop".to_owned(), 1.0);

        let image = render_trimmed_badge(&design, 64, 1).expect("render transparent badge");

        assert_eq!(image.get_pixel(32, 32), &Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn raster_export_rotates_elements_around_their_center() {
        let mut design = Design::default();
        design.background_style = BackgroundStyle::None;
        design.elements.truncate(1);
        {
            let element = &mut design.elements[0];
            element.position = [0.0, 0.0];
            element.kind = ElementKind::Shape {
                shape: ShapeKind::Diamond,
                size: 0.3,
            };
        }

        let upright = render_trimmed_badge(&design, 200, 1).expect("render upright element");
        design.elements[0].rotation = 90.0;
        let rotated = render_trimmed_badge(&design, 200, 1).expect("render rotated element");

        let opaque_size = |image: &RgbaImage| {
            let opaque = image
                .enumerate_pixels()
                .filter(|(_, _, pixel)| pixel[3] > 0)
                .map(|(x, y, _)| (x, y))
                .collect::<Vec<_>>();
            let min_x = opaque.iter().map(|(x, _)| *x).min().expect("opaque pixel");
            let max_x = opaque.iter().map(|(x, _)| *x).max().expect("opaque pixel");
            let min_y = opaque.iter().map(|(_, y)| *y).min().expect("opaque pixel");
            let max_y = opaque.iter().map(|(_, y)| *y).max().expect("opaque pixel");
            (max_x - min_x + 1, max_y - min_y + 1)
        };

        let upright_size = opaque_size(&upright);
        let rotated_size = opaque_size(&rotated);
        assert!(upright_size.0 < upright_size.1);
        assert!(rotated_size.0 > rotated_size.1);
        assert!((upright_size.0 as i32 - rotated_size.1 as i32).abs() <= 2);
        assert!((upright_size.1 as i32 - rotated_size.0 as i32).abs() <= 2);

        for image in [&upright, &rotated] {
            let (left, top, right, bottom) = alpha_bounds(image).expect("visible shape pixels");
            assert!(((left + right) as f32 / 2.0 - 100.0).abs() <= 1.0);
            assert!(((top + bottom) as f32 / 2.0 - 100.0).abs() <= 1.0);
        }
    }

    #[test]
    fn raster_text_is_centered_by_visible_glyphs() {
        let mut canvas = RgbaImage::from_pixel(320, 180, Rgba([0, 0, 0, 0]));
        raster_centered_text(
            &mut canvas,
            pos2(160.0, 90.0),
            "MAKE IT YOURS",
            32.0,
            &TextFont::default(),
            Rgba([255, 255, 255, 255]),
        );

        let (left, top, right, bottom) = alpha_bounds(&canvas).expect("visible text pixels");
        assert!(((left + right) as f32 / 2.0 - 160.0).abs() <= 1.0);
        assert!(((top + bottom) as f32 / 2.0 - 90.0).abs() <= 1.0);
    }

    #[test]
    fn qr_raster_contains_quiet_zone_and_dark_modules() {
        let mut canvas = RgbaImage::from_pixel(240, 240, Rgba([0, 0, 0, 0]));
        raster_qr_code(
            &mut canvas,
            120.0,
            120.0,
            180.0,
            "https://example.com/buttons",
            DesignColor::INK,
            Some(DesignColor::WHITE),
        )
        .expect("render QR code");

        assert_eq!(canvas.get_pixel(31, 120), &Rgba([255, 255, 255, 255]));
        assert!(
            canvas
                .pixels()
                .any(|pixel| pixel == &rgba(DesignColor::INK))
        );
        assert!(
            canvas
                .pixels()
                .any(|pixel| pixel == &Rgba([255, 255, 255, 255]))
        );
    }

    #[test]
    fn qr_raster_supports_colored_and_transparent_backgrounds() {
        let underlying = Rgba([23, 87, 61, 255]);
        let mut transparent_canvas = RgbaImage::from_pixel(240, 240, underlying);
        raster_qr_code(
            &mut transparent_canvas,
            120.0,
            120.0,
            180.0,
            "https://example.com/buttons",
            DesignColor::INK,
            None,
        )
        .expect("render transparent QR code");

        let background = DesignColor::rgb(246, 190, 59);
        let mut colored_canvas = RgbaImage::from_pixel(240, 240, underlying);
        raster_qr_code(
            &mut colored_canvas,
            120.0,
            120.0,
            180.0,
            "https://example.com/buttons",
            DesignColor::INK,
            Some(background),
        )
        .expect("render colored QR code");

        assert_eq!(transparent_canvas.get_pixel(31, 120), &underlying);
        assert_eq!(colored_canvas.get_pixel(31, 120), &rgba(background));
        assert!(
            transparent_canvas
                .pixels()
                .any(|pixel| pixel == &rgba(DesignColor::INK))
        );
    }

    #[test]
    fn preview_texture_is_physically_clipped_to_a_circle() {
        let size = 192_u32;
        let image = render_preview_image(&Design::default(), size).expect("render preview");
        let center = size as f32 / 2.0;
        let trim_radius = center;

        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - center;
                let dy = y as f32 + 0.5 - center;
                if dx * dx + dy * dy > trim_radius * trim_radius {
                    assert_eq!(image.pixels[(y * size + x) as usize].a(), 0);
                }
            }
        }
        assert_eq!(
            image.pixels[(size as usize / 2) * size as usize + size as usize / 2].a(),
            255
        );
    }

    #[test]
    fn export_cover_crops_a_background_photo_inside_the_button() {
        let temporary = std::env::temp_dir();
        let background_path = temporary.join(format!(
            "button-studio-background-{}.png",
            std::process::id()
        ));
        let export_path = temporary.join(format!(
            "button-studio-photo-export-{}.png",
            std::process::id()
        ));
        RgbaImage::from_pixel(8, 4, Rgba([210, 40, 70, 255]))
            .save(&background_path)
            .expect("save background fixture");
        let mut design = Design::default();
        design.elements.clear();
        let (asset_id, aspect_ratio) = design
            .import_asset_path(&background_path)
            .expect("import background fixture");
        design.set_background_image(asset_id, aspect_ratio);
        std::fs::remove_file(&background_path).expect("remove background fixture");

        let size = export_png(&design, &export_path).expect("export photo backdrop");
        let image = image::open(&export_path)
            .expect("open photo export")
            .to_rgba8();
        std::fs::remove_file(export_path).expect("remove photo export");

        assert_eq!(
            image.get_pixel(size / 2, size / 2),
            &Rgba([210, 40, 70, 255])
        );
        assert_eq!(image.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn foreground_image_renders_after_original_file_is_deleted() {
        let image_path = std::env::temp_dir().join(format!(
            "button-studio-foreground-{}.png",
            std::process::id()
        ));
        RgbaImage::from_pixel(4, 4, Rgba([25, 180, 90, 255]))
            .save(&image_path)
            .expect("save foreground fixture");
        let mut design = Design::default();
        design.background_style = BackgroundStyle::None;
        design.elements.clear();
        let (asset_id, aspect_ratio) = design
            .import_asset_path(&image_path)
            .expect("import foreground fixture");
        design.add_image(asset_id, aspect_ratio);
        std::fs::remove_file(image_path).expect("remove foreground fixture");

        let rendered = render_trimmed_badge(&design, 64, 1).expect("render embedded foreground");

        assert_eq!(rendered.get_pixel(32, 32), &Rgba([25, 180, 90, 255]));
    }
}
