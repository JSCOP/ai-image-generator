//! Requested sizes, provider padding and Gemini size buckets (`scripts/gen_image.py`).

use serde::Serialize;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    /// Providers want multiples of 16; the saved file keeps the requested size.
    pub fn padded16(self) -> Size {
        Size { width: self.width.div_ceil(16) * 16, height: self.height.div_ceil(16) * 16 }
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

/// Error text matches `ai_image._validate_size`.
pub fn parse_size(text: &str) -> Result<Size, String> {
    let invalid = || format!("invalid size '{text}'; expected WIDTHxHEIGHT");
    let lower = text.to_lowercase();
    let mut parts = lower.split('x');
    let (Some(w), Some(h), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(invalid());
    };
    let (Ok(w), Ok(h)) = (w.trim().parse::<i64>(), h.trim().parse::<i64>()) else {
        return Err(invalid());
    };
    if w <= 0 || h <= 0 {
        return Err(format!("size {text}: width and height must be positive"));
    }
    let (Ok(width), Ok(height)) = (u32::try_from(w), u32::try_from(h)) else {
        return Err(invalid());
    };
    Ok(Size { width, height })
}

const GEMINI_RATIOS: [(&str, f64); 10] = [
    ("1:1", 1.0),
    ("2:3", 2.0 / 3.0),
    ("3:2", 3.0 / 2.0),
    ("3:4", 3.0 / 4.0),
    ("4:3", 4.0 / 3.0),
    ("4:5", 4.0 / 5.0),
    ("5:4", 5.0 / 4.0),
    ("9:16", 9.0 / 16.0),
    ("16:9", 16.0 / 9.0),
    ("21:9", 21.0 / 9.0),
];

pub fn gemini_aspect_ratio(size: Size) -> &'static str {
    let ratio = size.width as f64 / size.height as f64;
    let mut best = GEMINI_RATIOS[0];
    for candidate in GEMINI_RATIOS {
        if (candidate.1 - ratio).abs() < (best.1 - ratio).abs() {
            best = candidate;
        }
    }
    best.0
}

pub fn gemini_image_size(size: Size) -> &'static str {
    match size.width.max(size.height) {
        0..=768 => "512",
        769..=1536 => "1K",
        1537..=3072 => "2K",
        _ => "4K",
    }
}
