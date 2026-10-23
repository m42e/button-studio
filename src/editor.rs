//! Reusable editing operations shared by the canvas and tests.
use crate::model::{
    Design, ELEMENT_POSITION_LIMIT, Element, ElementKind, IMAGE_WIDTH_LIMIT, ShapeKind,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalBounds {
    pub x_mm: f32,
    pub y_mm: f32,
    pub width_mm: f32,
    pub height_mm: f32,
}

pub fn physical_bounds(design: &Design, element: &Element) -> PhysicalBounds {
    let d = design.artwork_diameter_mm();
    let (w, h) = element_dimensions_ratio(element);
    PhysicalBounds {
        x_mm: (element.position[0] + 1.0) * d * 0.5,
        y_mm: (element.position[1] + 1.0) * d * 0.5,
        width_mm: w * d,
        height_mm: h * d,
    }
}

pub fn element_dimensions_ratio(element: &Element) -> (f32, f32) {
    let mut unrotated = element.clone();
    unrotated.position = [0.0, 0.0];
    unrotated.rotation = 0.0;
    let rect = crate::render::normalized_element_bounds(&unrotated);
    (
        (rect.width() * 0.5).max(0.02),
        (rect.height() * 0.5).max(0.02),
    )
}

pub fn dimensions_ratio(kind: &ElementKind) -> (f32, f32) {
    match kind {
        ElementKind::Text {
            font_size, text, ..
        } => (
            *font_size * (text.chars().count().max(1) as f32 * 0.58),
            *font_size * 1.25,
        ),
        ElementKind::Shape {
            shape,
            size,
            width,
            height,
        } => (
            width.unwrap_or(*size),
            height.unwrap_or(if *shape == ShapeKind::Rectangle {
                *size * 2.0 / 3.0
            } else {
                *size
            }),
        ),
        ElementKind::Image {
            width,
            aspect_ratio,
            ..
        } => (*width, *width / aspect_ratio.max(0.01)),
        ElementKind::QrCode { size, .. } => (*size, *size),
    }
}

pub fn set_physical_bounds(
    design: &Design,
    element: &mut Element,
    bounds: PhysicalBounds,
    keep_ratio: bool,
) {
    if element.locked {
        return;
    }
    let d = design.artwork_diameter_mm().max(1.0);
    element.position = [bounds.x_mm * 2.0 / d - 1.0, bounds.y_mm * 2.0 / d - 1.0];
    set_element_dimensions(
        element,
        bounds.width_mm / d,
        bounds.height_mm / d,
        keep_ratio,
    );
    clamp_position(&mut element.position);
}

pub fn set_element_dimensions(
    element: &mut Element,
    requested_w: f32,
    requested_h: f32,
    keep_ratio: bool,
) {
    if element.locked {
        return;
    }
    let (old_w, old_h) = element_dimensions_ratio(element);
    let mut w = requested_w.max(0.02);
    let mut h = requested_h.max(0.02);
    if keep_ratio
        || matches!(
            element.kind,
            ElementKind::Text { .. } | ElementKind::QrCode { .. }
        )
    {
        let sx = w / old_w.max(0.02);
        let sy = h / old_h.max(0.02);
        let scale = if (sx - 1.0).abs() >= (sy - 1.0).abs() {
            sx
        } else {
            sy
        };
        w = old_w * scale;
        h = old_h * scale;
    }
    match &mut element.kind {
        ElementKind::Text { font_size, .. } => {
            *font_size = (*font_size * (w / old_w.max(0.02))).clamp(0.025, 0.24)
        }
        ElementKind::Shape {
            size,
            width,
            height,
            ..
        } => {
            *size = w.max(h).clamp(0.05, 0.8);
            *width = Some(w.clamp(0.02, 2.0));
            *height = Some(h.clamp(0.02, 2.0));
        }
        ElementKind::Image { width, .. } => {
            *width = w.clamp(0.05, IMAGE_WIDTH_LIMIT);
            element.appearance.image_frame_aspect = Some((w / h.max(0.01)).clamp(0.01, 100.0));
        }
        ElementKind::QrCode { size, .. } => *size = w.max(h).clamp(0.08, 1.2),
    }
}

pub fn clamp_position(position: &mut [f32; 2]) {
    let length = position[0].hypot(position[1]);
    if length > ELEMENT_POSITION_LIMIT {
        position[0] *= ELEMENT_POSITION_LIMIT / length;
        position[1] *= ELEMENT_POSITION_LIMIT / length;
    }
}

pub fn center_on_artwork(element: &mut Element) {
    if !element.locked {
        element.position = [0.0, 0.0];
    }
}

pub fn group(design: &mut Design, ids: &[u64]) -> Option<u64> {
    let members = selection_ids(design, ids);
    if members.len() < 2 {
        return None;
    }
    let group_id = *members.iter().min()?;
    for id in members {
        if let Some(el) = design.element_mut(id) {
            el.group_id = Some(group_id);
        }
    }
    Some(group_id)
}

pub fn ungroup(design: &mut Design, ids: &[u64]) {
    for id in selection_ids(design, ids) {
        if let Some(el) = design.element_mut(id) {
            el.group_id = None;
        }
    }
}

pub fn selection_ids(design: &Design, ids: &[u64]) -> Vec<u64> {
    let mut out = Vec::new();
    for id in ids {
        let Some(el) = design.element(*id) else {
            continue;
        };
        if let Some(group) = el.group_id {
            for member in design
                .elements
                .iter()
                .filter(|other| other.group_id == Some(group))
                .map(|e| e.id)
            {
                if !out.contains(&member) {
                    out.push(member);
                }
            }
        } else if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

pub fn move_selection(design: &mut Design, ids: &[u64], delta: [f32; 2]) {
    let movable = selection_ids(design, ids)
        .into_iter()
        .filter_map(|id| {
            design
                .element(id)
                .filter(|e| !e.locked)
                .map(|e| (id, e.position))
        })
        .collect::<Vec<_>>();
    let scale = movement_scale(&movable, delta);
    for (id, _) in movable {
        if let Some(el) = design.element_mut(id) {
            el.position[0] += delta[0] * scale;
            el.position[1] += delta[1] * scale;
        }
    }
}

/// Place a dragged selection from its original pointer-down positions. This
/// keeps snap corrections from accumulating across pointer events.
pub fn move_selection_from_snapshot(
    design: &mut Design,
    snapshot: &[Element],
    delta: [f32; 2],
) -> bool {
    let movable = snapshot
        .iter()
        .filter_map(|original| {
            design
                .element(original.id)
                .filter(|element| !element.locked)
                .map(|_| (original.id, original.position))
        })
        .collect::<Vec<_>>();
    if movable.is_empty() {
        return false;
    }

    let scale = movement_scale(&movable, delta);
    for (id, position) in movable {
        if let Some(element) = design.element_mut(id) {
            element.position = [
                position[0] + delta[0] * scale,
                position[1] + delta[1] * scale,
            ];
        }
    }
    true
}

fn movement_scale(movable: &[(u64, [f32; 2])], delta: [f32; 2]) -> f32 {
    if movable.is_empty() {
        return 0.0;
    }
    let valid = |scale: f32| {
        movable.iter().all(|(_, position)| {
            (position[0] + delta[0] * scale).hypot(position[1] + delta[1] * scale)
                <= ELEMENT_POSITION_LIMIT
        })
    };
    if valid(1.0) {
        return 1.0;
    }

    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..24 {
        let middle = (low + high) * 0.5;
        if valid(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    low * (1.0 - 1e-6)
}

pub fn duplicate_selection(design: &mut Design, ids: &[u64]) -> Vec<u64> {
    let source = selection_ids(design, ids);
    let mut created = Vec::new();
    let mut group_mapping = std::collections::HashMap::new();
    for id in source {
        let old_group = design.element(id).and_then(|element| element.group_id);
        if let Some(new_id) = design.duplicate(id) {
            if let Some(el) = design.element_mut(new_id) {
                el.group_id = old_group.map(|group| *group_mapping.entry(group).or_insert(new_id));
            }
            created.push(new_id);
        }
    }
    created
}

pub fn delete_selection(design: &mut Design, ids: &[u64]) {
    for id in selection_ids(design, ids) {
        if design.element(id).is_some_and(|e| !e.locked) {
            design.delete(id);
        }
    }
}

pub fn paste_elements(
    design: &mut Design,
    templates: &[Element],
    assets: &std::collections::HashMap<String, crate::model::ImageAsset>,
) -> Vec<u64> {
    design.assets.extend(assets.clone());
    let mut ids = Vec::new();
    let mut groups = std::collections::HashMap::new();
    for template in templates {
        let mut element = template.clone();
        let old_group = element.group_id;
        element.position[0] += 0.06;
        element.position[1] += 0.06;
        let id = design.insert_element(element);
        if let Some(element) = design.element_mut(id) {
            element.group_id = old_group.map(|group| *groups.entry(group).or_insert(id));
        }
        ids.push(id);
    }
    ids
}

pub fn snap_coordinate(
    value: f32,
    candidates: &[f32],
    tolerance: f32,
    enabled: bool,
) -> Option<f32> {
    if !enabled {
        return None;
    }
    candidates
        .iter()
        .copied()
        .filter(|candidate| (value - candidate).abs() <= tolerance)
        .min_by(|a, b| (value - *a).abs().total_cmp(&(value - *b).abs()))
}

pub fn snap_bounds_axis(
    min: f32,
    center: f32,
    max: f32,
    candidates: &[f32],
    tolerance: f32,
) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for point in [min, center, max] {
        for guide in candidates {
            let delta = *guide - point;
            if delta.abs() <= tolerance && best.is_none_or(|(old, _)| delta.abs() < old.abs()) {
                best = Some((delta, *guide));
            }
        }
    }
    best
}

pub fn distribute_centers(design: &mut Design, ids: &[u64], horizontal: bool) -> bool {
    let mut items: Vec<_> = selection_ids(design, ids)
        .into_iter()
        .filter_map(|id| {
            let e = design.element(id)?;
            (!e.locked).then_some((id, e.position[if horizontal { 0 } else { 1 }]))
        })
        .collect();
    if items.len() < 3 {
        return false;
    }
    items.sort_by(|a, b| a.1.total_cmp(&b.1));
    let first = items.first().unwrap().1;
    let last = items.last().unwrap().1;
    let step = (last - first) / (items.len() - 1) as f32;
    for (i, (id, _)) in items.iter().enumerate().skip(1).take(items.len() - 2) {
        if let Some(e) = design.element_mut(*id) {
            e.position[if horizontal { 0 } else { 1 }] = first + step * i as f32;
        }
    }
    true
}

pub fn scale_selection(design: &mut Design, ids: &[u64], factor: f32) -> bool {
    let items = selection_ids(design, ids)
        .into_iter()
        .filter_map(|id| {
            design
                .element(id)
                .filter(|e| !e.locked)
                .map(|e| (id, e.position))
        })
        .collect::<Vec<_>>();
    if items.is_empty() || !factor.is_finite() || factor <= 0.0 {
        return false;
    }
    let center = [
        items.iter().map(|(_, p)| p[0]).sum::<f32>() / items.len() as f32,
        items.iter().map(|(_, p)| p[1]).sum::<f32>() / items.len() as f32,
    ];
    for (id, p) in items {
        if let Some(e) = design.element_mut(id) {
            e.position = [
                center[0] + (p[0] - center[0]) * factor,
                center[1] + (p[1] - center[1]) * factor,
            ];
            clamp_position(&mut e.position);
            match &mut e.kind {
                ElementKind::Text { font_size, .. } => {
                    *font_size = (*font_size * factor).clamp(0.025, 0.24)
                }
                ElementKind::Shape {
                    size,
                    width,
                    height,
                    ..
                } => {
                    *size = (*size * factor).clamp(0.05, 0.8);
                    if let Some(w) = width {
                        *w = (*w * factor).clamp(0.02, 2.0);
                    }
                    if let Some(h) = height {
                        *h = (*h * factor).clamp(0.02, 2.0);
                    }
                }
                ElementKind::Image { width, .. } => {
                    *width = (*width * factor).clamp(0.05, IMAGE_WIDTH_LIMIT)
                }
                ElementKind::QrCode { size, .. } => *size = (*size * factor).clamp(0.08, 1.2),
            }
        }
    }
    true
}

pub fn rotate_selection(design: &mut Design, ids: &[u64], degrees: f32) -> bool {
    let items = selection_ids(design, ids)
        .into_iter()
        .filter_map(|id| {
            design
                .element(id)
                .filter(|e| !e.locked)
                .map(|e| (id, e.position))
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return false;
    }
    let center = [
        items.iter().map(|(_, p)| p[0]).sum::<f32>() / items.len() as f32,
        items.iter().map(|(_, p)| p[1]).sum::<f32>() / items.len() as f32,
    ];
    let a = degrees.to_radians();
    let (sin, cos) = a.sin_cos();
    for (id, p) in items {
        if let Some(e) = design.element_mut(id) {
            let x = p[0] - center[0];
            let y = p[1] - center[1];
            e.position = [center[0] + x * cos - y * sin, center[1] + x * sin + y * cos];
            clamp_position(&mut e.position);
            e.rotation = (e.rotation + degrees + 180.0).rem_euclid(360.0) - 180.0;
        }
    }
    true
}

/// Capture the gesture's original state so repeated pointer events never compound
/// rounding or clamp one member independently of the rest of a composition.
pub fn resize_snapshot(design: &Design, ids: &[u64]) -> Vec<Element> {
    selection_ids(design, ids)
        .into_iter()
        .filter_map(|id| {
            design
                .element(id)
                .filter(|element| !element.locked)
                .cloned()
        })
        .collect()
}

pub fn composition_bounds(elements: &[Element]) -> Option<eframe::egui::Rect> {
    elements
        .iter()
        .map(crate::render::normalized_element_bounds)
        .reduce(|bounds, next| bounds.union(next))
}

pub fn resize_from_snapshot(design: &mut Design, snapshot: &[Element], requested: f32) -> bool {
    let Some(bounds) = composition_bounds(snapshot) else {
        return false;
    };
    if !requested.is_finite() || requested <= 0.0 {
        return false;
    }
    let pivot = bounds.center();
    let (mut minimum, mut maximum) = (0.0_f32, f32::INFINITY);
    for element in snapshot {
        let dimensions = match &element.kind {
            ElementKind::Text { font_size, .. } => vec![(*font_size, 0.025, 0.24)],
            ElementKind::Shape { .. } => {
                let (w, h) = dimensions_ratio(&element.kind);
                vec![(w, 0.02, 2.0), (h, 0.02, 2.0)]
            }
            ElementKind::Image { width, .. } => vec![(*width, 0.05, IMAGE_WIDTH_LIMIT)],
            ElementKind::QrCode { size, .. } => vec![(*size, 0.08, 1.2)],
        };
        for (size, low, high) in dimensions {
            minimum = minimum.max(low / size.max(f32::EPSILON));
            maximum = maximum.min(high / size.max(f32::EPSILON));
        }
    }
    if minimum > maximum {
        return false;
    }
    let mut factor = requested.clamp(minimum, maximum);
    let positions_fit = |scale: f32| {
        snapshot.iter().all(|element| {
            let x = pivot.x + (element.position[0] - pivot.x) * scale;
            let y = pivot.y + (element.position[1] - pivot.y) * scale;
            x.hypot(y) <= ELEMENT_POSITION_LIMIT + 1e-6
        })
    };
    if positions_fit(1.0) && !positions_fit(factor) {
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..24 {
            let fraction = (low + high) * 0.5;
            if positions_fit(1.0 + (factor - 1.0) * fraction) {
                low = fraction;
            } else {
                high = fraction;
            }
        }
        factor = 1.0 + (factor - 1.0) * low;
    }
    let mut changed = false;
    for original in snapshot {
        let Some(element) = design
            .element_mut(original.id)
            .filter(|element| !element.locked)
        else {
            continue;
        };
        let mut next = original.clone();
        next.position = [
            pivot.x + (original.position[0] - pivot.x) * factor,
            pivot.y + (original.position[1] - pivot.y) * factor,
        ];
        match &mut next.kind {
            ElementKind::Text { font_size, .. } => *font_size *= factor,
            ElementKind::Shape {
                size,
                width,
                height,
                ..
            } => {
                let (w, h) = dimensions_ratio(&original.kind);
                *size = (*size * factor).clamp(0.05, 0.8);
                *width = Some(w * factor);
                *height = Some(h * factor);
            }
            ElementKind::Image { width, .. } => *width *= factor,
            ElementKind::QrCode { size, .. } => *size *= factor,
        }
        next.appearance.outline_width = (original.appearance.outline_width * factor).min(0.25);
        next.appearance.corner_radius = (original.appearance.corner_radius * factor).min(0.5);
        changed |= *element != next;
        *element = next;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_round_trip_and_proportions() {
        let d = Design::default();
        let mut el = d.elements[0].clone();
        let mut b = physical_bounds(&d, &el);
        b.width_mm = 20.;
        b.height_mm = 20.;
        set_physical_bounds(&d, &mut el, b, true);
        let (w, h) = dimensions_ratio(&el.kind);
        assert!((w - h).abs() < 0.001);
        assert!((el.position[0] - d.elements[0].position[0]).abs() < 0.001);
    }
    #[test]
    fn dimension_setter_scales_measured_text_and_freeforms_image_frame() {
        let d = Design::default();
        let mut text = d.elements[1].clone();
        if let ElementKind::Text { curvature, .. } = &mut text.kind {
            *curvature = 0.55;
        }
        let (tw, th) = element_dimensions_ratio(&text);
        let font_before = match &text.kind {
            ElementKind::Text { font_size, .. } => *font_size,
            _ => 0.,
        };
        set_element_dimensions(&mut text, tw * 1.4, th * 1.4, false);
        let font_after = match &text.kind {
            ElementKind::Text { font_size, .. } => *font_size,
            _ => 0.,
        };
        assert!(font_after > font_before);
        let (new_w, new_h) = element_dimensions_ratio(&text);
        assert!((new_w / tw - 1.4).abs() < 0.03);
        assert!((new_h / th - 1.4).abs() < 0.03);
        let mut image = d.clone();
        let image_id = image.add_image("asset".into(), 2.0);
        let mut el = image.element(image_id).unwrap().clone();
        el.appearance.image_fit = crate::appearance::ImageFit::Cover;
        set_element_dimensions(&mut el, 0.6, 0.3, false);
        let (iw, ih) = element_dimensions_ratio(&el);
        assert!((iw - 0.6).abs() < 0.02);
        assert!((ih - 0.3).abs() < 0.02);
        assert_eq!(el.appearance.image_frame_aspect, Some(2.0));
    }
    #[test]
    fn physical_coordinates_map_center_and_edges_to_artwork() {
        let d = Design::default();
        let mut el = d.elements[0].clone();
        let dia = d.artwork_diameter_mm();
        el.position = [0.0, 0.0];
        let b = physical_bounds(&d, &el);
        assert!((b.x_mm - dia * 0.5).abs() < 0.001);
        assert!((b.y_mm - dia * 0.5).abs() < 0.001);
        el.position = [-1.0, 1.0];
        let b = physical_bounds(&d, &el);
        assert!(b.x_mm.abs() < 0.001);
        assert!((b.y_mm - dia).abs() < 0.001);
    }
    #[test]
    fn circle_dimensions_are_reported_as_diameter_fraction_and_physical_size() {
        let mut d = Design::default();
        let id = d.add_shape(ShapeKind::Circle);
        if let ElementKind::Shape {
            size,
            width,
            height,
            ..
        } = &mut d.element_mut(id).unwrap().kind
        {
            *size = 0.2;
            *width = None;
            *height = None;
        }
        let el = d.element(id).unwrap();
        let (w, h) = element_dimensions_ratio(el);
        assert!((w - 0.2).abs() < 0.002);
        assert!((h - 0.2).abs() < 0.002);
        let physical = physical_bounds(&d, el);
        assert!((physical.width_mm - 0.2 * d.artwork_diameter_mm()).abs() < 0.02);
    }
    #[test]
    fn object_metadata_round_trips_and_legacy_fields_default() {
        let mut d = Design::default();
        d.elements[0].name_override = Some("Logo".into());
        d.elements[0].locked = true;
        d.elements[0].group_id = Some(777);
        d.elements[0].flip_x = true;
        d.elements[1].group_id = Some(777);
        let bytes = d.to_project_json().unwrap();
        let restored = Design::from_project_json(&bytes, None).unwrap();
        assert_eq!(restored.elements[0].name_override.as_deref(), Some("Logo"));
        assert!(restored.elements[0].locked);
        assert_eq!(restored.elements[0].group_id, Some(777));
        assert!(restored.elements[0].flip_x);
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        for element in value["elements"].as_array_mut().unwrap() {
            for field in ["name_override", "locked", "group_id", "flip_x", "flip_y"] {
                element.as_object_mut().unwrap().remove(field);
            }
        }
        let legacy = Design::from_project_json(&serde_json::to_vec(&value).unwrap(), None).unwrap();
        assert_eq!(legacy.elements[0].name_override, None);
        assert!(!legacy.elements[0].locked);
        assert_eq!(legacy.elements[0].group_id, None);
    }
    #[test]
    fn paste_allocates_fresh_ids_and_preserves_copied_group_after_clear() {
        let mut source = Design::default();
        let ids = [source.elements[0].id, source.elements[1].id];
        group(&mut source, &ids);
        let clipboard = source.elements.clone();
        let mut target = Design::default();
        target.elements.clear();
        let pasted = paste_elements(&mut target, &clipboard, &std::collections::HashMap::new());
        assert_eq!(pasted.len(), 2);
        assert_ne!(pasted[0], pasted[1]);
        assert_eq!(
            target.element(pasted[0]).unwrap().group_id,
            target.element(pasted[1]).unwrap().group_id
        );
        let position = target.element(pasted[0]).unwrap().position;
        assert!((position[0] - 0.06).abs() < 1e-6 && (position[1] + 0.12).abs() < 1e-6);
    }
    #[test]
    fn paste_carries_embedded_image_assets_across_projects() {
        let mut source = Design::default();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let (asset_id, aspect) = source.import_asset(bytes.into_inner(), None).unwrap();
        let id = source.add_image(asset_id.clone(), aspect);
        let clipboard = vec![source.element(id).unwrap().clone()];
        let assets = [(asset_id.clone(), source.assets[&asset_id].clone())]
            .into_iter()
            .collect();
        let mut target = Design::default();
        target.elements.clear();
        let pasted = paste_elements(&mut target, &clipboard, &assets);
        assert_eq!(pasted.len(), 1);
        assert!(target.asset(&asset_id).is_ok());
        assert!(target.validate_assets().is_ok());
    }
    #[test]
    fn grouping_expands_multi_selection_and_locked_move_is_ignored() {
        let mut d = Design::default();
        let ids = [d.elements[0].id, d.elements[1].id];
        let g = group(&mut d, &ids).unwrap();
        assert_eq!(selection_ids(&d, &[ids[0]]), ids);
        assert_eq!(d.element(ids[1]).unwrap().group_id, Some(g));
        d.element_mut(ids[0]).unwrap().locked = true;
        move_selection(&mut d, &ids, [0.2, 0.]);
        assert_eq!(d.element(ids[0]).unwrap().position[0], 0.0);
        assert!(d.element(ids[1]).unwrap().position[0] > 0.0);
    }
    #[test]
    fn layer_reorder_places_source_at_target_stack_position() {
        let mut d = Design::default();
        let ids = [d.elements[0].id, d.elements[1].id];
        assert!(d.reorder_layer(ids[0], ids[1]));
        assert_eq!(d.elements.last().unwrap().id, ids[0]);
        assert!(!d.reorder_layer(ids[0], ids[0]));
        assert!(d.reorder_layer(ids[0], ids[1]));
        assert_eq!(d.elements.first().unwrap().id, ids[0]);
    }
    #[test]
    fn bulk_move_clamps_by_one_delta_to_preserve_spacing() {
        let mut d = Design::default();
        d.elements[0].position = [0.8, 0.0];
        d.elements[1].position = [0.6, 0.0];
        move_selection(&mut d, &[1, 2], [0.5, 0.0]);
        let a = d.element(1).unwrap().position[0];
        let b = d.element(2).unwrap().position[0];
        assert!((a - b - 0.2).abs() < 0.001);
        assert!(a <= 1.0);
    }
    #[test]
    fn duplicate_delete_selection_are_bulk_and_undoable_by_design_snapshot() {
        let mut d = Design::default();
        let ids = [1, 2];
        let previous = d.clone();
        let copies = duplicate_selection(&mut d, &ids);
        assert_eq!(copies.len(), 2);
        assert_eq!(d.elements.len(), 4);
        d = previous;
        delete_selection(&mut d, &ids);
        assert_eq!(d.elements.len(), 0);
    }
    #[test]
    fn snapping_can_be_disabled_and_distributes_evenly() {
        assert_eq!(
            snap_coordinate(0.02, &[-0.5, 0.0, 0.4], 0.05, true),
            Some(0.0)
        );
        assert_eq!(snap_coordinate(0.02, &[0.0], 0.05, false), None);
        let mut d = Design::default();
        let id = d.add_shape(ShapeKind::Circle);
        d.elements[0].position[0] = -0.5;
        d.elements[1].position[0] = 0.0;
        d.element_mut(id).unwrap().position[0] = 0.5;
        assert!(distribute_centers(&mut d, &[1, 2, id], true));
        assert!(d.element(2).unwrap().position[0].abs() < 0.001);
        d.element_mut(1).unwrap().position[1] = -0.5;
        d.element_mut(2).unwrap().position[1] = 0.4;
        d.element_mut(id).unwrap().position[1] = 0.5;
        assert!(distribute_centers(&mut d, &[1, 2, id], false));
        assert!((d.element(2).unwrap().position[1] - 0.0).abs() < 0.001);
        assert_eq!(
            snap_bounds_axis(-0.2, 0.0, 0.2, &[0.5], 0.31),
            Some((0.3, 0.5))
        );
    }

    fn small_shape(design: &mut Design, x: f32) -> u64 {
        let id = design.add_shape(ShapeKind::Rectangle);
        let element = design.element_mut(id).unwrap();
        element.position = [x, 0.0];
        if let ElementKind::Shape {
            size,
            width,
            height,
            ..
        } = &mut element.kind
        {
            *size = 0.12;
            *width = Some(0.12);
            *height = Some(0.08);
        }
        id
    }

    #[test]
    fn composition_resize_applies_one_common_scale_factor_to_members() {
        let mut design = Design::default();
        design.elements.clear();
        let a = small_shape(&mut design, -0.25);
        let b = small_shape(&mut design, 0.35);
        let snapshot = resize_snapshot(&design, &[a, b]);
        let before_distance =
            design.element(a).unwrap().position[0] - design.element(b).unwrap().position[0];
        let before_a = element_dimensions_ratio(design.element(a).unwrap()).0;
        let before_b = element_dimensions_ratio(design.element(b).unwrap()).0;

        assert!(resize_from_snapshot(&mut design, &snapshot, 1.5));

        let after_distance =
            design.element(a).unwrap().position[0] - design.element(b).unwrap().position[0];
        let after_a = element_dimensions_ratio(design.element(a).unwrap()).0;
        let after_b = element_dimensions_ratio(design.element(b).unwrap()).0;
        let factor = after_distance / before_distance;
        assert!((factor - 1.5).abs() < 0.002);
        assert!((after_a / before_a - factor).abs() < 0.002);
        assert!((after_b / before_b - factor).abs() < 0.002);
    }

    #[test]
    fn composition_resize_snapshot_is_deterministic_and_ignores_locked_objects() {
        let mut design = Design::default();
        design.elements.clear();
        let a = small_shape(&mut design, -0.2);
        let b = small_shape(&mut design, 0.2);
        let locked = small_shape(&mut design, 0.9);
        design.element_mut(locked).unwrap().locked = true;
        let locked_before = design.element(locked).unwrap().clone();
        let snapshot = resize_snapshot(&design, &[a, b, locked]);
        assert_eq!(
            snapshot
                .iter()
                .map(|element| element.id)
                .collect::<Vec<_>>(),
            [a, b]
        );

        assert!(resize_from_snapshot(&mut design, &snapshot, 1.4));
        let first_result = design.clone();
        assert!(!resize_from_snapshot(&mut design, &snapshot, 1.4));
        assert_eq!(
            design, first_result,
            "same pointer snapshot must not compound resizing"
        );
        assert_eq!(design.element(locked).unwrap(), &locked_before);
    }

    #[test]
    fn composition_resize_clamps_near_edge_members_with_shared_factor() {
        let mut design = Design::default();
        design.elements.clear();
        let a = small_shape(&mut design, 0.66);
        let b = small_shape(&mut design, 0.91);
        let snapshot = resize_snapshot(&design, &[a, b]);
        let before_delta =
            design.element(b).unwrap().position[0] - design.element(a).unwrap().position[0];
        assert!(resize_from_snapshot(&mut design, &snapshot, 2.0));
        let a_after = design.element(a).unwrap().position;
        let b_after = design.element(b).unwrap().position;
        let factor = (b_after[0] - a_after[0]) / before_delta;
        assert!(factor < 2.0 && factor > 1.0);
        assert!(a_after[0].hypot(a_after[1]) <= 1.000001);
        assert!(b_after[0].hypot(b_after[1]) <= 1.000001);
    }

    #[test]
    fn resizing_image_frame_keeps_source_ratio_and_embedded_asset() {
        let mut design = Design::default();
        design.elements.clear();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(3, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let (asset_id, source_aspect) = design.import_asset(png.into_inner(), None).unwrap();
        let image_id = design.add_image(asset_id.clone(), source_aspect);
        design
            .element_mut(image_id)
            .unwrap()
            .appearance
            .image_frame_aspect = Some(1.25);
        let assets_before = design.assets.clone();
        let snapshot = resize_snapshot(&design, &[image_id]);

        assert!(resize_from_snapshot(&mut design, &snapshot, 1.5));
        let ElementKind::Image {
            width,
            aspect_ratio,
            asset_id: after_asset,
            ..
        } = &design.element(image_id).unwrap().kind
        else {
            panic!("image layer expected")
        };
        let ElementKind::Image {
            width: original_width,
            ..
        } = &snapshot[0].kind
        else {
            panic!("image snapshot expected")
        };
        assert!((*width - *original_width * 1.5).abs() < 0.002);
        assert!((*aspect_ratio - source_aspect).abs() < f32::EPSILON);
        assert_eq!(after_asset, &asset_id);
        assert_eq!(
            design
                .element(image_id)
                .unwrap()
                .appearance
                .image_frame_aspect,
            Some(1.25)
        );
        assert_eq!(design.assets, assets_before);
    }
}
