//! "배경 제거 (내 PC)" (spec §7.3, option 2): ISNet general-use (DIS, Apache-2.0) on the CPU through ONNX Runtime.
//! The model is not in the installer: the first use downloads it into `<home>/models` and checks its size and SHA-256.
//! The source image is never changed; the cut-out is saved as a new gallery image with its own record.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use image::imageops::FilterType;
use image::{GrayImage, Luma, RgbImage, RgbaImage};
use image_core::error::Category;
use image_core::mask::load_oriented;
use image_core::normalize::{alpha_stats, encode_png};
use image_core::paths::{absolute, safe_topic_slug, topic_dirs};
use image_core::store::{reserve, Record};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::generate::claim_topic;
use super::settings::load_settings;
use super::{Ctx, ServiceError};

/// A background removal model pinned by URL, size and hash (spec §7.3: redistributable license, fixed in the plan).
pub struct MatteModel {
    pub name: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
    pub license: &'static str,
}

/// rembg's ONNX export of DIS `isnet-general-use` (Apache-2.0); SHA-256 measured 2026-10-19.
pub const ISNET: MatteModel = MatteModel {
    name: "isnet-general-use",
    file: "isnet-general-use.onnx",
    url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx",
    sha256: "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
    bytes: 178_648_008,
    license: "Apache-2.0",
};

/// ISNet input side (square, RGB).
const INPUT_SIDE: u32 = 1024;
/// A stalled download fails instead of hanging.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// Progress sent to the screen.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "event", content = "data", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MatteEvent {
    Downloading { done: u64, total: u64 },
    Removing,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MatteResult {
    pub output: String,
    pub topic: String,
    pub transparent_ratio: f64,
}

fn internal(message: impl Into<String>) -> ServiceError {
    ServiceError::new(Category::Internal, message)
}

pub fn model_path(home: &Path) -> PathBuf {
    home.join("models").join(ISNET.file)
}

fn file_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Model files whose hash was checked in this process (hashing 170 MB on every use is wasted time).
static VERIFIED: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

fn verified(path: &Path, model: &MatteModel) -> bool {
    let mut seen = VERIFIED.lock().unwrap_or_else(|e| e.into_inner());
    let seen = seen.get_or_insert_with(HashSet::new);
    if seen.contains(path) {
        return true;
    }
    let ok = std::fs::metadata(path).is_ok_and(|m| m.len() == model.bytes) && file_sha256(path).is_ok_and(|hash| hash == model.sha256);
    if ok {
        seen.insert(path.to_path_buf());
    }
    ok
}

/// Makes sure `dest` holds the model: a verified file is reused; anything else is downloaded again to `.part`,
/// checked (size and SHA-256) and only then renamed into place. A mismatch deletes the download and fails.
pub async fn ensure_model(dest: &Path, model: &MatteModel, emit: &(dyn Fn(MatteEvent) + Send + Sync)) -> Result<(), ServiceError> {
    if verified(dest, model) {
        return Ok(());
    }
    let parent = dest.parent().ok_or_else(|| internal("모델 폴더를 정할 수 없습니다."))?;
    std::fs::create_dir_all(parent).map_err(|e| internal(format!("모델 폴더를 만들지 못했습니다: {e}")))?;
    let part = dest.with_extension("onnx.part");
    let failed = |message: String| {
        let _ = std::fs::remove_file(&part);
        ServiceError::new(Category::Provider, message)
    };
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(READ_TIMEOUT)
        .build()
        .map_err(|e| internal(format!("다운로드를 준비하지 못했습니다: {e}")))?;
    let mut response = client.get(model.url).send().await.map_err(|e| failed(format!("배경 제거 모델을 받지 못했습니다: {e}")))?;
    if !response.status().is_success() {
        return Err(failed(format!("배경 제거 모델을 받지 못했습니다: HTTP {}", response.status().as_u16())));
    }
    let mut file = std::fs::File::create(&part).map_err(|e| internal(format!("모델 파일을 만들지 못했습니다: {e}")))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    emit(MatteEvent::Downloading { done, total: model.bytes });
    while let Some(chunk) = response.chunk().await.map_err(|e| failed(format!("배경 제거 모델을 받다가 끊겼습니다: {e}")))? {
        done += chunk.len() as u64;
        if done > model.bytes {
            return Err(failed("배경 제거 모델 파일이 예상보다 큽니다.".into()));
        }
        hasher.update(&chunk);
        std::io::Write::write_all(&mut file, &chunk).map_err(|e| failed(format!("모델 파일을 쓰지 못했습니다: {e}")))?;
        emit(MatteEvent::Downloading { done, total: model.bytes });
    }
    drop(file);
    let hash: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if done != model.bytes || hash != model.sha256 {
        return Err(failed(format!("받은 배경 제거 모델이 올바르지 않습니다(크기 {done}, SHA-256 {hash}). 다시 시도해 주세요.")));
    }
    std::fs::rename(&part, dest).map_err(|e| failed(format!("모델 파일을 옮기지 못했습니다: {e}")))?;
    VERIFIED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new).insert(dest.to_path_buf());
    Ok(())
}

