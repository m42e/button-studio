//! SVG import deliberately creates a single embedded raster image. Native
//! vector editing remains in the source application; project assets are portable.

pub const IMPORT_NOTICE: &str = "SVG artwork will be imported as one image at 2048 pixels on its longest side. Individual paths and text will not be editable. Check the preview against the original before printing.";

pub fn rasterize_svg(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let xml = std::str::from_utf8(bytes).map_err(|_| "SVG must be UTF-8 XML".to_owned())?;
    let document =
        roxmltree::Document::parse(xml).map_err(|error| format!("Could not read SVG: {error}"))?;
    if document.root_element().tag_name().name() != "svg" {
        return Err("The file is not an SVG document".to_owned());
    }
    for node in document.descendants().filter(|node| node.is_element()) {
        if matches!(
            node.tag_name().name(),
            "foreignObject" | "script" | "animate" | "animateTransform" | "animateMotion" | "set"
        ) {
            return Err(format!(
                "SVG contains unsupported <{}> content. Export a static SVG or PNG from the source application.",
                node.tag_name().name()
            ));
        }
        for attribute in node.attributes() {
            if attribute.name() == "href" {
                let value = attribute.value().trim();
                if !value.starts_with('#') && !value.starts_with("data:") {
                    return Err("SVG links to an external resource. Embed linked images in the source application before importing.".to_owned());
                }
            }
        }
    }
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    options
        .fontdb_mut()
        .load_font_data(include_bytes!("../assets/fonts/ArchivoBlack-Regular.ttf").to_vec());
    let tree = resvg::usvg::Tree::from_data(bytes, &options)
        .map_err(|error| format!("Could not render SVG: {error}"))?;
    let size = tree.size();
    let scale = 2048.0 / size.width().max(size.height());
    let width = (size.width() * scale).round().max(1.0) as u32;
    let height = (size.height() * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "Could not allocate SVG preview".to_owned())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|error| format!("Could not encode SVG image: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_transparency_and_aspect_ratio_without_source_dependencies() {
        let png = rasterize_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="100" height="100" fill="red"/></svg>"#).unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (2048, 1024));
        assert_eq!(image.get_pixel(200, 200).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(1800, 200).0[3], 0);
    }

    #[test]
    fn refuses_linked_artwork_and_unsupported_active_content() {
        assert!(
            rasterize_svg(br#"<svg><image href="file:///tmp/logo.png"/></svg>"#)
                .unwrap_err()
                .contains("Embed")
        );
        assert!(
            rasterize_svg(br#"<svg><foreignObject/></svg>"#)
                .unwrap_err()
                .contains("unsupported")
        );
        assert!(rasterize_svg(b"not an SVG").is_err());
    }
}
