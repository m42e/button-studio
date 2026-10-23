//! SVG import deliberately creates a single embedded raster image. Native
//! vector editing remains in the source application; project assets are portable.

use std::borrow::Cow;

pub const IMPORT_NOTICE: &str = "SVG artwork will be imported as one image at 2048 pixels on its longest side. Individual paths and text will not be editable. Check the preview against the original before printing.";

pub fn rasterize_svg(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let xml = std::str::from_utf8(bytes).map_err(|_| "SVG must be UTF-8 XML".to_owned())?;
    let xml = strip_doctype(xml)?;
    let document = roxmltree::Document::parse(xml.as_ref())
        .map_err(|error| format!("Could not read SVG: {error}"))?;
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
    let tree = resvg::usvg::Tree::from_data(xml.as_bytes(), &options)
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

/// Remove the optional XML DTD declaration without resolving external DTDs.
/// SVG renderers do not need the legacy SVG 1.1 DTD to process its elements.
fn strip_doctype(xml: &str) -> Result<Cow<'_, str>, String> {
    let mut offset = xml.len() - xml.trim_start_matches('\u{feff}').len();
    loop {
        let remaining = &xml[offset..];
        let trimmed = remaining.trim_start();
        offset += remaining.len() - trimmed.len();

        if trimmed.starts_with("<?") {
            let end = trimmed.find("?>").ok_or_else(|| {
                "Could not read SVG: unterminated processing instruction".to_owned()
            })?;
            offset += end + 2;
        } else if trimmed.starts_with("<!--") {
            let end = trimmed
                .find("-->")
                .ok_or_else(|| "Could not read SVG: unterminated comment".to_owned())?;
            offset += end + 3;
        } else if trimmed.starts_with("<!DOCTYPE") {
            let start = offset;
            let end = doctype_end(xml, start + "<!DOCTYPE".len())?;
            let mut sanitized = String::with_capacity(xml.len() - (end - start));
            sanitized.push_str(&xml[..start]);
            sanitized.push_str(&xml[end..]);
            return Ok(Cow::Owned(sanitized));
        } else {
            return Ok(Cow::Borrowed(xml));
        }
    }
}

fn doctype_end(xml: &str, mut offset: usize) -> Result<usize, String> {
    let bytes = xml.as_bytes();
    let mut quote = None;
    let mut subset_depth = 0usize;

    while offset < bytes.len() {
        if quote.is_none() && bytes[offset..].starts_with(b"<!--") {
            let end = xml[offset + 4..]
                .find("-->")
                .ok_or_else(|| "Could not read SVG: unterminated DTD comment".to_owned())?;
            offset += 4 + end + 3;
            continue;
        }

        let byte = bytes[offset];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'[' => subset_depth += 1,
                b']' => subset_depth = subset_depth.saturating_sub(1),
                b'>' if subset_depth == 0 => return Ok(offset + 1),
                _ => {}
            }
        }
        offset += 1;
    }

    Err("Could not read SVG: unterminated DOCTYPE declaration".to_owned())
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

    #[test]
    fn imports_svg_with_legacy_external_doctype() {
        let svg = br#"<?xml version="1.0"?><!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd"><svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;
        let png = rasterize_svg(svg).expect("render SVG without fetching its external DTD");
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (2048, 1024));
        assert_eq!(image.get_pixel(1000, 500).0, [255, 0, 0, 255]);
    }
}
