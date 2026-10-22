//! Mask partial edits (spec §7.2). Providers behind CLIProxyAPI ignore an OpenAI `mask` (measured 2026-10-19:
//! the edit landed outside the marked area), so the engine sends only a crop around the marked area and pastes
//! the marked pixels back: everything outside the mask stays byte-identical to the source.

use crate::error::{Category, EngineError};
use crate::size::Size;
use image::{DynamicImage, GrayImage, ImageDecoder, Luma, RgbaImage};
use std::path::Path;

/// Soft edge inside the marked area, in source pixels (spec §7.2 "경계 부드럽게 기본 4px").
pub const FEATHER_PX: f32 = 4.0;
/// Context around the marked area sent with the crop, as a fraction of the area's longer side.
const CONTEXT: f64 = 0.35;
const MIN_CONTEXT_PX: u32 = 48;
/// Longer side of the size asked from the provider for the crop.
const PROVIDER_SIDE: u32 = 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

fn invalid(message: String) -> EngineError {
    EngineError::new(Category::InvalidRequest, message)
}

/// Decodes as RGBA with the EXIF orientation applied, so the mask and the source share one coordinate system.
pub fn load_oriented(path: &Path) -> Result<RgbaImage, EngineError> {
    let bad = |e: String| invalid(format!("image not readable: {}: {e}", path.display()));
    let mut decoder = image::ImageReader::open(path)
        .map_err(|e| bad(e.to_string()))?
        .with_guessed_format()
        .map_err(|e| bad(e.to_string()))?
        .into_decoder()
        .map_err(|e| bad(e.to_string()))?;
    let orientation = decoder.orientation().map_err(|e| bad(e.to_string()))?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| bad(e.to_string()))?;
    image.apply_orientation(orientation);
    Ok(image.to_rgba8())
}

/// Edit weight per pixel from an OpenAI-style mask: alpha 0 = edit fully, 255 = keep.
pub fn edit_weights(mask: &RgbaImage) -> GrayImage {
    GrayImage::from_fn(mask.width(), mask.height(), |x, y| Luma([255 - mask.get_pixel(x, y)[3]]))
}

/// Smallest rectangle holding every pixel with a non-zero weight; `None` when nothing is marked.
pub fn marked_bounds(weights: &GrayImage) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in weights.enumerate_pixels() {
        if p[0] > 0 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    (x0 != u32::MAX).then(|| Rect { x: x0, y: y0, width: x1 - x0 + 1, height: y1 - y0 + 1 })
}

/// The marked area plus surrounding context, clamped to the image.
pub fn crop_rect(marked: Rect, width: u32, height: u32) -> Rect {
    let pad = ((marked.width.max(marked.height) as f64 * CONTEXT).round() as u32).max(MIN_CONTEXT_PX);
    let x = marked.x.saturating_sub(pad);
    let y = marked.y.saturating_sub(pad);
    let right = (marked.x + marked.width + pad).min(width);
    let bottom = (marked.y + marked.height + pad).min(height);
    Rect { x, y, width: right - x, height: bottom - y }
}

/// Size asked from the provider for a crop: same shape, longer side 1024.
pub fn provider_size(crop: Rect) -> Size {
    let scale = PROVIDER_SIDE as f64 / crop.width.max(crop.height) as f64;
    Size { width: ((crop.width as f64 * scale).round() as u32).max(1), height: ((crop.height as f64 * scale).round() as u32).max(1) }
}

/// Validated inputs of one masked edit: the source, the weights and where to crop.
pub struct MaskedEdit {
    pub source: RgbaImage,
    pub weights: GrayImage,
    /// Bounds of the marked pixels and the crop sent around them, in source pixels.
    pub marked: Rect,
    pub crop: Rect,
}

