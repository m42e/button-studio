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
        move_snapshot: Vec::new(),
        marquee_start: None,
        font_catalog: FontCatalog::discover(),
        font_search: String::new(),
        font_browser_family: String::new(),
        font_browser_weight: FontWeight::Regular,
        font_browser_slant: FontSlant::Normal,
        font_preview_key: None,
        font_preview_texture: None,
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

fn temp_dir() -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("button-studio-app-{unique}"));
    std::fs::create_dir_all(&path).unwrap();
    path
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

#[test]
fn layer_commands_grouping_style_paste_and_history_keep_selection_consistent() {
    let mut app = test_app();
    let initial_count = app.design.elements.len();
    app.selected = Some(2);
    app.selected_ids = vec![2];
    app.duplicate_selected();
    assert_eq!(app.design.elements.len(), initial_count + 1);
    assert!(app.selected.is_some_and(|id| id > 2));
    app.undo();
    assert_eq!(app.design.elements.len(), initial_count);
    app.redo();
    assert_eq!(app.design.elements.len(), initial_count + 1);

    app.selected = Some(1);
    app.selected_ids = vec![1];
    app.move_selected_to_edge(false);
    assert_eq!(app.design.elements.first().unwrap().id, 1);
    app.move_selected(true);
    app.move_selected_to_edge(true);
    assert_eq!(app.design.elements.last().unwrap().id, 1);
    app.bulk_transform(false, true);
    app.bulk_transform(true, false);

    let source = app.design.element_mut(1).unwrap();
    source.color = DesignColor::rgb(12, 34, 56);
    source.opacity = 123;
    source.appearance.outline = Some(DesignColor::INK);
    app.copy_selected();
    app.selected = Some(2);
    app.selected_ids = vec![2];
    app.paste_style();
    let target = app.design.element(2).unwrap();
    assert_eq!(target.color, DesignColor::rgb(12, 34, 56));
    assert_eq!(target.opacity, 123);
    assert_eq!(target.appearance.outline, Some(DesignColor::INK));

    app.selected_ids = vec![1, 2];
    app.toggle_group(false);
    assert_eq!(
        app.design.element(1).unwrap().group_id,
        app.design.element(2).unwrap().group_id
    );
    app.toggle_group(true);
    assert_eq!(app.design.element(1).unwrap().group_id, None);

    app.selected = Some(2);
    app.selected_ids = vec![2];
    app.delete_selected();
    assert!(app.design.element(2).is_none());
    app.undo();
    assert!(app.design.element(2).is_some());
    app.clear_canvas();
    assert!(app.design.elements.is_empty());
    app.undo();
    assert!(!app.design.elements.is_empty());

    app.selected = None;
    app.selected_ids.clear();
    app.duplicate_selected();
    app.delete_selected();
    app.move_selected(true);
    app.move_selected_to_edge(false);
    app.toggle_group(false);
    app.bulk_transform(true, true);
    app.paste_style();
    app.clear_canvas();
}

