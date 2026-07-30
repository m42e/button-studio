use crate::{
    document::SharedDocument,
    fonts::{AvailableFont, FontCatalog},
    model::{
        BackgroundStyle, Design, DesignColor, ELEMENT_POSITION_LIMIT, ElementKind, History,
        IMAGE_WIDTH_LIMIT, ShapeKind, TextFont,
    },
    render::{
        TextureMap, element_bounds, export_pdf_series, export_pdf_sheet, export_pixel_size,
        export_png, hit_test, load_texture, paint_design, paint_preview, paint_trim_boundary,
        render_project_preview_png, selection_bounds, selection_handles,
    },
    series::SeriesData,
    theme::{ACCENT, INK, MINT, MUTED, PAPER, SURFACE, display_font, icon, icon_text},
};
use eframe::egui::{
    self, Align, Button, Color32, Frame, Key, Layout, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, Ui, Vec2, vec2,
};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Text,
    Shape,
    Image,
    QrCode,
    Background,
    Series,
}

#[derive(Clone, Copy)]
enum CanvasInteraction {
    Move {
        id: u64,
    },
    Resize {
        id: u64,
        start_distance: f32,
        start_size: f32,
    },
}

#[derive(Clone, Copy)]
enum ElementAlignment {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
}

impl CanvasInteraction {
    fn id(self) -> u64 {
        match self {
            Self::Move { id } | Self::Resize { id, .. } => id,
        }
    }
}

impl Tool {
    const ALL: [Self; 7] = [
        Self::Select,
        Self::Text,
        Self::Shape,
        Self::Image,
        Self::QrCode,
        Self::Background,
        Self::Series,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Text => "Text",
            Self::Shape => "Shapes",
            Self::Image => "Images",
            Self::QrCode => "QR Code",
            Self::Background => "Backdrop",
            Self::Series => "Series",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Select => "mouse-pointer-2",
            Self::Text => "type",
            Self::Shape => "shapes",
            Self::Image => "image-plus",
            Self::QrCode => "qr-code",
            Self::Background => "palette",
            Self::Series => "table-2",
        }
    }
}

pub struct ButtonStudio {
    design: Design,
    shared_document: Option<SharedDocument>,
    shared_revision: u64,
    history: History,
    selected: Option<u64>,
    alignment_reference: Option<u64>,
    tool: Tool,
    draft_text: String,
    draft_text_curvature: f32,
    draft_qr_content: String,
    textures: TextureMap,
    project_path: Option<PathBuf>,
    status: String,
    status_is_error: bool,
    show_guides: bool,
    canvas_interaction: Option<CanvasInteraction>,
    font_catalog: FontCatalog,
    font_search: String,
    font_browser_open: bool,
    font_browser_target: Option<u64>,
    focus_font_search: bool,
    pending_font_change: Option<(u64, TextFont)>,
    pending_project: Option<(Design, PathBuf)>,
    series: Option<SeriesData>,
}

