//! Exact output size (spec §7.1): `cover` crops the centre like Pillow `ImageOps.fit`,
//! `contain` keeps the whole subject on a transparent canvas.

use crate::error::{Category, EngineError};
use crate::size::Size;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fit {
    Cover,
    Contain,
}

impl Fit {
    pub fn parse(text: &str) -> Option<Fit> {
        match text {
            "cover" => Some(Fit::Cover),
            "contain" => Some(Fit::Contain),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Fit::Cover => "cover",
            Fit::Contain => "contain",
        }
    }
}

#[derive(Debug)]
pub struct Normalized {
    pub png: Vec<u8>,
    pub provider_size: Size,
    pub saved_size: Size,
    /// `"none"` when the provider bytes were already a PNG of the requested size.
    pub fit_applied: &'static str,
}

fn decode(bytes: &[u8]) -> Result<(DynamicImage, Option<ImageFormat>), EngineError> {
    let format = image::guess_format(bytes).ok();
    let image = image::load_from_memory(bytes)
        .map_err(|e| EngineError::new(Category::Provider, format!("provider returned an undecodable image: {e}")))?;
    Ok((image, format))
}

pub fn encode_png(image: &DynamicImage) -> Result<Vec<u8>, EngineError> {
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, ImageFormat::Png).map_err(|e| EngineError::new(Category::Internal, format!("PNG encoding failed: {e}")))?;
    Ok(out.into_inner())
}

pub fn normalize(bytes: &[u8], target: Size, fit: Fit) -> Result<Normalized, EngineError> {
    let (image, format) = decode(bytes)?;
    let provider_size = Size { width: image.width(), height: image.height() };
    if provider_size == target && format == Some(ImageFormat::Png) {
        return Ok(Normalized { png: bytes.to_vec(), provider_size, saved_size: target, fit_applied: "none" });
    }
    let fitted = match fit {
        Fit::Cover => cover(&image, target),
        Fit::Contain => contain(&image, target),
    };
    Ok(Normalized { png: encode_png(&fitted)?, provider_size, saved_size: target, fit_applied: fit.as_str() })
}

fn cover(image: &DynamicImage, target: Size) -> DynamicImage {
    let (w, h) = (image.width() as f64, image.height() as f64);
    let target_ratio = target.width as f64 / target.height as f64;
    let (crop_w, crop_h) = if w / h > target_ratio { (h * target_ratio, h) } else { (w, w / target_ratio) };
    let x = ((w - crop_w) / 2.0).round() as u32;
    let y = ((h - crop_h) / 2.0).round() as u32;
    let cropped = image.crop_imm(x, y, (crop_w.round() as u32).max(1), (crop_h.round() as u32).max(1));
    cropped.resize_exact(target.width, target.height, FilterType::Lanczos3)
}

fn contain(image: &DynamicImage, target: Size) -> DynamicImage {
    let scale = (target.width as f64 / image.width() as f64).min(target.height as f64 / image.height() as f64);
    let w = ((image.width() as f64 * scale).round() as u32).clamp(1, target.width);
    let h = ((image.height() as f64 * scale).round() as u32).clamp(1, target.height);
    let resized = image.resize_exact(w, h, FilterType::Lanczos3).to_rgba8();
    let mut canvas = RgbaImage::from_pixel(target.width, target.height, Rgba([0, 0, 0, 0]));
    image::imageops::overlay(&mut canvas, &resized, ((target.width - w) / 2) as i64, ((target.height - h) / 2) as i64);
    DynamicImage::ImageRgba8(canvas)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AlphaStats {
    pub has_alpha_channel: bool,
    pub transparent_ratio: f64,
    pub partial_ratio: f64,
    pub opaque_ratio: f64,
    /// Top-left, top-right, bottom-left, bottom-right.
    pub corner_alpha: [u8; 4],
}

pub fn alpha_stats(bytes: &[u8]) -> Result<AlphaStats, EngineError> {
    let (image, _) = decode(bytes)?;
    if !image.color().has_alpha() {
        return Ok(AlphaStats {
            has_alpha_channel: false,
            transparent_ratio: 0.0,
            partial_ratio: 0.0,
            opaque_ratio: 1.0,
            corner_alpha: [255; 4],
        });
    }
    let rgba = image.to_rgba8();
    let (mut transparent, mut partial) = (0u64, 0u64);
    for pixel in rgba.pixels() {
        match pixel[3] {
            0 => transparent += 1,
            255 => {}
            _ => partial += 1,
        }
    }
    let total = (rgba.width() as u64 * rgba.height() as u64).max(1) as f64;
    let (w, h) = (rgba.width() - 1, rgba.height() - 1);
    let corner = |x, y| rgba.get_pixel(x, y)[3];
    Ok(AlphaStats {
        has_alpha_channel: true,
        transparent_ratio: transparent as f64 / total,
        partial_ratio: partial as f64 / total,
        opaque_ratio: (total - transparent as f64 - partial as f64) / total,
        corner_alpha: [corner(0, 0), corner(w, 0), corner(0, h), corner(w, h)],
    })
}
