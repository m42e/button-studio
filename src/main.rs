use button_studio::app::ButtonStudio;
use button_studio::{document::SharedDocument, model::Design};
use std::path::PathBuf;
use std::sync::mpsc;

#[cfg(target_os = "macos")]
mod macos;

#[path = "bin/button-studio-mcp.rs"]
mod mcp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let headless = std::env::args()
        .skip(1)
        .any(|argument| argument == "--headless");
    if headless {
        return mcp::run(mcp::ButtonStudioServer::default());
    }

    let project_path = project_path_from_args(std::env::args().skip(1));
    let document = SharedDocument::new(Design::default());
    let server = mcp::ButtonStudioServer::with_document(document.clone());
    std::thread::spawn(move || {
        if let Err(error) = mcp::run(server) {
            eprintln!("MCP server stopped: {error}");
        }
    });
    run_gui(document, project_path)?;
    Ok(())
}

fn project_path_from_args(arguments: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    arguments
        .into_iter()
        .find(|argument| !argument.starts_with('-'))
        .map(PathBuf::from)
}

fn run_gui(document: SharedDocument, project_path: Option<PathBuf>) -> eframe::Result {
    let (project_sender, project_receiver) = mpsc::channel();
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
        Box::new(move |context| {
            let mut app =
                ButtonStudio::with_document_and_project_paths(context, document, project_receiver);
            #[cfg(target_os = "macos")]
            macos::install_open_document_handler(project_sender, context.egui_ctx.clone());
            if let Some(path) = project_path {
                app.open_project_path(&context.egui_ctx, path);
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::project_path_from_args;

    #[test]
    fn finds_document_path_after_launcher_arguments() {
        let path = project_path_from_args([
            "--some-launcher-flag".to_owned(),
            "/Users/test/Downloads/my-button.buttonstudio".to_owned(),
        ]);

        assert_eq!(
            path.and_then(|path| path.to_str().map(str::to_owned)),
            Some("/Users/test/Downloads/my-button.buttonstudio".to_owned())
        );
    }
}
