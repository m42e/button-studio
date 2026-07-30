use button_studio::app::ButtonStudio;
use button_studio::{document::SharedDocument, model::Design};

#[path = "bin/button-studio-mcp.rs"]
mod mcp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let headless = std::env::args()
        .skip(1)
        .any(|argument| argument == "--headless");
    if headless {
        return mcp::run(mcp::ButtonStudioServer::default());
    }

    let document = SharedDocument::new(Design::default());
    let server = mcp::ButtonStudioServer::with_document(document.clone());
    std::thread::spawn(move || {
        if let Err(error) = mcp::run(server) {
            eprintln!("MCP server stopped: {error}");
        }
    });
    run_gui(document)?;
    Ok(())
}

fn run_gui(document: SharedDocument) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Pincraft Button Studio")
            .with_inner_size([1_440.0, 900.0])
            .with_min_inner_size([1_120.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Pincraft Button Studio",
        options,
        Box::new(move |context| Ok(Box::new(ButtonStudio::with_document(context, document)))),
    )
}
