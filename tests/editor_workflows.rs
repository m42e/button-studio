use button_studio::{
    appearance::{ImageFit, TextAlignment},
    editor,
    model::{BackgroundStyle, Design, DesignColor, ElementKind, History, ShapeKind},
    production::{self, IssueSeverity},
    render, svg_import,
};

#[test]
fn imported_artwork_and_styled_group_survive_save_undo_and_export() {
    let mut design = Design::default();
    design.elements.clear();
    design.background_style = BackgroundStyle::None;
    let png = svg_import::rasterize_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="blue"/></svg>"#).unwrap();
    let (asset_id, ratio) = design.import_asset(png, None).unwrap();
    let image_id = design.add_image(asset_id, ratio);
    let shape_id = design.add_shape(ShapeKind::Rectangle);
    let text_id = design.add_text("FIRST\nSECOND".into());
    let image = design.element_mut(image_id).unwrap();
    image.flip_x = true;
    image.appearance.image_fit = ImageFit::Cover;
    image.appearance.crop_pan = [0.5, 0.0];
    let shape = design.element_mut(shape_id).unwrap();
    shape.name_override = Some("Label background".into());
    shape.appearance.fill = Some(DesignColor::WHITE);
    shape.appearance.outline = Some(DesignColor::INK);
    shape.appearance.outline_width = 0.005;
    shape.appearance.corner_radius = 0.025;
    let text = design.element_mut(text_id).unwrap();
    text.color = DesignColor::INK;
    text.appearance.text_alignment = TextAlignment::Start;
    text.appearance.line_spacing = 1.5;
    text.appearance.letter_spacing = 2.0;
    let ids = [image_id, shape_id, text_id];
    editor::group(&mut design, &ids).unwrap();
    let before_move = design.clone();
    let mut history = History::default();
    history.record(before_move.clone());
    editor::move_selection(&mut design, &[text_id], [0.1, -0.1]);
    assert!(
        ids.iter()
            .all(|id| design.element(*id).unwrap().position == [0.1, -0.1])
    );
    assert!(history.undo(&mut design));
    assert_eq!(design, before_move);
    assert!(history.redo(&mut design));

    let preview = render::render_project_preview_png(&design, 192).unwrap();
    let project = design
        .to_project_json_with_preview(Some((&preview, 192, 192)))
        .unwrap();
    let restored = Design::from_project_json(&project, None).unwrap();
    assert_eq!(restored, design);
    assert_eq!(
        render::render_project_preview_png(&restored, 192).unwrap(),
        preview
    );
    assert!(
        !production::preflight_design(&restored)
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Error)
    );
}

#[test]
fn personalized_rows_keep_editable_source_and_render_distinct_artwork() {
    use std::collections::HashMap;
    let mut design = Design::default();
    design.elements.clear();
    design.background_style = BackgroundStyle::Solid;
    let id = design.add_text("{{name}}".into());
    if let ElementKind::Text { font_size, .. } = &mut design.element_mut(id).unwrap().kind {
        *font_size = 0.08;
    }
    let rows = vec![
        HashMap::from([("name".into(), "ADA".into())]),
        HashMap::from([("name".into(), "LINUS".into())]),
    ];
    let source = design.clone();
    let reports = production::preflight_series(&design, &rows, 1);
    assert!(reports.iter().all(|row| {
        row.issues
            .iter()
            .all(|issue| issue.code != "unresolved_variable")
    }));
    let first = render::render_project_preview_png(&design.personalized(&rows[0]), 128).unwrap();
    let second = render::render_project_preview_png(&design.personalized(&rows[1]), 128).unwrap();
    assert_ne!(first, second);
    assert_eq!(design, source);
    assert_eq!(
        production::select_rows(&rows, 2, 2, 2).unwrap(),
        vec![rows[1].clone(), rows[1].clone()]
    );
}
