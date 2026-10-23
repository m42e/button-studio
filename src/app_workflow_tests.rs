use super::*;

fn test_app() -> ButtonStudio {
    let design = Design::default();
    let selected = design.elements.last().map(|element| element.id);
    let document = SharedDocument::new(design.clone());
    let (_, project_paths) = mpsc::channel();
    ButtonStudio {
        design,
        document,
        document_revision: 0,
        history: History::default(),
        selected,
        selected_ids: selected.into_iter().collect(),
        clipboard: Vec::new(),
        clipboard_assets: HashMap::new(),
        layer_drag_source: None,
        keep_proportions: false,
        snapping_enabled: true,
        tool: Tool::Select,
        draft_text: "YOUR TEXT".into(),
        draft_text_curvature: 0.0,
        draft_qr: "https://example.com".into(),
        series_data: None,
        align_reference: None,
        textures: TextureMap::default(),
        project_path: None,
        status: "test".into(),
        status_is_error: false,
        show_guides: true,
        canvas_interaction: None,
        resize_snapshot: Vec::new(),
        marquee_start: None,
        font_catalog: FontCatalog::discover(),
        font_search: String::new(),
        font_browser_open: false,
        font_browser_target: None,
        focus_font_search: false,
        pending_font_change: None,
        pending_project: None,
        project_paths,
        library_open: false,
        export_review_open: false,
        export_kind: ExportKind::PdfSheet,
        export_start: 1,
        export_end: 1,
        export_copies: 1,
        export_preview_entry: 1,
        skip_invalid_rows: false,
        export_preflight_cache: None,
        saved_palette: None,
        pending_library_update: None,
        library_template_name: "My button template".into(),
        help_open: false,
        confirm_clear: false,
    }
}

#[test]
fn copy_clear_paste_restores_objects_with_fresh_ids() {
    let mut app = test_app();
    let source_ids = app.design.elements.iter().map(|e| e.id).collect::<Vec<_>>();
    app.selected_ids = source_ids.clone();
    app.selected = source_ids.last().copied();
    app.copy_selected();
    app.design.elements.clear();
    let context = egui::Context::default();
    app.paste_clipboard(&context);
    assert_eq!(app.design.elements.len(), source_ids.len());
    assert!(
        app.design
            .elements
            .iter()
            .all(|e| !source_ids.contains(&e.id))
    );
}

#[test]
fn shift_selection_toggles_a_group_as_one_composition() {
    let mut app = test_app();
    crate::editor::group(&mut app.design, &[1, 2]);
    app.select_hit(Some(1), false);
    assert_eq!(app.selected_ids, vec![1, 2]);
    app.select_hit(Some(2), true);
    assert!(app.selected_ids.is_empty());
    app.select_hit(Some(2), true);
    assert_eq!(app.selected_ids, vec![1, 2]);
}

#[test]
fn invalid_personalization_is_reported_before_opening_an_export_dialog() {
    let mut app = test_app();
    app.design.elements.clear();
    app.design.add_text("{{missing_column}}".into());
    app.series_data = Some(SeriesData {
        source_name: "names.csv".into(),
        columns: vec!["name".into()],
        rows: vec![HashMap::from([("name".into(), "Ada".into())])],
    });
    assert!(!app.perform_export(ExportKind::PdfSeries, 1, 1, 1, false));
    assert!(app.status_is_error);
    assert!(app.status.contains("CSV rows: 2"));
    assert!(!app.perform_export(ExportKind::PdfSeries, 1, 1, 1, true));
    assert!(app.status_is_error);
}

#[test]
fn focus_state_prevents_global_shortcuts_from_mutating_selection_or_tools() {
    let mut app = test_app();
    let context = egui::Context::default();
    let selected = app.selected;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1120.0, 720.0),
        )),
        modifiers: egui::Modifiers::COMMAND,
        events: vec![egui::Event::Key {
            key: egui::Key::D,
            physical_key: Some(egui::Key::D),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        ..Default::default()
    };
    let before = app.design.clone();
    let _ = context.run_ui(input, |ui| {
        let mut text = String::new();
        let response = ui.text_edit_singleline(&mut text);
        response.request_focus();
        app.handle_shortcuts(ui.ctx());
    });
    assert!(app.tool == Tool::Select);
    assert_eq!(app.selected, selected);
    assert_eq!(app.design, before);
}

#[test]
fn selected_ids_reconcile_after_add_delete_or_document_replacement() {
    let mut app = test_app();
    app.selected_ids = vec![u64::MAX];
    let new_id = app.design.add_shape(ShapeKind::Circle);
    app.selected = Some(new_id);
    app.sync_selection_ids();
    assert_eq!(app.selected_ids, vec![new_id]);
    app.design.delete(new_id);
    app.sync_selection_ids();
    assert!(app.selected_ids.is_empty());
}

#[test]
fn locked_objects_ignore_bulk_numeric_transform() {
    let mut app = test_app();
    let id = app.selected.unwrap();
    let element = app.design.element_mut(id).unwrap();
    element.locked = true;
    let before = crate::editor::element_dimensions_ratio(element);
    crate::editor::set_element_dimensions(element, before.0 * 2.0, before.1 * 2.0, false);
    assert_eq!(crate::editor::element_dimensions_ratio(element), before);
}

#[test]
fn editor_panels_and_discoverability_windows_render_at_minimum_size() {
    let mut app = test_app();
    let context = egui::Context::default();
    crate::theme::install(&context);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1120.0, 720.0),
        )),
        ..Default::default()
    };
    for tool in Tool::ALL {
        app.tool = tool;
        let _ = context.run_ui(input.clone(), |ui| {
            app.top_bar(ui);
            app.tool_panel(ui);
            app.right_panel(ui);
            app.canvas(ui);
            app.library_open = true;
            app.export_review_open = true;
            app.help_open = true;
            app.library_window(ui.ctx());
            app.export_review_window(ui.ctx());
            app.help_window(ui.ctx());
        });
    }
}
