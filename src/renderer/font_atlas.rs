use serde::Deserialize;
use std::collections::HashMap;
use std::io::Cursor;

#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    pub uv_x: f32,
    pub uv_y: f32,
    pub uv_w: f32,
    pub uv_h: f32,
    /// Glyph quad size in atlas pixels.
    pub width: f32,
    pub height: f32,
    /// Horizontal offset from pen position (atlas pixels).
    pub offset_x: f32,
    /// Vertical offset from line top (atlas pixels, y-down).
    pub offset_y: f32,
    /// Horizontal advance (atlas pixels).
    pub advance: f32,
}

pub struct FontAtlas {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub glyphs: HashMap<char, GlyphInfo>,
    /// Line height in atlas pixels — used as the reference size for scaling.
    pub font_size: f32,
}

// msdf-atlas-gen v1.3 JSON structures

#[derive(Deserialize)]
struct MsdfFont {
    atlas: AtlasInfo,
    metrics: FontMetrics,
    glyphs: Vec<MsdfGlyph>,
}

#[derive(Deserialize)]
struct AtlasInfo {
    size: f32,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct FontMetrics {
    #[serde(rename = "lineHeight")]
    line_height: f32,
    ascender: f32,
}

#[derive(Deserialize)]
struct MsdfGlyph {
    unicode: u32,
    advance: f32,
    #[serde(rename = "planeBounds")]
    plane_bounds: Option<Bounds>,
    #[serde(rename = "atlasBounds")]
    atlas_bounds: Option<Bounds>,
}

#[derive(Deserialize)]
struct Bounds {
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
}

impl FontAtlas {
    pub fn load_msdf(json_bytes: &[u8], png_bytes: &[u8]) -> Self {
        let font: MsdfFont =
            serde_json::from_slice(json_bytes).expect("Failed to parse MSDF font JSON");

        // Decode PNG
        let decoder = png::Decoder::new(Cursor::new(png_bytes));
        let mut reader = decoder.read_info().expect("Failed to read PNG info");
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let frame_info = reader.next_frame(&mut buf).expect("Failed to decode PNG");

        let width = frame_info.width;
        let height = frame_info.height;

        // Convert to RGBA
        let pixels = match frame_info.color_type {
            png::ColorType::Rgba => buf[..frame_info.buffer_size()].to_vec(),
            png::ColorType::Rgb => {
                let rgb = &buf[..frame_info.buffer_size()];
                let pixel_count = (width * height) as usize;
                let mut rgba = Vec::with_capacity(pixel_count * 4);
                for i in 0..pixel_count {
                    rgba.push(rgb[i * 3]);
                    rgba.push(rgb[i * 3 + 1]);
                    rgba.push(rgb[i * 3 + 2]);
                    rgba.push(255);
                }
                rgba
            }
            other => panic!("Unsupported PNG color type: {:?}", other),
        };

        let atlas_w = font.atlas.width as f32;
        let atlas_h = font.atlas.height as f32;
        let em_scale = font.atlas.size; // pixels per em
        let ascender = font.metrics.ascender; // in em units

        let mut glyphs = HashMap::new();

        for g in &font.glyphs {
            let ch = match char::from_u32(g.unicode) {
                Some(c) => c,
                None => continue,
            };

            match (&g.plane_bounds, &g.atlas_bounds) {
                (Some(pb), Some(ab)) => {
                    let glyph_w = ab.right - ab.left;
                    let glyph_h = ab.top - ab.bottom;

                    // UVs: atlas is bottom-origin, texture is top-origin → flip Y
                    let uv_x = ab.left / atlas_w;
                    let uv_y = (atlas_h - ab.top) / atlas_h;
                    let uv_w = glyph_w / atlas_w;
                    let uv_h = glyph_h / atlas_h;

                    // Offsets in atlas pixels (pen_x-relative, line_top-relative)
                    let offset_x = pb.left * em_scale;
                    let offset_y = (ascender - pb.top) * em_scale;

                    glyphs.insert(
                        ch,
                        GlyphInfo {
                            uv_x,
                            uv_y,
                            uv_w,
                            uv_h,
                            width: glyph_w,
                            height: glyph_h,
                            offset_x,
                            offset_y,
                            advance: g.advance * em_scale,
                        },
                    );
                }
                _ => {
                    // Non-visual glyph (e.g. space): advance only
                    glyphs.insert(
                        ch,
                        GlyphInfo {
                            uv_x: 0.0,
                            uv_y: 0.0,
                            uv_w: 0.0,
                            uv_h: 0.0,
                            width: 0.0,
                            height: 0.0,
                            offset_x: 0.0,
                            offset_y: 0.0,
                            advance: g.advance * em_scale,
                        },
                    );
                }
            }
        }

        Self {
            pixels,
            width,
            height,
            glyphs,
            font_size: font.metrics.line_height * em_scale,
        }
    }
}
