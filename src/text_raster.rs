use crate::model::TextFont;
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
    for run in buffer.layout_runs() {
        let baseline = run.line_y.round() as i32;
        for glyph in run.glyphs.iter() {
            let physical = glyph.physical((0.0, 0.0), 1.0);
            let Some(image) = cache.get_image_uncached(&mut fonts, physical.cache_key) else {
                glyphs.push(PlacedGlyph {
                    tile: RgbaImage::new(0, 0),
                    left: 0,
                    top: 0,
                    advance: glyph.w,
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
            });
        }
    }
    glyphs
}

/// Rasterize `text` on a single line into a tightly cropped image, using system
/// font fallback so missing glyphs (emoji, symbols) resolve to a covering font.
/// Color-emoji glyphs keep their own colors; monochrome glyphs use `color`.
pub fn render_line(text: &str, px: f32, font: &TextFont, color: Rgba<u8>) -> Option<RgbaImage> {
    let glyphs = shape(text, px, font, color);
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
