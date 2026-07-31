use base64::{Engine, engine::general_purpose::STANDARD};
use button_studio::{
    document::SharedDocument,
    model::{BackgroundStyle, Design, DesignColor, ElementKind, ShapeKind},
    render::{export_pdf_sheet, export_png, render_project_preview_png},
};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars::{self, JsonSchema},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, path::Path, sync::MutexGuard};

#[derive(Clone)]
pub struct ButtonStudioServer {
    document: SharedDocument,
}

impl Default for ButtonStudioServer {
    fn default() -> Self {
        Self {
            document: SharedDocument::new(blank_design(59)),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NewDesignParams {
    /// Finished button diameter in millimeters: 25, 32, 38, 50, 59, or 75.
    #[serde(default = "default_diameter")]
    diameter_mm: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PathParams {
    /// Absolute or workspace-relative file path.
    path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AddTextParams {
    text: String,
    #[serde(default)]
    x: Option<f32>,
    #[serde(default)]
    y: Option<f32>,
    /// Text size relative to the artwork radius, normally 0.025 to 0.24.
    #[serde(default)]
    size: Option<f32>,
    /// Color as #RRGGBB or #RRGGBBAA.
    #[serde(default)]
    color: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AddShapeParams {
    /// One of star, heart, circle, rectangle, diamond, or bolt.
    shape: String,
    #[serde(default)]
    x: Option<f32>,
    #[serde(default)]
    y: Option<f32>,
    /// Shape size relative to the artwork radius, normally 0.05 to 0.8.
    #[serde(default)]
    size: Option<f32>,
    /// Color as #RRGGBB or #RRGGBBAA.
    #[serde(default)]
    color: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AddQrParams {
    content: String,
    #[serde(default)]
    x: Option<f32>,
    #[serde(default)]
    y: Option<f32>,
    #[serde(default)]
    size: Option<f32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AddImageParams {
    /// PNG, JPEG, or WebP file to embed in the design.
    path: String,
    /// Use the image as a cover-cropped background instead of an object.
    #[serde(default)]
    background: bool,
    #[serde(default)]
    x: Option<f32>,
    #[serde(default)]
    y: Option<f32>,
    /// Image width relative to the artwork diameter, normally 0.05 to 4.0.
    #[serde(default)]
    width: Option<f32>,
    /// Extend the outermost pixels to fill a square around the image.
    #[serde(default)]
    bleed_to_square: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct UpdateObjectParams {
    id: u64,
    /// Horizontal position relative to the artwork radius, from -1 to 1.
    #[serde(default)]
    x: Option<f32>,
    /// Vertical position relative to the artwork radius, from -1 to 1.
    #[serde(default)]
    y: Option<f32>,
    /// Clockwise rotation in degrees.
    #[serde(default)]
    rotation: Option<f32>,
    /// Color as #RRGGBB or #RRGGBBAA.
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    opacity: Option<u8>,
    #[serde(default)]
    visible: Option<bool>,
    /// Replacement text or QR content. Only valid for text and QR objects.
    #[serde(default)]
    content: Option<String>,
    /// Text, shape, or QR size. Only valid for those object types.
    #[serde(default)]
    size: Option<f32>,
    /// Image width relative to artwork diameter. Only valid for image objects.
    #[serde(default)]
    width: Option<f32>,
    /// Text curvature from -1 to 1. Only valid for text objects.
    #[serde(default)]
    curvature: Option<f32>,
    /// Extend the outermost pixels to fill a square. Only valid for image objects.
    #[serde(default)]
    bleed_to_square: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ObjectIdParams {
    id: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct LayerParams {
    id: u64,
    /// Either front or back; moves the object by one layer.
    direction: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DesignSettingsParams {
    #[serde(default)]
    diameter_mm: Option<u32>,
    /// Color as #RRGGBB or #RRGGBBAA.
    #[serde(default)]
    background_color: Option<String>,
    /// One of solid, none, sunburst, confetti, or grid.
    #[serde(default)]
    background_style: Option<String>,
    /// Set true to remove the current background image.
    #[serde(default)]
    clear_background_image: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ExportParams {
    /// Output path. Parent directories must already exist.
    path: String,
    /// One of png, pdf, or project.
    format: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SnapshotParams {
    /// Square PNG size in pixels, from 128 to 2048.
    #[serde(default = "default_snapshot_size")]
    size: u32,
}

#[tool_router]
impl ButtonStudioServer {
    #[tool(description = "Replace the current design with a blank button")]
    fn new_design(
        &self,
        Parameters(params): Parameters<NewDesignParams>,
    ) -> Result<CallToolResult, McpError> {
        validate_diameter(params.diameter_mm)?;
        *self.lock()? = blank_design(params.diameter_mm);
        self.changed();
        Ok(structured(
            json!({ "diameter_mm": params.diameter_mm, "objects": 0 }),
        ))
    }

    #[tool(description = "Open a .buttonstudio project into the current server session")]
    fn open_project(
        &self,
        Parameters(params): Parameters<PathParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = Path::new(&params.path);
        let bytes = fs::read(path).map_err(tool_error)?;
        let design = Design::from_project_json(&bytes, Some(path)).map_err(tool_error)?;
        let count = design.elements.len();
        *self.lock()? = design;
        self.changed();
        Ok(structured(json!({ "path": params.path, "objects": count })))
    }

    #[tool(
        description = "Save the current editable design as a self-contained .buttonstudio project"
    )]
    fn save_project(
        &self,
        Parameters(params): Parameters<PathParams>,
    ) -> Result<CallToolResult, McpError> {
        let design = self.lock()?;
        let preview = render_project_preview_png(&design, 512).map_err(tool_error)?;
        let bytes = design
            .to_project_json_with_preview(Some((&preview, 512, 512)))
            .map_err(tool_error)?;
        fs::write(&params.path, bytes).map_err(tool_error)?;
        Ok(structured(
            json!({ "path": params.path, "format": "project" }),
        ))
    }

    #[tool(
        description = "Return the current design as structured JSON, including object IDs and properties"
    )]
    fn get_design(&self) -> Result<CallToolResult, McpError> {
        let value = serde_json::to_value(&*self.lock()?).map_err(tool_error)?;
        Ok(structured(value))
    }

    #[tool(description = "Add a text object and return its ID")]
    fn add_text(
        &self,
        Parameters(params): Parameters<AddTextParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut design = self.lock()?;
        let id = design.add_text(params.text);
        let update = apply_new_object(
            &mut design,
            id,
            params.x,
            params.y,
            params.size,
            None,
            params.color,
        );
        self.changed();
        update?;
        Ok(object_result(&design, id))
    }

    #[tool(description = "Add a geometric shape object and return its ID")]
    fn add_shape(
        &self,
        Parameters(params): Parameters<AddShapeParams>,
    ) -> Result<CallToolResult, McpError> {
        let shape = parse_shape(&params.shape)?;
        let mut design = self.lock()?;
        let id = design.add_shape(shape);
        let update = apply_new_object(
            &mut design,
            id,
            params.x,
            params.y,
            params.size,
            None,
            params.color,
        );
        self.changed();
        update?;
        Ok(object_result(&design, id))
    }

    #[tool(description = "Add a QR code object and return its ID")]
    fn add_qr_code(
        &self,
        Parameters(params): Parameters<AddQrParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut design = self.lock()?;
        let id = design.add_qr_code(params.content);
        let update = apply_new_object(&mut design, id, params.x, params.y, params.size, None, None);
        self.changed();
        update?;
        Ok(object_result(&design, id))
    }

    #[tool(description = "Embed an image file as an image object or cover-cropped background")]
    fn add_image(
        &self,
        Parameters(params): Parameters<AddImageParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut design = self.lock()?;
        let (asset_id, aspect_ratio) = design
            .import_asset_path(Path::new(&params.path))
            .map_err(tool_error)?;
        let id = if params.background {
            design.set_background_image(asset_id, aspect_ratio)
        } else {
            let id = design.add_image(asset_id, aspect_ratio);
            apply_new_object(
                &mut design,
                id,
                params.x,
                params.y,
                None,
                params.width,
                None,
            )?;
            if params.bleed_to_square
                && let Some(ElementKind::Image {
                    bleed_to_square, ..
                }) = design.element_mut(id).map(|element| &mut element.kind)
            {
                *bleed_to_square = true;
            }
            id
        };
        self.changed();
        Ok(structured(
            json!({ "id": id, "background": params.background, "path": params.path }),
        ))
    }

    #[tool(
        description = "Update position, appearance, visibility, or type-specific content of an object"
    )]
    fn update_object(
        &self,
        Parameters(params): Parameters<UpdateObjectParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut design = self.lock()?;
        let color = params.color.as_deref().map(parse_color).transpose()?;
        let element = design
            .element_mut(params.id)
            .ok_or_else(|| invalid(format!("Object {} does not exist", params.id)))?;
        if let Some(x) = params.x {
            element.position[0] = finite(x, "x")?;
        }
        if let Some(y) = params.y {
            element.position[1] = finite(y, "y")?;
        }
        if let Some(rotation) = params.rotation {
            element.rotation = finite(rotation, "rotation")?;
        }
        if let Some(color) = color {
            element.color = color;
        }
        if let Some(opacity) = params.opacity {
            element.opacity = opacity;
        }
        if let Some(visible) = params.visible {
            element.visible = visible;
        }
        let update = update_kind(
            &mut element.kind,
            params.content,
            params.size,
            params.width,
            params.curvature,
            params.bleed_to_square,
        );
        design.normalize_after_load();
        self.changed();
        update?;
        Ok(object_result(&design, params.id))
    }

    #[tool(description = "Delete an object by ID")]
    fn delete_object(
        &self,
        Parameters(params): Parameters<ObjectIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let deleted = self.lock()?.delete(params.id);
        if !deleted {
            return Err(invalid(format!("Object {} does not exist", params.id)));
        }
        self.changed();
        Ok(structured(json!({ "deleted": params.id })))
    }

    #[tool(description = "Duplicate an object and return the new object ID")]
    fn duplicate_object(
        &self,
        Parameters(params): Parameters<ObjectIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut design = self.lock()?;
        let id = design
            .duplicate(params.id)
            .ok_or_else(|| invalid(format!("Object {} does not exist", params.id)))?;
        self.changed();
        Ok(object_result(&design, id))
    }

    #[tool(description = "Move an object one layer toward the front or back")]
    fn move_layer(
        &self,
        Parameters(params): Parameters<LayerParams>,
    ) -> Result<CallToolResult, McpError> {
        let toward_front = match params.direction.as_str() {
            "front" => true,
            "back" => false,
            _ => return Err(invalid("direction must be front or back")),
        };
        let moved = self.lock()?.move_layer(params.id, toward_front);
        if moved {
            self.changed();
        }
        Ok(structured(
            json!({ "id": params.id, "moved": moved, "direction": params.direction }),
        ))
    }

    #[tool(
        description = "Change button size, background color/style, or clear its background image"
    )]
    fn set_design(
        &self,
        Parameters(params): Parameters<DesignSettingsParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut design = self.lock()?;
        if let Some(diameter) = params.diameter_mm {
            validate_diameter(diameter)?;
            design.diameter_mm = diameter;
        }
        if let Some(color) = params.background_color {
            design.background = parse_color(&color)?;
        }
        if let Some(style) = params.background_style {
            design.background_style = parse_background_style(&style)?;
        }
        if params.clear_background_image {
            design.background_image = None;
        }
        self.changed();
        Ok(structured(
            json!({ "diameter_mm": design.diameter_mm, "background_style": design.background_style.label() }),
        ))
    }

    #[tool(
        description = "Export the current design to a transparent PNG, printable A4 PDF sheet, or .buttonstudio project"
    )]
    fn export(
        &self,
        Parameters(params): Parameters<ExportParams>,
    ) -> Result<CallToolResult, McpError> {
        let design = self.lock()?;
        let result = match params.format.as_str() {
            "png" => {
                json!({ "pixels": export_png(&design, Path::new(&params.path)).map_err(tool_error)? })
            }
            "pdf" => {
                json!({ "buttons_per_page": export_pdf_sheet(&design, Path::new(&params.path)).map_err(tool_error)? })
            }
            "project" => {
                let preview = render_project_preview_png(&design, 512).map_err(tool_error)?;
                let bytes = design
                    .to_project_json_with_preview(Some((&preview, 512, 512)))
                    .map_err(tool_error)?;
                fs::write(&params.path, bytes).map_err(tool_error)?;
                json!({})
            }
            _ => return Err(invalid("format must be png, pdf, or project")),
        };
        Ok(structured(
            json!({ "path": params.path, "format": params.format, "details": result }),
        ))
    }

    #[tool(description = "Render and return an inline PNG snapshot of the current button")]
    fn get_snapshot(
        &self,
        Parameters(params): Parameters<SnapshotParams>,
    ) -> Result<CallToolResult, McpError> {
        if !(128..=2048).contains(&params.size) {
            return Err(invalid("size must be between 128 and 2048"));
        }
        let design = self.lock()?;
        let png = render_project_preview_png(&design, params.size).map_err(tool_error)?;
        Ok(CallToolResult::success(vec![
            ContentBlock::text(format!("{} x {} PNG snapshot", params.size, params.size)),
            ContentBlock::image(STANDARD.encode(png), "image/png"),
        ]))
    }
}

#[tool_handler(
    name = "button-studio",
    version = "0.1.0",
    instructions = "Create and edit Pincraft Button Studio designs. Object positions are normalized relative to the artwork radius. Call get_design to inspect object IDs and get_snapshot to visually verify changes."
)]
impl ServerHandler for ButtonStudioServer {}

impl ButtonStudioServer {
    pub fn with_document(document: SharedDocument) -> Self {
        Self { document }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Design>, McpError> {
        self.document
            .lock()
            .map_err(|error| McpError::internal_error(error, None))
    }

    fn changed(&self) {
        self.document.mark_changed();
    }
}

fn blank_design(diameter_mm: u32) -> Design {
    let mut design = Design::default();
    design.diameter_mm = diameter_mm;
    design.elements.clear();
    design.normalize_after_load();
    design
}

fn apply_new_object(
    design: &mut Design,
    id: u64,
    x: Option<f32>,
    y: Option<f32>,
    size: Option<f32>,
    width: Option<f32>,
    color: Option<String>,
) -> Result<(), McpError> {
    let element = design.element_mut(id).expect("new object exists");
    if let Some(x) = x {
        element.position[0] = finite(x, "x")?;
    }
    if let Some(y) = y {
        element.position[1] = finite(y, "y")?;
    }
    if let Some(color) = color {
        element.color = parse_color(&color)?;
    }
    update_kind(&mut element.kind, None, size, width, None, None)?;
    design.normalize_after_load();
    Ok(())
}

fn update_kind(
    kind: &mut ElementKind,
    content: Option<String>,
    size: Option<f32>,
    width: Option<f32>,
    curvature: Option<f32>,
    bleed_to_square: Option<bool>,
) -> Result<(), McpError> {
    match kind {
        ElementKind::Text {
            text,
            font_size,
            curvature: current,
            ..
        } => {
            if let Some(content) = content {
                *text = content;
            }
            if let Some(size) = size {
                *font_size = finite(size, "size")?;
            }
            if let Some(curvature) = curvature {
                *current = finite(curvature, "curvature")?;
            }
            if width.is_some() || bleed_to_square.is_some() {
                return Err(invalid(
                    "width and bleed_to_square are only valid for image objects",
                ));
            }
        }
        ElementKind::Shape { size: current, .. } => {
            if content.is_some()
                || curvature.is_some()
                || width.is_some()
                || bleed_to_square.is_some()
            {
                return Err(invalid("shape objects only support size"));
            }
            if let Some(size) = size {
                *current = finite(size, "size")?;
            }
        }
        ElementKind::Image {
            width: current,
            bleed_to_square: current_bleed,
            ..
        } => {
            if content.is_some() || size.is_some() || curvature.is_some() {
                return Err(invalid("image objects use width, not content or size"));
            }
            if let Some(width) = width {
                *current = finite(width, "width")?;
            }
            if let Some(bleed_to_square) = bleed_to_square {
                *current_bleed = bleed_to_square;
            }
        }
        ElementKind::QrCode {
            content: current_content,
            size: current_size,
            ..
        } => {
            if let Some(content) = content {
                *current_content = content;
            }
            if let Some(size) = size {
                *current_size = finite(size, "size")?;
            }
            if width.is_some() || curvature.is_some() || bleed_to_square.is_some() {
                return Err(invalid("QR objects do not support width or curvature"));
            }
        }
    }
    Ok(())
}

fn parse_shape(value: &str) -> Result<ShapeKind, McpError> {
    match value {
        "star" => Ok(ShapeKind::Star),
        "heart" => Ok(ShapeKind::Heart),
        "circle" => Ok(ShapeKind::Circle),
        "rectangle" => Ok(ShapeKind::Rectangle),
        "diamond" => Ok(ShapeKind::Diamond),
        "bolt" => Ok(ShapeKind::Bolt),
        _ => Err(invalid(
            "shape must be star, heart, circle, rectangle, diamond, or bolt",
        )),
    }
}

fn parse_background_style(value: &str) -> Result<BackgroundStyle, McpError> {
    match value {
        "solid" => Ok(BackgroundStyle::Solid),
        "none" => Ok(BackgroundStyle::None),
        "sunburst" => Ok(BackgroundStyle::Sunburst),
        "confetti" => Ok(BackgroundStyle::Confetti),
        "grid" => Ok(BackgroundStyle::Grid),
        _ => Err(invalid(
            "background_style must be solid, none, sunburst, confetti, or grid",
        )),
    }
}

fn parse_color(value: &str) -> Result<DesignColor, McpError> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 && hex.len() != 8 {
        return Err(invalid("color must be #RRGGBB or #RRGGBBAA"));
    }
    let byte = |offset| {
        u8::from_str_radix(&hex[offset..offset + 2], 16)
            .map_err(|_| invalid("color contains invalid hexadecimal digits"))
    };
    Ok(DesignColor {
        red: byte(0)?,
        green: byte(2)?,
        blue: byte(4)?,
        alpha: if hex.len() == 8 { byte(6)? } else { 255 },
    })
}

fn validate_diameter(value: u32) -> Result<(), McpError> {
    if [25, 32, 38, 50, 59, 75].contains(&value) {
        Ok(())
    } else {
        Err(invalid("diameter_mm must be 25, 32, 38, 50, 59, or 75"))
    }
}

fn finite(value: f32, name: &str) -> Result<f32, McpError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid(format!("{name} must be finite")))
    }
}