#[test]
fn project_and_library_updates_apply_atomically_and_refresh_selection() {
    let mut app = test_app();
    let context = egui::Context::default();
    let directory = temp_dir();
    let path = directory.join("saved.buttonstudio");
    let mut replacement = Design::default();
    replacement.elements.clear();
    replacement.add_text("Loaded project".into());
    std::fs::write(&path, replacement.to_project_json().unwrap()).unwrap();

    app.open_project_path(&context, path.clone());
    assert!(app.pending_project.is_some());
    app.apply_pending_updates(&context);
    assert_eq!(app.project_path, Some(path));
    assert_eq!(app.design, replacement);
    assert_eq!(app.status, "Project opened");

    app.open_project_path(&context, directory.join("missing.buttonstudio"));
    assert!(app.status_is_error);
    assert!(app.status.contains("Could not open project"));

    let selected_id = app.selected.unwrap();
    app.selected_ids = vec![selected_id];
    let style = SavedStyle {
        name: "Ink title".into(),
        color: DesignColor::INK,
        font: Some(TextFont::default()),
        font_size: Some(0.13),
        opacity: 210,
        appearance: crate::appearance::Appearance {
            line_spacing: 1.4,
            ..Default::default()
        },
    };
    app.queue_style_update(&context, style.clone());
    app.apply_pending_updates(&context);
    let element = app.design.element(selected_id).unwrap();
    assert_eq!(element.color, DesignColor::INK);
    assert_eq!(element.opacity, 210);
    assert_eq!(element.appearance.line_spacing, 1.4);
    assert!(matches!(
        &element.kind,
        ElementKind::Text { font_size, .. } if *font_size == 0.13
    ));
    assert_eq!(app.selected_style().unwrap().name, "Loaded project");

    app.pending_font_change = Some((selected_id, TextFont::default()));
    app.apply_pending_updates(&context);
    app.queue_style_update(&context, style);
    app.selected_ids = vec![selected_id, u64::MAX];
    app.apply_pending_updates(&context);
    assert!(!app.status_is_error);

    let mut another = Design::default();
    another.elements.clear();
    another.add_shape(ShapeKind::Heart);
    app.queue_design_update(&context, another.clone());
    app.apply_pending_updates(&context);
    assert_eq!(app.design, another);
    assert_eq!(app.selected_ids, vec![app.selected.unwrap()]);
    assert!(app.history.can_undo());

    assert!(app.selected_style().is_some());
    app.selected = Some(u64::MAX);
    assert!(app.selected_style().is_none());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn export_preflight_cache_selection_and_display_helpers_handle_edges() {
    let mut app = test_app();
    app.refresh_export_preflight();
    assert!(app.export_preflight_cache.is_some());
    let cached = app.export_preflight_cache.as_ref().unwrap().design.clone();
    app.refresh_export_preflight();
    assert_eq!(app.export_preflight_cache.as_ref().unwrap().design, cached);
    app.design.background = DesignColor::INK;
    app.refresh_export_preflight();
    assert_eq!(
        app.export_preflight_cache
            .as_ref()
            .unwrap()
            .design
            .background,
        DesignColor::INK
    );

    app.series_data = Some(SeriesData {
        source_name: "names.csv".into(),
        columns: vec!["name".into()],
        rows: vec![
            HashMap::from([("name".into(), "Ada".into())]),
            HashMap::from([("name".into(), "Lin".into())]),
        ],
    });
    app.export_kind = ExportKind::PdfSheet;
    app.export_start = 0;
    app.export_end = 99;
    app.export_preview_entry = 99;
    app.open_export_review();
    assert_eq!(app.export_kind, ExportKind::PdfSeries);
    assert_eq!(
        (app.export_start, app.export_end, app.export_preview_entry),
        (1, 2, 2)
    );
    app.series_data = None;
    app.open_export_review();
    assert_eq!(app.export_kind, ExportKind::PdfSheet);

    assert_eq!(
        format_row_numbers(&(1..=15).collect::<Vec<_>>()),
        "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, … (+3 more)"
    );
    assert_eq!(format_row_numbers(&[]), "");
    assert_eq!(safe_file_stem("  ../My: button  "), "My-button");
    assert_eq!(safe_file_stem("---"), "button-template");
    assert_eq!(design_palette(&app.design).len(), 3);
    assert_eq!(truncate("short", 10), "short");
    assert_eq!(truncate("abcdefgh", 5), "abcd…");
    assert_eq!(shape_icon(ShapeKind::Bolt), "zap");
    assert_eq!(
        shape_layer_dimensions(ShapeKind::Rectangle, 0.3, None, None),
        (0.3, 0.2)
    );
    assert_eq!(
        shape_layer_dimensions(ShapeKind::Circle, 0.3, Some(0.4), Some(0.5)),
        (0.4, 0.5)
    );

    let mut kind = ElementKind::Shape {
        shape: ShapeKind::Rectangle,
        size: 0.2,
        width: None,
        height: None,
    };
    set_element_size(&mut kind, 0.5);
    assert_eq!(element_size(&kind), 0.5);
    let mut text = ElementKind::Text {
        text: "A".into(),
        font_size: 0.1,
        font: TextFont::default(),
        curvature: 0.0,
    };
    set_element_size(&mut text, 2.0);
    assert_eq!(element_size(&text), 0.2);
}
