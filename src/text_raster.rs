use crate::{appearance::TextAlignment, model::TextFont};
use cosmic_text::fontdb;
use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent, Weight,
    Wrap,
};
use image::{Rgba, RgbaImage};
use std::sync::{Mutex, OnceLock};

const DISPLAY_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/ArchivoBlack-Regular.ttf");

fn font_system() -> &'static Mutex<FontSystem> {
    static FONTS: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut fonts = FontSystem::new();
        fonts.db_mut().load_font_data(DISPLAY_FONT_BYTES.to_vec());
        Mutex::new(fonts)
    })
}

fn swash_cache() -> &'static Mutex<SwashCache> {
    static CACHE: OnceLock<Mutex<SwashCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(SwashCache::new()))
}

/// One shaped glyph rasterized to a tight, already-colored bitmap.
struct PlacedGlyph {
    tile: RgbaImage,
    left: i32,
    top: i32,
    advance: f32,
    line: usize,
    baseline: i32,
}

fn face_attrs(fonts: &FontSystem, font: &TextFont) -> (Weight, Style) {
    for face in fonts.db().faces() {
        if face.post_script_name == font.face {
            let style = match face.style {
                fontdb::Style::Italic => Style::Italic,
                fontdb::Style::Oblique => Style::Oblique,
                fontdb::Style::Normal => Style::Normal,
            };
            return (Weight(face.weight.0), style);
        }
    }
    (Weight::NORMAL, Style::Normal)
}

fn tile_from(image: &cosmic_text::SwashImage, color: Rgba<u8>) -> Option<RgbaImage> {
    let width = image.placement.width;
    let height = image.placement.height;
    if width == 0 || height == 0 {
        return None;
    }
    let mut tile = RgbaImage::new(width, height);
    match image.content {
        SwashContent::Mask => {
            for (index, pixel) in tile.pixels_mut().enumerate() {
                let coverage = image.data.get(index).copied().unwrap_or(0) as u16;
                let alpha = (coverage * color[3] as u16 / 255) as u8;
                *pixel = Rgba([color[0], color[1], color[2], alpha]);
            }
        }
        SwashContent::Color => {
            for (index, pixel) in tile.pixels_mut().enumerate() {
                let base = index * 4;
                let Some(chunk) = image.data.get(base..base + 4) else {
                    break;
                };
                let alpha = (chunk[3] as u16 * color[3] as u16 / 255) as u8;
                *pixel = Rgba([chunk[0], chunk[1], chunk[2], alpha]);
            }
        }
        SwashContent::SubpixelMask => {
            for (index, pixel) in tile.pixels_mut().enumerate() {
                let base = index * 4;
                let Some(chunk) = image.data.get(base..base + 4) else {
                    break;
                };
                let coverage = chunk[0].max(chunk[1]).max(chunk[2]) as u16;
                let alpha = (coverage * color[3] as u16 / 255) as u8;
                *pixel = Rgba([color[0], color[1], color[2], alpha]);
            }
        }
    }
    Some(tile)
}

fn shape(text: &str, px: f32, font: &TextFont, color: Rgba<u8>) -> Vec<PlacedGlyph> {
    if text.is_empty() || px <= 0.0 {
        return Vec::new();
    }
    let mut fonts = font_system().lock().expect("font system poisoned");
    let mut cache = swash_cache().lock().expect("swash cache poisoned");

    let (weight, style) = face_attrs(&fonts, font);
    let metrics = Metrics::new(px, px * 1.25);
    let mut buffer = Buffer::new(&mut fonts, metrics);
    buffer.set_wrap(Wrap::None);
    buffer.set_size(None, None);
    let attrs = Attrs::new()
        .family(Family::Name(&font.family))
        .weight(weight)
        .style(style);
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut fonts, false);

    let mut glyphs = Vec::new();
    for (line, run) in buffer.layout_runs().enumerate() {
        let baseline = run.line_y.round() as i32;
        for glyph in run.glyphs.iter() {
            let physical = glyph.physical((0.0, 0.0), 1.0);
            let Some(image) = cache.get_image_uncached(&mut fonts, physical.cache_key) else {
                glyphs.push(PlacedGlyph {
                    tile: RgbaImage::new(0, 0),
                    left: 0,
                    top: 0,
                    advance: glyph.w,
                    line,
                    baseline,
                });
                continue;
            };
            let left = physical.x + image.placement.left;
            let top = baseline + physical.y - image.placement.top;
            let tile = tile_from(&image, color).unwrap_or_else(|| RgbaImage::new(0, 0));
            glyphs.push(PlacedGlyph {
                tile,
                left,
                top,
                advance: glyph.w,
                line,
                baseline,
            });
        }
    }
    glyphs
}