/// Checks the mask against the source (same oriented size, something marked) and the requested size.
pub fn prepare(source: &Path, mask: &Path, size: Size) -> Result<MaskedEdit, EngineError> {
    let source = load_oriented(source)?;
    let mask_image = load_oriented(mask)?;
    let (w, h) = source.dimensions();
    if mask_image.dimensions() != (w, h) {
        let (mw, mh) = mask_image.dimensions();
        return Err(invalid(format!("mask is {mw}x{mh} but the source image is {w}x{h}; they must match")));
    }
    if (size.width, size.height) != (w, h) {
        return Err(invalid(format!("mask edits keep the source size; set size to {w}x{h}")));
    }
    let weights = edit_weights(&mask_image);
    let marked = marked_bounds(&weights).ok_or_else(|| invalid("mask marks no area to edit (no pixel with alpha below 255)".into()))?;
    Ok(MaskedEdit { crop: crop_rect(marked, w, h), marked, source, weights })
}

impl MaskedEdit {
    pub fn crop_png(&self) -> Result<Vec<u8>, EngineError> {
        let c = self.crop;
        let crop = image::imageops::crop_imm(&self.source, c.x, c.y, c.width, c.height).to_image();
        crate::normalize::encode_png(&DynamicImage::ImageRgba8(crop))
    }

    /// Guide image for the provider: the crop's marked pixels white, the rest black.
    pub fn guide_png(&self) -> Result<Vec<u8>, EngineError> {
        let c = self.crop;
        let guide =
            GrayImage::from_fn(c.width, c.height, |x, y| Luma([if self.weights.get_pixel(c.x + x, c.y + y)[0] > 0 { 255 } else { 0 }]));
        crate::normalize::encode_png(&DynamicImage::ImageLuma8(guide))
    }

    /// Prompt sent with the crop: the user's words, where the marked area lies in the crop, and the guide when sent.
    /// Without this the model fills the whole crop and the paste-back cuts its drawing off at the mask edge.
    pub fn crop_prompt(&self, prompt: &str, guide: bool) -> String {
        let (c, m) = (self.crop, self.marked);
        let pct = |offset: u32, side: u32| (offset as f64 * 100.0 / side as f64).round();
        let (x0, x1) = (pct(m.x - c.x, c.width), pct(m.x - c.x + m.width, c.width));
        let (y0, y1) = (pct(m.y - c.y, c.height), pct(m.y - c.y + m.height, c.height));
        let shown = if guide { " (white in the second image, which is a mask of this crop)" } else { "" };
        format!(
            "{prompt}\n\nThe first image is a crop of a larger picture. Make the change only inside the marked area{shown}, which spans \
             {x0}%-{x1}% of the width and {y0}%-{y1}% of the height of this crop. Everything you add must fit completely inside \
             that area. Keep the rest of the crop unchanged, keep lighting, perspective and style consistent, and return the whole \
             crop with the same framing."
        )
    }

    /// Source with the provider's crop pasted in where marked; pixels with zero weight are copied unchanged.
    pub fn composite(&self, edited: &DynamicImage) -> RgbaImage {
        let c = self.crop;
        let edited = crate::normalize::cover(edited, Size { width: c.width, height: c.height }).to_rgba8();
        let local = image::imageops::crop_imm(&self.weights, c.x, c.y, c.width, c.height).to_image();
        // Feather only inward: min(weight, blurred weight) is 0 wherever the weight is 0.
        let blurred = image::imageops::blur(&local, FEATHER_PX / 2.0);
        let mut out = self.source.clone();
        for (x, y, edited_px) in edited.enumerate_pixels() {
            let weight = local.get_pixel(x, y)[0].min(blurred.get_pixel(x, y)[0]);
            if weight == 0 {
                continue;
            }
            let t = weight as f32 / 255.0;
            let px = out.get_pixel_mut(c.x + x, c.y + y);
            for ch in 0..4 {
                px[ch] = (px[ch] as f32 * (1.0 - t) + edited_px[ch] as f32 * t).round() as u8;
            }
        }
        out
    }
}