impl ButtonStudio {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install(&context.egui_ctx);
        let font_catalog = FontCatalog::discover();
        let font_count = font_catalog.available.len();
        Self {
            design: Design::default(),
            shared_document: None,
            shared_revision: 0,
            history: History::default(),
            selected: Some(2),
            alignment_reference: None,
            tool: Tool::Select,
            draft_text: "YOUR TEXT".to_owned(),
            draft_text_curvature: 0.0,
            draft_qr_content: "https://example.com".to_owned(),
            textures: TextureMap::default(),
            project_path: None,
            status: format!("Ready · {font_count} fonts available"),
            status_is_error: false,
            show_guides: true,
            canvas_interaction: None,
            font_catalog,
            font_search: String::new(),
            font_browser_open: false,
            font_browser_target: None,
            focus_font_search: false,
            pending_font_change: None,
            pending_project: None,
            series: None,
        }
    }

    pub fn with_document(
        context: &eframe::CreationContext<'_>,
        shared_document: SharedDocument,
    ) -> Self {
        let mut app = Self::new(context);
        if let Ok((design, revision)) = shared_document.snapshot() {
            app.design = design;
            app.shared_revision = revision;
            app.reload_textures(&context.egui_ctx);
        }
        app.shared_document = Some(shared_document);
        app
    }

    fn pull_shared_document(&mut self, context: &egui::Context) {
        let Some(shared_document) = &self.shared_document else {
            return;
        };
        let Ok((design, revision)) = shared_document.snapshot() else {
            return;
        };
        if revision == self.shared_revision {
            return;
        }
        self.history.record(self.design.clone());
        self.design = design;
        self.shared_revision = revision;
        self.selected = self
            .selected
            .filter(|id| self.design.element(*id).is_some())
            .or_else(|| self.design.elements.last().map(|element| element.id));
        self.reload_textures(context);
        self.set_status("Design updated through MCP", false);
    }

    fn publish_shared_document(&mut self, context: &egui::Context) {
        let Some(shared_document) = &self.shared_document else {
            return;
        };
        match shared_document.replace_if_revision(self.shared_revision, &self.design) {
            Ok(revision) => self.shared_revision = revision,
            Err(_) => context.request_repaint(),
        }
    }

    fn top_bar(&mut self, root: &mut Ui) {
        let context = root.ctx().clone();
        egui::Panel::top("top_bar")
            .default_size(76.0)
            .min_size(76.0)
            .max_size(76.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(INK).inner_margin(16.0))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("PINCRAFT")
                                .font(display_font(20.0))
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new("BUTTON STUDIO")
                                .size(10.0)
                                .strong()
                                .color(Color32::from_gray(164)),
                        );
                    });
                    ui.add_space(26.0);
                    ui.label(
                        RichText::new("SIZE")
                            .size(10.0)
                            .strong()
                            .color(Color32::from_gray(164)),
                    );
                    let previous_size = self.design.diameter_mm;
                    egui::ComboBox::from_id_salt("button_size")
                        .selected_text(format!("{} mm round", self.design.diameter_mm))
                        .show_ui(ui, |ui| {
                            for size in [25, 32, 38, 50, 59, 75] {
                                ui.selectable_value(
                                    &mut self.design.diameter_mm,
                                    size,
                                    format!("{size} mm round"),
                                );
                            }
                        });
                    if previous_size != self.design.diameter_mm {
                        let mut previous = self.design.clone();
                        previous.diameter_mm = previous_size;
                        self.history.record(previous);
                        self.set_status("Product size updated", false);
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (pdf_label, pdf_tooltip) = if let Some(series) = &self.series {
                            (
                                "Export series",
                                format!(
                                    "Export one full-size button for each of {} CSV entries",
                                    series.rows.len()
                                ),
                            )
                        } else {
                            (
                                "Export PDF",
                                "Fill an A4 PDF with full-size print-ready buttons".to_owned(),
                            )
                        };
                        if ui
                            .add(
                                Button::new(icon_text("files", pdf_label))
                                    .min_size(vec2(118.0, 40.0)),
                            )
                            .on_hover_text(pdf_tooltip)
                            .clicked()
                        {
                            self.export_pdf();
                        }
                        if ui
                            .add(
                                Button::new(
                                    RichText::new(icon_text("download", "Export PNG"))
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(ACCENT)
                                .min_size(vec2(132.0, 40.0)),
                            )
                            .on_hover_text("Export a transparent 300-DPI PNG")
                            .clicked()
                        {
                            self.export_design();
                        }
                        if ui
                            .add(Button::new(icon_text("save", "Save")).min_size(vec2(88.0, 40.0)))
                            .on_hover_text("Save project")
                            .clicked()
                        {
                            self.save_project();
                        }
                        if top_icon_button(ui, "folder-open", "Open project", true).clicked() {
                            self.open_project(&context);
                        }
                        ui.separator();
                        if top_icon_button(
                            ui,
                            "trash-2",
                            "Clear canvas",
                            !self.design.elements.is_empty(),
                        )
                        .clicked()
                        {
                            self.clear_canvas();
                        }
                        if top_icon_button(ui, "redo-2", "Redo", self.history.can_redo()).clicked()
                        {
                            self.redo();
                        }
                        if top_icon_button(ui, "undo-2", "Undo", self.history.can_undo()).clicked()
                        {
                            self.undo();
                        }
                    });
                });
            });
    }

    fn tool_panel(&mut self, root: &mut Ui) {
        let context = root.ctx().clone();
        egui::Panel::left("tool_panel")
            .default_size(306.0)
            .min_size(306.0)
            .max_size(306.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(SURFACE).inner_margin(18.0))
            .show(root, |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(RichText::new("TOOLS").size(10.0).strong().color(MUTED));
                        ui.horizontal_wrapped(|ui| {
                            for tool in Tool::ALL {
                                let selected = self.tool == tool;
                                let fill = if selected {
                                    ACCENT
                                } else {
                                    Color32::from_rgb(239, 238, 233)
                                };
                                let foreground = if selected { Color32::WHITE } else { INK };
                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new(icon(tool.icon()))
                                                .size(18.0)
                                                .color(foreground),
                                        )
                                        .fill(fill)
                                        .min_size(vec2(43.0, 42.0)),
                                    )
                                    .on_hover_text(tool.label())
                                    .clicked()
                                {
                                    self.tool = tool;
                                }
                            }
                        });
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(6.0);
                        ui.heading(self.tool.label());
                        ui.add_space(6.0);

                        match self.tool {
                            Tool::Select => self.selection_controls(ui),
                            Tool::Text => self.text_controls(ui),
                            Tool::Shape => self.shape_controls(ui),
                            Tool::Image => self.image_controls(ui, &context),
                            Tool::QrCode => self.qr_code_controls(ui),
                            Tool::Background => self.background_controls(ui, &context),
                            Tool::Series => self.series_controls(ui),
                        }
                    });
            });
    }

    fn right_panel(&mut self, root: &mut Ui) {
        egui::Panel::right("preview_panel")
            .default_size(294.0)
            .min_size(294.0)
            .max_size(294.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(SURFACE).inner_margin(18.0))
            .show(root, |ui| {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("PRODUCT PREVIEW")
                                    .size(10.0)
                                    .strong()
                                    .color(MUTED),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new("FRONT").size(10.0).strong().color(MINT));
                            });
                        });
                        ui.add_space(8.0);
                        let width = ui.available_width();
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(width, width * 0.96), Sense::hover());
                        paint_preview(
                            ui.painter(),
                            rect,
                            &self.design,
                            &self.textures,
                        );

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{} mm", self.design.diameter_mm))
                                    .font(display_font(18.0)),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new("300 DPI").size(11.0).strong().color(MINT));
                            });
                        });
                        let pixels = export_pixel_size(&self.design);
                        ui.label(
                            RichText::new(format!(
                                "{:.1} mm artwork · {pixels} × {pixels} px export",
                                self.design.artwork_diameter_mm()
                            ))
                            .color(MUTED),
                        );
                        ui.checkbox(
                            &mut self.show_guides,
                            format!("Show finished edge ({} mm)", self.design.diameter_mm),
                        )
                        .on_hover_text(
                            "The product preview shows the artwork inside this dashed circle. Artwork outside it wraps around the button.",
                        );

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);
                        self.layer_panel(ui);

                        ui.add_space(14.0);
                        let status_color = if self.status_is_error { ACCENT } else { MINT };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(icon(if self.status_is_error {
                                    "circle-alert"
                                } else {
                                    "circle-check"
                                }))
                                .color(status_color),
                            );
                            ui.label(RichText::new(&self.status).size(12.0).color(status_color));
                        });
                    });
            });
    }

    fn canvas(&mut self, root: &mut Ui) {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(PAPER).inner_margin(18.0))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("FRONT ARTWORK")
                            .size(10.0)
                            .strong()
                            .color(MUTED),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let selected_name = self
                            .selected
                            .and_then(|id| self.design.element(id))
                            .map(|element| element.name())
                            .unwrap_or_else(|| "No selection".to_owned());
                        ui.label(
                            RichText::new(truncate(&selected_name, 30))
                                .size(11.0)
                                .color(MUTED),
                        );
                    });
                });
                ui.add_space(8.0);

                let available = ui.available_size();
                let side = available.x.min(available.y).max(280.0);
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 9.0, Color32::from_rgb(226, 225, 219));
                    for x in (12..side as i32).step_by(22) {
                        for y in (12..side as i32).step_by(22) {
                            painter.circle_filled(
                                rect.left_top() + vec2(x as f32, y as f32),
                                1.2,
                                Color32::from_black_alpha(20),
                            );
                        }
                    }

                    let center = rect.center();
                    let radius = side * 0.405;
                    painter.circle_filled(
                        center + vec2(0.0, 10.0),
                        radius + 4.0,
                        Color32::from_black_alpha(28),
                    );
                    paint_design(
                        &painter,
                        center,
                        radius,
                        &self.design,
                        &self.textures,
                        self.selected,
                    );
                    if self.show_guides {
                        paint_trim_boundary(&painter, center, radius, &self.design);
                    }
                    painter.rect_stroke(
                        rect,
                        9.0,
                        Stroke::new(1.0, Color32::from_black_alpha(18)),
                        StrokeKind::Inside,
                    );

                    if response.clicked()
                        && let Some(pointer) = response.interact_pointer_pos()
                    {
                        self.selected = hit_test(&painter, pointer, center, radius, &self.design);
                        if self.selected.is_some() {
                            self.tool = Tool::Select;
                        }
                    }

                    if response.drag_started()
                        && let Some(pointer) = response.interact_pointer_pos()
                    {
                        let resize = self.selected.and_then(|id| {
                            let element = self.design.element(id)?;
                            let bounds = selection_bounds(&painter, center, radius, element);
                            resize_handle_at(pointer, bounds, rect).map(|_| {
                                CanvasInteraction::Resize {
                                    id,
                                    start_distance: pointer
                                        .distance(
                                            center
                                                + vec2(element.position[0], element.position[1])
                                                    * radius,
                                        )
                                        .max(1.0),
                                    start_size: element_size(&element.kind),
                                }
                            })
                        });
                        let interaction = resize.or_else(|| {
                            hit_test(&painter, pointer, center, radius, &self.design)
                                .map(|id| CanvasInteraction::Move { id })
                        });

                        if let Some(interaction) = interaction {
                            self.selected = Some(interaction.id());
                            self.canvas_interaction = Some(interaction);
                            self.tool = Tool::Select;
                            self.history.record(self.design.clone());
                        }
                    }

                    if response.dragged() {
                        match self.canvas_interaction {
                            Some(CanvasInteraction::Move { id }) => {
                                let delta = ui.input(|input| input.pointer.delta()) / radius;
                                if let Some(element) = self.design.element_mut(id) {
                                    element.position[0] += delta.x;
                                    element.position[1] += delta.y;
                                    clamp_element_position(&mut element.position);
                                }
                            }
                            Some(CanvasInteraction::Resize {
                                id,
                                start_distance,
                                start_size,
                            }) => {
                                if let Some(pointer) = response.interact_pointer_pos()
                                    && let Some(element) = self.design.element(id)
                                {
                                    let element_center = center
                                        + vec2(element.position[0], element.position[1]) * radius;
                                    let scale = pointer.distance(element_center) / start_distance;
                                    if let Some(element) = self.design.element_mut(id) {
                                        set_element_size(&mut element.kind, start_size * scale);
                                    }
                                }
                            }
                            None => {}
                        }
                    }

                    if !ui.input(|input| input.pointer.primary_down()) {
                        self.canvas_interaction = None;
                    }
                    let resize_cursor = response.hover_pos().and_then(|pointer| {
                        let element = self.selected.and_then(|id| self.design.element(id))?;
                        resize_handle_at(
                            pointer,
                            selection_bounds(&painter, center, radius, element),
                            rect,
                        )
                        .map(|index| {
                            if index == 0 || index == 3 {
                                egui::CursorIcon::ResizeNwSe
                            } else {
                                egui::CursorIcon::ResizeNeSw
                            }
                        })
                    });
                    if let Some(cursor) = resize_cursor {
                        response.on_hover_cursor(cursor);
                    } else {
                        response.on_hover_cursor(egui::CursorIcon::Grab);
                    }
                });
            });
    }

    fn selection_controls(&mut self, ui: &mut Ui) {
        let Some(id) = self.selected else {
            ui.label(RichText::new("No layer selected").color(MUTED));
            return;
        };
        let Some(name) = self.design.element(id).map(|element| element.name()) else {
            self.selected = None;
            return;
        };

        ui.label(RichText::new(truncate(&name, 32)).font(display_font(15.0)));
        ui.add_space(4.0);
        let previous = self.design.clone();
        let mut changed = false;
        let mut open_font_browser = false;

        if let Some(element) = self.design.element_mut(id) {
            match &mut element.kind {
                ElementKind::Text {
                    text,
                    font_size,
                    font,
                    curvature,
                } => {
                    ui.label(section_label("CONTENT"));
                    changed |= ui.text_edit_multiline(text).changed();
                    changed |= ui
                        .add(egui::Slider::new(font_size, 0.025..=0.2).text("Size"))
                        .changed();
                    ui.add_space(6.0);
                    ui.label(section_label("LAYOUT"));
                    changed |= text_arc_controls(ui, curvature);
                    ui.add_space(6.0);
                    open_font_browser = font_picker_button(ui, font, &self.font_catalog.available);
                }
                ElementKind::Shape { size, .. } => {
                    ui.label(section_label("SIZE"));
                    changed |= ui
                        .add(egui::Slider::new(size, 0.06..=0.68).text("Scale"))
                        .changed();
                }
                ElementKind::Image { width, .. } => {
                    ui.label(section_label("SIZE"));
                    changed |= ui
                        .add(egui::Slider::new(width, 0.08..=IMAGE_WIDTH_LIMIT).text("Width"))
                        .changed();
                }
                ElementKind::QrCode {
                    content,
                    size,
                    background,
                    transparent_background,
                } => {
                    ui.label(section_label("CONTENT"));
                    changed |= ui.text_edit_multiline(content).changed();
                    ui.add_space(6.0);
                    ui.label(section_label("SIZE"));
                    changed |= ui
                        .add(egui::Slider::new(size, 0.08..=1.2).text("Scale"))
                        .changed();
                    ui.add_space(6.0);
                    ui.label(section_label("BACKGROUND"));
                    changed |= ui
                        .checkbox(transparent_background, "Fully transparent")
                        .changed();
                    if !*transparent_background {
                        let mut background_color: Color32 = (*background).into();
                        if ui.color_edit_button_srgba(&mut background_color).changed() {
                            *background = background_color.into();
                            changed = true;
                        }
                    }
                }
            }

            ui.add_space(6.0);
            ui.label(section_label("POSITION"));
            changed |= ui
                .add(
                    egui::Slider::new(
                        &mut element.position[0],
                        -ELEMENT_POSITION_LIMIT..=ELEMENT_POSITION_LIMIT,
                    )
                    .text("X"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(
                        &mut element.position[1],
                        -ELEMENT_POSITION_LIMIT..=ELEMENT_POSITION_LIMIT,
                    )
                    .text("Y"),
                )
                .changed();

            ui.add_space(6.0);
            ui.label(section_label("ROTATION"));
            changed |= ui
                .add(egui::Slider::new(&mut element.rotation, -180.0..=180.0).suffix("°"))
                .changed();

            if !matches!(element.kind, ElementKind::Image { .. }) {
                ui.add_space(6.0);
                ui.label(section_label("COLOR"));
                let mut color: Color32 = element.color.into();
                if ui.color_edit_button_srgba(&mut color).changed() {
                    element.color = color.into();
                    changed = true;
                }
            }

            let mut opacity = element.opacity as f32 / 2.55;
            if ui
                .add(egui::Slider::new(&mut opacity, 10.0..=100.0).text("Opacity"))
                .changed()
            {
                element.opacity = (opacity * 2.55).round() as u8;
                changed = true;
            }
            changed |= ui.checkbox(&mut element.visible, "Visible").changed();
        }

        if self.alignment_reference == Some(id)
            || self
                .alignment_reference
                .is_some_and(|reference| self.design.element(reference).is_none())
        {
            self.alignment_reference = None;
        }

        ui.add_space(6.0);
        ui.label(section_label("ALIGN"));
        let reference_label = self
            .alignment_reference
            .and_then(|reference| self.design.element(reference))
            .map(|element| truncate(&element.name(), 24))
            .unwrap_or_else(|| "Artwork page".to_owned());
        egui::ComboBox::from_id_salt("alignment_reference")
            .selected_text(reference_label)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.alignment_reference, None, "Artwork page");
                for element in self
                    .design
                    .elements
                    .iter()
                    .filter(|element| element.id != id)
                {
                    ui.selectable_value(
                        &mut self.alignment_reference,
                        Some(element.id),
                        truncate(&element.name(), 24),
                    );
                }
            });

        let mut alignment = None;
        ui.horizontal(|ui| {
            for (value, icon_name, tooltip) in [
                (ElementAlignment::Left, "align-start-vertical", "Align left"),
                (
                    ElementAlignment::HorizontalCenter,
                    "align-center-vertical",
                    "Align horizontal centers",
                ),
                (ElementAlignment::Right, "align-end-vertical", "Align right"),
                (ElementAlignment::Top, "align-start-horizontal", "Align top"),
                (
                    ElementAlignment::VerticalCenter,
                    "align-center-horizontal",
                    "Align vertical centers",
                ),
                (
                    ElementAlignment::Bottom,
                    "align-end-horizontal",
                    "Align bottom",
                ),
            ] {
                if icon_button(ui, icon_name, tooltip, true).clicked() {
                    alignment = Some(value);
                }
            }
        });

        if let Some(alignment) = alignment {
            let painter = ui.painter().clone();
            if align_element(
                &mut self.design,
                &painter,
                id,
                self.alignment_reference,
                alignment,
            ) {
                changed = true;
                self.set_status("Layer aligned", false);
            }
        }

        if open_font_browser {
            self.font_browser_target = Some(id);
            self.font_browser_open = true;
            self.focus_font_search = true;
            self.font_search.clear();
        }

        if changed {
            self.history.record(previous);
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(icon_text("copy", "Duplicate")).clicked() {
                self.duplicate_selected();
            }
            if icon_button(ui, "trash-2", "Delete layer", true).clicked() {
                self.delete_selected();
            }
        });
    }

    fn text_controls(&mut self, ui: &mut Ui) {
        ui.label(section_label("CONTENT"));
        ui.text_edit_singleline(&mut self.draft_text);
        ui.add_space(8.0);
        ui.label(section_label("LAYOUT"));
        text_arc_controls(ui, &mut self.draft_text_curvature);
        ui.add_space(8.0);
        if ui
            .add_enabled(
                !self.draft_text.trim().is_empty(),
                Button::new(RichText::new(icon_text("plus", "Add text")).color(Color32::WHITE))
                    .fill(INK),
            )
            .clicked()
        {
            self.history.record(self.design.clone());
            let id = self.design.add_text(self.draft_text.trim().to_owned());
            if let Some(element) = self.design.element_mut(id)
                && let ElementKind::Text { curvature, .. } = &mut element.kind
            {
                *curvature = self.draft_text_curvature;
            }
            self.selected = Some(id);
            self.tool = Tool::Select;
            self.set_status("Text layer added", false);
        }

        ui.add_space(12.0);
        ui.label(section_label("QUICK TEXT"));
        for preset in ["HELLO", "TEAM 2026", "I MADE THIS"] {
            if ui.button(preset).clicked() {
                self.draft_text = preset.to_owned();
            }
        }
    }

    fn shape_controls(&mut self, ui: &mut Ui) {
        ui.label(section_label("SHAPE LIBRARY"));
        egui::Grid::new("shape_grid")
            .num_columns(2)
            .spacing(vec2(8.0, 8.0))
            .show(ui, |ui| {
                for (index, shape) in ShapeKind::ALL.into_iter().enumerate() {
                    if ui
                        .add_sized(
                            [124.0, 42.0],
                            Button::new(icon_text(shape_icon(shape), shape.label())),
                        )
                        .clicked()
                    {
                        self.history.record(self.design.clone());
                        let id = self.design.add_shape(shape);
                        self.selected = Some(id);
                        self.tool = Tool::Select;
                        self.set_status("Shape layer added", false);
                    }
                    if index % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
    }

    fn image_controls(&mut self, ui: &mut Ui, context: &egui::Context) {
        ui.label(section_label("ARTWORK"));
        if ui
            .add_sized(
                [ui.available_width(), 48.0],
                Button::new(
                    RichText::new(icon_text("upload", "Import image")).color(Color32::WHITE),
                )
                .fill(INK),
            )
            .clicked()
        {
            self.import_image(context);
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for format in ["PNG", "JPEG", "WEBP"] {
                ui.label(
                    RichText::new(format)
                        .size(10.0)
                        .strong()
                        .color(MUTED)
                        .background_color(Color32::from_rgb(239, 238, 233)),
                );
            }
        });
    }

    fn qr_code_controls(&mut self, ui: &mut Ui) {
        ui.label(section_label("CONTENT"));
        ui.text_edit_multiline(&mut self.draft_qr_content);
        ui.label(
            RichText::new("URL, text, contact details, or any other QR content")
                .size(11.0)
                .color(MUTED),
        );
        ui.add_space(8.0);
        if ui
            .add_enabled(
                !self.draft_qr_content.trim().is_empty(),
                Button::new(RichText::new(icon_text("plus", "Add QR code")).color(Color32::WHITE))
                    .fill(INK),
            )
            .clicked()
        {
            self.history.record(self.design.clone());
            let id = self
                .design
                .add_qr_code(self.draft_qr_content.trim().to_owned());
            self.selected = Some(id);
            self.tool = Tool::Select;
            self.set_status("QR code layer added", false);
        }
    }

    fn series_controls(&mut self, ui: &mut Ui) {
        ui.label(section_label("CSV DATA"));
        if ui
            .add_sized(
                [ui.available_width(), 46.0],
                Button::new(RichText::new(icon_text("upload", "Import CSV")).color(Color32::WHITE))
                    .fill(INK),
            )
            .clicked()
        {
            self.import_series();
        }

        let Some(series) = &self.series else {
            ui.label(RichText::new("No series loaded").color(MUTED));
            return;
        };
        let source_name = series.source_name.clone();
        let row_count = series.rows.len();
        let columns = series.columns.clone();
        ui.add_space(8.0);
        ui.label(RichText::new(truncate(&source_name, 30)).strong());
        ui.label(RichText::new(format!("{row_count} entries")).color(MUTED));
        if ui.button(icon_text("x", "Remove data")).clicked() {
            self.series = None;
            self.set_status("Series data removed", false);
            return;
        }

        ui.add_space(12.0);
        ui.label(section_label("VARIABLES"));
        for column in columns {
            let placeholder = format!("{{{{{column}}}}}");
            if ui
                .add_sized(
                    [ui.available_width(), 34.0],
                    Button::new(icon_text("plus", &placeholder)),
                )
                .on_hover_text("Insert into the selected text or QR layer")
                .clicked()
            {
                self.insert_series_variable(&placeholder);
            }
        }
    }

    fn background_controls(&mut self, ui: &mut Ui, context: &egui::Context) {
        ui.label(section_label("PHOTO"));
        let photo_name = self.design.background_image.as_ref().map(|image| {
            self.design
                .assets
                .get(&image.asset_id)
                .and_then(|asset| asset.source_path.as_ref())
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("Backdrop image")
                .to_owned()
        });
        if let Some(photo_name) = photo_name {
            ui.label(RichText::new(truncate(&photo_name, 30)).strong());
            ui.horizontal(|ui| {
                if ui.button(icon_text("image-up", "Replace")).clicked() {
                    self.import_background_image(context);
                }
                if icon_button(ui, "trash-2", "Remove backdrop photo", true).clicked() {
                    let previous = self.design.clone();
                    self.design.background_image = None;
                    self.history.record(previous);
                    self.set_status("Backdrop photo removed", false);
                }
            });
        } else if ui
            .add_sized(
                [ui.available_width(), 46.0],
                Button::new(
                    RichText::new(icon_text("image-up", "Choose photo")).color(Color32::WHITE),
                )
                .fill(INK),
            )
            .clicked()
        {
            self.import_background_image(context);
        }

        ui.add_space(12.0);
        let previous = self.design.clone();
        let mut changed = false;

        ui.label(section_label("STYLE"));
        egui::Grid::new("background_styles")
            .num_columns(2)
            .spacing(vec2(8.0, 8.0))
            .show(ui, |ui| {
                for (index, style) in BackgroundStyle::ALL.into_iter().enumerate() {
                    let selected = self.design.background_style == style;
                    let foreground = if selected { Color32::WHITE } else { INK };
                    let button = Button::new(RichText::new(style.label()).color(foreground)).fill(
                        if selected {
                            INK
                        } else {
                            Color32::from_rgb(239, 238, 233)
                        },
                    );
                    if ui.add_sized([124.0, 38.0], button).clicked() {
                        self.design.background_style = style;
                        changed = true;
                    }
                    if index % 2 == 1 {
                        ui.end_row();
                    }
                }
            });

        ui.add_space(12.0);
        ui.add_enabled_ui(
            self.design.background_style != BackgroundStyle::None,
            |ui| {
                ui.label(section_label("COLOR"));
                let mut color: Color32 = self.design.background.into();
                if ui.color_edit_button_srgba(&mut color).changed() {
                    self.design.background = color.into();
                    changed = true;
                }
                ui.horizontal_wrapped(|ui| {
                    for swatch in palette() {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                        ui.painter()
                            .circle_filled(rect.center(), 14.0, Color32::from(swatch));
                        ui.painter().circle_stroke(
                            rect.center(),
                            14.0,
                            Stroke::new(1.0, Color32::from_black_alpha(42)),
                        );
                        if response.clicked() {
                            self.design.background = swatch;
                            changed = true;
                        }
                    }
                });
            },
        );

        if changed {
            self.history.record(previous);
        }
    }

    fn layer_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("LAYERS").size(10.0).strong().color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(self.design.elements.len().to_string())
                        .size(11.0)
                        .color(MUTED),
                );
            });
        });
        ui.add_space(5.0);

        let rows = self
            .design
            .elements
            .iter()
            .rev()
            .map(|element| (element.id, element.name(), element.visible))
            .collect::<Vec<_>>();
        ScrollArea::vertical().max_height(190.0).show(ui, |ui| {
            for (id, name, visible) in rows {
                ui.horizontal(|ui| {
                    if icon_button(
                        ui,
                        if visible { "eye" } else { "eye-off" },
                        if visible { "Hide layer" } else { "Show layer" },
                        true,
                    )
                    .clicked()
                    {
                        let previous = self.design.clone();
                        if let Some(element) = self.design.element_mut(id) {
                            element.visible = !element.visible;
                        }
                        self.history.record(previous);
                    }
                    if ui
                        .selectable_label(self.selected == Some(id), truncate(&name, 22))
                        .clicked()
                    {
                        self.selected = Some(id);
                        self.tool = Tool::Select;
                    }
                });
            }
        });

        ui.horizontal(|ui| {
            let enabled = self.selected.is_some();
            if icon_button(ui, "arrow-up", "Bring forward", enabled).clicked() {
                self.move_selected(true);
            }
            if icon_button(ui, "arrow-down", "Send backward", enabled).clicked() {
                self.move_selected(false);
            }
            if icon_button(ui, "copy", "Duplicate layer", enabled).clicked() {
                self.duplicate_selected();
            }
            if icon_button(ui, "trash-2", "Delete layer", enabled).clicked() {
                self.delete_selected();
            }
        });
    }

    fn handle_shortcuts(&mut self, context: &egui::Context) {
        let accepts_global_keys = !context.egui_wants_keyboard_input();
        let (undo, redo, save, open, export, duplicate, delete, nudge) = context.input(|input| {
            let command = input.modifiers.command;
            let shift = input.modifiers.shift;
            let nudge_step = if shift { 0.05 } else { 0.01 };
            let nudge = if accepts_global_keys && !command {
                Some([
                    (input.key_pressed(Key::ArrowRight) as i8
                        - input.key_pressed(Key::ArrowLeft) as i8) as f32
                        * nudge_step,
                    (input.key_pressed(Key::ArrowDown) as i8
                        - input.key_pressed(Key::ArrowUp) as i8) as f32
                        * nudge_step,
                ])
                .filter(|delta| delta[0] != 0.0 || delta[1] != 0.0)
            } else {
                None
            };
            (
                command && !shift && input.key_pressed(Key::Z),
                command && (input.key_pressed(Key::Y) || (shift && input.key_pressed(Key::Z))),
                command && input.key_pressed(Key::S),
                command && input.key_pressed(Key::O),
                command && input.key_pressed(Key::E),
                command && input.key_pressed(Key::D),
                accepts_global_keys
                    && (input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace)),
                nudge,
            )
        });

        if undo {
            self.undo();
        } else if redo {
            self.redo();
        }
        if save {
            self.save_project();
        }
        if open {
            self.open_project(context);
        }
        if export {
            self.export_design();
        }
        if duplicate {
            self.duplicate_selected();
        }
        if delete {
            self.delete_selected();
        }
        if let (Some(id), Some(delta)) = (self.selected, nudge) {
            let previous = self.design.clone();
            if let Some(element) = self.design.element_mut(id) {
                element.position[0] += delta[0];
                element.position[1] += delta[1];
                clamp_element_position(&mut element.position);
                self.history.record(previous);
            }
        }
    }

    fn import_image(&mut self, context: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        else {
            return;
        };

        let (asset_id, aspect_ratio) = match self.design.import_asset_path(&path) {
            Ok(imported) => imported,
            Err(error) => {
                self.set_status(format!("Could not import image: {error}"), true);
                return;
            }
        };
        let texture = match self
            .design
            .asset(&asset_id)
            .and_then(|asset| load_texture(context, &asset_id, &asset.data))
        {
            Ok(texture) => texture,
            Err(error) => {
                self.set_status(error, true);
                return;
            }
        };
        self.history.record(self.design.clone());
        let id = self.design.add_image(asset_id.clone(), aspect_ratio);
        self.textures.insert(asset_id, texture);
        self.selected = Some(id);
        self.tool = Tool::Select;
        self.set_status("Image layer added", false);
    }

    fn import_background_image(&mut self, context: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        else {
            return;
        };

        let (asset_id, aspect_ratio) = match self.design.import_asset_path(&path) {
            Ok(imported) => imported,
            Err(error) => {
                self.set_status(format!("Could not import backdrop: {error}"), true);
                return;
            }
        };
        let texture = match self
            .design
            .asset(&asset_id)
            .and_then(|asset| load_texture(context, &asset_id, &asset.data))
        {
            Ok(texture) => texture,
            Err(error) => {
                self.set_status(error, true);
                return;
            }
        };
        self.history.record(self.design.clone());
        self.design
            .set_background_image(asset_id.clone(), aspect_ratio);
        self.textures.insert(asset_id, texture);
        self.set_status("Backdrop photo added", false);
    }

    fn import_series(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("CSV data", &["csv", "tsv"])
            .pick_file()
        else {
            return;
        };
        match SeriesData::from_path(&path) {
            Ok(series) => {
                let row_count = series.rows.len();
                let column_count = series.columns.len();
                self.series = Some(series);
                self.set_status(
                    format!("Loaded {row_count} entries with {column_count} variables"),
                    false,
                );
            }
            Err(error) => self.set_status(error, true),
        }
    }

    fn insert_series_variable(&mut self, placeholder: &str) {
        let Some(id) = self.selected else {
            self.set_status("Select a text or QR layer first", true);
            return;
        };
        let previous = self.design.clone();
        let Some(element) = self.design.element_mut(id) else {
            return;
        };
        let content = match &mut element.kind {
            ElementKind::Text { text, .. } => text,
            ElementKind::QrCode { content, .. } => content,
            _ => {
                self.set_status("Variables can be inserted into text or QR layers", true);
                return;
            }
        };
        if !content.is_empty() && !content.ends_with(char::is_whitespace) {
            content.push(' ');
        }
        content.push_str(placeholder);
        self.history.record(previous);
        self.set_status(format!("Inserted {placeholder}"), false);
    }

    fn save_project(&mut self) {
        let path = self.project_path.clone().or_else(|| {
            rfd::FileDialog::new()
                .add_filter("Button Studio project", &["buttonstudio"])
                .set_file_name("my-button.buttonstudio")
                .save_file()
        });
        let Some(mut path) = path else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension("buttonstudio");
        }

        const PREVIEW_SIZE: u32 = 512;
        let preview = match render_project_preview_png(&self.design, PREVIEW_SIZE) {
            Ok(preview) => preview,
            Err(error) => {
                self.set_status(format!("Could not render project preview: {error}"), true);
                return;
            }
        };
        let json = match self.design.to_project_json_with_preview(Some((
            &preview,
            PREVIEW_SIZE,
            PREVIEW_SIZE,
        ))) {
            Ok(json) => json,
            Err(error) => {
                self.set_status(format!("Could not prepare project: {error}"), true);
                return;
            }
        };
        match fs::write(&path, json) {
            Ok(()) => {
                self.project_path = Some(path);
                self.set_status("Project saved", false);
            }
            Err(error) => self.set_status(format!("Could not save project: {error}"), true),
        }
    }

    fn open_project(&mut self, context: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Button Studio project", &["buttonstudio", "json"])
            .pick_file()
        else {
            return;
        };
        let result = fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| Design::from_project_json(&bytes, Some(&path)));
        match result {
            Ok(design) => {
                let fonts = design_fonts(&design);
                match self.font_catalog.register_design_fonts(context, &fonts) {
                    Ok(()) => {
                        self.pending_project = Some((design, path));
                        context.request_discard("activate project fonts");
                        context.request_repaint();
                    }
                    Err(error) => self.set_status(error, true),
                }
            }
            Err(error) => self.set_status(format!("Could not open project: {error}"), true),
        }
    }

    fn reload_textures(&mut self, context: &egui::Context) {
        self.textures.clear();
        let mut asset_ids = self
            .design
            .elements
            .iter()
            .filter_map(|element| match &element.kind {
                ElementKind::Image { asset_id, .. } => Some(asset_id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if let Some(background_image) = &self.design.background_image {
            asset_ids.push(background_image.asset_id.clone());
        }
        asset_ids.sort_unstable();
        asset_ids.dedup();
        for asset_id in asset_ids {
            if let Ok(asset) = self.design.asset(&asset_id)
                && let Ok(texture) = load_texture(context, &asset_id, &asset.data)
            {
                self.textures.insert(asset_id, texture);
            }
        }
    }

    fn apply_pending_updates(&mut self, context: &egui::Context) {
        if let Some((design, path)) = self.pending_project.take() {
            self.design = design;
            self.project_path = Some(path);
            self.selected = self.design.elements.last().map(|element| element.id);
            self.history.clear();
            self.reload_textures(context);
            self.set_status("Project opened", false);
        }

        if let Some((id, font)) = self.pending_font_change.take() {
            let previous = self.design.clone();
            if let Some(element) = self.design.element_mut(id)
                && let ElementKind::Text {
                    font: element_font, ..
                } = &mut element.kind
            {
                *element_font = font;
                self.history.record(previous);
                self.set_status("Text font updated", false);
            }
        }
    }

    fn font_browser(&mut self, context: &egui::Context) {
        if !self.font_browser_open {
            return;
        }
        let Some(target) = self.font_browser_target else {
            self.font_browser_open = false;
            return;
        };
        let Some(current) = self
            .design
            .element(target)
            .and_then(|element| match &element.kind {
                ElementKind::Text { font, .. } => Some(font.clone()),
                _ => None,
            })
        else {
            self.font_browser_open = false;
            self.font_browser_target = None;
            return;
        };

        let available = &self.font_catalog.available;
        let search = &mut self.font_search;
        let focus_search = &mut self.focus_font_search;
        let mut selected = None;
        let mut open = self.font_browser_open;
        let mut close_after_selection = false;

        egui::Window::new("Installed fonts")
            .id(egui::Id::new("installed-font-browser"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(430.0, 520.0))
            .min_width(340.0)
            .min_height(360.0)
            .show(context, |ui| {
                ui.label(
                    RichText::new("Search every font face installed on this computer.")
                        .size(12.0)
                        .color(MUTED),
                );
                let response = ui.add(
                    egui::TextEdit::singleline(search)
                        .hint_text("Search family, weight, or style")
                        .desired_width(ui.available_width()),
                );
                if *focus_search {
                    response.request_focus();
                    *focus_search = false;
                }

                let matches = matching_fonts(available, search);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} matching faces", matches.len()))
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{} installed", available.len()))
                                .size(11.0)
                                .color(MUTED),
                        );
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, 30.0, matches.len(), |ui, range| {
                        for font in &matches[range] {
                            if ui
                                .selectable_label(font.spec == current, &font.label)
                                .clicked()
                            {
                                selected = Some(font.spec.clone());
                                close_after_selection = true;
                            }
                        }
                    });
            });

        self.font_browser_open = open && !close_after_selection;
        if !self.font_browser_open {
            self.font_browser_target = None;
        }
        if let Some(font) = selected {
            match self.font_catalog.ensure_registered(context, &font) {
                Ok(()) => {
                    self.pending_font_change = Some((target, font));
                    context.request_discard("activate selected system font");
                    context.request_repaint();
                }
                Err(error) => self.set_status(error, true),
            }
        }
    }

    fn export_design(&mut self) {
        let file_name = format!("button-{}mm.png", self.design.diameter_mm);
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name(&file_name)
            .save_file()
        else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension("png");
        }
        match export_png(&self.design, &path) {
            Ok(size) => self.set_status(format!("Exported {size} × {size} px PNG"), false),
            Err(error) => self.set_status(error, true),
        }
    }

    fn export_pdf(&mut self) {
        let kind = if self.series.is_some() {
            "series"
        } else {
            "sheet"
        };
        let file_name = format!("button-{}mm-{kind}.pdf", self.design.diameter_mm);
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter("PDF document", &["pdf"])
            .set_file_name(&file_name)
            .save_file()
        else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension("pdf");
        }
        if let Some(series) = &self.series {
            match export_pdf_series(&self.design, &series.rows, &path) {
                Ok(summary) => self.set_status(
                    format!(
                        "Exported {} personalized buttons across {} A4 pages",
                        summary.buttons, summary.pages
                    ),
                    false,
                ),
                Err(error) => self.set_status(error, true),
            }
        } else {
            match export_pdf_sheet(&self.design, &path) {
                Ok(count) => self.set_status(
                    format!("Exported A4 PDF with {count} full-size buttons"),
                    false,
                ),
                Err(error) => self.set_status(error, true),
            }
        }
    }

    fn clear_canvas(&mut self) {
        if self.design.elements.is_empty() {
            return;
        }
        self.history.record(self.design.clone());
        self.design.elements.clear();
        self.selected = None;
        self.set_status("Canvas cleared · Undo is available", false);
    }

    fn duplicate_selected(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        self.history.record(self.design.clone());
        if let Some(new_id) = self.design.duplicate(id) {
            self.selected = Some(new_id);
            self.set_status("Layer duplicated", false);
        }
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        self.history.record(self.design.clone());
        if self.design.delete(id) {
            self.selected = self.design.elements.last().map(|element| element.id);
            self.set_status("Layer deleted", false);
        }
    }

    fn move_selected(&mut self, toward_front: bool) {
        let Some(id) = self.selected else {
            return;
        };
        let previous = self.design.clone();
        if self.design.move_layer(id, toward_front) {
            self.history.record(previous);
            self.set_status("Layer order updated", false);
        }
    }

    fn undo(&mut self) {
        if self.history.undo(&mut self.design) {
            self.selected = self
                .selected
                .filter(|id| self.design.element(*id).is_some())
                .or_else(|| self.design.elements.last().map(|element| element.id));
            self.set_status("Undid last change", false);
        }
    }

    fn redo(&mut self) {
        if self.history.redo(&mut self.design) {
            self.selected = self
                .selected
                .filter(|id| self.design.element(*id).is_some())
                .or_else(|| self.design.elements.last().map(|element| element.id));
            self.set_status("Redid change", false);
        }
    }

    fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = message.into();
        self.status_is_error = is_error;
    }
}

