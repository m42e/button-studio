use crate::{
    document::SharedDocument,
    fonts::{AvailableFont, FontCatalog, FontSlant, FontWeight},
    model::{
        BackgroundStyle, Design, DesignColor, ELEMENT_POSITION_LIMIT, Element, ElementKind,
        History, IMAGE_WIDTH_LIMIT, ImageAsset, ShapeKind, TextFont,
    },
    production::{self, IssueSeverity, PreflightIssue, RowPreflight},
    render::{
        TextureMap, element_bounds, export_pdf_series, export_pdf_sheet, export_pixel_size,
        export_png, hit_test, load_texture, paint_design, paint_preview, paint_trim_boundary,
        render_project_preview_png, selection_bounds, selection_handles,
    },
    series::SeriesData,
    svg,
    templates::{self, DesignTemplate, SavedPalette, SavedStyle},
    theme::{ACCENT, INK, MINT, MUTED, PAPER, SURFACE, display_font, icon, icon_text},
};
use eframe::egui::{
    self, Align, Button, Color32, Frame, Key, Layout, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, Ui, Vec2, vec2,
};
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

const SNAP_TOLERANCE_POINTS: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Text,
    Shape,
    QrCode,
    Image,
    Background,
    Series,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExportKind {
    Png,
    PdfSheet,
    PdfSeries,
    Svg,
}

#[derive(Clone, Copy)]
enum LibraryAction {
    ApplyBuiltin(usize),
    SaveTemplate,
    LoadTemplate,
    SavePalette,
    LoadPalette,
    SaveStyle,
    LoadStyle,
}

#[derive(Clone)]
enum PendingLibraryUpdate {
    Design(Design),
    Style(SavedStyle, Vec<u64>),
}

struct ExportPreflightCache {
    design: Design,
    series_signature: u64,
    design_issues: Vec<PreflightIssue>,
    row_issues: Vec<RowPreflight>,
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
    ResizeShape {
        id: u64,
        width: f32,
        height: f32,
        center: egui::Pos2,
        rotation: f32,
    },
    ResizeSelection {
        id: u64,
        pivot: egui::Pos2,
        start_distance: f32,
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
            Self::Move { id }
            | Self::Resize { id, .. }
            | Self::ResizeShape { id, .. }
            | Self::ResizeSelection { id, .. } => id,
        }
    }
}

impl Tool {
    const ALL: [Self; 7] = [
        Self::Select,
        Self::Text,
        Self::Shape,
        Self::QrCode,
        Self::Image,
        Self::Background,
        Self::Series,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Text => "Text",
            Self::Shape => "Shapes",
            Self::QrCode => "QR code",
            Self::Image => "Images",
            Self::Background => "Backdrop",
            Self::Series => "Series",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Select => "mouse-pointer-2",
            Self::Text => "type",
            Self::Shape => "shapes",
            Self::QrCode => "qr-code",
            Self::Image => "image-plus",
            Self::Background => "palette",
            Self::Series => "table-2",
        }
    }
}

pub struct ButtonStudio {
    design: Design,
    document: SharedDocument,
    document_revision: u64,
    history: History,
    selected: Option<u64>,
    selected_ids: Vec<u64>,
    clipboard: Vec<Element>,
    clipboard_assets: HashMap<String, ImageAsset>,
    layer_drag_source: Option<u64>,
    keep_proportions: bool,
    snapping_enabled: bool,
    tool: Tool,
    draft_text: String,
    draft_text_curvature: f32,
    draft_qr: String,
    series_data: Option<SeriesData>,
    align_reference: Option<u64>,
    textures: TextureMap,
    project_path: Option<PathBuf>,
    status: String,
    status_is_error: bool,
    show_guides: bool,
    canvas_interaction: Option<CanvasInteraction>,
    resize_snapshot: Vec<Element>,
    marquee_start: Option<egui::Pos2>,
    font_catalog: FontCatalog,
    font_search: String,
    font_browser_family: String,
    font_browser_weight: FontWeight,
    font_browser_slant: FontSlant,
    font_preview_key: Option<String>,
    font_preview_texture: Option<egui::TextureHandle>,
    font_browser_open: bool,
    font_browser_target: Option<u64>,
    focus_font_search: bool,
    pending_font_change: Option<(u64, TextFont)>,
    pending_project: Option<(Design, PathBuf)>,
    project_paths: Receiver<PathBuf>,
    library_open: bool,
    export_review_open: bool,
    export_kind: ExportKind,
    export_start: usize,
    export_end: usize,
    export_copies: usize,
    export_preview_entry: usize,
    skip_invalid_rows: bool,
    export_preflight_cache: Option<ExportPreflightCache>,
    saved_palette: Option<SavedPalette>,
    pending_library_update: Option<PendingLibraryUpdate>,
    library_template_name: String,
    help_open: bool,
    confirm_clear: bool,
}

