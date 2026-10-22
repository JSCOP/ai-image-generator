//! Mask edit input from the editor: painted layers (PNG, base64) become the engine's mask file and, when the user
//! drew on the image, the drawn source. Both are kept in the topic's metadata folder (spec §7.2).

use std::path::{Path, PathBuf};

use base64::Engine;
use image::{Rgba, RgbaImage};
use image_core::error::Category;
use image_core::mask::load_oriented;
use image_core::normalize::encode_png;
use serde::{Deserialize, Serialize};

use super::ServiceError;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EditInput {
    /// Area to change: PNG of the source size; painted pixels have alpha above 0.
    pub mask: String,
    /// Drawing over the image (PNG of the source size); drawn pixels count as marked.
    #[serde(default)]
    pub strokes: Option<String>,
}

/// Files a mask edit sends to the engine; `written` lists what this call created.
#[derive(Debug)]
pub struct EditFiles {
    pub reference: PathBuf,
    pub mask: PathBuf,
    pub written: Vec<PathBuf>,
}

fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError::new(Category::InvalidRequest, message)
}

/// PNG from base64, with or without a `data:image/png;base64,` prefix.
pub fn decode_layer(data: &str, what: &str) -> Result<RgbaImage, ServiceError> {
    let encoded = data.split_once(',').map_or(data, |(_, rest)| rest);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| invalid(format!("{what} 데이터를 읽지 못했습니다.")))?;
    Ok(image::load_from_memory(&bytes).map_err(|_| invalid(format!("{what} 이미지를 읽지 못했습니다.")))?.to_rgba8())
}

/// OpenAI-style mask (alpha 0 = edit): the painted alpha, and every drawn pixel fully marked.
pub fn engine_mask(painted: &RgbaImage, strokes: Option<&RgbaImage>) -> RgbaImage {
    RgbaImage::from_fn(painted.width(), painted.height(), |x, y| {
        let drawn = strokes.is_some_and(|s| s.get_pixel(x, y)[3] > 0);
        let weight = if drawn { 255 } else { painted.get_pixel(x, y)[3] };
        Rgba([0, 0, 0, 255 - weight])
    })
}

/// The source with the drawing on top.
pub fn draw_over(source: &RgbaImage, strokes: &RgbaImage) -> RgbaImage {
    let mut out = source.clone();
    image::imageops::overlay(&mut out, strokes, 0, 0);
    out
}

/// Writes `png` unless the same bytes are already there (a retry of the same run); never replaces other content.
fn write_once(path: &Path, png: &[u8], written: &mut Vec<PathBuf>) -> Result<(), ServiceError> {
    match std::fs::read(path) {
        Ok(existing) if existing == png => return Ok(()),
        Ok(_) => return Err(ServiceError::new(Category::Conflict, format!("다른 내용의 파일이 이미 있습니다: {}", path.display()))),
        Err(_) => {}
    }
    std::fs::write(path, png)
        .map_err(|e| ServiceError::new(Category::Internal, format!("파일을 쓰지 못했습니다: {}: {e}", path.display())))?;
    written.push(path.to_path_buf());
    Ok(())
}

/// Checks the layers against the source and writes `<topic>-mask.png` (and `<topic>-drawn.png`) to `metadata_dir`.
pub fn write_edit_files(source: &Path, edit: &EditInput, metadata_dir: &Path, topic: &str) -> Result<EditFiles, ServiceError> {
    let image = load_oriented(source)?;
    let size = image.dimensions();
    let painted = decode_layer(&edit.mask, "선택 영역")?;
    let strokes = edit.strokes.as_deref().map(|s| decode_layer(s, "그림")).transpose()?;
    for layer in std::iter::once(&painted).chain(&strokes) {
        if layer.dimensions() != size {
            let (w, h) = layer.dimensions();
            return Err(invalid(format!("선택 영역({w}×{h})이 원본 크기({}×{})와 다릅니다.", size.0, size.1)));
        }
    }
    let mask = engine_mask(&painted, strokes.as_ref());
    if mask.pixels().all(|p| p[3] == 255) {
        return Err(invalid("바꿀 부분을 칠하거나 그려 주세요."));
    }
    std::fs::create_dir_all(metadata_dir)
        .map_err(|e| ServiceError::new(Category::Internal, format!("폴더를 만들지 못했습니다: {}: {e}", metadata_dir.display())))?;
    let mut written = Vec::new();
    let mask_path = metadata_dir.join(format!("{topic}-mask.png"));
    write_once(&mask_path, &encode_png(&image::DynamicImage::ImageRgba8(mask))?, &mut written)?;
    let reference = match &strokes {
        Some(strokes) => {
            let drawn = metadata_dir.join(format!("{topic}-drawn.png"));
            write_once(&drawn, &encode_png(&image::DynamicImage::ImageRgba8(draw_over(&image, strokes)))?, &mut written)?;
            drawn
        }
        None => source.to_path_buf(),
    };
    Ok(EditFiles { reference, mask: mask_path, written })
}