impl eframe::App for ButtonStudio {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.pull_shared_document(ui.ctx());
        self.apply_pending_updates(ui.ctx());
        self.handle_shortcuts(ui.ctx());
        self.top_bar(ui);
        self.tool_panel(ui);
        self.right_panel(ui);
        self.canvas(ui);
        self.font_browser(ui.ctx());
        self.publish_shared_document(ui.ctx());
    }
}

fn icon_button(ui: &mut Ui, icon_name: &str, tooltip: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        Button::new(RichText::new(icon(icon_name)).size(17.0))
            .frame(false)
            .min_size(vec2(34.0, 34.0)),
    )
    .on_hover_text(tooltip)
}

fn top_icon_button(ui: &mut Ui, icon_name: &str, tooltip: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        Button::new(
            RichText::new(icon(icon_name))
                .size(17.0)
                .color(Color32::from_gray(220)),
        )
        .frame(false)
        .min_size(vec2(34.0, 34.0)),
    )
    .on_hover_text(tooltip)
}

fn section_label(text: &str) -> RichText {
    RichText::new(text).size(10.0).strong().color(MUTED)
}

fn font_picker_button(ui: &mut Ui, current: &TextFont, available: &[AvailableFont]) -> bool {
    ui.label(section_label("FONT"));
    let current_label = available
        .iter()
        .find(|candidate| candidate.spec == *current)
        .map(|candidate| candidate.label.as_str())
        .unwrap_or(current.family.as_str());
    ui.add_sized(
        [ui.available_width(), 38.0],
        Button::new(icon_text("search", current_label)),
    )
    .on_hover_text("Browse installed fonts")
    .clicked()
}