impl ButtonStudio {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        Self::with_document(context, SharedDocument::new(Design::default()))
    }

    pub fn with_document(context: &eframe::CreationContext<'_>, document: SharedDocument) -> Self {
        let (_, project_paths) = mpsc::channel();
        Self::with_document_and_project_paths(context, document, project_paths)
    }

    pub fn with_document_and_project_paths(
        context: &eframe::CreationContext<'_>,
        document: SharedDocument,
        project_paths: Receiver<PathBuf>,
    ) -> Self {
        crate::theme::install(&context.egui_ctx);
        let font_catalog = FontCatalog::discover();
        let font_count = font_catalog.available.len();
        let (design, document_revision) = document
            .snapshot()
            .unwrap_or_else(|_| (Design::default(), 0));
        let selected = design.elements.last().map(|element| element.id);
        Self {
            design,
            document,
            document_revision,
            history: History::default(),
            selected,
            selected_ids: selected.into_iter().collect(),
            clipboard: Vec::new(),
            clipboard_assets: HashMap::new(),
            layer_drag_source: None,
            keep_proportions: false,
            snapping_enabled: true,
            tool: Tool::Select,
            draft_text: "YOUR TEXT".to_owned(),
            draft_text_curvature: 0.0,
            draft_qr: "https://example.com".to_owned(),
            series_data: None,
            align_reference: None,
            textures: TextureMap::default(),
            project_path: None,
            status: format!("Ready · {font_count} fonts available"),
            status_is_error: false,
            show_guides: true,
            canvas_interaction: None,
            resize_snapshot: Vec::new(),
            marquee_start: None,
            font_catalog,
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

    fn sync_from_document(&mut self, context: &egui::Context) {
        let Ok((design, revision)) = self.document.snapshot() else {
            return;
        };
        if revision == self.document_revision {
            return;
        }
        if design == self.design {
            self.document_revision = revision;
            return;
        }
        self.history.record(self.design.clone());
        self.design = design;
        self.document_revision = revision;
        self.selected = self
            .selected
            .filter(|id| self.design.element(*id).is_some())
            .or_else(|| self.design.elements.last().map(|element| element.id));
        self.selected_ids
            .retain(|id| self.design.element(*id).is_some());
        if self.selected_ids.is_empty() {
            self.selected_ids = self.selected.into_iter().collect();
        }
        self.reload_textures(context);
        self.set_status("Design updated through MCP", false);
    }

    fn publish_to_document(&mut self) {
        if let Ok(revision) = self
            .document
            .replace_if_revision(self.document_revision, &self.design)
        {
            self.document_revision = revision;
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
                        if ui
                            .add(
                                Button::new(
                                    RichText::new(icon_text("download", "Export & review"))
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(ACCENT)
                                .min_size(vec2(150.0, 40.0)),
                            )
                            .on_hover_text("Check production warnings and export options")
                            .clicked()
                        {
                            self.open_export_review();
                        }
                        if ui.button(icon_text("library", "Library")).clicked() {
                            self.library_open = true;
                        }
                        if ui.button(icon_text("circle-help", "Help")).clicked() {
                            self.help_open = true;
                        }
                        if ui
                            .add(Button::new(icon_text("save", "Save")).min_size(vec2(88.0, 40.0)))
                            .on_hover_text("Save project")
                            .clicked()
                        {
                            self.save_project();
                        }
                        if ui
                            .add(
                                Button::new(icon_text("save", "Save As"))
                                    .min_size(vec2(100.0, 40.0)),
                            )
                            .on_hover_text("Save project to a new file")
                            .clicked()
                        {
                            self.save_project_as();
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
                            self.confirm_clear = true;
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
                                            RichText::new(icon_text(tool.icon(), tool.label()))
                                                .color(foreground),
                                        )
                                        .fill(fill)
                                        .min_size(vec2(124.0, 42.0)),
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
                            Tool::QrCode => self.qr_code_controls(ui),
                            Tool::Image => self.image_controls(ui, &context),
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
                        paint_preview(ui.painter(), rect, &self.design, &self.textures);

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
                        ui.label(RichText::new("BLEED · TRIM · SAFE").size(9.0).color(MUTED));
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
                    painter.circle_filled(center, radius + 4.0, Color32::from_black_alpha(28));
                    paint_design(
                        &painter,
                        center,
                        radius,
                        &self.design,
                        &self.textures,
                        self.selected.filter(|_| self.selected_ids.len() <= 1),
                    );
                    if self.selected_ids.len() > 1 {
                        let combined = self
                            .selected_ids
                            .iter()
                            .filter_map(|id| self.design.element(*id))
                            .map(|element| selection_bounds(&painter, center, radius, element))
                            .reduce(|a, b| a.union(b));
                        if let Some(bounds) = combined {
                            painter.rect_stroke(
                                bounds,
                                0.0,
                                Stroke::new(1.5, ACCENT),
                                StrokeKind::Outside,
                            );
                            for handle in selection_handles(bounds, rect) {
                                painter.circle_filled(handle, 4.5, Color32::WHITE);
                                painter.circle_stroke(handle, 4.5, Stroke::new(1.0, ACCENT));
                            }
                        }
                    }
                    if self.show_guides {
                        painter.circle_stroke(
                            center,
                            radius,
                            Stroke::new(1.0, Color32::from_rgba_unmultiplied(224, 153, 45, 120)),
                        );
                        paint_trim_boundary(&painter, center, radius, &self.design);
                        let safe_radius = ((self.design.diameter_mm as f32 / 2.0 - 2.0).max(0.0))
                            / (self.design.artwork_diameter_mm() / 2.0)
                            * radius;
                        painter.circle_stroke(
                            center,
                            safe_radius,
                            Stroke::new(1.0, Color32::from_rgba_unmultiplied(54, 148, 113, 150)),
                        );
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
                        let hit = hit_test(&painter, pointer, center, radius, &self.design);
                        self.select_hit(hit, ui.input(|input| input.modifiers.shift));
                        if self.selected.is_some() {
                            self.tool = Tool::Select;
                        }
                    }

                    if response.drag_started()
                        && let Some(pointer) = response.interact_pointer_pos()
                    {
                        let resize = if self.selected_ids.len() > 1 {
                            let snapshot =
                                crate::editor::resize_snapshot(&self.design, &self.selected_ids);
                            crate::editor::composition_bounds(&snapshot).and_then(|bounds| {
                                let screen = egui::Rect::from_min_max(
                                    center + bounds.min.to_vec2() * radius,
                                    center + bounds.max.to_vec2() * radius,
                                )
                                .expand(5.0);
                                resize_handle_at(pointer, screen, rect).and_then(|_| {
                                    let id = self.selected?;
                                    let pivot = center + bounds.center().to_vec2() * radius;
                                    self.resize_snapshot = snapshot;
                                    Some(CanvasInteraction::ResizeSelection {
                                        id,
                                        pivot,
                                        start_distance: pointer.distance(pivot).max(1.0),
                                    })
                                })
                            })
                        } else {
                            self.selected.and_then(|id| {
                                let element = self.design.element(id)?;
                                if element.locked || !element.visible {
                                    return None;
                                }
                                let bounds = selection_bounds(&painter, center, radius, element);
                                resize_handle_at(pointer, bounds, rect).map(|_| {
                                    if let ElementKind::Shape {
                                        shape,
                                        size,
                                        width,
                                        height,
                                    } = &element.kind
                                    {
                                        let (width, height) =
                                            shape_layer_dimensions(*shape, *size, *width, *height);
                                        CanvasInteraction::ResizeShape {
                                            id,
                                            width,
                                            height,
                                            center: center
                                                + vec2(element.position[0], element.position[1])
                                                    * radius,
                                            rotation: element.rotation,
                                        }
                                    } else {
                                        CanvasInteraction::Resize {
                                            id,
                                            start_distance: pointer
                                                .distance(
                                                    center
                                                        + vec2(
                                                            element.position[0],
                                                            element.position[1],
                                                        ) * radius,
                                                )
                                                .max(1.0),
                                            start_size: element_size(&element.kind),
                                        }
                                    }
                                })
                            })
                        };
                        let interaction = resize.or_else(|| {
                            hit_test(&painter, pointer, center, radius, &self.design)
                                .map(|id| CanvasInteraction::Move { id })
                        });
                        if interaction.is_none() {
                            self.marquee_start = Some(pointer);
                        }

                        if let Some(interaction) = interaction {
                            self.selected = Some(interaction.id());
                            if !self.selected_ids.contains(&interaction.id()) {
                                self.selected_ids =
                                    crate::editor::selection_ids(&self.design, &[interaction.id()]);
                            }
                            self.canvas_interaction = Some(interaction);
                            self.tool = Tool::Select;
                            self.history.record(self.design.clone());
                        }
                    }

                    if response.dragged() {
                        if self.canvas_interaction.is_none()
                            && let (Some(start), Some(pointer)) =
                                (self.marquee_start, response.interact_pointer_pos())
                        {
                            let marquee = egui::Rect::from_two_pos(start, pointer);
                            painter.rect_stroke(
                                marquee,
                                0.0,
                                Stroke::new(1.0, ACCENT),
                                StrokeKind::Outside,
                            );
                        }
                        match self.canvas_interaction {
                            Some(CanvasInteraction::ResizeSelection {
                                pivot,
                                start_distance,
                                ..
                            }) => {
                                if let Some(pointer) = response.interact_pointer_pos() {
                                    crate::editor::resize_from_snapshot(
                                        &mut self.design,
                                        &self.resize_snapshot,
                                        pointer.distance(pivot) / start_distance,
                                    );
                                }
                            }
                            Some(CanvasInteraction::Move { id }) => {
                                let delta = ui.input(|input| input.pointer.delta()) / radius;
                                let ids = if self.selected_ids.contains(&id) {
                                    self.selected_ids.clone()
                                } else {
                                    vec![id]
                                };
                                crate::editor::move_selection(
                                    &mut self.design,
                                    &ids,
                                    [delta.x, delta.y],
                                );
                                if self.snapping_enabled {
                                    let bounds = self
                                        .selected_ids
                                        .iter()
                                        .filter(|sid| ids.contains(sid))
                                        .filter_map(|sid| self.design.element(*sid))
                                        .map(|e| selection_bounds(&painter, center, radius, e))
                                        .reduce(|a, b| a.union(b));
                                    if let Some(bounds) = bounds {
                                        let nx = [
                                            (bounds.left() - center.x) / radius,
                                            (bounds.center().x - center.x) / radius,
                                            (bounds.right() - center.x) / radius,
                                        ];
                                        let ny = [
                                            (bounds.top() - center.y) / radius,
                                            (bounds.center().y - center.y) / radius,
                                            (bounds.bottom() - center.y) / radius,
                                        ];
                                        let trim = self.design.trim_ratio();
                                        let safe = (self.design.diameter_mm as f32 - 4.0).max(0.0)
                                            / self.design.artwork_diameter_mm();
                                        let mut xs =
                                            vec![-1.0, 0.0, 1.0, -trim, 0.0, trim, -safe, safe];
                                        let mut ys = xs.clone();
                                        for other in self.design.elements.iter().filter(|other| {
                                            other.visible && !ids.contains(&other.id)
                                        }) {
                                            let b =
                                                selection_bounds(&painter, center, radius, other);
                                            xs.extend([
                                                (b.left() - center.x) / radius,
                                                (b.center().x - center.x) / radius,
                                                (b.right() - center.x) / radius,
                                            ]);
                                            ys.extend([
                                                (b.top() - center.y) / radius,
                                                (b.center().y - center.y) / radius,
                                                (b.bottom() - center.y) / radius,
                                            ]);
                                        }
                                        let sx = crate::editor::snap_bounds_axis(
                                            nx[0],
                                            nx[1],
                                            nx[2],
                                            &xs,
                                            SNAP_TOLERANCE_POINTS / radius,
                                        );
                                        let sy = crate::editor::snap_bounds_axis(
                                            ny[0],
                                            ny[1],
                                            ny[2],
                                            &ys,
                                            SNAP_TOLERANCE_POINTS / radius,
                                        );
                                        let correction = [
                                            sx.map_or(0.0, |(d, _)| d),
                                            sy.map_or(0.0, |(d, _)| d),
                                        ];
                                        crate::editor::move_selection(
                                            &mut self.design,
                                            &ids,
                                            correction,
                                        );
                                        if let Some((_, guide)) = sx {
                                            let x = center.x + guide * radius;
                                            painter.line_segment(
                                                [
                                                    egui::pos2(x, rect.top()),
                                                    egui::pos2(x, rect.bottom()),
                                                ],
                                                Stroke::new(1.0, ACCENT),
                                            );
                                        }
                                        if let Some((_, guide)) = sy {
                                            let y = center.y + guide * radius;
                                            painter.line_segment(
                                                [
                                                    egui::pos2(rect.left(), y),
                                                    egui::pos2(rect.right(), y),
                                                ],
                                                Stroke::new(1.0, ACCENT),
                                            );
                                        }
                                    }
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
                            Some(CanvasInteraction::ResizeShape {
                                id,
                                width,
                                height,
                                center: element_center,
                                rotation,
                            }) => {
                                if let Some(pointer) = response.interact_pointer_pos() {
                                    let delta = pointer - element_center;
                                    let angle = -rotation.to_radians();
                                    let local = vec2(
                                        delta.x * angle.cos() - delta.y * angle.sin(),
                                        delta.x * angle.sin() + delta.y * angle.cos(),
                                    );
                                    let mut next_width = (local.x.abs() / radius).clamp(0.02, 2.0);
                                    let mut next_height = (local.y.abs() / radius).clamp(0.02, 2.0);
                                    if ui.input(|input| input.modifiers.shift) {
                                        let width_scale = next_width / width;
                                        let height_scale = next_height / height;
                                        if (width_scale - 1.0).abs() >= (height_scale - 1.0).abs() {
                                            next_height =
                                                (next_width / width * height).clamp(0.02, 2.0);
                                        } else {
                                            next_width =
                                                (next_height / height * width).clamp(0.02, 2.0);
                                        }
                                    }
                                    if let Some(element) = self.design.element_mut(id)
                                        && let ElementKind::Shape { width, height, .. } =
                                            &mut element.kind
                                    {
                                        *width = Some(next_width);
                                        *height = Some(next_height);
                                    }
                                }
                            }
                            None => {}
                        }
                    }

                    if response.drag_stopped()
                        && let (Some(start), Some(pointer)) =
                            (self.marquee_start.take(), response.interact_pointer_pos())
                    {
                        let marquee = egui::Rect::from_two_pos(start, pointer);
                        self.selected_ids = self
                            .design
                            .elements
                            .iter()
                            .filter(|element| {
                                element.visible
                                    && !element.locked
                                    && selection_bounds(&painter, center, radius, element)
                                        .intersects(marquee)
                            })
                            .map(|element| element.id)
                            .collect();
                        self.selected_ids =
                            crate::editor::selection_ids(&self.design, &self.selected_ids);
                        self.selected = self.selected_ids.last().copied();
                        self.tool = Tool::Select;
                    }

                    if !ui.input(|input| input.pointer.primary_down()) {
                        self.canvas_interaction = None;
                        self.resize_snapshot.clear();
                    }
                    let resize_cursor = response.hover_pos().and_then(|pointer| {
                        if self.selected_ids.len() > 1 {
                            let snapshot =
                                crate::editor::resize_snapshot(&self.design, &self.selected_ids);
                            let bounds = crate::editor::composition_bounds(&snapshot)?;
                            let screen = egui::Rect::from_min_max(
                                center + bounds.min.to_vec2() * radius,
                                center + bounds.max.to_vec2() * radius,
                            )
                            .expand(5.0);
                            return resize_handle_at(pointer, screen, rect).map(|index| {
                                if index == 0 || index == 3 {
                                    egui::CursorIcon::ResizeNwSe
                                } else {
                                    egui::CursorIcon::ResizeNeSw
                                }
                            });
                        }
                        let element = self.selected.and_then(|id| self.design.element(id))?;
                        if element.locked {
                            return None;
                        }
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
        let current_font = self
            .design
            .element(id)
            .and_then(|element| match &element.kind {
                ElementKind::Text { font, .. } => Some(font.clone()),
                _ => None,
            });

        ui.label(RichText::new(truncate(&name, 32)).font(display_font(15.0)));
        ui.add_space(4.0);
        let previous = self.design.clone();
        let mut changed = false;
        let mut open_font_browser = false;

        let artwork_mm = self.design.artwork_diameter_mm();
        if let Some(element) = self.design.element_mut(id) {
            ui.horizontal(|ui| {
                ui.label("Name");
                let mut label = element
                    .name_override
                    .clone()
                    .unwrap_or_else(|| element.name());
                if ui.text_edit_singleline(&mut label).changed() {
                    element.name_override = if label.trim().is_empty() {
                        None
                    } else {
                        Some(label)
                    };
                    changed = true;
                }
            });
            changed |= ui.checkbox(&mut element.locked, "Lock layer").changed();
            if element.locked {
                ui.label(
                    RichText::new("Locked layers cannot be transformed or edited.").color(MUTED),
                );
                if changed {
                    self.history.record(previous);
                }
                return;
            }
            match &mut element.kind {
                ElementKind::Text {
                    text,
                    font,
                    curvature,
                    ..
                } => {
                    ui.label(section_label("CONTENT"));
                    changed |= ui.text_edit_multiline(text).changed();
                    ui.add_space(6.0);
                    ui.label(section_label("LAYOUT"));
                    changed |= text_arc_controls(ui, curvature);
                    ui.add_space(6.0);
                    open_font_browser = font_picker_button(ui, font, &self.font_catalog.available);
                }
                ElementKind::Shape { .. } => {}
                ElementKind::Image {
                    bleed_to_square, ..
                } => {
                    changed |= ui
                        .checkbox(bleed_to_square, "Bleed edges to square")
                        .changed();
                }
                ElementKind::QrCode {
                    content,
                    size: _,
                    background,
                    transparent_background,
                } => {
                    ui.label(section_label("CONTENT"));
                    changed |= ui.text_edit_multiline(content).changed();
                    changed |= ui
                        .checkbox(transparent_background, "Transparent background")
                        .changed();
                    if !*transparent_background {
                        ui.label(section_label("QR BACKGROUND"));
                        changed |= crate::color::edit_design_color(ui, background);
                    }
                }
            }

            ui.label(section_label("PHYSICAL SIZE"));
            let (mut width_mm, mut height_mm) = {
                let (w, h) = crate::editor::element_dimensions_ratio(element);
                (w * artwork_mm, h * artwork_mm)
            };
            let before_size = (width_mm, height_mm);
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut width_mm)
                        .range(0.5..=artwork_mm * 3.0)
                        .speed(0.1)
                        .suffix(" mm"),
                );
                ui.add(
                    egui::DragValue::new(&mut height_mm)
                        .range(0.5..=artwork_mm * 3.0)
                        .speed(0.1)
                        .suffix(" mm"),
                );
            });
            ui.checkbox(&mut self.keep_proportions, "Keep proportions while editing");
            if (width_mm, height_mm) != before_size {
                crate::editor::set_element_dimensions(
                    element,
                    width_mm / artwork_mm,
                    height_mm / artwork_mm,
                    self.keep_proportions,
                );
                changed = true;
            }

            ui.add_space(6.0);
            ui.label(section_label("POSITION"));
            ui.checkbox(&mut self.snapping_enabled, "Snap while dragging");
            ui.horizontal(|ui| {
                ui.label("X mm");
                let mut x_mm = (element.position[0] + 1.0) * artwork_mm * 0.5;
                if ui
                    .add(egui::DragValue::new(&mut x_mm).speed(0.1).suffix(" mm"))
                    .changed()
                {
                    element.position[0] = (x_mm * 2.0 / artwork_mm - 1.0).clamp(-1.0, 1.0);
                    changed = true;
                }
                ui.label("Y mm");
                let mut y_mm = (element.position[1] + 1.0) * artwork_mm * 0.5;
                if ui
                    .add(egui::DragValue::new(&mut y_mm).speed(0.1).suffix(" mm"))
                    .changed()
                {
                    element.position[1] = (y_mm * 2.0 / artwork_mm - 1.0).clamp(-1.0, 1.0);
                    changed = true;
                }
            });
            ui.add_space(6.0);
            ui.label(section_label("ROTATION"));
            changed |= ui
                .add(egui::Slider::new(&mut element.rotation, -180.0..=180.0).suffix("°"))
                .changed();
            changed |= ui
                .add(
                    egui::DragValue::new(&mut element.rotation)
                        .range(-180.0..=180.0)
                        .speed(0.25)
                        .suffix("°"),
                )
                .changed();
            {
                ui.horizontal(|ui| {
                    let can_flip = matches!(
                        element.kind,
                        ElementKind::Image { .. } | ElementKind::Shape { .. }
                    );
                    if can_flip && ui.button("Flip H").clicked() {
                        element.flip_x = !element.flip_x;
                        changed = true;
                    }
                    if can_flip && ui.button("Flip V").clicked() {
                        element.flip_y = !element.flip_y;
                        changed = true;
                    }
                    if ui.button("Center").clicked() && !element.locked {
                        element.position = [0.0, 0.0];
                        changed = true;
                    }
                });
            }

            if !matches!(element.kind, ElementKind::Image { .. }) {
                ui.add_space(6.0);
                ui.label(section_label("COLOR"));
                changed |= crate::color::edit_design_color(ui, &mut element.color);
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
            changed |= crate::appearance::controls(ui, element, artwork_mm);
        }

        if self.align_reference == Some(id)
            || self
                .align_reference
                .is_some_and(|reference| self.design.element(reference).is_none())
        {
            self.align_reference = None;
        }

        ui.add_space(6.0);
        ui.label(section_label("ALIGN"));
        let reference_label = self
            .align_reference
            .and_then(|reference| self.design.element(reference))
            .map(|element| truncate(&element.name(), 24))
            .unwrap_or_else(|| "Artwork page".to_owned());
        egui::ComboBox::from_id_salt("alignment_reference")
            .selected_text(reference_label)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.align_reference, None, "Artwork page");
                for element in self
                    .design
                    .elements
                    .iter()
                    .filter(|element| element.id != id)
                {
                    ui.selectable_value(
                        &mut self.align_reference,
                        Some(element.id),
                        truncate(&element.name(), 24),
                    );
                }
            });

        let mut alignment = None;
        ui.horizontal_wrapped(|ui| {
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
            let aligned = if self.selected_ids.len() > 1 {
                align_selection(
                    &mut self.design,
                    &painter,
                    &self.selected_ids,
                    self.align_reference,
                    alignment,
                )
            } else {
                align_element(
                    &mut self.design,
                    &painter,
                    id,
                    self.align_reference,
                    alignment,
                )
            };
            if aligned {
                changed = true;
                self.set_status("Layer aligned", false);
            }
        }

        if open_font_browser {
            self.font_browser_target = Some(id);
            self.font_browser_open = true;
            self.focus_font_search = true;
            self.font_search.clear();
            let initial_font = current_font
                .as_ref()
                .and_then(|current| {
                    self.font_catalog
                        .available
                        .iter()
                        .find(|candidate| candidate.spec == *current)
                        .or_else(|| {
                            self.font_catalog.available.iter().find(|candidate| {
                                candidate.spec.family.eq_ignore_ascii_case(&current.family)
                            })
                        })
                })
                .or_else(|| self.font_catalog.available.first());
            if let Some(font) = initial_font {
                self.font_browser_family = font.spec.family.clone();
                self.font_browser_weight = font.weight;
                self.font_browser_slant = font.slant;
            }
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
            let text = self.draft_text.clone();
            self.add_text_layer(&text);
        }

        ui.add_space(12.0);
        ui.label(section_label("ADD A QUICK PHRASE"));
        ui.label(
            RichText::new("Each phrase creates a new text layer.")
                .size(11.0)
                .color(MUTED),
        );
        for preset in ["HELLO", "TEAM 2026", "I MADE THIS"] {
            if ui.button(icon_text("plus", preset)).clicked() {
                self.draft_text = preset.to_owned();
                self.add_text_layer(preset);
            }
        }
    }

    fn add_text_layer(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.history.record(self.design.clone());
        let id = self.design.add_text(text.to_owned());
        if let Some(element) = self.design.element_mut(id)
            && let ElementKind::Text { curvature, .. } = &mut element.kind
        {
            *curvature = self.draft_text_curvature;
        }
        self.selected = Some(id);
        self.tool = Tool::Select;
        self.set_status("Text layer added", false);
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

    fn qr_code_controls(&mut self, ui: &mut Ui) {
        ui.label(section_label("DESTINATION OR CONTENT"));
        ui.text_edit_multiline(&mut self.draft_qr);
        if ui
            .add_enabled(
                !self.draft_qr.trim().is_empty(),
                Button::new(RichText::new(icon_text("plus", "Add QR code")).color(Color32::WHITE))
                    .fill(INK),
            )
            .clicked()
        {
            self.history.record(self.design.clone());
            let id = self.design.add_qr_code(self.draft_qr.trim().to_owned());
            self.selected = Some(id);
            self.tool = Tool::Select;
            self.set_status("QR code added", false);
        }
    }

    fn series_controls(&mut self, ui: &mut Ui) {
        let selected = self.selected;
        if let Some(series) = &self.series_data {
            ui.label(RichText::new(&series.source_name).strong());
            ui.label(
                RichText::new(format!(
                    "{} rows · {} columns",
                    series.rows.len(),
                    series.columns.len()
                ))
                .color(MUTED),
            );
            ui.add_space(8.0);
            ui.label(section_label("INSERT VARIABLE"));
            let columns = series.columns.clone();
            let mut inserted = None;
            ui.horizontal_wrapped(|ui| {
                for column in columns {
                    if ui.button(format!("{{{{{column}}}}}")).clicked() {
                        inserted = Some(column);
                    }
                }
            });
            if let (Some(id), Some(column)) = (selected, inserted) {
                let previous = self.design.clone();
                if let Some(element) = self.design.element_mut(id) {
                    let content = match &mut element.kind {
                        ElementKind::Text { text, .. } => Some(text),
                        ElementKind::QrCode { content, .. } => Some(content),
                        _ => None,
                    };
                    if let Some(content) = content {
                        if !content.is_empty() && !content.ends_with(char::is_whitespace) {
                            content.push(' ');
                        }
                        content.push_str(&format!("{{{{{column}}}}}"));
                        self.history.record(previous);
                        self.set_status("Series variable inserted", false);
                    } else {
                        self.set_status("Select a text or QR layer first", true);
                    }
                }
            }
            ui.add_space(10.0);
            if ui
                .add_sized(
                    [ui.available_width(), 42.0],
                    Button::new(icon_text("printer", "Review series export")),
                )
                .clicked()
            {
                self.open_export_review();
            }
            if ui.button(icon_text("x", "Remove series data")).clicked() {
                self.series_data = None;
                self.set_status("Series data removed", false);
            }
        } else {
            ui.label(RichText::new("Import CSV, TSV, or semicolon-separated data.").color(MUTED));
            if ui
                .add_sized(
                    [ui.available_width(), 48.0],
                    Button::new(
                        RichText::new(icon_text("table-2", "Import series data"))
                            .color(Color32::WHITE),
                    )
                    .fill(INK),
                )
                .clicked()
            {
                self.import_series();
            }
        }
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
            for format in ["PNG", "JPEG", "WEBP", "SVG"] {
                ui.label(
                    RichText::new(format)
                        .size(10.0)
                        .strong()
                        .color(MUTED)
                        .background_color(Color32::from_rgb(239, 238, 233)),
                );
            }
        });
        ui.label(
            RichText::new("SVG imports as one raster image.")
                .size(10.0)
                .color(MUTED),
        )
        .on_hover_text(crate::svg_import::IMPORT_NOTICE);
    }

    fn background_controls(&mut self, ui: &mut Ui, context: &egui::Context) {
        ui.label(section_label("PHOTO"));
        let photo_name = self.design.background_image.as_ref().map(|image| {
            self.design
                .assets
                .get(&image.asset_id)
                .and_then(|asset| asset.source_path.as_deref())
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
        ui.label(section_label("COLOR"));
        changed |= crate::color::edit_design_color(ui, &mut self.design.background);
        let background_swatches = self
            .saved_palette
            .as_ref()
            .map(|palette| palette.colors.clone())
            .unwrap_or_else(|| palette().to_vec());
        ui.horizontal_wrapped(|ui| {
            for swatch in background_swatches {
                let (rect, response) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
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
        ui.label(RichText::new("Shift-click or drag on canvas to select several. Open Help for keyboard shortcuts.").size(10.0).color(MUTED));

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
                    let drag = ui.add(egui::Label::new("⠿").sense(Sense::drag()).selectable(false));
                    if drag.drag_started() {
                        self.layer_drag_source = Some(id);
                    }
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
                    let label =
                        ui.selectable_label(self.selected_ids.contains(&id), truncate(&name, 22));
                    if let Some(source) = self.layer_drag_source
                        && label.rect.contains(ui.input(|input| {
                            input.pointer.interact_pos().unwrap_or(label.rect.center())
                        }))
                        && ui.input(|input| input.pointer.any_released())
                    {
                        let previous = self.design.clone();
                        if self.design.reorder_layer(source, id) {
                            self.history.record(previous);
                        }
                        self.layer_drag_source = None;
                    }
                    if label.clicked() {
                        self.select_hit(Some(id), ui.input(|input| input.modifiers.shift));
                        self.tool = Tool::Select;
                    }
                });
            }
        });
        if ui.input(|input| input.pointer.any_released()) {
            self.layer_drag_source = None;
        }

        ui.horizontal_wrapped(|ui| {
            let enabled = self.selected.is_some();
            if ui
                .add_enabled(self.selected_ids.len() >= 2, Button::new("Group"))
                .clicked()
            {
                self.toggle_group(false);
            }
            if ui
                .add_enabled(self.selected_ids.len() >= 2, Button::new("Ungroup"))
                .clicked()
            {
                self.toggle_group(true);
            }
            if ui.add_enabled(enabled, Button::new("Scale −")).clicked() {
                self.bulk_transform(false, false);
            }
            if ui.add_enabled(enabled, Button::new("Scale +")).clicked() {
                self.bulk_transform(false, true);
            }
            if ui.add_enabled(enabled, Button::new("Rotate −")).clicked() {
                self.bulk_transform(true, false);
            }
            if ui.add_enabled(enabled, Button::new("Rotate +")).clicked() {
                self.bulk_transform(true, true);
            }
            if ui
                .add_enabled(self.selected_ids.len() >= 3, Button::new("Space H"))
                .clicked()
                && self.selected_ids.len() >= 3
            {
                let previous = self.design.clone();
                if crate::editor::distribute_centers(&mut self.design, &self.selected_ids, true) {
                    self.history.record(previous);
                }
            }
            if ui
                .add_enabled(self.selected_ids.len() >= 3, Button::new("Space V"))
                .clicked()
                && self.selected_ids.len() >= 3
            {
                let previous = self.design.clone();
                if crate::editor::distribute_centers(&mut self.design, &self.selected_ids, false) {
                    self.history.record(previous);
                }
            }
            if icon_button(ui, "arrow-up", "Bring forward", enabled).clicked() {
                self.move_selected(true);
            }
            if icon_button(ui, "arrow-down", "Send backward", enabled).clicked() {
                self.move_selected(false);
            }
            if icon_button(ui, "copy", "Duplicate layer", enabled).clicked() {
                self.duplicate_selected();
            }
            if ui.add_enabled(enabled, Button::new("Copy")).clicked() {
                self.copy_selected();
            }
            if ui
                .add_enabled(!self.clipboard.is_empty(), Button::new("Paste"))
                .clicked()
            {
                self.paste_clipboard(ui.ctx());
            }
            if ui
                .add_enabled(
                    enabled && !self.clipboard.is_empty(),
                    Button::new("Paste style"),
                )
                .clicked()
            {
                self.paste_style();
            }
            if icon_button(ui, "trash-2", "Delete layer", enabled).clicked() {
                self.delete_selected();
            }
        });
    }

    fn handle_shortcuts(&mut self, context: &egui::Context) {
        let accepts_global_keys = !context.egui_wants_keyboard_input();
        let (tool_text, tool_shape, tool_image) = context.input(|input| {
            (
                accepts_global_keys
                    && !input.modifiers.command
                    && !input.modifiers.shift
                    && input.key_pressed(Key::T),
                accepts_global_keys
                    && !input.modifiers.command
                    && !input.modifiers.shift
                    && input.key_pressed(Key::S),
                accepts_global_keys
                    && !input.modifiers.command
                    && !input.modifiers.shift
                    && input.key_pressed(Key::I),
            )
        });
        if tool_text {
            self.tool = Tool::Text;
        }
        if tool_shape {
            self.tool = Tool::Shape;
        }
        if tool_image {
            self.import_image(context);
        }
        let (
            undo,
            redo,
            save,
            open,
            export,
            duplicate,
            delete,
            copy,
            paste,
            paste_style,
            group,
            ungroup,
            front,
            back,
            nudge,
        ) = context.input(|input| {
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
                accepts_global_keys && command && !shift && input.key_pressed(Key::Z),
                accepts_global_keys
                    && command
                    && (input.key_pressed(Key::Y) || (shift && input.key_pressed(Key::Z))),
                command && input.key_pressed(Key::S),
                command && input.key_pressed(Key::O),
                accepts_global_keys && command && input.key_pressed(Key::E),
                accepts_global_keys && command && input.key_pressed(Key::D),
                accepts_global_keys
                    && (input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace)),
                accepts_global_keys && command && !shift && input.key_pressed(Key::C),
                accepts_global_keys && command && !shift && input.key_pressed(Key::V),
                accepts_global_keys && command && shift && input.key_pressed(Key::V),
                accepts_global_keys && command && !shift && input.key_pressed(Key::G),
                accepts_global_keys && command && shift && input.key_pressed(Key::G),
                accepts_global_keys && command && input.key_pressed(Key::PageUp),
                accepts_global_keys && command && input.key_pressed(Key::PageDown),
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
            self.open_export_review();
        }
        if duplicate {
            self.duplicate_selected();
        }
        if delete {
            self.delete_selected();
        }
        if copy {
            self.copy_selected();
        }
        if paste {
            self.paste_clipboard(context);
        }
        if paste_style {
            self.paste_style();
        }
        if group {
            self.toggle_group(false);
        }
        if ungroup {
            self.toggle_group(true);
        }
        if front {
            self.move_selected_to_edge(true);
        }
        if back {
            self.move_selected_to_edge(false);
        }
        if let Some(delta) = nudge {
            let previous = self.design.clone();
            crate::editor::move_selection(&mut self.design, &self.selected_ids, delta);
            self.history.record(previous);
        }
    }

    fn import_image(&mut self, context: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "svg"])
            .pick_file()
        else {
            return;
        };

        let previous = self.design.clone();
        let (asset_id, aspect_ratio) = match self.design.import_asset_path(&path) {
            Ok(asset) => asset,
            Err(error) => {
                self.set_status(format!("Could not import image: {error}"), true);
                return;
            }
        };
        let texture = self
            .design
            .asset(&asset_id)
            .and_then(|asset| load_texture(context, &asset_id, &asset.data));
        match texture {
            Ok(texture) => {
                let id = self.design.add_image(asset_id.clone(), aspect_ratio);
                self.history.record(previous);
                self.textures.insert(asset_id, texture);
                self.selected = Some(id);
                self.tool = Tool::Select;
                self.set_status("Image layer added", false);
            }
            Err(error) => {
                self.design = previous;
                self.set_status(error, true);
            }
        }
    }

    fn import_background_image(&mut self, context: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "svg"])
            .pick_file()
        else {
            return;
        };

        let previous = self.design.clone();
        let (asset_id, aspect_ratio) = match self.design.import_asset_path(&path) {
            Ok(asset) => asset,
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
                self.design = previous;
                self.set_status(error, true);
                return;
            }
        };
        self.design
            .set_background_image(asset_id.clone(), aspect_ratio);
        self.history.record(previous);
        self.textures.insert(asset_id, texture);
        self.set_status("Backdrop photo added", false);
    }

    fn import_series(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Delimited data", &["csv", "tsv", "txt"])
            .pick_file()
        else {
            return;
        };
        match SeriesData::from_path(&path) {
            Ok(series) => {
                let rows = series.rows.len();
                self.series_data = Some(series);
                self.export_kind = ExportKind::PdfSeries;
                self.export_start = 1;
                self.export_end = rows;
                self.export_preview_entry = 1;
                self.export_preflight_cache = None;
                self.set_status(format!("Imported {rows} series rows"), false);
            }
            Err(error) => self.set_status(error, true),
        }
    }

    fn save_project(&mut self) {
        let path = self.project_path.clone().or_else(|| {
            rfd::FileDialog::new()
                .add_filter("Button Studio project", &["buttonstudio"])
                .set_file_name("my-button.buttonstudio")
                .save_file()
        });
        let Some(path) = path else {
            return;
        };
        self.write_project(path);
    }

    fn save_project_as(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Button Studio project", &["buttonstudio"])
            .set_file_name("my-button.buttonstudio")
            .save_file()
        else {
            return;
        };
        self.write_project(path);
    }

    fn write_project(&mut self, mut path: PathBuf) {
        if path.extension().is_none() {
            path.set_extension("buttonstudio");
        }

        let preview = match render_project_preview_png(&self.design, 512) {
            Ok(preview) => preview,
            Err(error) => {
                self.set_status(format!("Could not prepare project preview: {error}"), true);
                return;
            }
        };
        let json = match self
            .design
            .to_project_json_with_preview(Some((&preview, 512, 512)))
        {
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
        self.open_project_path(context, path);
    }

    pub fn open_project_path(&mut self, context: &egui::Context, path: PathBuf) {
        let result = fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| Design::from_project_json(&bytes, Some(&path)));
        match result {
            Ok(design) => {
                self.pending_project = Some((design, path));
                context.request_repaint();
            }
            Err(error) => self.set_status(format!("Could not open project: {error}"), true),
        }
    }

    fn open_external_project(&mut self, context: &egui::Context) {
        if let Some(path) = self.project_paths.try_iter().last() {
            self.open_project_path(context, path);
        }
    }

    fn reload_textures(&mut self, context: &egui::Context) {
        self.textures.clear();
        let assets = self
            .design
            .assets
            .iter()
            .map(|(asset_id, asset)| (asset_id.clone(), asset.data.clone()))
            .collect::<Vec<_>>();
        for (asset_id, data) in assets {
            if let Ok(texture) = load_texture(context, &asset_id, &data) {
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

        if let Some(update) = self.pending_library_update.take() {
            match update {
                PendingLibraryUpdate::Design(design) => {
                    self.history.record(self.design.clone());
                    self.design = design;
                    self.selected = self.design.elements.last().map(|element| element.id);
                    self.selected_ids = self.selected.into_iter().collect();
                    self.reload_textures(context);
                    self.set_status("Template applied", false);
                }
                PendingLibraryUpdate::Style(style, ids) => {
                    let previous = self.design.clone();
                    for id in ids {
                        if let Some(element) = self.design.element_mut(id) {
                            element.color = style.color;
                            element.opacity = style.opacity;
                            element.appearance = style.appearance.clone();
                            if let ElementKind::Text {
                                font, font_size, ..
                            } = &mut element.kind
                            {
                                if let Some(saved_font) = &style.font {
                                    *font = saved_font.clone();
                                }
                                if let Some(saved_size) = style.font_size {
                                    *font_size = saved_size;
                                }
                            }
                        }
                    }
                    if self.design != previous {
                        self.history.record(previous);
                        self.set_status("Saved style applied", false);
                    }
                }
            }
            self.export_preflight_cache = None;
        }
    }

    fn refresh_font_preview(&mut self, context: &egui::Context) {
        let font = font_variant(
            &self.font_catalog.available,
            &self.font_browser_family,
            self.font_browser_weight,
            self.font_browser_slant,
        )
        .map(|font| font.spec.clone());
        let key = font.as_ref().map(TextFont::registration_key);
        if self.font_preview_key == key {
            return;
        }
        self.font_preview_key = key.clone();
        let texture_key = key.unwrap_or_else(|| "unavailable".to_owned());
        self.font_preview_texture = font.and_then(|font| {
            let image = crate::text_raster::render_line(
                "Aa Bb 123 · Quick brown fox",
                28.0,
                &font,
                image::Rgba([29, 31, 36, 255]),
            )?;
            let size = [image.width() as usize, image.height() as usize];
            if size[0] == 0 || size[1] == 0 {
                return None;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            Some(context.load_texture(
                format!("font-preview-{texture_key}"),
                image,
                egui::TextureOptions::LINEAR,
            ))
        });
    }

    fn font_browser(&mut self, context: &egui::Context) {
        if !self.font_browser_open {
            return;
        }
        let Some(target) = self.font_browser_target else {
            self.font_browser_open = false;
            return;
        };
        if !self
            .design
            .element(target)
            .is_some_and(|element| matches!(&element.kind, ElementKind::Text { .. }))
        {
            self.font_browser_open = false;
            self.font_browser_target = None;
            return;
        }

        self.refresh_font_preview(context);
        let preview_texture = self.font_preview_texture.clone();

        let available = &self.font_catalog.available;
        let search = &mut self.font_search;
        let focus_search = &mut self.focus_font_search;
        let mut selected = None;
        let mut open = self.font_browser_open;
        let mut close_after_selection = false;
        let mut selected_family = self.font_browser_family.clone();
        let mut selected_weight = self.font_browser_weight;
        let mut selected_slant = self.font_browser_slant;
        let previous_selection = (selected_family.clone(), selected_weight, selected_slant);

        egui::Window::new("Installed fonts")
            .id(egui::Id::new("installed-font-browser"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(760.0, 560.0))
            .min_width(620.0)
            .min_height(460.0)
            .show(context, |ui| {
                ui.label(
                    RichText::new(
                        "Choose a font family, then set its weight and style independently.",
                    )
                    .size(12.0)
                    .color(MUTED),
                );
                let response = ui.add(
                    egui::TextEdit::singleline(search)
                        .hint_text("Search font families, weights, or styles")
                        .desired_width(ui.available_width()),
                );
                if *focus_search {
                    response.request_focus();
                    *focus_search = false;
                }

                let families = matching_font_families(available, search);
                let matching_face_count = matching_fonts(available, search).len();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} families · {matching_face_count} matching faces",
                            families.len()
                        ))
                        .size(11.0)
                        .color(MUTED),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} families · {} faces installed",
                                matching_font_families(available, "").len(),
                                available.len()
                            ))
                            .size(11.0)
                            .color(MUTED),
                        );
                    });
                });
                ui.separator();

                ui.horizontal(|ui| {
                    let family_width = (ui.available_width() * 0.4).clamp(220.0, 290.0);
                    const FAMILY_LIST_HEIGHT: f32 = 320.0;
                    ui.allocate_ui(vec2(family_width, FAMILY_LIST_HEIGHT), |ui| {
                        ui.vertical(|ui| {
                            ui.label(section_label("FONT FAMILIES"));
                            ui.add_space(4.0);
                            if families.is_empty() {
                                ui.label(RichText::new("No matching font families.").color(MUTED));
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("font-family-list")
                                    .min_scrolled_height(220.0)
                                    .max_height(FAMILY_LIST_HEIGHT - 28.0)
                                    .scroll_bar_visibility(
                                        egui::containers::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                    )
                                    .show(ui, |ui| {
                                        for family in &families {
                                            let face_count = available
                                                .iter()
                                                .filter(|font| {
                                                    font.spec.family.eq_ignore_ascii_case(family)
                                                })
                                                .count();
                                            let label = format!(
                                                "{}  ·  {face_count}",
                                                truncate(family, 26)
                                            );
                                            if ui
                                                .selectable_label(
                                                    selected_family.eq_ignore_ascii_case(family),
                                                    label,
                                                )
                                                .clicked()
                                            {
                                                selected_family = family.clone();
                                                if let Some(font) = preferred_font_variant(
                                                    available,
                                                    &selected_family,
                                                ) {
                                                    selected_weight = font.weight;
                                                    selected_slant = font.slant;
                                                }
                                            }
                                        }
                                    });
                            }
                        });
                    });

                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(section_label("FONT STYLE"));
                        ui.add_space(4.0);

                        let weight_options =
                            font_weights_for_family(available, &selected_family, selected_slant);
                        let previous_weight = selected_weight;
                        ui.horizontal(|ui| {
                            ui.label("Weight");
                            egui::ComboBox::from_id_salt("font_weight")
                                .selected_text(selected_weight.label())
                                .width(148.0)
                                .show_ui(ui, |ui| {
                                    for weight in &weight_options {
                                        ui.selectable_value(
                                            &mut selected_weight,
                                            *weight,
                                            weight.label(),
                                        );
                                    }
                                });
                        });
                        if previous_weight != selected_weight {
                            let slant_options = font_slants_for_family(
                                available,
                                &selected_family,
                                selected_weight,
                            );
                            if !slant_options.contains(&selected_slant) {
                                selected_slant = preferred_slant(&slant_options);
                            }
                        }

                        let slant_options =
                            font_slants_for_family(available, &selected_family, selected_weight);
                        ui.horizontal(|ui| {
                            ui.label("Style");
                            egui::ComboBox::from_id_salt("font_slant")
                                .selected_text(selected_slant.label())
                                .width(148.0)
                                .show_ui(ui, |ui| {
                                    for slant in &slant_options {
                                        ui.selectable_value(
                                            &mut selected_slant,
                                            *slant,
                                            slant.label(),
                                        );
                                    }
                                });
                        });

                        let chosen_variant = font_variant(
                            available,
                            &selected_family,
                            selected_weight,
                            selected_slant,
                        );
                        ui.add_space(10.0);
                        ui.label(section_label("PREVIEW"));
                        Frame::new()
                            .fill(Color32::from_rgb(250, 249, 246))
                            .stroke(Stroke::new(1.0, Color32::from_gray(220)))
                            .inner_margin(egui::Margin::same(12))
                            .corner_radius(5.0)
                            .show(ui, |ui| {
                                if let Some(texture) = &preview_texture {
                                    let size = texture.size_vec2();
                                    let scale = (ui.available_width() / size.x).min(1.0);
                                    ui.add(egui::Image::new((texture.id(), size * scale)));
                                } else {
                                    ui.label(
                                        RichText::new("Preview unavailable for this face.")
                                            .color(MUTED),
                                    );
                                }
                            });
                        if let Some(font) = chosen_variant {
                            ui.label(
                                RichText::new(format!(
                                    "{} · {} · {}",
                                    font.spec.family,
                                    font.weight.label(),
                                    font.slant.label()
                                ))
                                .size(11.0)
                                .color(MUTED),
                            );
                            if ui.button(icon_text("check", "Use this font")).clicked() {
                                selected = Some(font.spec.clone());
                                close_after_selection = true;
                            }
                        } else {
                            ui.label(
                                RichText::new("No face has this style combination.").color(MUTED),
                            );
                        }
                    });
                });
            });

        let selection = (selected_family, selected_weight, selected_slant);
        if selection
            != (
                previous_selection.0.clone(),
                previous_selection.1,
                previous_selection.2,
            )
        {
            self.font_browser_family = selection.0;
            self.font_browser_weight = selection.1;
            self.font_browser_slant = selection.2;
            self.refresh_font_preview(context);
            context.request_repaint();
        }
        self.font_browser_open = open && !close_after_selection;
        if !self.font_browser_open {
            self.font_browser_target = None;
        }
        if let Some(font) = selected {
            self.pending_font_change = Some((target, font));
            context.request_repaint();
        }
    }

    fn open_export_review(&mut self) {
        if let Some(series) = &self.series_data {
            self.export_kind = ExportKind::PdfSeries;
            self.export_start = self.export_start.clamp(1, series.rows.len());
            self.export_end = self.export_end.clamp(self.export_start, series.rows.len());
            self.export_preview_entry = self.export_preview_entry.clamp(1, series.rows.len());
        } else if self.export_kind == ExportKind::PdfSeries {
            self.export_kind = ExportKind::PdfSheet;
        }
        self.export_review_open = true;
    }

    fn refresh_export_preflight(&mut self) {
        let signature = self.series_data.as_ref().map(series_signature).unwrap_or(0);
        let cached = self.export_preflight_cache.as_ref().is_some_and(|cache| {
            cache.design == self.design && cache.series_signature == signature
        });
        if cached {
            return;
        }
        let design_issues = production::preflight_design(&self.design);
        let row_issues = self
            .series_data
            .as_ref()
            .map(|series| production::preflight_series(&self.design, &series.rows, 2))
            .unwrap_or_default();
        self.export_preflight_cache = Some(ExportPreflightCache {
            design: self.design.clone(),
            series_signature: signature,
            design_issues,
            row_issues,
        });
    }

    fn export_review_window(&mut self, context: &egui::Context) {
        if !self.export_review_open {
            return;
        }
        self.refresh_export_preflight();
        let mut open = self.export_review_open;
        let design = self.design.clone();
        let textures = self.textures.clone();
        let series = self.series_data.clone();
        let issues = self
            .export_preflight_cache
            .as_ref()
            .map(|cache| cache.design_issues.clone())
            .unwrap_or_default();
        let row_issues = self
            .export_preflight_cache
            .as_ref()
            .map(|cache| cache.row_issues.clone())
            .unwrap_or_default();
        let mut kind = self.export_kind;
        let mut start = self.export_start;
        let mut end = self.export_end;
        let mut copies = self.export_copies;
        let mut preview_entry = self.export_preview_entry;
        let mut skip_invalid = self.skip_invalid_rows;
        let mut run_export = false;
        let mut close_window = false;

        egui::Window::new("Review production export")
            .id(egui::Id::new("production-export-review"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(760.0, 640.0))
            .min_size(vec2(620.0, 500.0))
            .max_height((context.content_rect().height() - 48.0).max(300.0))
            .vscroll(true)
            .show(context, |ui| {
                ui.label(RichText::new("Review physical size, artwork boundaries, resolution, and personalization before saving.").color(MUTED));
                ui.horizontal(|ui| {
                    ui.label("Output");
                    egui::ComboBox::from_id_salt("export-kind")
                        .selected_text(export_kind_label(kind))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut kind, ExportKind::Png, "Transparent PNG");
                            ui.selectable_value(&mut kind, ExportKind::PdfSheet, "A4 print sheet PDF");
                            if series.is_some() { ui.selectable_value(&mut kind, ExportKind::PdfSeries, "Personalized series PDF"); }
                            ui.selectable_value(&mut kind, ExportKind::Svg, "SVG");
                        });
                });
                ui.separator();
                ui.columns(2, |columns| {
                    let preview_design = series.as_ref().filter(|_| kind == ExportKind::PdfSeries)
                        .and_then(|data| data.rows.get(preview_entry.saturating_sub(1)))
                        .map(|values| design.personalized(values))
                        .unwrap_or_else(|| design.clone());
                    columns[0].label(RichText::new("PREVIEW").size(10.0).strong().color(MUTED));
                    let width = columns[0].available_width().min(280.0);
                    let (rect, _) = columns[0].allocate_exact_size(vec2(width, width), Sense::hover());
                    paint_preview(columns[0].painter(), rect, &preview_design, &textures);
                    if kind == ExportKind::PdfSeries && let Some(data) = &series {
                        columns[0].horizontal(|ui| {
                            ui.label("Preview CSV entry");
                            ui.add(egui::DragValue::new(&mut preview_entry).range(1..=data.rows.len()));
                        });
                        if let Some(row) = data.rows.get(preview_entry.saturating_sub(1)) {
                            for (key, value) in row.iter().take(5) {
                                columns[0].label(RichText::new(format!("{key}: {}", truncate(value, 32))).size(11.0).color(MUTED));
                            }
                        }
                    }

                    let pixels = export_pixel_size(&design);
                    columns[1].label(RichText::new("PRODUCTION").size(10.0).strong().color(MUTED));
                    columns[1].label(format!("Trim size: {} mm diameter", design.diameter_mm));
                    columns[1].label(format!("Artwork including bleed: {:.1} mm diameter", design.artwork_diameter_mm()));
                    columns[1].label(format!("PNG basis: {pixels} × {pixels} px · 300 DPI"));
                    columns[1].label("PDF sheet: A4 · 210 × 297 mm · centered button layout");
                    columns[1].add_space(8.0);

                    if kind == ExportKind::PdfSeries && let Some(data) = &series {
                        columns[1].label(RichText::new("CSV ROWS").size(10.0).strong().color(MUTED));
                        columns[1].horizontal(|ui| {
                            ui.label("From entry"); ui.add(egui::DragValue::new(&mut start).range(1..=data.rows.len()));
                            ui.label("To"); ui.add(egui::DragValue::new(&mut end).range(1..=data.rows.len()));
                        });
                        if start > end { end = start; }
                        columns[1].horizontal(|ui| {
                            ui.label("Copies per entry"); ui.add(egui::DragValue::new(&mut copies).range(1..=999));
                        });
                        let selected_count = end.saturating_sub(start).saturating_add(1);
                        let chosen_issues = row_issues.iter().filter(|row| row.row_number > start && row.row_number <= end + 1).collect::<Vec<_>>();
                        let invalid = chosen_issues.iter().filter(|row| row.issues.iter().any(|i| i.severity == IssueSeverity::Error)).collect::<Vec<_>>();
                        let export_count = selected_count.saturating_mul(copies);
                        let valid_count = selected_count.saturating_sub(invalid.len());
                        let actual_count = if skip_invalid { valid_count.saturating_mul(copies) } else { export_count };
                        columns[1].label(format!("{selected_count} entries · {copies} copies each · {actual_count} buttons to export"));
                        if skip_invalid && !invalid.is_empty() { columns[1].label(RichText::new(format!("{} invalid entries will be reported and skipped", invalid.len())).color(Color32::from_rgb(171, 105, 32))); }
                        if !invalid.is_empty() {
                            columns[1].checkbox(&mut skip_invalid, "Skip invalid rows and report them");
                            for row in invalid.iter().take(8) {
                                let details = row.issues.iter().filter(|i| i.severity == IssueSeverity::Error).map(|i| i.message.as_str()).collect::<Vec<_>>().join("; ");
                                columns[1].colored_label(Color32::from_rgb(178, 63, 52), format!("CSV row {}: {}", row.row_number, truncate(&details, 90)));
                            }
                            if invalid.len() > 8 { columns[1].label(format!("… and {} more invalid rows", invalid.len() - 8)); }
                        } else { columns[1].label(RichText::new("All selected CSV rows passed preflight.").color(MINT)); }
                        if export_count > 10_000 { columns[1].colored_label(Color32::from_rgb(178, 63, 52), "Limit this batch to 10,000 buttons."); }
                    }
                });

                if matches!(kind, ExportKind::PdfSheet | ExportKind::PdfSeries) {
                    let count = if kind == ExportKind::PdfSeries {
                        let invalid = row_issues.iter().filter(|row| row.row_number > start && row.row_number <= end + 1 && row.issues.iter().any(|issue| issue.severity == IssueSeverity::Error)).count();
                        Some(end.saturating_sub(start).saturating_add(1).saturating_sub(if skip_invalid { invalid } else { 0 }).saturating_mul(copies))
                    } else { None };
                    ui.collapsing("A4 sheet placement", |ui| { production::paint_sheet_preview(ui, &design, count); });
                }
                ui.separator();
                ui.label(RichText::new("PREFLIGHT").size(10.0).strong().color(MUTED));
                let all_issues = if kind == ExportKind::PdfSeries {
                    let mut all = issues.clone();
                    for row in row_issues.iter().filter(|row| row.row_number > start && row.row_number <= end + 1) {
                        all.extend(row.issues.iter().cloned());
                    }
                    all
                } else { issues.clone() };
                if all_issues.is_empty() { ui.label(RichText::new("No production issues found.").color(MINT)); }
                else {
                    egui::ScrollArea::vertical().max_height(112.0).show(ui, |ui| {
                        for issue in all_issues.iter().take(14) { show_preflight_issue(ui, issue); }
                        if all_issues.len() > 14 { ui.label(format!("… and {} more findings", all_issues.len() - 14)); }
                    });
                }
                if kind == ExportKind::Svg && !svg::fallback_reasons(&design).is_empty() {
                    ui.colored_label(Color32::from_rgb(171, 105, 32), "SVG will embed a 2400 px PNG to preserve background effects, curved text, QR codes, or other unsupported details. The artwork will not be editable as separate objects.");
                }
                if kind == ExportKind::Svg { ui.label(RichText::new("SVG fonts are not embedded; another viewer may substitute a font.").size(11.0).color(MUTED)); }

                let base_errors = issues.iter().any(|issue| issue.severity == IssueSeverity::Error);
                let selected_has_invalid = row_issues.iter().any(|row| row.row_number > start && row.row_number <= end + 1 && row.issues.iter().any(|issue| issue.severity == IssueSeverity::Error));
                let series_valid = kind != ExportKind::PdfSeries || (series.is_some() && (!selected_has_invalid || skip_invalid));
                let range_valid = series.as_ref().is_none_or(|data| start >= 1 && end >= start && end <= data.rows.len());
                let too_many = kind == ExportKind::PdfSeries && end.saturating_sub(start).saturating_add(1).saturating_mul(copies) > 10_000;
                ui.horizontal(|ui| {
                    if ui.add_enabled(!base_errors && series_valid && range_valid && !too_many, Button::new(RichText::new("Export…").color(Color32::WHITE)).fill(ACCENT)).clicked() { run_export = true; }
                    if ui.button("Close").clicked() { close_window = true; }
                });
            });

        if close_window {
            open = false;
        }
        self.export_review_open = open;
        self.export_kind = kind;
        self.export_start = start;
        self.export_end = end;
        self.export_copies = copies;
        self.export_preview_entry = preview_entry;
        self.skip_invalid_rows = skip_invalid;
        if run_export && self.perform_export(kind, start, end, copies, skip_invalid) {
            self.export_review_open = false;
        }
    }

    fn perform_export(
        &mut self,
        kind: ExportKind,
        start: usize,
        end: usize,
        copies: usize,
        skip_invalid: bool,
    ) -> bool {
        let series_rows = if kind == ExportKind::PdfSeries {
            let Some(series) = &self.series_data else {
                self.set_status("Import a CSV before exporting a series", true);
                return false;
            };
            let chosen = match production::select_rows(&series.rows, start, end, 1) {
                Ok(rows) => rows,
                Err(error) => {
                    self.set_status(error, true);
                    return false;
                }
            };
            let reports = production::preflight_series(&self.design, &chosen, start + 1);
            let invalid = reports
                .iter()
                .filter(|row| {
                    row.issues
                        .iter()
                        .any(|i| i.severity == IssueSeverity::Error)
                })
                .map(|row| row.row_number)
                .collect::<Vec<_>>();
            if !invalid.is_empty() && !skip_invalid {
                self.set_status(
                    format!(
                        "Fix or skip invalid CSV rows: {}",
                        format_row_numbers(&invalid)
                    ),
                    true,
                );
                return false;
            }
            let invalid_data_indices = invalid
                .iter()
                .map(|row| row - start - 1)
                .collect::<std::collections::HashSet<_>>();
            let valid_rows = chosen
                .into_iter()
                .enumerate()
                .filter_map(|(index, row)| (!invalid_data_indices.contains(&index)).then_some(row))
                .collect::<Vec<_>>();
            if valid_rows.is_empty() {
                self.set_status("No valid CSV rows remain in this range", true);
                return false;
            }
            if self.design_issues_have_errors() {
                self.set_status(
                    "Resolve design-wide preflight errors before exporting",
                    true,
                );
                return false;
            }
            let repeated = match production::select_rows(&valid_rows, 1, valid_rows.len(), copies) {
                Ok(rows) => rows,
                Err(error) => {
                    self.set_status(error, true);
                    return false;
                }
            };
            Some((repeated, invalid))
        } else {
            if let Some(issue) = production::preflight_design(&self.design)
                .into_iter()
                .find(|issue| issue.severity == IssueSeverity::Error)
            {
                self.set_status(issue.message, true);
                return false;
            }
            None
        };

        let (filter, extension, filename) = match kind {
            ExportKind::Png => (
                "PNG image",
                "png",
                format!("button-{}mm.png", self.design.diameter_mm),
            ),
            ExportKind::PdfSheet => (
                "PDF document",
                "pdf",
                format!("button-sheet-{}mm.pdf", self.design.diameter_mm),
            ),
            ExportKind::PdfSeries => (
                "PDF document",
                "pdf",
                format!("button-series-{}mm.pdf", self.design.diameter_mm),
            ),
            ExportKind::Svg => (
                "SVG artwork",
                "svg",
                format!("button-{}mm.svg", self.design.diameter_mm),
            ),
        };
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter(filter, &[extension])
            .set_file_name(&filename)
            .save_file()
        else {
            return false;
        };
        if path.extension().is_none() {
            path.set_extension(extension);
        }
        let result = match kind {
            ExportKind::Png => export_png(&self.design, &path)
                .map(|pixels| (format!("Exported {pixels} × {pixels} px PNG"), Vec::new())),
            ExportKind::PdfSheet => export_pdf_sheet(&self.design, &path)
                .map(|count| (format!("Exported {count} buttons to A4 PDF"), Vec::new())),
            ExportKind::PdfSeries => {
                let (rows, skipped) = series_rows.expect("series export prepared");
                export_pdf_series(&self.design, &rows, &path).map(|summary| {
                    let skipped_text = if skipped.is_empty() {
                        String::new()
                    } else {
                        format!(" · skipped CSV rows {}", format_row_numbers(&skipped))
                    };
                    (
                        format!(
                            "Exported {} personalized buttons across {} pages{}",
                            summary.buttons, summary.pages, skipped_text
                        ),
                        Vec::new(),
                    )
                })
            }
            ExportKind::Svg => svg::export_svg(&self.design).and_then(|output| {
                fs::write(&path, output.svg)
                    .map_err(|error| format!("Could not save SVG: {error}"))
                    .map(|()| ("SVG exported".to_owned(), output.warnings))
            }),
        };
        match result {
            Ok((message, warnings)) => {
                let message = if warnings.is_empty() {
                    message
                } else {
                    format!("{message} · {} export warning(s)", warnings.len())
                };
                self.set_status(message, false);
                true
            }
            Err(error) => {
                self.set_status(error, true);
                false
            }
        }
    }

    fn design_issues_have_errors(&self) -> bool {
        production::preflight_design(&self.design)
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Error)
    }

    fn library_window(&mut self, context: &egui::Context) {
        if !self.library_open {
            return;
        }
        let mut open = self.library_open;
        let mut action = None;
        let mut template_name = self.library_template_name.clone();
        let builtin = templates::builtin_templates();
        egui::Window::new("Templates and styles")
            .id(egui::Id::new("design-library"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(560.0, 500.0))
            .max_height((context.content_rect().height() - 48.0).max(300.0))
            .vscroll(true)
            .show(context, |ui| {
                ui.label(
                    RichText::new(
                        "Start from a layout, or save reusable designs, colors, and object styles.",
                    )
                    .color(MUTED),
                );
                ui.label(
                    RichText::new("BUILT-IN LAYOUTS")
                        .size(10.0)
                        .strong()
                        .color(MUTED),
                );
                for (index, template) in builtin.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(&template.name);
                            ui.label(RichText::new(&template.description).size(11.0).color(MUTED));
                        });
                        if ui.button("Use layout").clicked() {
                            action = Some(LibraryAction::ApplyBuiltin(index));
                        }
                    });
                }
                ui.separator();
                ui.label("Template name");
                ui.text_edit_singleline(&mut template_name);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Save current as template…").clicked() {
                        action = Some(LibraryAction::SaveTemplate);
                    }
                    if ui.button("Load template…").clicked() {
                        action = Some(LibraryAction::LoadTemplate);
                    }
                    if ui.button("Save palette…").clicked() {
                        action = Some(LibraryAction::SavePalette);
                    }
                    if ui.button("Load palette…").clicked() {
                        action = Some(LibraryAction::LoadPalette);
                    }
                    if ui.button("Save selected style…").clicked() {
                        action = Some(LibraryAction::SaveStyle);
                    }
                    if ui.button("Load/apply style…").clicked() {
                        action = Some(LibraryAction::LoadStyle);
                    }
                });
                if let Some(palette) = &self.saved_palette {
                    ui.label(
                        RichText::new(format!(
                            "Active palette: {} ({} colors)",
                            palette.name,
                            palette.colors.len()
                        ))
                        .color(MINT),
                    );
                }
            });
        self.library_open = open;
        self.library_template_name = template_name.clone();
        if let Some(action) = action {
            match action {
                LibraryAction::ApplyBuiltin(index) => {
                    if let Some(template) = builtin.get(index) {
                        self.queue_design_update(context, template.design.clone());
                    }
                }
                LibraryAction::SaveTemplate => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Button Studio template", &["buttontemplate", "json"])
                        .set_file_name(format!("{}.buttontemplate", safe_file_stem(&template_name)))
                        .save_file()
                    {
                        let template = DesignTemplate {
                            name: template_name.clone(),
                            description: "User-saved Button Studio layout".into(),
                            design: self.design.clone(),
                        };
                        match templates::save_template(&path, &template) {
                            Ok(()) => self.set_status("Template saved", false),
                            Err(error) => self.set_status(error, true),
                        }
                    }
                }
                LibraryAction::LoadTemplate => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Button Studio template", &["buttontemplate", "json"])
                        .pick_file()
                    {
                        match templates::load_template(&path) {
                            Ok(template) => self.queue_design_update(context, template.design),
                            Err(error) => self.set_status(error, true),
                        }
                    }
                }
                LibraryAction::SavePalette => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Palette", &["buttonpalette", "json"])
                        .set_file_name("my-palette.buttonpalette")
                        .save_file()
                    {
                        let palette = SavedPalette {
                            name: path
                                .file_stem()
                                .and_then(|x| x.to_str())
                                .unwrap_or("My palette")
                                .to_owned(),
                            colors: design_palette(&self.design),
                        };
                        match templates::save_palette(&path, &palette) {
                            Ok(()) => self.set_status("Palette saved", false),
                            Err(error) => self.set_status(error, true),
                        }
                    }
                }
                LibraryAction::LoadPalette => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Palette", &["buttonpalette", "json"])
                        .pick_file()
                    {
                        match templates::load_palette(&path) {
                            Ok(palette) => {
                                self.saved_palette = Some(palette);
                                self.set_status("Palette loaded and ready to apply", false)
                            }
                            Err(error) => self.set_status(error, true),
                        }
                    }
                }
                LibraryAction::SaveStyle => {
                    if let Some(style) = self.selected_style() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Object style", &["buttonstyle", "json"])
                            .set_file_name("my-style.buttonstyle")
                            .save_file()
                        {
                            match templates::save_style(&path, &style) {
                                Ok(()) => self.set_status("Style saved", false),
                                Err(error) => self.set_status(error, true),
                            }
                        }
                    } else {
                        self.set_status("Select an object to save its style", true)
                    }
                }
                LibraryAction::LoadStyle => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Object style", &["buttonstyle", "json"])
                        .pick_file()
                    {
                        match templates::load_style(&path) {
                            Ok(style) => self.queue_style_update(context, style),
                            Err(error) => self.set_status(error, true),
                        }
                    }
                }
            }
        }
    }

    fn selected_style(&self) -> Option<SavedStyle> {
        let element = self.selected.and_then(|id| self.design.element(id))?;
        let (font, font_size) = match &element.kind {
            ElementKind::Text {
                font, font_size, ..
            } => (Some(font.clone()), Some(*font_size)),
            _ => (None, None),
        };
        Some(SavedStyle {
            name: element.name(),
            color: element.color,
            font,
            font_size,
            opacity: element.opacity,
            appearance: element.appearance.clone(),
        })
    }

    fn queue_design_update(&mut self, context: &egui::Context, design: Design) {
        self.pending_library_update = Some(PendingLibraryUpdate::Design(design));
        context.request_repaint();
    }

    fn queue_style_update(&mut self, context: &egui::Context, style: SavedStyle) {
        self.pending_library_update = Some(PendingLibraryUpdate::Style(
            style,
            self.selected_ids.clone(),
        ));
        context.request_discard("activate saved style font");
        context.request_repaint();
    }

    fn help_window(&mut self, context: &egui::Context) {
        if !self.help_open {
            return;
        }
        let mut open = self.help_open;
        egui::Window::new("Keyboard shortcuts")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .show(context, |ui| {
                ui.set_min_width(520.0);
                egui::Grid::new("shortcut-grid")
                    .num_columns(2)
                    .spacing(vec2(16.0, 7.0))
                    .show(ui, |ui| {
                        ui.label(section_label("SHORTCUT"));
                        ui.label(section_label("ACTION"));
                        ui.end_row();
                        for (shortcut, action) in [
                            ("T", "Choose Text tool"),
                            ("S", "Choose Shapes tool"),
                            ("I", "Import image"),
                            ("Cmd/Ctrl+S", "Save project"),
                            ("Cmd/Ctrl+O", "Open project"),
                            ("Cmd/Ctrl+E", "Export and review"),
                            ("Cmd/Ctrl+D", "Duplicate selection"),
                            ("Cmd/Ctrl+C", "Copy objects"),
                            ("Cmd/Ctrl+V", "Paste objects"),
                            ("Cmd/Ctrl+Shift+V", "Paste style"),
                            ("Cmd/Ctrl+G", "Group selection"),
                            ("Cmd/Ctrl+Shift+G", "Ungroup selection"),
                            (
                                "Cmd/Ctrl+PageUp / PageDown",
                                "Bring to front / send to back",
                            ),
                            ("Arrow keys", "Nudge selection"),
                            ("Shift + arrows", "Larger nudge"),
                            ("Delete / Backspace", "Delete selection"),
                            ("Cmd/Ctrl+Z", "Undo"),
                            ("Cmd+Shift+Z / Ctrl+Y", "Redo"),
                        ] {
                            Frame::new()
                                .fill(Color32::from_rgb(239, 238, 233))
                                .inner_margin(egui::Margin::symmetric(6, 3))
                                .corner_radius(4.0)
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(shortcut).monospace().strong().color(INK),
                                    );
                                });
                            ui.label(action);
                            ui.end_row();
                        }
                    });
                ui.label(
                    RichText::new(
                        "Canvas shortcuts pause while a text field or keyboard editor has focus. Save and Open remain available.",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
            });
        self.help_open = open;
    }

    fn clear_confirmation(&mut self, context: &egui::Context) {
        if !self.confirm_clear {
            return;
        }
        let mut open = true;
        let mut clear = false;
        let mut cancel = false;
        egui::Window::new("Clear artwork?")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Remove all objects from this design? You can undo this change.");
                ui.horizontal(|ui| {
                    clear = ui.button("Clear artwork").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if clear {
            self.clear_canvas();
        }
        self.confirm_clear = open && !clear && !cancel;
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
        if self.selected.is_none() {
            return;
        }
        self.history.record(self.design.clone());
        let new_ids = crate::editor::duplicate_selection(&mut self.design, &self.selected_ids);
        if let Some(new_id) = new_ids.last().copied() {
            self.selected = Some(new_id);
            self.selected_ids = new_ids;
            self.set_status("Layer duplicated", false);
        }
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        self.history.record(self.design.clone());
        crate::editor::delete_selection(&mut self.design, &self.selected_ids);
        if self.design.element(id).is_none() {
            self.selected = self.design.elements.last().map(|element| element.id);
            self.selected_ids = self.selected.into_iter().collect();
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

    fn move_selected_to_edge(&mut self, toward_front: bool) {
        let Some(id) = self.selected else {
            return;
        };
        let Some(index) = self
            .design
            .elements
            .iter()
            .position(|element| element.id == id)
        else {
            return;
        };
        let edge = if toward_front {
            self.design.elements.len().saturating_sub(1)
        } else {
            0
        };
        if index == edge {
            return;
        }
        let previous = self.design.clone();
        let element = self.design.elements.remove(index);
        let insert_at = if toward_front {
            self.design.elements.len()
        } else {
            0
        };
        self.design.elements.insert(insert_at, element);
        self.history.record(previous);
        self.set_status(
            if toward_front {
                "Layer brought to front"
            } else {
                "Layer sent to back"
            },
            false,
        );
    }

    fn toggle_group(&mut self, ungroup: bool) {
        if self.selected_ids.len() < 2 {
            return;
        }
        let previous = self.design.clone();
        if ungroup {
            crate::editor::ungroup(&mut self.design, &self.selected_ids);
        } else {
            crate::editor::group(&mut self.design, &self.selected_ids);
        }
        self.history.record(previous);
        self.set_status(
            if ungroup {
                "Objects ungrouped"
            } else {
                "Objects grouped"
            },
            false,
        );
    }

    fn bulk_transform(&mut self, rotate: bool, positive: bool) {
        let previous = self.design.clone();
        let changed = if rotate {
            crate::editor::rotate_selection(
                &mut self.design,
                &self.selected_ids,
                if positive { 15.0 } else { -15.0 },
            )
        } else {
            crate::editor::scale_selection(
                &mut self.design,
                &self.selected_ids,
                if positive { 1.1 } else { 0.9 },
            )
        };
        if changed {
            self.history.record(previous);
        }
    }

    fn copy_selected(&mut self) {
        self.clipboard = crate::editor::selection_ids(&self.design, &self.selected_ids)
            .into_iter()
            .filter_map(|id| self.design.element(id).cloned())
            .collect();
        self.clipboard_assets.clear();
        for element in &self.clipboard {
            if let ElementKind::Image { asset_id, .. } = &element.kind
                && let Some(asset) = self.design.assets.get(asset_id)
            {
                self.clipboard_assets
                    .insert(asset_id.clone(), asset.clone());
            }
        }
    }

    fn paste_clipboard(&mut self, context: &egui::Context) {
        if self.clipboard.is_empty() {
            return;
        }
        let previous = self.design.clone();
        let ids = crate::editor::paste_elements(
            &mut self.design,
            &self.clipboard,
            &self.clipboard_assets,
        );
        self.history.record(previous);
        self.selected = ids.last().copied();
        self.selected_ids = ids;
        self.reload_textures(context);
        self.set_status("Pasted objects", false);
    }

    fn paste_style(&mut self) {
        let Some(source) = self.clipboard.first() else {
            return;
        };
        let previous = self.design.clone();
        let targets = crate::editor::selection_ids(&self.design, &self.selected_ids);
        let mut changed = false;
        for target_id in targets {
            if let Some(target) = self
                .design
                .element_mut(target_id)
                .filter(|target| !target.locked)
            {
                target.color = source.color;
                target.opacity = source.opacity;
                target.appearance = source.appearance.clone();
                changed = true;
            }
        }
        if changed {
            self.history.record(previous);
        }
    }

    fn select_hit(&mut self, hit: Option<u64>, additive: bool) {
        let members = hit
            .map(|id| crate::editor::selection_ids(&self.design, &[id]))
            .unwrap_or_default();
        if additive {
            if !members.is_empty() && members.iter().all(|id| self.selected_ids.contains(id)) {
                self.selected_ids.retain(|id| !members.contains(id));
            } else {
                for id in members {
                    if !self.selected_ids.contains(&id) {
                        self.selected_ids.push(id);
                    }
                }
            }
            self.selected = self.selected_ids.last().copied();
        } else {
            self.selected = hit;
            self.selected_ids = members;
        }
    }

    fn sync_selection_ids(&mut self) {
        self.selected_ids
            .retain(|id| self.design.element(*id).is_some());
        match self.selected {
            Some(id) if !self.selected_ids.contains(&id) => {
                self.selected_ids = crate::editor::selection_ids(&self.design, &[id])
            }
            None => self.selected_ids.clear(),
            _ => {}
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
        self.open_external_project(ui.ctx());
        self.sync_from_document(ui.ctx());
        self.apply_pending_updates(ui.ctx());
        self.sync_selection_ids();
        self.handle_shortcuts(ui.ctx());
        if self
            .design
            .assets
            .keys()
            .any(|id| !self.textures.contains_key(id))
            || self
                .textures
                .keys()
                .any(|id| !self.design.assets.contains_key(id))
        {
            self.reload_textures(ui.ctx());
        }
        self.top_bar(ui);
        self.tool_panel(ui);
        self.right_panel(ui);
        self.canvas(ui);
        self.font_browser(ui.ctx());
        self.library_window(ui.ctx());
        self.export_review_window(ui.ctx());
        self.help_window(ui.ctx());
        self.clear_confirmation(ui.ctx());
        self.publish_to_document();
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

fn export_kind_label(kind: ExportKind) -> &'static str {
    match kind {
        ExportKind::Png => "Transparent PNG",
        ExportKind::PdfSheet => "A4 print sheet PDF",
        ExportKind::PdfSeries => "Personalized series PDF",
        ExportKind::Svg => "SVG",
    }
}

fn series_signature(series: &SeriesData) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    series.source_name.hash(&mut hasher);
    series.columns.hash(&mut hasher);
    series.rows.len().hash(&mut hasher);
    for row in &series.rows {
        for column in &series.columns {
            column.hash(&mut hasher);
            row.get(column).hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn show_preflight_issue(ui: &mut Ui, issue: &PreflightIssue) {
    let color = match issue.severity {
        IssueSeverity::Error => Color32::from_rgb(178, 63, 52),
        IssueSeverity::Warning => Color32::from_rgb(171, 105, 32),
    };
    ui.colored_label(
        color,
        format!("{} · {}", issue.code.replace('_', " "), issue.message),
    );
}

fn format_row_numbers(rows: &[usize]) -> String {
    let list = rows
        .iter()
        .take(12)
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    if rows.len() > 12 {
        format!("{list}, … (+{} more)", rows.len() - 12)
    } else {
        list
    }
}

fn safe_file_stem(name: &str) -> String {
    let mut stem = name
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch)
            } else if ch == ' ' || ch == '-' || ch == '_' {
                Some('-')
            } else {
                None
            }
        })
        .take(64)
        .collect::<String>();
    while stem.contains("--") {
        stem = stem.replace("--", "-");
    }
    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        "button-template".into()
    } else {
        stem.into()
    }
}

fn design_palette(design: &Design) -> Vec<DesignColor> {
    let mut colors = vec![design.background];
    for element in &design.elements {
        for color in [
            Some(element.color),
            element.appearance.fill,
            element.appearance.outline,
        ]
        .into_iter()
        .flatten()
        {
            if !colors.contains(&color) {
                colors.push(color);
            }
            if colors.len() >= 32 {
                return colors;
            }
        }
    }
    colors
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

fn matching_fonts<'a>(available: &'a [AvailableFont], search: &str) -> Vec<&'a AvailableFont> {
    let query = search.trim().to_lowercase();
    available
        .iter()
        .filter(|font| query.is_empty() || font.label.to_lowercase().contains(&query))
        .collect()
}

fn matching_font_families(available: &[AvailableFont], search: &str) -> Vec<String> {
    let query = search.trim().to_lowercase();
    let mut families = available
        .iter()
        .filter(|font| query.is_empty() || font.label.to_lowercase().contains(&query))
        .map(|font| font.spec.family.clone())
        .collect::<Vec<_>>();
    families.sort_by_key(|family| family.to_lowercase());
    families.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    families
}

fn font_weights_for_family(
    available: &[AvailableFont],
    family: &str,
    slant: FontSlant,
) -> Vec<FontWeight> {
    let mut weights = available
        .iter()
        .filter(|font| font.spec.family.eq_ignore_ascii_case(family) && font.slant == slant)
        .map(|font| font.weight)
        .collect::<Vec<_>>();
    weights.sort();
    weights.dedup();
    weights
}

fn font_slants_for_family(
    available: &[AvailableFont],
    family: &str,
    weight: FontWeight,
) -> Vec<FontSlant> {
    let mut slants = available
        .iter()
        .filter(|font| font.spec.family.eq_ignore_ascii_case(family) && font.weight == weight)
        .map(|font| font.slant)
        .collect::<Vec<_>>();
    slants.sort();
    slants.dedup();
    slants
}

fn preferred_slant(options: &[FontSlant]) -> FontSlant {
    if options.contains(&FontSlant::Normal) {
        FontSlant::Normal
    } else {
        options.first().copied().unwrap_or(FontSlant::Normal)
    }
}

fn font_variant<'a>(
    available: &'a [AvailableFont],
    family: &str,
    weight: FontWeight,
    slant: FontSlant,
) -> Option<&'a AvailableFont> {
    available.iter().find(|font| {
        font.spec.family.eq_ignore_ascii_case(family)
            && font.weight == weight
            && font.slant == slant
    })
}