/// Rasterize `text` on a single line into a tightly cropped image, using system
/// font fallback so missing glyphs (emoji, symbols) resolve to a covering font.
/// Color-emoji glyphs keep their own colors; monochrome glyphs use `color`.
pub fn render_line(text: &str, px: f32, font: &TextFont, color: Rgba<u8>) -> Option<RgbaImage> {
    render_line_styled(text, px, font, color, 1.0, 0.0, TextAlignment::Center)
}

pub fn render_line_styled(
    text: &str,
    px: f32,
    font: &TextFont,
    color: Rgba<u8>,
    line_spacing: f32,
    letter_spacing: f32,
    alignment: TextAlignment,
) -> Option<RgbaImage> {
    let mut glyphs = shape(text, px, font, color);
    let extra = letter_spacing * px / 100.0;
    let line_count = glyphs.iter().map(|g| g.line).max().unwrap_or(0) + 1;
    let mut ordinals = vec![0usize; line_count];
    let first_baseline = glyphs
        .iter()
        .find(|g| g.line == 0)
        .map(|g| g.baseline)
        .unwrap_or(0);
    for glyph in &mut glyphs {
        glyph.left += (ordinals[glyph.line] as f32 * extra).round() as i32;
        let line_offset = glyph.baseline - first_baseline;
        glyph.top += (line_offset as f32 * (line_spacing.clamp(0.25, 4.0) - 1.0)).round() as i32;
        ordinals[glyph.line] += 1;
    }
    let mut line_bounds = vec![None::<(i32, i32)>; line_count];
    for glyph in &glyphs {
        let b = line_bounds[glyph.line]
            .get_or_insert((glyph.left, glyph.left + glyph.advance.ceil() as i32));
        b.0 = b.0.min(glyph.left);
        b.1 = b.1.max(glyph.left + glyph.advance.ceil() as i32);
    }
    let widest = line_bounds
        .iter()
        .flatten()
        .map(|(a, b)| b - a)
        .max()
        .unwrap_or(0);
    for glyph in &mut glyphs {
        let (left, right) = line_bounds[glyph.line].unwrap_or((0, 0));
        glyph.left += match alignment {
            TextAlignment::Start => -left,
            TextAlignment::Center => (widest - (right - left)) / 2 - left,
            TextAlignment::End => widest - right,
        };
    }
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    for glyph in &glyphs {
        if glyph.tile.width() == 0 || glyph.tile.height() == 0 {
            continue;
        }
        let right = glyph.left + glyph.tile.width() as i32;
        let bottom = glyph.top + glyph.tile.height() as i32;
        bounds = Some(match bounds {
            Some((left, top, r, b)) => (
                left.min(glyph.left),
                top.min(glyph.top),
                r.max(right),
                b.max(bottom),
            ),
            None => (glyph.left, glyph.top, right, bottom),
        });
    }
    let (left, top, right, bottom) = bounds?;
    let mut canvas = RgbaImage::new((right - left) as u32, (bottom - top) as u32);
    for glyph in &glyphs {
        if glyph.tile.width() == 0 || glyph.tile.height() == 0 {
            continue;
        }
        image::imageops::overlay(
            &mut canvas,
            &glyph.tile,
            (glyph.left - left) as i64,
            (glyph.top - top) as i64,
        );
    }
    Some(canvas)
}

/// Per-glyph tiles for curved text: each entry is a tight glyph bitmap (or `None`
/// for blank advances like spaces) paired with its advance width.
pub fn render_glyph_tiles(
    text: &str,
    px: f32,
    font: &TextFont,
    color: Rgba<u8>,
) -> Vec<(Option<RgbaImage>, f32)> {
    shape(text, px, font, color)
        .into_iter()
        .map(|glyph| {
            let tile = (glyph.tile.width() > 0 && glyph.tile.height() > 0).then_some(glyph.tile);
            (tile, glyph.advance)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiline_alignment_and_spacing_change_the_raster_consistently() {
        let color = Rgba([255, 255, 255, 255]);
        let centered = render_line_styled(
            "A\nAAAA",
            36.0,
            &TextFont::default(),
            color,
            1.0,
            0.0,
            TextAlignment::Center,
        )
        .unwrap();
        let end_spaced = render_line_styled(
            "A\nAAAA",
            36.0,
            &TextFont::default(),
            color,
            1.4,
            3.0,
            TextAlignment::End,
        )
        .unwrap();
        assert!(end_spaced.height() > centered.height());
        assert_ne!(centered.as_raw(), end_spaced.as_raw());
        let one = render_line_styled(
            "BASELINE",
            36.0,
            &TextFont::default(),
            color,
            1.0,
            0.0,
            TextAlignment::Center,
        )
        .unwrap();
        let two = render_line_styled(
            "BASELINE",
            36.0,
            &TextFont::default(),
            color,
            2.0,
            0.0,
            TextAlignment::Center,
        )
        .unwrap();
        assert_eq!(one, two, "line spacing must not alter a single line");
    }
}