fn design_fonts(design: &Design) -> Vec<TextFont> {
    design
        .elements
        .iter()
        .filter_map(|element| match &element.kind {
            ElementKind::Text { font, .. } => Some(font.clone()),
            _ => None,
        })
        .collect()
}

fn matching_fonts<'a>(available: &'a [AvailableFont], search: &str) -> Vec<&'a AvailableFont> {
    let query = search.trim().to_lowercase();
    available
        .iter()
        .filter(|font| query.is_empty() || font.label.to_lowercase().contains(&query))
        .collect()
}

fn text_arc_controls(ui: &mut Ui, curvature: &mut f32) -> bool {
    const DEFAULT_CURVATURE: f32 = 0.65;
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui
            .selectable_label(curvature.abs() <= f32::EPSILON, "Straight")
            .clicked()
        {
            changed |= *curvature != 0.0;
            *curvature = 0.0;
        }
        if ui.selectable_label(*curvature > 0.0, "Top arc").clicked() {
            let next = if *curvature > 0.0 {
                *curvature
            } else {
                DEFAULT_CURVATURE
            };
            changed |= *curvature != next;
            *curvature = next;
        }
        if ui
            .selectable_label(*curvature < 0.0, "Bottom arc")
            .clicked()
        {
            let next = if *curvature < 0.0 {
                *curvature
            } else {
                -DEFAULT_CURVATURE
            };
            changed |= *curvature != next;
            *curvature = next;
        }
    });
    changed |= ui
        .add(egui::Slider::new(curvature, -1.0..=1.0).text("Curve strength"))
        .on_hover_text("Adjust the text's arc; zero keeps it straight")
        .changed();
    changed
}