fn preferred_font_variant<'a>(
    available: &'a [AvailableFont],
    family: &str,
) -> Option<&'a AvailableFont> {
    available
        .iter()
        .find(|font| {
            font.spec.family.eq_ignore_ascii_case(family)
                && font.weight == FontWeight::Regular
                && font.slant == FontSlant::Normal
        })
        .or_else(|| {
            available.iter().find(|font| {
                font.spec.family.eq_ignore_ascii_case(family) && font.slant == FontSlant::Normal
            })
        })
        .or_else(|| {
            available.iter().find(|font| {
                font.spec.family.eq_ignore_ascii_case(family) && font.weight == FontWeight::Regular
            })
        })
        .or_else(|| {
            available
                .iter()
                .find(|font| font.spec.family.eq_ignore_ascii_case(family))
        })
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

fn shape_layer_dimensions(
    shape: ShapeKind,
    size: f32,
    width: Option<f32>,
    height: Option<f32>,
) -> (f32, f32) {
    let default_width = size;
    let default_height = if shape == ShapeKind::Rectangle {
        size * 2.0 / 3.0
    } else {
        default_width
    };
    (
        width.unwrap_or(default_width),
        height.unwrap_or(default_height),
    )
}

fn set_element_size(kind: &mut ElementKind, size: f32) {
    match kind {
        ElementKind::Text { font_size, .. } => *font_size = size.clamp(0.025, 0.2),
        ElementKind::Shape {
            size: shape_size,
            width,
            height,
            shape,
        } => {
            *shape_size = size.clamp(0.06, 0.68);
            let (next_width, next_height) = shape_layer_dimensions(*shape, *shape_size, None, None);
            *width = Some(next_width);
            *height = Some(next_height);
        }
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

fn align_selection(
    design: &mut Design,
    painter: &egui::Painter,
    ids: &[u64],
    reference: Option<u64>,
    alignment: ElementAlignment,
) -> bool {
    const R: f32 = 1000.0;
    let center = egui::Pos2::ZERO;
    let ids = crate::editor::selection_ids(design, ids);
    let mut bounds = ids
        .iter()
        .filter_map(|id| design.element(*id))
        .filter(|e| !e.locked)
        .map(|e| element_bounds(painter, center, R, e));
    let Some(first) = bounds.next() else {
        return false;
    };
    let source = bounds.fold(first, |a, b| a.union(b));
    if let Some(reference_id) = reference.filter(|id| design.element(*id).is_some()) {
        let reference = design.element(reference_id).unwrap();
        let target = element_bounds(painter, center, R, reference);
        let mut changed = false;
        for id in ids {
            if id == reference_id {
                continue;
            }
            if let Some(e) = design.element_mut(id).filter(|e| !e.locked) {
                let delta =
                    alignment_delta(element_bounds(painter, center, R, e), target, alignment) / R;
                e.position[0] += delta.x;
                e.position[1] += delta.y;
                changed = true;
            }
        }
        return changed;
    }
    let target = egui::Rect::from_center_size(center, Vec2::splat(R * 2.0));
    let delta = alignment_delta(source, target, alignment) / R;
    if delta == Vec2::ZERO {
        return false;
    }
    crate::editor::move_selection(design, &ids, [delta.x, delta.y]);
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

#[cfg(test)]
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
            width: None,
            height: None,
        };
        let mut image = ElementKind::Image {
            asset_id: "asset".to_owned(),
            width: 0.5,
            aspect_ratio: 1.0,
            bleed_to_square: false,
        };
        let mut qr_code = ElementKind::QrCode {
            content: "https://example.com".to_owned(),
            size: 0.5,
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
                weight: FontWeight::Bold,
                slant: FontSlant::Italic,
            },
            AvailableFont {
                label: "Courier · Regular".to_owned(),
                spec: TextFont::default(),
                weight: FontWeight::Regular,
                slant: FontSlant::Normal,
            },
        ];

        assert_eq!(matching_fonts(&available, "avenir").len(), 1);
        assert_eq!(matching_fonts(&available, "BOLD ITALIC").len(), 1);
        assert_eq!(matching_fonts(&available, "regular").len(), 1);
        assert_eq!(matching_fonts(&available, "missing").len(), 0);
    }
}

#[cfg(test)]
#[path = "app_workflow_tests.rs"]
mod workflow_tests;