/// ISNet input: RGB resized to 1024², divided by the brightest channel value, minus 0.5 (rembg `DisSession`).
pub fn model_input(image: &RgbaImage) -> Vec<f32> {
    let rgb = RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y);
        image::Rgb([p[0], p[1], p[2]])
    });
    let small = image::imageops::resize(&rgb, INPUT_SIDE, INPUT_SIDE, FilterType::Lanczos3);
    let max = small.pixels().flat_map(|p| p.0).max().unwrap_or(1).max(1) as f32;
    let plane = (INPUT_SIDE * INPUT_SIDE) as usize;
    let mut data = vec![0f32; 3 * plane];
    for (x, y, p) in small.enumerate_pixels() {
        let i = (y * INPUT_SIDE + x) as usize;
        for c in 0..3 {
            data[c * plane + i] = p[c] as f32 / max - 0.5;
        }
    }
    data
}

/// Model output (1024², any range) stretched to 0..255 and resized to the image.
pub fn alpha_from_prediction(prediction: &[f32], width: u32, height: u32) -> GrayImage {
    let (lo, hi) = prediction.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let range = (hi - lo).max(f32::EPSILON);
    let small = GrayImage::from_fn(INPUT_SIDE, INPUT_SIDE, |x, y| {
        Luma([(((prediction[(y * INPUT_SIDE + x) as usize] - lo) / range) * 255.0).round() as u8])
    });
    image::imageops::resize(&small, width, height, FilterType::Lanczos3)
}

/// The source with its alpha lowered to the matte (an already transparent pixel stays transparent).
pub fn apply_alpha(image: &RgbaImage, alpha: &GrayImage) -> RgbaImage {
    let mut out = image.clone();
    for (x, y, p) in out.enumerate_pixels_mut() {
        p[3] = p[3].min(alpha.get_pixel(x, y)[0]);
    }
    out
}

/// Runs the model on the CPU.
fn predict(model: &Path, image: &RgbaImage) -> Result<GrayImage, ServiceError> {
    let ort_failed = |e: ort::Error| internal(format!("배경 제거 모델을 실행하지 못했습니다: {e}"));
    let env = ort::init().build().map_err(ort_failed)?;
    let mut session = ort::session::Session::builder(&env).map_err(ort_failed)?.commit_from_file(model).map_err(ort_failed)?;
    let side = INPUT_SIDE as usize;
    let input = ort::value::Tensor::from_array(([1usize, 3, side, side], model_input(image).into_boxed_slice())).map_err(ort_failed)?;
    let outputs = session.run(ort::inputs![input]).map_err(ort_failed)?;
    let (_, prediction) = outputs[0].try_extract_tensor::<f32>().map_err(ort_failed)?;
    if prediction.len() < side * side {
        return Err(internal("배경 제거 모델의 출력 크기가 예상과 다릅니다."));
    }
    Ok(alpha_from_prediction(&prediction[..side * side], image.width(), image.height()))
}

/// Cuts the subject out of `source` into a new topic of the work folder.
pub async fn remove_background(
    ctx: &Ctx,
    source: PathBuf,
    emit: impl Fn(MatteEvent) + Send + Sync + 'static,
) -> Result<MatteResult, ServiceError> {
    if !source.is_file() {
        return Err(ServiceError::new(Category::InvalidRequest, format!("이미지를 찾을 수 없습니다: {}", source.display())));
    }
    let model = model_path(&ctx.home);
    ensure_model(&model, &ISNET, &emit).await?;
    emit(MatteEvent::Removing);
    let settings = load_settings(&ctx.home);
    tauri::async_runtime::spawn_blocking(move || cut_out(&settings.work_dir, &model, &source))
        .await
        .map_err(|e| internal(format!("작업이 중단되었습니다: {e}")))?
}

/// Model run and publication; `model` must already be verified.
pub fn cut_out(work_dir: &Path, model: &Path, source: &Path) -> Result<MatteResult, ServiceError> {
    let image = load_oriented(source)?;
    let alpha = predict(model, &image)?;
    save_cut_out(work_dir, source, &apply_alpha(&image, &alpha))
}

/// Saves the cut-out as `<stem>-nobg-<time>` with a record (action `remove_background`), never over another file.
pub fn save_cut_out(work_dir: &Path, source: &Path, image: &RgbaImage) -> Result<MatteResult, ServiceError> {
    let stem = source.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
    let stem: String = stem.chars().take(48).collect();
    let base = safe_topic_slug(&format!("{stem}-nobg-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let claim = claim_topic(work_dir, &base);
    let topic = claim.topic().to_string();
    let (images, records) = topic_dirs(work_dir, &topic);
    let output = absolute(&images.join(format!("{topic}.png")));
    let record_path = records.join(format!("{topic}.png.json"));
    let png = encode_png(&image::DynamicImage::ImageRgba8(image.clone()))?;
    let alpha = alpha_stats(&png)?;
    let size = format!("{}x{}", image.width(), image.height());
    let record = Record {
        prompt: format!("배경 제거(내 PC): {}", source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
        image_model: ISNET.name.into(),
        size: size.clone(),
        quality: "local".into(),
        action: "remove_background".into(),
        reference_images: vec![absolute(source).display().to_string()],
        output: output.display().to_string(),
        background: Some("transparent".into()),
        saved_size: Some(size),
        alpha: Some(alpha.clone()),
        engine: Some(format!("studio {} + onnxruntime ({}, {})", env!("CARGO_PKG_VERSION"), ISNET.name, ISNET.license)),
        ..Record::default()
    };
    let reservation = reserve(&output, Some(&record_path), &records)?;
    if let Err(err) = reservation.write_pending(&record) {
        reservation.fail(record, &err);
        return Err(err.into());
    }
    if let Err(err) = reservation.publish(&png, record.clone()) {
        reservation.fail(record, &err);
        return Err(err.into());
    }
    Ok(MatteResult { output: output.display().to_string(), topic, transparent_ratio: alpha.transparent_ratio })
}