fn resize_handle_at(
    pointer: egui::Pos2,
    bounds: egui::Rect,
    canvas_rect: egui::Rect,
) -> Option<usize> {
    selection_handles(bounds, canvas_rect)
        .into_iter()
        .position(|handle| handle.distance(pointer) <= 13.0)
}

fn element_size(kind: &ElementKind) -> f32 {
    match kind {
        ElementKind::Text { font_size, .. } => *font_size,
        ElementKind::Shape { size, .. } => *size,
        ElementKind::Image { width, .. } => *width,
        ElementKind::QrCode { size, .. } => *size,
    }
}

fn set_element_size(kind: &mut ElementKind, size: f32) {
    match kind {
        ElementKind::Text { font_size, .. } => *font_size = size.clamp(0.025, 0.2),
        ElementKind::Shape {
            size: shape_size, ..
        } => *shape_size = size.clamp(0.06, 0.68),
        ElementKind::Image { width, .. } => *width = size.clamp(0.08, IMAGE_WIDTH_LIMIT),
        ElementKind::QrCode { size: qr_size, .. } => *qr_size = size.clamp(0.08, 1.2),
    }
}

fn align_element(
    design: &mut Design,
    painter: &egui::Painter,
    id: u64,
    reference: Option<u64>,
    alignment: ElementAlignment,
) -> bool {
    const MEASUREMENT_RADIUS: f32 = 1_000.0;

    let center = egui::Pos2::ZERO;
    let Some(element) = design.element(id) else {
        return false;
    };
    let source = element_bounds(painter, center, MEASUREMENT_RADIUS, element);
    let target = match reference.and_then(|reference| design.element(reference)) {
        Some(reference) => element_bounds(painter, center, MEASUREMENT_RADIUS, reference),
        None => egui::Rect::from_center_size(center, Vec2::splat(MEASUREMENT_RADIUS * 2.0)),
    };
    let delta = alignment_delta(source, target, alignment) / MEASUREMENT_RADIUS;
    if delta == Vec2::ZERO {
        return false;
    }

    let Some(element) = design.element_mut(id) else {
        return false;
    };
    element.position[0] =
        (element.position[0] + delta.x).clamp(-ELEMENT_POSITION_LIMIT, ELEMENT_POSITION_LIMIT);
    element.position[1] =
        (element.position[1] + delta.y).clamp(-ELEMENT_POSITION_LIMIT, ELEMENT_POSITION_LIMIT);
    true
}