fn object_result(design: &Design, id: u64) -> CallToolResult {
    structured(json!({ "object": design.element(id), "id": id }))
}

fn structured(value: Value) -> CallToolResult {
    CallToolResult::structured(value)
}
fn invalid(message: impl Into<String>) -> McpError {
    McpError::invalid_params(message.into(), None)
}
fn tool_error(error: impl std::fmt::Display) -> McpError {
    invalid(error.to_string())
}
const fn default_diameter() -> u32 {
    59
}
const fn default_snapshot_size() -> u32 {
    512
}

#[tokio::main]
pub async fn run(server: ButtonStudioServer) -> Result<(), Box<dyn std::error::Error>> {
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_design_has_no_objects() {
        let design = blank_design(38);
        assert_eq!(design.diameter_mm, 38);
        assert!(design.elements.is_empty());
    }

    #[test]
    fn colors_accept_rgb_and_rgba_hex() {
        assert_eq!(
            parse_color("#102030").unwrap(),
            DesignColor {
                red: 16,
                green: 32,
                blue: 48,
                alpha: 255
            }
        );
        assert_eq!(parse_color("10203040").unwrap().alpha, 64);
        assert!(parse_color("red").is_err());
    }

    #[test]
    fn tools_create_update_and_snapshot_a_design() {
        let server = ButtonStudioServer::default();
        server
            .add_text(Parameters(AddTextParams {
                text: "MCP WORKS".to_owned(),
                x: Some(0.1),
                y: Some(-0.2),
                size: Some(0.12),
                color: Some("#FFCC00".to_owned()),
            }))
            .unwrap();
        server
            .update_object(Parameters(UpdateObjectParams {
                id: 1,
                x: None,
                y: None,
                rotation: Some(12.0),
                color: None,
                opacity: Some(220),
                visible: None,
                content: None,
                size: None,
                width: None,
                curvature: None,
                bleed_to_square: None,
            }))
            .unwrap();

        let design = server.lock().unwrap();
        let element = design.element(1).unwrap();
        assert_eq!(element.rotation, 12.0);
        assert_eq!(element.opacity, 220);
        drop(design);

        let snapshot = server
            .get_snapshot(Parameters(SnapshotParams { size: 128 }))
            .unwrap();
        assert_eq!(snapshot.content.len(), 2);
        assert_eq!(
            snapshot.content[1].as_image().unwrap().mime_type,
            "image/png"
        );
    }

    #[test]
    fn mutations_are_published_to_the_shared_gui_document() {
        let document = SharedDocument::new(blank_design(59));
        let server = ButtonStudioServer::with_document(document.clone());
        server
            .add_shape(Parameters(AddShapeParams {
                shape: "circle".to_owned(),
                x: None,
                y: None,
                size: None,
                color: None,
            }))
            .unwrap();

        let (design, revision) = document.snapshot().unwrap();
        assert_eq!(revision, 1);
        assert_eq!(design.elements.len(), 1);
    }
}