fn alignment_delta(source: egui::Rect, target: egui::Rect, alignment: ElementAlignment) -> Vec2 {
    match alignment {
        ElementAlignment::Left => vec2(target.left() - source.left(), 0.0),
        ElementAlignment::HorizontalCenter => vec2(target.center().x - source.center().x, 0.0),
        ElementAlignment::Right => vec2(target.right() - source.right(), 0.0),
        ElementAlignment::Top => vec2(0.0, target.top() - source.top()),
        ElementAlignment::VerticalCenter => vec2(0.0, target.center().y - source.center().y),
        ElementAlignment::Bottom => vec2(0.0, target.bottom() - source.bottom()),
    }
}

fn shape_icon(shape: ShapeKind) -> &'static str {
    match shape {
        ShapeKind::Star => "star",
        ShapeKind::Heart => "heart",
        ShapeKind::Circle => "circle",
        ShapeKind::Rectangle => "rectangle-horizontal",
        ShapeKind::Diamond => "diamond",
        ShapeKind::Bolt => "zap",
    }
}

fn palette() -> [DesignColor; 8] {
    [
        DesignColor::rgb(27, 102, 91),
        DesignColor::rgb(225, 75, 52),
        DesignColor::rgb(246, 190, 59),
        DesignColor::rgb(48, 56, 75),
        DesignColor::rgb(43, 123, 178),
        DesignColor::rgb(220, 91, 135),
        DesignColor::rgb(247, 239, 218),
        DesignColor::INK,
    ]
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    let mut output = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

fn clamp_element_position(position: &mut [f32; 2]) {
    let length = (position[0].powi(2) + position[1].powi(2)).sqrt();
    if length > ELEMENT_POSITION_LIMIT {
        position[0] *= ELEMENT_POSITION_LIMIT / length;
        position[1] *= ELEMENT_POSITION_LIMIT / length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_handles_are_hit_near_corners_only() {
        let bounds = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(110.0, 80.0));
        let canvas_rect = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(120.0, 100.0));

        assert_eq!(
            resize_handle_at(egui::pos2(13.0, 22.0), bounds, canvas_rect),
            Some(0)
        );
        assert_eq!(resize_handle_at(bounds.center(), bounds, canvas_rect), None);
    }

    #[test]
    fn resize_handles_remain_reachable_for_oversized_elements() {
        let canvas_rect = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(100.0, 100.0));
        let bounds = egui::Rect::from_min_max(egui::pos2(-50.0, -80.0), egui::pos2(150.0, 180.0));

        assert_eq!(
            resize_handle_at(egui::pos2(8.0, 8.0), bounds, canvas_rect),
            Some(0)
        );
        assert_eq!(
            resize_handle_at(egui::pos2(92.0, 92.0), bounds, canvas_rect),
            Some(3)
        );
    }

    #[test]
    fn drawing_area_resize_clamps_each_element_type() {
        let mut text = ElementKind::Text {
            text: "Text".to_owned(),
            font_size: 0.1,
            font: Default::default(),
            curvature: 0.0,
        };
        let mut shape = ElementKind::Shape {
            shape: ShapeKind::Star,
            size: 0.2,
        };
        let mut image = ElementKind::Image {
            asset_id: "image".to_owned(),
            width: 0.5,
            aspect_ratio: 1.0,
        };
        let mut qr_code = ElementKind::QrCode {
            content: "https://example.com".to_owned(),
            size: 0.46,
            background: DesignColor::WHITE,
            transparent_background: false,
        };

        set_element_size(&mut text, 10.0);
        set_element_size(&mut shape, 10.0);
        set_element_size(&mut image, 10.0);
        set_element_size(&mut qr_code, 10.0);

        assert_eq!(element_size(&text), 0.2);
        assert_eq!(element_size(&shape), 0.68);
        assert_eq!(element_size(&image), IMAGE_WIDTH_LIMIT);
        assert_eq!(element_size(&qr_code), 1.2);
    }

    #[test]
    fn element_position_is_clamped_to_canvas_radius() {
        let mut position = [0.8, 0.8];

        clamp_element_position(&mut position);

        let length = (position[0].powi(2) + position[1].powi(2)).sqrt();
        assert!((length - ELEMENT_POSITION_LIMIT).abs() < f32::EPSILON);
    }

    #[test]
    fn alignment_delta_matches_requested_edges_and_centers() {
        let source = egui::Rect::from_min_max(egui::pos2(30.0, 40.0), egui::pos2(70.0, 80.0));
        let target = egui::Rect::from_min_max(egui::pos2(-100.0, -90.0), egui::pos2(100.0, 90.0));

        let left = source.translate(alignment_delta(source, target, ElementAlignment::Left));
        let horizontal_center = source.translate(alignment_delta(
            source,
            target,
            ElementAlignment::HorizontalCenter,
        ));
        let right = source.translate(alignment_delta(source, target, ElementAlignment::Right));
        let top = source.translate(alignment_delta(source, target, ElementAlignment::Top));
        let vertical_center = source.translate(alignment_delta(
            source,
            target,
            ElementAlignment::VerticalCenter,
        ));
        let bottom = source.translate(alignment_delta(source, target, ElementAlignment::Bottom));

        assert_eq!(left.left(), target.left());
        assert_eq!(horizontal_center.center().x, target.center().x);
        assert_eq!(right.right(), target.right());
        assert_eq!(top.top(), target.top());
        assert_eq!(vertical_center.center().y, target.center().y);
        assert_eq!(bottom.bottom(), target.bottom());
    }

    #[test]
    fn installed_font_search_matches_family_weight_and_style_case_insensitively() {
        let available = vec![
            AvailableFont {
                label: "Avenir Next · Bold Italic".to_owned(),
                spec: TextFont::default(),
            },
            AvailableFont {
                label: "Courier · Regular".to_owned(),
                spec: TextFont::default(),
            },
        ];

        assert_eq!(matching_fonts(&available, "avenir").len(), 1);
        assert_eq!(matching_fonts(&available, "BOLD ITALIC").len(), 1);
        assert_eq!(matching_fonts(&available, "regular").len(), 1);
        assert_eq!(matching_fonts(&available, "missing").len(), 0);
    }
}
